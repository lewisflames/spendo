use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tauri::State;

use crate::db::{ensure_same_book, run_blocking, run_io, with_tx_err, AppError, AppState};
use crate::history;
use crate::import_rules;
use crate::transactions;
use crate::utils;

/// Upper bounds keep a hostile CSV from materializing the whole book over IPC.
/// 10 MiB ≈ ~200k typical bank rows; the row cap binds the preview payload.
const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
const MAX_ROWS: usize = 20_000;
const SAMPLE_ROWS: usize = 5;

// ── Wire structs ─────────────────────────────────────────────────────────────

/// Column→field mapping. Exactly one of `amountCol` or the
/// `debitCol`+`creditCol` pair must be set; indices are 0-based into the
/// parsed record (missing cells read as empty).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvMapping {
    pub date_col: usize,
    pub desc_col: usize,
    pub amount_col: Option<usize>,
    pub debit_col: Option<usize>,
    pub credit_col: Option<usize>,
    pub num_col: Option<usize>,
    pub notes_col: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvPreviewInput {
    pub path: String,
    /// Hash of the file bytes as detected; the preview refuses to run against
    /// different content so the dry run always matches what was parsed.
    pub file_sha256: String,
    pub encoding: Option<String>,
    pub delimiter: Option<String>,
    pub has_header: Option<bool>,
    pub mapping: CsvMapping,
    pub target_account_guid: String,
    pub offset_account_guid: String,
    /// `"dmy"` (default) or `"mdy"` — resolves ambiguous `01/02/2026`.
    pub date_preference: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvCommitInput {
    pub path: String,
    /// Hash captured at detect/preview time; the commit aborts when the file
    /// changed in between (row keys would otherwise select different rows).
    pub file_sha256: String,
    pub encoding: Option<String>,
    pub delimiter: Option<String>,
    pub has_header: Option<bool>,
    pub mapping: CsvMapping,
    pub target_account_guid: String,
    pub offset_account_guid: String,
    pub date_preference: Option<String>,
    pub selected_keys: Vec<i64>,
    /// Upsert the mapping as this header layout's preset when the commit lands.
    pub remember_mapping: bool,
}

/// Saved mapping for one bank-statement layout, applied automatically when a
/// file with the same normalized header row is detected. Headerless files get
/// no preset (no stable key).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvPreset {
    pub signature: String,
    pub name: String,
    pub encoding: Option<String>,
    pub delimiter: String,
    pub has_header: bool,
    pub date_preference: String,
    pub mapping: CsvMapping,
    pub target_account_guid: String,
    pub offset_account_guid: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvDetectResult {
    pub encoding: String,
    pub delimiter: String,
    pub has_header: bool,
    pub headers: Vec<String>,
    pub column_count: i64,
    pub row_count: i64,
    pub sample_rows: Vec<Vec<String>>,
    pub filename: String,
    /// SHA-256 of the file bytes; preview/commit echo it back for TOCTOU safety.
    pub sha256: String,
    /// Normalized header-row signature (`None` for headerless files).
    pub signature: Option<String>,
    /// Saved mapping for this signature, when one exists in the book.
    pub preset: Option<CsvPreset>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvPreviewRow {
    pub key: i64,
    pub raw: Vec<String>,
    pub post_date: Option<String>,
    pub description: Option<String>,
    pub amount: Option<f64>,
    pub valid: bool,
    pub duplicate: bool,
    /// Category this row will post against (import-rule match or the fallback
    /// offset account), for the preview's Category column.
    pub offset_account_name: String,
    /// True when an import rule picked the category.
    pub rule_matched: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvPreviewResult {
    pub rows: Vec<CsvPreviewRow>,
    pub total_rows: i64,
    pub valid_count: i64,
    pub duplicate_count: i64,
    pub error_count: i64,
    pub min_date: Option<String>,
    pub max_date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvCommitResult {
    pub created: i64,
    pub skipped_duplicates: i64,
    pub skipped_errors: i64,
    pub entry_id: i64,
    pub label: String,
}

// ── File + decoding ──────────────────────────────────────────────────────────

fn validate_csv_path(path: &str) -> Result<PathBuf, AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("CSV path is required".to_string()));
    }
    let p = PathBuf::from(trimmed);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "csv" | "txt" | "tsv") {
        return Err(AppError::Validation(format!(
            "Not a CSV file (expected .csv/.txt/.tsv): {trimmed}"
        )));
    }
    if !p.is_file() {
        return Err(AppError::Validation(format!("File not found: {trimmed}")));
    }
    Ok(p)
}

fn read_file_bytes(path: &str) -> Result<Vec<u8>, AppError> {
    let p = validate_csv_path(path)?;
    let bytes = std::fs::read(&p).map_err(|e| {
        AppError::Validation(format!("Failed to read CSV file: {}", e))
    })?;
    if bytes.is_empty() {
        return Err(AppError::Validation("CSV file is empty".to_string()));
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AppError::Validation(
            "CSV file is too large (max 10 MB)".to_string(),
        ));
    }
    Ok(bytes)
}

fn filename_of(path: &str) -> String {
    PathBuf::from(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("import.csv")
        .to_string()
}

/// SHA-256 of raw bytes, same hex encoding as `csv_row_hash`.
fn bytes_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// Refuse file content that no longer matches what the user selected/previewed.
fn ensure_unchanged_file(bytes: &[u8], expected: &str) -> Result<(), AppError> {
    if bytes_sha256(bytes) != expected {
        return Err(AppError::Validation(
            "CSV file changed since it was selected — pick the file again".to_string(),
        ));
    }
    Ok(())
}

/// Decode bytes to text. BOM-aware: UTF-8 BOM stripped, UTF-16 LE/BE via
/// `encoding_rs`, else UTF-8, else Windows-1252 fallback (bank-statement
/// Latin-1). Returns `(text, encoding_label)`.
fn decode_bytes(bytes: &[u8], forced: Option<&str>) -> (String, String) {
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

fn count_delim_outside_quotes(line: &str, delim: char) -> usize {
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
fn sniff_delimiter(text: &str) -> char {
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
        let counts: Vec<usize> = lines.iter().map(|l| count_delim_outside_quotes(l, delim)).collect();
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

fn parse_delimiter(raw: Option<&str>, text: &str) -> Result<(u8, String), AppError> {
    if let Some(d) = raw {
        let c = d.chars().next().ok_or_else(|| {
            AppError::Validation("Delimiter must be a single character".to_string())
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

fn parse_records(text: &str, delimiter: u8) -> Result<Vec<Vec<String>>, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());
    let mut out = Vec::new();
    for record in reader.records() {
        let record =
            record.map_err(|e| AppError::Validation(format!("Failed to parse CSV: {e}")))?;
        let cells: Vec<String> = record.iter().map(|c| c.to_string()).collect();
        // Skip blank lines (all cells whitespace-empty).
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        out.push(cells);
    }
    if out.is_empty() {
        return Err(AppError::Validation(
            "CSV has no data rows".to_string(),
        ));
    }
    if out.len() > MAX_ROWS + 1 {
        return Err(AppError::Validation(format!(
            "Too many rows (max {MAX_ROWS})"
        )));
    }
    Ok(out)
}

fn looks_like_header(row: &[String]) -> bool {
    let joined = row.join(" ").to_ascii_lowercase();
    let has_date = joined.contains("date") || joined.contains("datum");
    let has_other = ["desc", "narrat", "memo", "amount", "debit", "credit", "balance", "payee"]
        .iter()
        .any(|k| joined.contains(k));
    has_date && has_other
}

// ── Field parsers ────────────────────────────────────────────────────────────

fn try_ymd(year: i32, month: u32, day: u32) -> Option<String> {
    chrono::NaiveDate::from_ymd_opt(year, month, day).map(|d| d.format("%Y-%m-%d").to_string())
}

fn map_two_digit_year(yy: i32) -> i32 {
    if (0..=68).contains(&yy) {
        2000 + yy
    } else {
        1900 + yy
    }
}

/// Lenient date parse into canonical `YYYY-MM-DD`. Accepts ISO plus the bank
/// trio `DD/MM/YYYY`, `MM/DD/YYYY`, `DD.MM.YYYY`, `DD-MM-YYYY` and a few
/// textual forms. `preference` (`"mdy"` or anything else = DMY) resolves
/// ambiguous slash dates like `01/02/2026`.
fn parse_csv_date(raw: &str, preference: &str) -> Result<String, AppError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(AppError::Validation("Date is required".to_string()));
    }
    // ISO datetime prefix: `2024-03-01T…` / `2024-03-01 10:00`.
    let first_token = s.split(['T', ' ']).next().unwrap_or(s).trim();
    for fmt in ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d"] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(first_token, fmt) {
            return Ok(d.format("%Y-%m-%d").to_string());
        }
    }
    // Textual forms.
    for fmt in [
        "%d %b %Y",
        "%d %B %Y",
        "%b %d, %Y",
        "%B %d, %Y",
        "%d-%b-%Y",
        "%d-%b-%y",
    ] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, fmt) {
            return Ok(d.format("%Y-%m-%d").to_string());
        }
    }
    // Numeric 3-part dates.
    let normalized = first_token.replace(['.', '-'], "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() == 3 {
        let a: Option<i32> = parts[0].trim().parse().ok();
        let b: Option<i32> = parts[1].trim().parse().ok();
        let c_raw = parts[2].trim();
        let c: Option<i32> = c_raw.parse().ok();
        if let (Some(a), Some(b), Some(c)) = (a, b, c) {
            // Year-first already handled above, but tolerate `2024/3/1`.
            if parts[0].len() == 4 {
                let year = a;
                if let Some(d) = try_ymd(year, b as u32, c as u32) {
                    return Ok(d);
                }
            } else {
                let year = if parts[2].len() == 2 { map_two_digit_year(c) } else { c };
                let mdy_first: [(i32, i32); 2] = if preference.eq_ignore_ascii_case("mdy") {
                    [(a, b), (b, a)]
                } else {
                    [(b, a), (a, b)]
                };
                // mdy_first holds (month, day) attempts in preference order.
                for (month, day) in mdy_first {
                    if let Some(d) = try_ymd(year, month as u32, day as u32) {
                        // Guard the classic ambiguity: when both orders parse
                        // (e.g. 01/02/2026) the loop order already encodes the
                        // preference, so the first hit wins.
                        return Ok(d);
                    }
                }
            }
        }
    }
    // Compact `YYYYMMDD` (some exports).
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        let y: i32 = s[0..4].parse().unwrap_or(0);
        let m: u32 = s[4..6].parse().unwrap_or(0);
        let d: u32 = s[6..8].parse().unwrap_or(0);
        if let Some(out) = try_ymd(y, m, d) {
            return Ok(out);
        }
    }
    Err(AppError::Validation(format!(
        "Invalid date (expected YYYY-MM-DD or bank format): {s}"
    )))
}

