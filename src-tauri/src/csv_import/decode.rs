use super::*;

// ── File + decoding ──────────────────────────────────────────────────────────

pub(super) fn validate_csv_path(path: &str) -> Result<PathBuf, AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(
            "Pick a CSV file to get started".to_string(),
        ));
    }
    let p = PathBuf::from(trimmed);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "csv" | "txt" | "tsv") {
        return Err(AppError::Validation(format!(
            "That doesn't look like a CSV file (.csv, .txt or .tsv): {trimmed}"
        )));
    }
    if !p.is_file() {
        return Err(AppError::Validation(format!(
            "We couldn't find that file: {trimmed}"
        )));
    }
    Ok(p)
}

pub(super) fn read_file_bytes(path: &str) -> Result<Vec<u8>, AppError> {
    let p = validate_csv_path(path)?;
    // Read at most one byte past the cap so an oversized file is rejected
    // without materializing it in memory (`std::fs::read` allocates first).
    let file = std::fs::File::open(&p)
        .map_err(|e| AppError::Validation(format!("We couldn't read that CSV file: {}", e)))?;
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| AppError::Validation(format!("We couldn't read that CSV file: {}", e)))?;
    if bytes.is_empty() {
        return Err(AppError::Validation(
            "That CSV file is empty. Pick one with rows in it".to_string(),
        ));
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::Validation(
            "That CSV file is over 10 MB. Trim it down and try again".to_string(),
        ));
    }
    Ok(bytes)
}

pub(super) fn filename_of(path: &str) -> String {
    PathBuf::from(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("import.csv")
        .to_string()
}

/// SHA-256 of raw bytes, same hex encoding as `csv_row_hash`.
pub(super) fn bytes_sha256(bytes: &[u8]) -> String {
    utils::hex_digest(Sha256::digest(bytes))
}

/// Refuse file content that no longer matches what the user selected/previewed.
pub(super) fn ensure_unchanged_file(bytes: &[u8], expected: &str) -> Result<(), AppError> {
    if bytes_sha256(bytes) != expected {
        return Err(AppError::Validation(
            "CSV file changed since it was selected. Pick the file again".to_string(),
        ));
    }
    Ok(())
}

/// Decode bytes to text. BOM-aware: UTF-8 BOM stripped, UTF-16 LE/BE via
/// `encoding_rs`, else UTF-8, else Windows-1252 fallback (bank-statement
/// Latin-1). Returns `(text, encoding_label)`.
pub(super) fn decode_bytes(bytes: &[u8], forced: Option<&str>) -> (String, String) {
    if let Some(enc) = forced {
        if enc.eq_ignore_ascii_case("windows-1252") || enc.eq_ignore_ascii_case("latin1") {
            let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
            return (cow.into_owned(), "windows-1252".to_string());
        }
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(&bytes[3..]) {
            return (s.to_string(), "utf-8".to_string());
        }
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (cow, _, _) = encoding_rs::UTF_16LE.decode(bytes);
        return (cow.into_owned(), "utf-16le".to_string());
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (cow, _, _) = encoding_rs::UTF_16BE.decode(bytes);
        return (cow.into_owned(), "utf-16be".to_string());
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_string(), "utf-8".to_string());
    }
    let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    (cow.into_owned(), "windows-1252".to_string())
}

pub(super) fn count_delim_outside_quotes(line: &str, delim: char) -> usize {
    let mut count = 0;
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            if in_quotes && chars.peek() == Some(&'"') {
                chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        } else if c == delim && !in_quotes {
            count += 1;
        }
    }
    count
}

/// Delimiter sniff over the first non-empty lines. A delimiter scores when
/// every sampled line carries the same positive count (header + rows align);
/// inconsistent counts score half. Falls back to `,`.
pub(super) fn sniff_delimiter(text: &str) -> char {
    const CANDIDATES: [char; 4] = [',', ';', '\t', '|'];
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .take(8)
        .collect();
    if lines.is_empty() {
        return ',';
    }
    let mut best = (',', 0usize);
    for &delim in &CANDIDATES {
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| count_delim_outside_quotes(l, delim))
            .collect();
        let total: usize = counts.iter().sum();
        if total == 0 {
            continue;
        }
        let consistent = counts.iter().all(|&c| c == counts[0] && c > 0);
        let score = if consistent { total * 2 } else { total };
        if score > best.1 {
            best = (delim, score);
        }
    }
    // Ties keep the initial ','.
    best.0
}

pub(super) fn parse_delimiter(raw: Option<&str>, text: &str) -> Result<(u8, String), AppError> {
    if let Some(d) = raw {
        let c = d.chars().next().ok_or_else(|| {
            AppError::Validation("Pick a single character for the delimiter".to_string())
        })?;
        if !matches!(c, ',' | ';' | '\t' | '|') {
            return Err(AppError::Validation(format!(
                "Unsupported delimiter (expected , ; tab |): {d}"
            )));
        }
        return Ok((c as u8, c.to_string()));
    }
    let c = sniff_delimiter(text);
    Ok((c as u8, c.to_string()))
}

pub(super) fn parse_records(text: &str, delimiter: u8) -> Result<Vec<Vec<String>>, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());
    let mut out = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| {
            AppError::Validation(format!("We couldn't make sense of that CSV: {e}"))
        })?;
        let cells: Vec<String> = record.iter().map(|c| c.to_string()).collect();
        // Skip blank lines (all cells whitespace-empty).
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        out.push(cells);
        // Stop consuming the reader as soon as the cap is exceeded instead of
        // materializing every remaining record first.
        if out.len() > MAX_ROWS + 1 {
            return Err(AppError::Validation(format!(
                "Too many rows (max {MAX_ROWS})"
            )));
        }
    }
    if out.is_empty() {
        return Err(AppError::Validation("CSV has no data rows".to_string()));
    }
    Ok(out)
}

pub(super) fn looks_like_header(row: &[String]) -> bool {
    let joined = row.join(" ").to_ascii_lowercase();
    let has_date = joined.contains("date") || joined.contains("datum");
    let has_other = [
        "desc", "narrat", "memo", "amount", "debit", "credit", "balance", "payee",
    ]
    .iter()
    .any(|k| joined.contains(k));
    has_date && has_other
}