/// Lenient amount parse: strips currency symbols/spaces, `(…)` negatives,
/// normalizes typographic minus signs, and both thousand/decimal conventions
/// (last separator wins when both `,` and `.` are present; lone `,` with 1–2
/// trailing digits is a decimal comma). `dot_is_thousands` is the file-wide
/// convention from [`detect_dot_thousands`]; it makes a lone dot with 3-digit
/// groups a thousands separator (`1.234` → 1234). Zero is allowed so callers
/// can treat `0.00` in an unused Debit/Credit column as absent.
fn parse_csv_amount(raw: &str, dot_is_thousands: bool) -> Result<f64, AppError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(AppError::Validation("Amount is required".to_string()));
    }
    // Typographic minus/dash variants would otherwise be dropped by the filter
    // below, silently flipping a debit into a credit.
    let mut t: String = s
        .chars()
        .map(|c| match c {
            '\u{2212}' | '\u{2013}' | '\u{2014}' => '-',
            other => other,
        })
        .collect();
    let negative_parens = t.starts_with('(') && t.ends_with(')');
    if negative_parens {
        t = t[1..t.len() - 1].to_string();
    }
    // Keep digits, separators, sign, spaces; drop currency codes/symbols.
    let mut kept = String::with_capacity(t.len());
    for c in t.chars() {
        if c.is_ascii_digit() || matches!(c, ',' | '.' | '-' | '+' | ' ' | '\'' | '’') {
            kept.push(c);
        } else if c.is_whitespace() {
            kept.push(' ');
        }
    }
    let mut normalized: String = kept.chars().filter(|c| *c != ' ' && *c != '\'' && *c != '’').collect();
    if normalized.is_empty() || normalized == "-" || normalized == "+" {
        return Err(AppError::Validation(format!("Invalid amount: {s}")));
    }
    let has_comma = normalized.contains(',');
    let has_dot = normalized.contains('.');
    if has_comma && has_dot {
        let last_comma = normalized.rfind(',').unwrap_or(0);
        let last_dot = normalized.rfind('.').unwrap_or(0);
        if last_comma > last_dot {
            // `1.234,56` → remove dots, comma becomes the decimal point.
            normalized = normalized.replace('.', "");
            normalized = normalized.replace(',', ".");
        } else {
            // `1,234.56` → remove commas.
            normalized = normalized.replace(',', "");
        }
    } else if has_comma {
        let comma_count = normalized.matches(',').count();
        if comma_count > 1 {
            normalized = normalized.replace(',', "");
        } else if let Some(pos) = normalized.rfind(',') {
            let frac_len = normalized.len() - pos - 1;
            if frac_len == 2 || frac_len == 1 {
                normalized = normalized.replace(',', ".");
            } else if frac_len == 3
                && normalized.len() > 4
                && normalized[..pos].chars().all(|c| c.is_ascii_digit())
            {
                // `1,234` thousands — drop the comma; `12,345` likewise.
                // A 3-digit fraction with a short head is ambiguous, but bank
                // exports use `1,234.00` (handled above) or `1234,00` for
                // decimals, so thousands is the safer read here.
                normalized = normalized.replace(',', "");
            } else {
                normalized = normalized.replace(',', ".");
            }
        }
    } else if has_dot && dot_is_thousands {
        // Continental file (`1.234` means 1234). Only strip dots when every
        // group after the first has exactly three digits, so `1.23` stays a
        // (malformed but better-than-mangled) decimal.
        if normalized.split('.').skip(1).all(|group| group.len() == 3) {
            normalized = normalized.replace('.', "");
        }
    }
    let mut value: f64 = normalized
        .parse()
        .map_err(|_| AppError::Validation(format!("Invalid amount: {s}")))?;
    if negative_parens {
        value = -value.abs();
    }
    if !value.is_finite() {
        return Err(AppError::Validation(format!("Invalid amount: {s}")));
    }
    if value.abs() > 1_000_000_000.0 {
        return Err(AppError::Validation("Split amount is too large".to_string()));
    }
    Ok(value)
}

/// File-wide decimal convention: true when amounts use comma decimals
/// (`1,23`, `1234,56`) and never dot decimals (`1.23`). A lone dot must then
/// be a thousands separator, so `1.234` means 1234 (continental exports).
/// Mixed files keep the dot-decimal default.
fn detect_dot_thousands(data: &[Vec<String>], use_pair: bool, mapping: &CsvMapping) -> bool {
    fn tail_is_decimal(raw: &str, sep: char) -> bool {
        let t = raw.trim().trim_matches(|c: char| c == '(' || c == ')');
        let Some(pos) = t.rfind(sep) else { return false };
        let frac = &t[pos + 1..];
        let digits = frac.chars().filter(char::is_ascii_digit).count();
        (digits == 1 || digits == 2) && digits == frac.chars().count()
    }

    let mut comma_decimal = false;
    let mut dot_decimal = false;
    for cells in data {
        let indexes: Vec<usize> = if use_pair {
            [mapping.debit_col, mapping.credit_col]
                .into_iter()
                .flatten()
                .collect()
        } else {
            mapping.amount_col.into_iter().collect()
        };
        for index in indexes {
            if let Some(raw) = cells.get(index) {
                comma_decimal |= tail_is_decimal(raw, ',');
                dot_decimal |= tail_is_decimal(raw, '.');
            }
        }
    }
    comma_decimal && !dot_decimal
}

fn cell_at(cells: &[String], idx: Option<usize>) -> String {
    idx.and_then(|i| cells.get(i)).cloned().unwrap_or_default()
}

/// GUID→name for every account, used to label preview rows' categories.
fn load_account_names(conn: &Connection) -> Result<HashMap<String, String>, AppError> {
    let mut stmt = conn
        .prepare("SELECT guid, name FROM accounts")
        .map_err(AppError::from)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(AppError::from)?;
    rows.collect::<Result<HashMap<_, _>, _>>()
        .map_err(AppError::from)
}

fn require_col(idx: usize, column_count: usize, name: &str) -> Result<(), AppError> {
    if idx >= column_count {
        return Err(AppError::Validation(format!(
            "Column mapping out of range: {name} is column {}, but the file has {column_count}",
            idx + 1
        )));
    }
    Ok(())
}

fn validate_mapping(mapping: &CsvMapping, column_count: usize) -> Result<bool, AppError> {
    require_col(mapping.date_col, column_count, "date")?;
    require_col(mapping.desc_col, column_count, "description")?;
    let single = mapping.amount_col.is_some();
    let pair = mapping.debit_col.is_some() || mapping.credit_col.is_some();
    if single == pair {
        return Err(AppError::Validation(
            "Map either one Amount column or the Debit+Credit pair".to_string(),
        ));
    }
    if single {
        require_col(mapping.amount_col.unwrap_or(0), column_count, "amount")?;
        return Ok(false);
    }
    match (mapping.debit_col, mapping.credit_col) {
        (Some(d), Some(c)) => {
            require_col(d, column_count, "debit")?;
            require_col(c, column_count, "credit")?;
            if d == c {
                return Err(AppError::Validation(
                    "Debit and credit must be different columns".to_string(),
                ));
            }
            Ok(true)
        }
        _ => Err(AppError::Validation(
            "Debit+Credit mode needs both columns mapped".to_string(),
        )),
    }
}

/// Reject accounts that must not hold splits: the internal ROOT (posting to it
/// corrupts the balance-sheet identity) and placeholders. Shared by the CSV
/// pickers and the import-rule editor.
pub(crate) fn ensure_postable_account(
    conn: &Connection,
    guid: &str,
    label: &str,
) -> Result<(), AppError> {
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT COALESCE(placeholder, 0), account_type FROM accounts WHERE guid = ?1",
            params![guid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(AppError::from)?;
    let Some((placeholder, account_type)) = row else {
        return Err(AppError::NotFound(format!("{label} account not found")));
    };
    if account_type == "ROOT" {
        return Err(AppError::Validation(format!(
            "{label} account is the root account and cannot hold splits"
        )));
    }
    if placeholder != 0 {
        return Err(AppError::Validation(format!(
            "{label} account is a placeholder and cannot hold splits"
        )));
    }
    Ok(())
}

fn validate_accounts(conn: &Connection, target: &str, offset: &str) -> Result<(), AppError> {
    if target.trim().is_empty() || offset.trim().is_empty() {
        return Err(AppError::Validation(
            "Target and offset accounts are required".to_string(),
        ));
    }
    if target == offset {
        return Err(AppError::Validation(
            "Target and offset accounts must differ".to_string(),
        ));
    }
    // The internal ROOT is not a real account: posting to it corrupts the
    // balance-sheet identity (the picker never offers it).
    ensure_postable_account(conn, target, "Target")?;
    ensure_postable_account(conn, offset, "Offset")?;
    Ok(())
}

/// Stable duplicate key: `date | value_num (4dp) | lower(trim(desc))`.
fn csv_row_hash(post_date: &str, value_num: i64, description: &str) -> String {
    let normalized = format!(
        "{}|{}|{}",
        post_date,
        value_num,
        description.trim().to_lowercase()
    );
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

fn amount_to_value_num(amount: f64) -> i64 {
    (amount * transactions::VALUE_DENOM as f64).round() as i64
}

/// Existing rows in the target account for the parsed date window, keyed by
/// [`csv_row_hash`] with the number of copies the book holds. Counts (not a
/// set) let a file that legitimately contains more copies of a same-day charge
/// than the book import the surplus.
fn load_existing_counts(
    conn: &Connection,
    target_guid: &str,
    min_date: &str,
    max_date: &str,
) -> Result<HashMap<String, i64>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT substr(t.post_date, 1, 10), s.value_num, COALESCE(t.description, ''), COUNT(*)
             FROM splits s JOIN transactions t ON t.guid = s.tx_guid
             WHERE s.account_guid = ?1
               AND substr(t.post_date, 1, 10) BETWEEN ?2 AND ?3
             GROUP BY 1, 2, 3",
        )
        .map_err(AppError::from)?;
    let rows = stmt
        .query_map(params![target_guid, min_date, max_date], |row| {
            let date: String = row.get(0)?;
            let value_num: i64 = row.get(1)?;
            let desc: String = row.get(2)?;
            let count: i64 = row.get(3)?;
            Ok((date, value_num, desc, count))
        })
        .map_err(AppError::from)?;
    let mut out = HashMap::new();
    for row in rows {
        let (date, value_num, desc, count) = row.map_err(AppError::from)?;
        out.insert(csv_row_hash(&date, value_num, &desc), count);
    }
    Ok(out)
}

// ── Shared parse + validate ──────────────────────────────────────────────────

struct ParsedRow {
    key: i64,
    raw: Vec<String>,
    post_date: Option<String>,
    description: Option<String>,
    amount: Option<f64>,
    value_num: Option<i64>,
    num: Option<String>,
    notes: Option<String>,
    /// Category for this row: an import-rule match, else the fallback offset.
    /// `None` on error rows (they are never staged).
    offset_guid: Option<String>,
    /// True when an import rule picked the category.
    rule_matched: bool,
    error: Option<String>,
}

struct ParsedFile {
    rows: Vec<ParsedRow>,
}

impl ParsedRow {
    /// Error row; `value_num` mirrors `amount` so hashes match the valid path.
    fn invalid(
        key: i64,
        cells: &[String],
        post_date: Option<String>,
        description: &str,
        amount: Option<f64>,
        num: Option<String>,
        notes: Option<String>,
        error: String,
    ) -> Self {
        Self {
            key,
            raw: cells.to_vec(),
            post_date,
            description: Some(description.to_string()),
            amount,
            value_num: amount.map(amount_to_value_num),
            num,
            notes,
            offset_guid: None,
            rule_matched: false,
            error: Some(error),
        }
    }
}

fn parse_date_cell(
    cells: &[String],
    col: usize,
    date_preference: &str,
) -> Result<String, AppError> {
    parse_csv_date(&cell_at(cells, Some(col)), date_preference)
}

/// Resolve a row's signed amount from either the single Amount column or the
/// Debit/Credit pair; money out of the target is negative.
fn parse_amount_cell(
    cells: &[String],
    use_pair: bool,
    mapping: &CsvMapping,
    dot_is_thousands: bool,
) -> Result<f64, AppError> {
    if use_pair {
        let d_raw = cell_at(cells, mapping.debit_col);
        let c_raw = cell_at(cells, mapping.credit_col);
        let d_empty = d_raw.trim().is_empty();
        let c_empty = c_raw.trim().is_empty();
        if d_empty && c_empty {
            return Err(AppError::Validation(
                "Amount is required (debit or credit)".to_string(),
            ));
        }
        let d = if d_empty {
            0.0
        } else {
            parse_csv_amount(&d_raw, dot_is_thousands)?
        };
        let c = if c_empty {
            0.0
        } else {
            parse_csv_amount(&c_raw, dot_is_thousands)?
        };
        // `0.00` in the unused column means "unused"; only nonzero in both
        // columns is ambiguous, and an all-zero pair carries no amount.
        if d == 0.0 && c == 0.0 {
            return Err(AppError::Validation(
                "Amount is required (debit or credit)".to_string(),
            ));
        }
        if d != 0.0 && c != 0.0 {
            return Err(AppError::Validation(
                "Enter either debit or credit, not both".to_string(),
            ));
        }
        // Debit counts as money out of the target, credit as money in —
        // magnitudes only, so `-50` and `50` in Debit both read as −50.
        if d != 0.0 {
            Ok(-d.abs())
        } else {
            Ok(c.abs())
        }
    } else {
        let a_raw = cell_at(cells, mapping.amount_col);
        let amount = parse_csv_amount(&a_raw, dot_is_thousands)?;
        if amount == 0.0 {
            return Err(AppError::Validation("Amount must not be zero".to_string()));
        }
        Ok(amount)
    }
}

fn parse_file_rows(
    conn: &Connection,
    records: &[Vec<String>],
    has_header: bool,
    mapping: &CsvMapping,
    target_guid: &str,
    offset_guid: &str,
    date_preference: &str,
) -> Result<ParsedFile, AppError> {
    let column_count = records.iter().map(|r| r.len()).max().unwrap_or(0);
    if column_count == 0 {
        return Err(AppError::Validation("CSV has no columns".to_string()));
    }
    let use_pair = validate_mapping(mapping, column_count)?;
    validate_accounts(conn, target_guid, offset_guid)?;

    let data: &[Vec<String>] = if has_header { records.get(1..).unwrap_or(&[]) } else { records };

    // File-wide decimal convention, computed once: a lone dot is a thousands
    // separator only when the file demonstrably uses comma decimals.
    let dot_is_thousands = detect_dot_thousands(data, use_pair, mapping);
    // Import rules, loaded once: the first match picks the category per row,
    // the mapping step's offset account stays the fallback.
    let rules = import_rules::RuleMatcher::load(conn)?;

    let mut rows = Vec::with_capacity(data.len());
    for (i, cells) in data.iter().enumerate() {
        let key = i as i64;
        let description = cell_at(cells, Some(mapping.desc_col)).trim().to_string();

        let post_date = match parse_date_cell(cells, mapping.date_col, date_preference) {
            Ok(d) => d,
            Err(e) => {
                rows.push(ParsedRow::invalid(
                    key,
                    cells,
                    None,
                    &description,
                    None,
                    None,
                    None,
                    e.to_string(),
                ));
                continue;
            }
        };

        let amount = match parse_amount_cell(cells, use_pair, mapping, dot_is_thousands) {
            Ok(a) => a,
            Err(e) => {
                rows.push(ParsedRow::invalid(
                    key,
                    cells,
                    Some(post_date.clone()),
                    &description,
                    None,
                    None,
                    None,
                    e.to_string(),
                ));
                continue;
            }
        };

        let num = {
            let t = cell_at(cells, mapping.num_col).trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        };
        let notes = {
            let t = cell_at(cells, mapping.notes_col).trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        };
        let rule_match = rules.resolve(&description);
        let rule_matched = rule_match.is_some();
        let offset = rule_match.unwrap_or(offset_guid).to_string();
        // Reuse the manual-editor gate (future-date block, placeholder and
        // balance checks) so preview errors match commit errors exactly.
        let tx = transactions::TxInput {
            post_date: post_date.clone(),
            description: if description.is_empty() { None } else { Some(description.clone()) },
            num: num.clone(),
            notes: notes.clone(),
            splits: vec![
                transactions::SplitInput {
                    account_guid: target_guid.to_string(),
                    amount,
                    quantity: None,
                    memo: None,
                },
                transactions::SplitInput {
                    account_guid: offset.clone(),
                    amount: -amount,
                    quantity: None,
                    memo: None,
                },
            ],
        };
        if let Err(e) = transactions::validate_tx_input(conn, &tx) {
            rows.push(ParsedRow::invalid(
                key,
                cells,
                Some(post_date.clone()),
                &description,
                Some(amount),
                num,
                notes,
                e.to_string(),
            ));
            continue;
        }
        rows.push(ParsedRow {
            key,
            raw: cells.clone(),
            post_date: Some(post_date),
            description: Some(description),
            amount: Some(amount),
            value_num: Some(amount_to_value_num(amount)),
            num,
            notes,
            offset_guid: Some(offset),
            rule_matched,
            error: None,
        });
    }
    Ok(ParsedFile { rows })
}

/// One validated CSV row staged for insert: date, description, optional
/// num/notes, signed target-leg amount and the resolved category account.
struct StagedRow {
    post_date: String,
    description: String,
    num: Option<String>,
    notes: Option<String>,
    amount: f64,
    offset_guid: String,
}

struct StagedSelection {
    to_insert: Vec<StagedRow>,
    skipped_duplicates: i64,
    skipped_errors: i64,
    rules_applied: i64,
}

fn duplicate_flags(
    rows: &[ParsedRow],
    existing: &HashMap<String, i64>,
) -> HashMap<i64, bool> {
    let mut flags: HashMap<i64, bool> = HashMap::new();
    // Copy so the caller's counts stay untouched; consuming one copy per
    // duplicate keeps a file with two identical charges importable while the
    // book only holds one, and still flags a replay of an already-imported file.
    let mut remaining = existing.clone();
    for row in rows {
        if row.error.is_some() {
            flags.insert(row.key, false);
            continue;
        }
        let hash = csv_row_hash(
            row.post_date.as_deref().unwrap_or(""),
            row.value_num.unwrap_or(0),
            row.description.as_deref().unwrap_or(""),
        );
        let duplicate = match remaining.get_mut(&hash) {
            Some(count) if *count > 0 => {
                *count -= 1;
                true
            }
            _ => false,
        };
        flags.insert(row.key, duplicate);
    }
    flags
}

/// Duplicate flags for a parsed file, keyed by row key. The DB window spans
/// the valid dates only; rows already present in the target account flag.
fn duplicate_flags_for_rows(
    conn: &Connection,
    target_guid: &str,
    rows: &[ParsedRow],
) -> Result<HashMap<i64, bool>, AppError> {
    let mut dates: Vec<String> = rows
        .iter()
        .filter(|r| r.error.is_none())
        .filter_map(|r| r.post_date.clone())
        .collect();
    dates.sort();
    let existing = match (dates.first(), dates.last()) {
        (Some(min), Some(max)) => {
            load_existing_counts(conn, target_guid, min, max)?
        }
        _ => HashMap::new(),
    };
    Ok(duplicate_flags(rows, &existing))
}

// ── Mapping presets ──────────────────────────────────────────────────────────

/// Stable key for "the same bank statement layout": the normalized header row.
/// Headerless files have no stable key, so they don't get presets.
fn preset_signature(headers: &[String], has_header: bool) -> Option<String> {
    if !has_header {
        return None;
    }
    let normalized: Vec<String> = headers.iter().map(|h| h.trim().to_lowercase()).collect();
    if normalized.iter().all(|h| h.is_empty()) {
        return None;
    }
    Some(bytes_sha256(normalized.join("\u{1f}").as_bytes()))
}

/// Human-readable label for the detect notice.
fn preset_name(headers: &[String]) -> String {
    let mut name = headers
        .iter()
        .map(|h| h.trim())
        .filter(|h| !h.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" · ");
    if name.chars().count() > 80 {
        name = name.chars().take(80).collect();
    }
    if name.is_empty() {
        name = "CSV mapping".to_string();
    }
    name
}

fn load_preset(conn: &Connection, signature: &str) -> Result<Option<CsvPreset>, AppError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT mapping_json FROM csv_import_presets WHERE signature = ?1",
            params![signature],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::from)?;
    match json {
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| AppError::Validation(format!("Saved mapping is unreadable: {e}"))),
        None => Ok(None),
    }
}

fn save_preset(conn: &Connection, preset: &CsvPreset) -> Result<(), AppError> {
    let json = serde_json::to_string(preset)
        .map_err(|e| AppError::Validation(format!("Failed to store mapping: {e}")))?;
    conn.execute(
        "INSERT INTO csv_import_presets (signature, name, mapping_json, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(signature) DO UPDATE SET
           name = excluded.name,
           mapping_json = excluded.mapping_json,
           updated_at = excluded.updated_at",
        params![preset.signature, preset.name, json, utils::iso_now()],
    )
    .map_err(AppError::from)?;
    Ok(())
}

fn delete_preset(conn: &Connection, signature: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM csv_import_presets WHERE signature = ?1",
        params![signature],
    )
    .map_err(AppError::from)?;
    Ok(())
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn csv_detect(
    state: State<'_, AppState>,
    path: String,
) -> Result<CsvDetectResult, String> {
    // File read + parsing run on a blocking thread (the file may be ~10 MiB).
    let mut detect = run_io(move || csv_detect_inner(&path)).await?;
    // Saved mapping lookup is a book-scoped DB read, separate from file I/O.
    if let Some(signature) = detect.signature.clone() {
        detect.preset = run_blocking(state.db.clone(), move |conn| {
            load_preset(conn, &signature).map_err(String::from)
        })
        .await?;
    }
    Ok(detect)
}

fn csv_detect_inner(path: &str) -> Result<CsvDetectResult, String> {
    let bytes = read_file_bytes(path).map_err(String::from)?;
    let sha256 = bytes_sha256(&bytes);
    let (text, encoding) = decode_bytes(&bytes, None);
    let (_, delimiter) = parse_delimiter(None, &text).map_err(String::from)?;
    let delim_byte = delimiter.as_bytes()[0];
    let records = parse_records(&text, delim_byte).map_err(String::from)?;
    let has_header = records.first().map(|r| looks_like_header(r)).unwrap_or(false);
    let column_count = records.iter().map(|r| r.len()).max().unwrap_or(0) as i64;
    let data_len = if has_header {
        records.len().saturating_sub(1)
    } else {
        records.len()
    } as i64;
    let headers = if has_header {
        records.first().cloned().unwrap_or_default()
    } else {
        (1..=column_count).map(|i| format!("Column {i}")).collect()
    };
    let start = if has_header { 1 } else { 0 };
    let signature = preset_signature(&headers, has_header);
    let sample_rows: Vec<Vec<String>> = records
        .iter()
        .skip(start)
        .take(SAMPLE_ROWS)
        .cloned()
        .collect();
    Ok(CsvDetectResult {
        encoding,
        delimiter,
        has_header,
        headers,
        column_count,
        row_count: data_len,
        sample_rows,
        filename: filename_of(&path),
        sha256,
        signature,
        preset: None,
    })
}

fn preview_inner(
    conn: &Connection,
    input: CsvPreviewInput,
    bytes: &[u8],
) -> Result<CsvPreviewResult, AppError> {
    ensure_unchanged_file(bytes, &input.file_sha256)?;
    let (text, _) = decode_bytes(bytes, input.encoding.as_deref());
    let (delim_byte, _) = parse_delimiter(input.delimiter.as_deref(), &text)?;
    let records = parse_records(&text, delim_byte)?;
    let has_header = input.has_header.unwrap_or_else(|| {
        records.first().map(|r| looks_like_header(r)).unwrap_or(false)
    });
    let date_preference = input.date_preference.as_deref().unwrap_or("dmy");
    let parsed = parse_file_rows(
        conn,
        &records,
        has_header,
        &input.mapping,
        &input.target_account_guid,
        &input.offset_account_guid,
        date_preference,
    )?;

    let dup_flags = duplicate_flags_for_rows(conn, &input.target_account_guid, &parsed.rows)?;
    let account_names = load_account_names(conn)?;
    let fallback_name = account_names
        .get(&input.offset_account_guid)
        .cloned()
        .unwrap_or_default();

    let mut out_rows = Vec::with_capacity(parsed.rows.len());
    let mut valid_count = 0;
    let mut duplicate_count = 0;
    let mut error_count = 0;
    for row in parsed.rows {
        let duplicate = dup_flags.get(&row.key).copied().unwrap_or(false);
        let valid = row.error.is_none();
        if !valid {
            error_count += 1;
        } else if duplicate {
            duplicate_count += 1;
        } else {
            valid_count += 1;
        }
        let offset_account_name = row
            .offset_guid
            .as_ref()
            .and_then(|g| account_names.get(g))
            .cloned()
            .unwrap_or_else(|| fallback_name.clone());
        out_rows.push(CsvPreviewRow {
            key: row.key,
            raw: row.raw,
            post_date: row.post_date,
            description: row.description,
            amount: row.amount,
            valid,
            duplicate,
            offset_account_name,
            rule_matched: row.rule_matched,
            error: row.error,
        });
    }
    let min_date = out_rows
        .iter()
        .filter(|r| r.valid)
        .filter_map(|r| r.post_date.clone())
        .min();
    let max_date = out_rows
        .iter()
        .filter(|r| r.valid)
        .filter_map(|r| r.post_date.clone())
        .max();
    let total_rows = out_rows.len() as i64;
    Ok(CsvPreviewResult {
        rows: out_rows,
        total_rows,
        valid_count,
        duplicate_count,
        error_count,
        min_date,
        max_date,
    })
}

#[tauri::command]
pub async fn csv_preview(
    state: State<'_, AppState>,
    input: CsvPreviewInput,
) -> Result<CsvPreviewResult, String> {
    // Read the file off the runtime and before the DB lock.
    let path = input.path.clone();
    let bytes = run_io(move || read_file_bytes(&path).map_err(String::from)).await?;
    run_blocking(state.db.clone(), move |conn| {
        preview_inner(conn, input, &bytes).map_err(String::from)
    })
    .await
}

fn commit_inner(
    conn: &Connection,
    input: CsvCommitInput,
    bytes: &[u8],
) -> Result<CsvCommitResult, AppError> {
    ensure_unchanged_file(bytes, &input.file_sha256)?;
    let CommitParse {
        parsed,
        signature,
        name,
        delimiter,
        has_header,
    } = parse_commit_rows(conn, &input, bytes)?;
    let selection = select_rows_to_insert(
        conn,
        &input.target_account_guid,
        &parsed.rows,
        input.selected_keys.clone(),
    )?;
    let (entry_id, label) = insert_staged_rows(
        conn,
        &input.path,
        &input.target_account_guid,
        &input.offset_account_guid,
        selection.rules_applied,
        &selection.to_insert,
    )?;
    // Remember the mapping only after the rows landed, so a failed commit never
    // leaves a preset pointing at a mapping that was never applied.
    if input.remember_mapping {
        if let Some(signature) = signature {
            save_preset(
                conn,
                &CsvPreset {
                    signature,
                    name,
                    encoding: input.encoding.clone(),
                    delimiter,
                    has_header,
                    date_preference: input
                        .date_preference
                        .clone()
                        .unwrap_or_else(|| "dmy".to_string()),
                    mapping: input.mapping.clone(),
                    target_account_guid: input.target_account_guid.clone(),
                    offset_account_guid: input.offset_account_guid.clone(),
                },
            )?;
        }
    }
    Ok(CsvCommitResult {
        created: selection.to_insert.len() as i64,
        skipped_duplicates: selection.skipped_duplicates,
        skipped_errors: selection.skipped_errors,
        entry_id,
        label,
    })
}

/// Parsed commit plus the effective file options a saved preset needs.
struct CommitParse {
    parsed: ParsedFile,
    signature: Option<String>,
    name: String,
    delimiter: String,
    has_header: bool,
}

/// Decode, parse and validate the commit input into rows.
fn parse_commit_rows(
    conn: &Connection,
    input: &CsvCommitInput,
    bytes: &[u8],
) -> Result<CommitParse, AppError> {
    let (text, _) = decode_bytes(bytes, input.encoding.as_deref());
    let (delim_byte, delimiter) = parse_delimiter(input.delimiter.as_deref(), &text)?;
    let records = parse_records(&text, delim_byte)?;
    let has_header = input.has_header.unwrap_or_else(|| {
        records.first().map(|r| looks_like_header(r)).unwrap_or(false)
    });
    let headers: Vec<String> = if has_header {
        records.first().cloned().unwrap_or_default()
    } else {
        Vec::new()
    };
    let signature = preset_signature(&headers, has_header);
    let name = preset_name(&headers);
    let date_preference = input.date_preference.as_deref().unwrap_or("dmy");
    let preview_input = CsvPreviewInput {
        path: input.path.clone(),
        file_sha256: input.file_sha256.clone(),
        encoding: input.encoding.clone(),
        delimiter: input.delimiter.clone(),
        has_header: Some(has_header),
        mapping: input.mapping.clone(),
        target_account_guid: input.target_account_guid.clone(),
        offset_account_guid: input.offset_account_guid.clone(),
        date_preference: Some(date_preference.to_string()),
    };
    let parsed = parse_file_rows(
        conn,
        &records,
        has_header,
        &preview_input.mapping,
        &preview_input.target_account_guid,
        &preview_input.offset_account_guid,
        date_preference,
    )?;
    Ok(CommitParse {
        parsed,
        signature,
        name,
        delimiter,
        has_header,
    })
}

/// Filter the selected keys down to insertable rows, counting skipped
/// duplicates and errors. Errors when nothing is left to import.
fn select_rows_to_insert(
    conn: &Connection,
    target_guid: &str,
    rows: &[ParsedRow],
    selected_keys: Vec<i64>,
) -> Result<StagedSelection, AppError> {
    let dup_flags = duplicate_flags_for_rows(conn, target_guid, rows)?;

    let selected: HashSet<i64> = selected_keys.into_iter().collect();
    // Index parsed rows by key for O(1) selection.
    let by_key: HashMap<i64, &ParsedRow> = rows.iter().map(|r| (r.key, r)).collect();

    let mut to_insert: Vec<StagedRow> = Vec::new();
    let mut skipped_duplicates = 0;
    let mut skipped_errors = 0;
    let mut rules_applied = 0;
    let mut ordered_keys: Vec<i64> = by_key.keys().copied().collect();
    ordered_keys.sort();
    for key in ordered_keys {
        if !selected.contains(&key) {
            continue;
        }
        let Some(row) = by_key.get(&key) else {
            skipped_errors += 1;
            continue;
        };
        if row.error.is_some() {
            skipped_errors += 1;
            continue;
        }
        if dup_flags.get(&key).copied().unwrap_or(false) {
            skipped_duplicates += 1;
            continue;
        }
        let (Some(date), Some(desc), Some(amount), Some(offset)) = (
            row.post_date.clone(),
            row.description.clone(),
            row.amount,
            row.offset_guid.clone(),
        ) else {
            skipped_errors += 1;
            continue;
        };
        if row.rule_matched {
            rules_applied += 1;
        }
        to_insert.push(StagedRow {
            post_date: date,
            description: desc,
            num: row.num.clone(),
            notes: row.notes.clone(),
            amount,
            offset_guid: offset,
        });
    }
    if to_insert.is_empty() {
        return Err(AppError::Validation(
            "Nothing to import — all selected rows are duplicates or errors".to_string(),
        ));
    }
    Ok(StagedSelection {
        to_insert,
        skipped_duplicates,
        skipped_errors,
        rules_applied,
    })
}

/// Insert the staged rows inside one transaction and record a history entry.
/// Returns `(entry_id, label)`.
fn insert_staged_rows(
    conn: &Connection,
    path: &str,
    target_guid: &str,
    offset_guid: &str,
    rules_applied: i64,
    to_insert: &[StagedRow],
) -> Result<(i64, String), AppError> {
    let filename = filename_of(path);
    let label = format!("CSV import {filename} ({} transactions)", to_insert.len());
    let detail = serde_json::json!({
        "filePath": path,
        "filename": filename,
        "targetAccountGuid": target_guid,
        "offsetAccountGuid": offset_guid,
        "created": to_insert.len(),
        "rulesApplied": rules_applied,
    })
    .to_string();

    let entry_id = with_tx_err(conn, || {
        let mut snapshots: Vec<history::Snapshot<'_>> = Vec::new();
        // Own the GUID strings inside the txn so snapshot borrows stay valid.
        let mut owned_guids: Vec<(String, Vec<String>)> = Vec::new();
        for row in to_insert {
            let tx_guid = uuid::Uuid::new_v4().to_string();
            let description = {
                let t = row.description.trim();
                if t.is_empty() { None } else { Some(t.to_string()) }
            };
            conn.execute(
                "INSERT INTO transactions (guid, post_date, description, num, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    tx_guid,
                    row.post_date,
                    description,
                    row.num.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()),
                    row.notes.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()),
                ],
            )?;
            let splits = vec![
                transactions::SplitInput {
                    account_guid: target_guid.to_string(),
                    amount: row.amount,
                    quantity: None,
                    memo: None,
                },
                transactions::SplitInput {
                    account_guid: row.offset_guid.clone(),
                    amount: -row.amount,
                    quantity: None,
                    memo: None,
                },
            ];
            let split_guids = transactions::insert_split_rows(conn, &tx_guid, &splits)?;
            owned_guids.push((tx_guid, split_guids));
        }
        for (tx_guid, split_guids) in &owned_guids {
            snapshots.push(history::Snapshot {
                table_name: "transactions",
                row_guid: tx_guid.clone(),
                before_json: None,
            });
            for g in split_guids {
                snapshots.push(history::Snapshot {
                    table_name: "splits",
                    row_guid: g.clone(),
                    before_json: None,
                });
            }
        }
        let mut distinct_accounts: HashSet<&str> = HashSet::new();
        distinct_accounts.insert(target_guid);
        for row in to_insert {
            distinct_accounts.insert(row.offset_guid.as_str());
        }
        let entry_id = history::insert_change_entry(
            conn,
            "import",
            &label,
            "csv",
            to_insert.len() as i64,
            distinct_accounts.len() as i64,
            Some(&detail),
            &snapshots,
        )
        .map_err(AppError::from)?;
        Ok::<i64, AppError>(entry_id)
    })?;
    Ok((entry_id, label))
}

#[tauri::command]
pub async fn csv_commit(
    state: State<'_, AppState>,
    input: CsvCommitInput,
) -> Result<CsvCommitResult, String> {
    // Capture the active book before reading/parsing the file so a switch
    // during preparation aborts instead of writing into the new book.
    let generation = state
        .book_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    let path = input.path.clone();
    let bytes = run_io(move || read_file_bytes(&path).map_err(String::from)).await?;
    let generation_handle = state.book_generation.clone();
    run_blocking(state.db.clone(), move |conn| {
        ensure_same_book(&generation_handle, generation)?;
        commit_inner(conn, input, &bytes).map_err(String::from)
    })
    .await
}

/// Forget a saved mapping (the wizard's "Forget" action).
#[tauri::command]
pub async fn delete_csv_preset(
    state: State<'_, AppState>,
    signature: String,
) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        delete_preset(conn, &signature).map_err(String::from)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_delimiter_picks_semicolon() {
        let text = "a;b;c\n1;2;3\n4;5;6\n";
        assert_eq!(sniff_delimiter(text), ';');
    }

    #[test]
    fn sniff_delimiter_prefers_consistent_comma() {
        let text = "a,b,c\n1,2,3\n";
        assert_eq!(sniff_delimiter(text), ',');
    }

    #[test]
    fn parse_amount_handles_decimal_comma_and_thousands() {
        for raw in ["1.234,56", "1,234.56", "1234,56"] {
            assert!(
                (parse_csv_amount(raw, false).unwrap() - 1234.56).abs() < 1e-9,
                "{raw}"
            );
        }
        assert!((parse_csv_amount("(50.00)", false).unwrap() + 50.0).abs() < 1e-9);
        assert!((parse_csv_amount("$1,234.00", false).unwrap() - 1234.0).abs() < 1e-9);
        // Continental convention: lone dots group thousands.
        assert!((parse_csv_amount("1.234", true).unwrap() - 1234.0).abs() < 1e-9);
        assert!((parse_csv_amount("1.234.567", true).unwrap() - 1_234_567.0).abs() < 1e-9);
        // Two-digit fractions are never thousands groups; default stays decimal.
        assert!((parse_csv_amount("1.23", true).unwrap() - 1.23).abs() < 1e-9);
        assert!((parse_csv_amount("1.234", false).unwrap() - 1.234).abs() < 1e-9);
    }

    #[test]
    fn parse_amount_normalizes_typographic_minus() {
        // `−` (U+2212) used to be dropped, flipping a debit into a credit.
        assert!((parse_csv_amount("\u{2212}50.00", false).unwrap() + 50.0).abs() < 1e-9);
        assert!((parse_csv_amount("\u{2013}12,50", false).unwrap() + 12.5).abs() < 1e-9);
    }

    #[test]
    fn parse_amount_rejects_empty_and_keeps_zero_for_callers() {
        assert!(parse_csv_amount("", false).is_err());
        assert_eq!(parse_csv_amount("0.00", false).unwrap(), 0.0);
    }

    #[test]
    fn detect_dot_thousands_reads_the_file_convention() {
        fn mapping() -> CsvMapping {
            CsvMapping {
                date_col: 0,
                desc_col: 1,
                amount_col: Some(2),
                debit_col: None,
                credit_col: None,
                num_col: None,
                notes_col: None,
            }
        }
        let rows = |values: &[&str]| -> Vec<Vec<String>> {
            values
                .iter()
                .map(|v| vec!["2024-01-01".to_string(), "x".to_string(), (*v).to_string()])
                .collect()
        };
        assert!(detect_dot_thousands(&rows(&["1,23", "1.234"]), false, &mapping()));
        assert!(!detect_dot_thousands(&rows(&["1,23", "1.23"]), false, &mapping()));
        assert!(!detect_dot_thousands(&rows(&["1.23", "1.234"]), false, &mapping()));
    }

    /// The internal ROOT must never be a CSV target/offset, even though the
    /// backend would otherwise accept it (placeholder = 0).
    #[test]
    fn validate_accounts_rejects_root() {
        let conn = csv_e2e_setup();
        let err = validate_accounts(&conn, "root", "groceries").unwrap_err();
        assert!(err.to_string().contains("root account"), "unexpected: {err}");
    }

    #[test]
    fn pair_mode_treats_zero_cells_as_unused() {
        let mapping = CsvMapping {
            date_col: 0,
            desc_col: 1,
            amount_col: None,
            debit_col: Some(2),
            credit_col: Some(3),
            num_col: None,
            notes_col: None,
        };
        let cells = |debit: &str, credit: &str| {
            vec![
                "2024-01-01".to_string(),
                "x".to_string(),
                debit.to_string(),
                credit.to_string(),
            ]
        };
        // Bank exports often write 0.00 in the unused column.
        assert_eq!(
            parse_amount_cell(&cells("0.00", "50.00"), true, &mapping, false).unwrap(),
            50.0
        );
        assert_eq!(
            parse_amount_cell(&cells("50.00", "0.00"), true, &mapping, false).unwrap(),
            -50.0
        );
        // Both zero carries no amount; both nonzero stays ambiguous.
        assert!(parse_amount_cell(&cells("0.00", "0.00"), true, &mapping, false).is_err());
        assert!(parse_amount_cell(&cells("10.00", "20.00"), true, &mapping, false).is_err());
    }

    #[test]
    fn parse_date_handles_iso_and_bank_formats() {
        assert_eq!(parse_csv_date("2024-03-01", "dmy").unwrap(), "2024-03-01");
        assert_eq!(parse_csv_date("01/02/2024", "dmy").unwrap(), "2024-02-01");
        assert_eq!(parse_csv_date("01/02/2024", "mdy").unwrap(), "2024-01-02");
        assert_eq!(parse_csv_date("01.02.2024", "dmy").unwrap(), "2024-02-01");
        assert_eq!(parse_csv_date("20240301", "dmy").unwrap(), "2024-03-01");
    }

    #[test]
    fn row_hash_is_case_and_space_insensitive() {
        let a = csv_row_hash("2024-01-01", 10000, "  Coffee ");
        let b = csv_row_hash("2024-01-01", 10000, "coffee");
        assert_eq!(a, b);
        assert_ne!(a, csv_row_hash("2024-01-01", 20000, "coffee"));
    }

    #[test]
    fn decode_prefers_utf8_and_falls_back_to_latin1() {
        let (text, enc) = decode_bytes("héllo,1".as_bytes(), None);
        assert_eq!(enc, "utf-8");
        assert!(text.contains('h'));
        // 0xE9 alone is invalid UTF-8 → windows-1252 fallback yields é.
        let (text, enc) = decode_bytes(b"caf\xe9,1", None);
        assert_eq!(enc, "windows-1252");
        assert!(text.contains('é'));
    }

    fn csv_e2e_setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
                    ('assets', 'Assets', 'ASSET', 'root', 1, 'c-eur'),
                    ('checking', 'Checking', 'BANK', 'assets', 0, 'c-eur'),
                    ('groceries', 'Groceries', 'EXPENSE', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn
    }

    fn write_temp_csv(name: &str, contents: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("spendo-{name}-{}-{}.csv", std::process::id(), rand_suffix()));
        std::fs::write(&p, contents).unwrap();
        p.to_string_lossy().to_string()
    }

    fn rand_suffix() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        format!("{nanos}")
    }

    fn preview_input(path: &str) -> CsvPreviewInput {
        CsvPreviewInput {
            path: path.to_string(),
            file_sha256: bytes_sha256(&read_file_bytes(path).unwrap()),
            encoding: None,
            delimiter: None,
            has_header: Some(true),
            mapping: CsvMapping {
                date_col: 0,
                desc_col: 1,
                amount_col: Some(2),
                debit_col: None,
                credit_col: None,
                num_col: None,
                notes_col: None,
            },
            target_account_guid: "checking".to_string(),
            offset_account_guid: "groceries".to_string(),
            date_preference: Some("dmy".to_string()),
        }
    }

    #[test]
    fn preview_commit_then_repreview_flags_duplicates() {
        let conn = csv_e2e_setup();
        let csv = "Date,Description,Amount\n2024-01-05,Coffee,-3.50\n2024-01-06,Bakery,-12.00\n";
        let path = write_temp_csv("preview-commit", csv);
        let input = preview_input(&path);

        let bytes = read_file_bytes(&path).unwrap();
        let first = preview_inner(&conn, input.clone(), &bytes).expect("preview runs");
        assert_eq!(first.total_rows, 2);
        assert_eq!(first.valid_count, 2);
        assert_eq!(first.duplicate_count, 0);
        assert_eq!(first.error_count, 0);

        let commit = CsvCommitInput {
            path: path.clone(),
            file_sha256: bytes_sha256(&bytes),
            encoding: None,
            delimiter: None,
            has_header: Some(true),
            mapping: input.mapping.clone(),
            target_account_guid: input.target_account_guid.clone(),
            offset_account_guid: input.offset_account_guid.clone(),
            date_preference: Some("dmy".to_string()),
            selected_keys: vec![0, 1],
            remember_mapping: true,
        };
        let result = commit_inner(&conn, commit, &bytes).expect("commit runs");
        assert_eq!(result.created, 2);
        let tx_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tx_count, 2);
        let (kind, source): (String, String) = conn
            .query_row(
                "SELECT kind, source FROM change_log WHERE id = ?1",
                params![result.entry_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((kind.as_str(), source.as_str()), ("import", "csv"));

        // Second preview of the same file flags both rows as duplicates.
        let second = preview_inner(&conn, input, &bytes).expect("re-preview runs");
        assert_eq!(second.valid_count, 0);
        assert_eq!(second.duplicate_count, 2);

        // Linear undo removes the imported transactions.
        crate::history::undo_history_entry_inner(&conn, result.entry_id).expect("undo runs");
        let tx_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tx_count, 0);
        std::fs::remove_file(&path).ok();
    }

    /// Regression: two identical same-day charges are legitimate and must both
    /// be importable — only rows already in the book flag as duplicates.
    #[test]
    fn intra_file_repeats_are_both_importable() {
        let conn = csv_e2e_setup();
        let csv = "Date,Description,Amount\n2024-02-01,Shop,-5.00\n2024-02-01,Shop,-5.00\n";
        let path = write_temp_csv("intra-dup", csv);
        let bytes = read_file_bytes(&path).unwrap();
        let report = preview_inner(&conn, preview_input(&path), &bytes).expect("preview runs");
        assert_eq!(report.valid_count, 2);
        assert_eq!(report.duplicate_count, 0);

        let commit = CsvCommitInput {
            path: path.clone(),
            file_sha256: bytes_sha256(&bytes),
            encoding: None,
            delimiter: None,
            has_header: Some(true),
            mapping: preview_input(&path).mapping,
            target_account_guid: "checking".to_string(),
            offset_account_guid: "groceries".to_string(),
            date_preference: Some("dmy".to_string()),
            selected_keys: vec![0, 1],
            remember_mapping: true,
        };
        let result = commit_inner(&conn, commit, &bytes).expect("commit runs");
        assert_eq!(result.created, 2);
        assert_eq!(result.skipped_duplicates, 0);

        // A second commit of the same file is still rejected by the DB hash.
        let replay = commit_inner(
            &conn,
            CsvCommitInput {
                path: path.clone(),
                file_sha256: bytes_sha256(&bytes),
                encoding: None,
                delimiter: None,
                has_header: Some(true),
                mapping: preview_input(&path).mapping,
                target_account_guid: "checking".to_string(),
                offset_account_guid: "groceries".to_string(),
                date_preference: Some("dmy".to_string()),
                selected_keys: vec![0, 1],
            remember_mapping: true,
            },
            &bytes,
        )
        .unwrap_err();
        assert!(replay.to_string().contains("duplicates or errors"), "{replay}");

        std::fs::remove_file(&path).ok();
    }

    fn commit_input(path: &str, bytes: &[u8]) -> CsvCommitInput {
        CsvCommitInput {
            path: path.to_string(),
            file_sha256: bytes_sha256(bytes),
            encoding: None,
            delimiter: None,
            has_header: Some(true),
            mapping: CsvMapping {
                date_col: 0,
                desc_col: 1,
                amount_col: Some(2),
                debit_col: None,
                credit_col: None,
                num_col: None,
                notes_col: None,
            },
            target_account_guid: "checking".to_string(),
            offset_account_guid: "groceries".to_string(),
            date_preference: Some("dmy".to_string()),
            selected_keys: vec![0, 1],
            remember_mapping: true,
        }
    }

    /// The file must not change between detect/preview and commit: row keys are
    /// indices, so altered content would select different rows.
    #[test]
    fn stale_file_hash_is_rejected() {
        let conn = csv_e2e_setup();
        let csv = "Date,Description,Amount\n2024-01-05,Coffee,-3.50\n";
        let path = write_temp_csv("stale-hash", csv);
        let bytes = read_file_bytes(&path).unwrap();

        let mut input = preview_input(&path);
        input.file_sha256 = "0".repeat(64);
        let err = preview_inner(&conn, input, &bytes).unwrap_err();
        assert!(err.to_string().contains("changed"), "unexpected: {err}");

        let stale = commit_input(&path, &bytes);
        let mut stale = stale;
        stale.file_sha256 = "0".repeat(64);
        let err = commit_inner(&conn, stale, &bytes).unwrap_err();
        assert!(err.to_string().contains("changed"), "unexpected: {err}");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);

        std::fs::remove_file(&path).ok();
    }

    /// Regression for count-based dedupe: the book holds one copy, the file has
    /// two, so exactly the surplus imports.
    #[test]
    fn file_with_more_copies_than_book_imports_the_extras() {
        let conn = csv_e2e_setup();
        let one = "Date,Description,Amount\n2024-02-01,Shop,-5.00\n";
        let path_one = write_temp_csv("one-copy", one);
        let bytes_one = read_file_bytes(&path_one).unwrap();
        let first = commit_inner(&conn, commit_input(&path_one, &bytes_one), &bytes_one).unwrap();
        assert_eq!(first.created, 1);

        let two = "Date,Description,Amount\n2024-02-01,Shop,-5.00\n2024-02-01,Shop,-5.00\n";
        let path = write_temp_csv("two-copies", two);
        let bytes = read_file_bytes(&path).unwrap();
        let report = preview_inner(&conn, preview_input(&path), &bytes).unwrap();
        assert_eq!(report.duplicate_count, 1);
        assert_eq!(report.valid_count, 1);

        let result = commit_inner(&conn, commit_input(&path, &bytes), &bytes).unwrap();
        assert_eq!(result.created, 1);
        assert_eq!(result.skipped_duplicates, 1);

        // A third copy is a duplicate again.
        let err = commit_inner(&conn, commit_input(&path, &bytes), &bytes).unwrap_err();
        assert!(err.to_string().contains("duplicates or errors"), "{err}");

        std::fs::remove_file(&path).ok();
        std::fs::remove_file(&path_one).ok();
    }

    #[test]
    fn preset_signature_normalizes_headers_and_skips_headerless() {
        let a = preset_signature(&["Date".to_string(), " Description ".to_string()], true);
        let b = preset_signature(&[" date".to_string(), "description".to_string()], true);
        assert!(a.is_some());
        assert_eq!(a, b);
        assert!(preset_signature(&["Date".to_string()], false).is_none());
        assert!(preset_signature(&["".to_string(), " ".to_string()], true).is_none());
    }

    #[test]
    fn commit_saves_and_delete_forgets_preset() {
        let conn = csv_e2e_setup();
        let csv = "Date,Description,Amount\n2024-01-05,Coffee,-3.50\n";
        let path = write_temp_csv("preset", csv);
        let bytes = read_file_bytes(&path).unwrap();
        let signature = preset_signature(
            &[
                "Date".to_string(),
                "Description".to_string(),
                "Amount".to_string(),
            ],
            true,
        )
        .unwrap();
        assert!(load_preset(&conn, &signature).unwrap().is_none());

        commit_inner(&conn, commit_input(&path, &bytes), &bytes).unwrap();
        let preset = load_preset(&conn, &signature).unwrap().expect("preset saved");
        assert_eq!(preset.target_account_guid, "checking");
        assert_eq!(preset.offset_account_guid, "groceries");
        assert_eq!(preset.delimiter, ",");
        assert!(preset.has_header);
        assert_eq!(preset.mapping.amount_col, Some(2));
        assert_eq!(preset.name, "Date · Description · Amount");

        delete_preset(&conn, &signature).unwrap();
        assert!(load_preset(&conn, &signature).unwrap().is_none());

        std::fs::remove_file(&path).ok();
    }

    /// Import rules pick the category per row; unmatched rows keep the mapping
    /// step's fallback offset account.
    #[test]
    fn import_rules_override_the_fallback_offset() {
        let conn = csv_e2e_setup();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('coffee', 'Coffee', 'EXPENSE', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        crate::import_rules::create_import_rule_inner(
            &conn,
            &crate::import_rules::ImportRuleInput {
                priority: 1,
                match_type: "contains".to_string(),
                pattern: "coffee".to_string(),
                account_guid: "coffee".to_string(),
            },
        )
        .unwrap();

        let csv = "Date,Description,Amount\n2024-01-05,Morning Coffee,-3.50\n2024-01-06,Bakery,-12.00\n";
        let path = write_temp_csv("rules", csv);
        let bytes = read_file_bytes(&path).unwrap();

        let report = preview_inner(&conn, preview_input(&path), &bytes).unwrap();
        assert!(report.rows[0].rule_matched);
        assert_eq!(report.rows[0].offset_account_name, "Coffee");
        assert!(!report.rows[1].rule_matched);
        assert_eq!(report.rows[1].offset_account_name, "Groceries");

        let result = commit_inner(&conn, commit_input(&path, &bytes), &bytes).unwrap();
        assert_eq!(result.created, 2);
        let splits_for = |account: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM splits WHERE account_guid = ?1",
                params![account],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(splits_for("coffee"), 1);
        assert_eq!(splits_for("groceries"), 1);

        let detail: String = conn
            .query_row(
                "SELECT detail_json FROM change_log WHERE id = ?1",
                params![result.entry_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(detail.contains("\"rulesApplied\":1"), "{detail}");

        std::fs::remove_file(&path).ok();
    }
}
