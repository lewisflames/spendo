use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tauri::State;

use crate::db::{run_blocking, run_io, with_tx_err, AppError, AppState};
use crate::history;
use crate::transactions;

/// Upper bounds keep a hostile CSV from materializing the whole book over IPC.
/// 10 MiB ≈ ~200k typical bank rows; the row cap binds the preview payload.
const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
const MAX_ROWS: usize = 20_000;
const SAMPLE_ROWS: usize = 5;

// ── Wire structs ─────────────────────────────────────────────────────────────

/// Column→field mapping. Exactly one of `amountCol` or the
/// `debitCol`+`creditCol` pair must be set; indices are 0-based into the
/// parsed record (missing cells read as empty).
#[derive(Debug, Clone, Deserialize)]
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
    pub encoding: Option<String>,
    pub delimiter: Option<String>,
    pub has_header: Option<bool>,
    pub mapping: CsvMapping,
    pub target_account_guid: String,
    pub offset_account_guid: String,
    pub date_preference: Option<String>,
    pub selected_keys: Vec<i64>,
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
/// and both thousand/decimal conventions (last separator wins when both `,`
/// and `.` are present; lone `,` with 1–2 trailing digits is a decimal comma).
fn parse_csv_amount(raw: &str) -> Result<f64, AppError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(AppError::Validation("Amount is required".to_string()));
    }
    let mut t = s.to_string();
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
    if value == 0.0 {
        return Err(AppError::Validation("Amount must not be zero".to_string()));
    }
    Ok(value)
}

fn cell_at(cells: &[String], idx: Option<usize>) -> String {
    idx.and_then(|i| cells.get(i)).cloned().unwrap_or_default()
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
    for (label, guid) in [("Target", target), ("Offset", offset)] {
        let row: Option<(i64,)> = conn
            .query_row(
                "SELECT placeholder FROM accounts WHERE guid = ?1",
                params![guid],
                |r| r.get(0),
            )
            .optional()
            .map_err(AppError::from)?
            .map(|p: Option<i64>| (p.unwrap_or(0),));
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM accounts WHERE guid = ?1",
                params![guid],
                |_| Ok(()),
            )
            .optional()
            .map_err(AppError::from)?
            .is_some();
        if !exists {
            return Err(AppError::NotFound(format!("{label} account not found")));
        }
        if row.map(|(p,)| p).unwrap_or(0) != 0 {
            return Err(AppError::Validation(format!(
                "{label} account is a placeholder and cannot hold splits"
            )));
        }
    }
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

fn load_existing_hashes(
    conn: &Connection,
    target_guid: &str,
    min_date: &str,
    max_date: &str,
) -> Result<HashSet<String>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT substr(t.post_date, 1, 10), s.value_num, t.description
             FROM splits s JOIN transactions t ON t.guid = s.tx_guid
             WHERE s.account_guid = ?1
               AND substr(t.post_date, 1, 10) BETWEEN ?2 AND ?3",
        )
        .map_err(AppError::from)?;
    let rows = stmt
        .query_map(params![target_guid, min_date, max_date], |row| {
            let date: String = row.get(0)?;
            let value_num: i64 = row.get(1)?;
            let desc: Option<String> = row.get(2)?;
            Ok((date, value_num, desc.unwrap_or_default()))
        })
        .map_err(AppError::from)?;
    let mut out = HashSet::new();
    for row in rows {
        let (date, value_num, desc) = row.map_err(AppError::from)?;
        out.insert(csv_row_hash(&date, value_num, &desc));
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
            parse_csv_amount(&d_raw)?
        };
        let c = if c_empty {
            0.0
        } else {
            parse_csv_amount(&c_raw)?
        };
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
        parse_csv_amount(&a_raw)
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

        let amount = match parse_amount_cell(cells, use_pair, mapping) {
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
                    account_guid: offset_guid.to_string(),
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
            error: None,
        });
    }
    Ok(ParsedFile { rows })
}

/// One validated CSV row staged for insert: date, description, original key,
/// optional num/notes, and the signed target-leg amount.
type StagedRow = (String, String, String, Option<String>, Option<String>, f64);

struct StagedSelection {
    to_insert: Vec<StagedRow>,
    skipped_duplicates: i64,
    skipped_errors: i64,
}

fn duplicate_flags(
    rows: &[ParsedRow],
    existing: &HashSet<String>,
) -> HashMap<i64, bool> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut flags: HashMap<i64, bool> = HashMap::new();
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
        // First occurrence wins: DB hits and later intra-file repeats flag.
        let is_dup = existing.contains(&hash) || !seen.insert(hash);
        flags.insert(row.key, is_dup);
    }
    flags
}

/// Duplicate flags for a parsed file, keyed by row key. The DB window spans
/// the valid dates only; first occurrence wins (DB + intra-file).
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
            load_existing_hashes(conn, target_guid, min, max)?
        }
        _ => HashSet::new(),
    };
    Ok(duplicate_flags(rows, &existing))
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn csv_detect(path: String) -> Result<CsvDetectResult, String> {
    // File read + parsing run on a blocking thread (the file may be ~10 MiB).
    run_io(move || csv_detect_inner(&path)).await
}

fn csv_detect_inner(path: &str) -> Result<CsvDetectResult, String> {
    let bytes = read_file_bytes(path).map_err(String::from)?;
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
    })
}

fn preview_inner(
    conn: &Connection,
    input: CsvPreviewInput,
    bytes: &[u8],
) -> Result<CsvPreviewResult, AppError> {
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
        out_rows.push(CsvPreviewRow {
            key: row.key,
            raw: row.raw,
            post_date: row.post_date,
            description: row.description,
            amount: row.amount,
            valid,
            duplicate,
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
    let parsed = parse_commit_rows(conn, &input, bytes)?;
    let selection = select_rows_to_insert(
        conn,
        &input.target_account_guid,
        &parsed.rows,
        input.selected_keys,
    )?;
    let (entry_id, label) = insert_staged_rows(
        conn,
        &input.path,
        &input.target_account_guid,
        &input.offset_account_guid,
        &selection.to_insert,
    )?;
    Ok(CsvCommitResult {
        created: selection.to_insert.len() as i64,
        skipped_duplicates: selection.skipped_duplicates,
        skipped_errors: selection.skipped_errors,
        entry_id,
        label,
    })
}

/// Decode, parse and validate the commit input into rows.
fn parse_commit_rows(
    conn: &Connection,
    input: &CsvCommitInput,
    bytes: &[u8],
) -> Result<ParsedFile, AppError> {
    let (text, _) = decode_bytes(bytes, input.encoding.as_deref());
    let (delim_byte, _) = parse_delimiter(input.delimiter.as_deref(), &text)?;
    let records = parse_records(&text, delim_byte)?;
    let has_header = input.has_header.unwrap_or_else(|| {
        records.first().map(|r| looks_like_header(r)).unwrap_or(false)
    });
    let date_preference = input.date_preference.as_deref().unwrap_or("dmy");
    let preview_input = CsvPreviewInput {
        path: input.path.clone(),
        encoding: input.encoding.clone(),
        delimiter: input.delimiter.clone(),
        has_header: Some(has_header),
        mapping: input.mapping.clone(),
        target_account_guid: input.target_account_guid.clone(),
        offset_account_guid: input.offset_account_guid.clone(),
        date_preference: Some(date_preference.to_string()),
    };
    parse_file_rows(
        conn,
        &records,
        has_header,
        &preview_input.mapping,
        &preview_input.target_account_guid,
        &preview_input.offset_account_guid,
        date_preference,
    )
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
        let (Some(date), Some(desc), Some(amount)) =
            (row.post_date.clone(), row.description.clone(), row.amount)
        else {
            skipped_errors += 1;
            continue;
        };
        to_insert.push((
            date,
            desc,
            key.to_string(),
            row.num.clone(),
            row.notes.clone(),
            amount,
        ));
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
    })
}

/// Insert the staged rows inside one transaction and record a history entry.
/// Returns `(entry_id, label)`.
fn insert_staged_rows(
    conn: &Connection,
    path: &str,
    target_guid: &str,
    offset_guid: &str,
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
    })
    .to_string();

    let entry_id = with_tx_err(conn, || {
        let mut snapshots: Vec<history::Snapshot<'_>> = Vec::new();
        // Own the GUID strings inside the txn so snapshot borrows stay valid.
        let mut owned_guids: Vec<(String, Vec<String>)> = Vec::new();
        for (date, desc, _, num, notes, amount) in to_insert {
            let tx_guid = uuid::Uuid::new_v4().to_string();
            let description = {
                let t = desc.trim();
                if t.is_empty() { None } else { Some(t.to_string()) }
            };
            conn.execute(
                "INSERT INTO transactions (guid, post_date, description, num, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    tx_guid,
                    date,
                    description,
                    num.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()),
                    notes.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()),
                ],
            )?;
            let splits = vec![
                transactions::SplitInput {
                    account_guid: target_guid.to_string(),
                    amount: *amount,
                    quantity: None,
                    memo: None,
                },
                transactions::SplitInput {
                    account_guid: offset_guid.to_string(),
                    amount: -*amount,
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
        let distinct_accounts: HashSet<&str> = [target_guid, offset_guid].into_iter().collect();
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
    let path = input.path.clone();
    let bytes = run_io(move || read_file_bytes(&path).map_err(String::from)).await?;
    run_blocking(state.db.clone(), move |conn| {
        commit_inner(conn, input, &bytes).map_err(String::from)
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
        assert!((parse_csv_amount("1.234,56").unwrap() - 1234.56).abs() < 1e-9);
        assert!((parse_csv_amount("1,234.56").unwrap() - 1234.56).abs() < 1e-9);
        assert!((parse_csv_amount("1234,56").unwrap() - 1234.56).abs() < 1e-9);
        assert!((parse_csv_amount("(50.00)").unwrap() + 50.0).abs() < 1e-9);
        assert!((parse_csv_amount("$1,234.00").unwrap() - 1234.0).abs() < 1e-9);
    }

    #[test]
    fn parse_amount_rejects_empty_and_zero() {
        assert!(parse_csv_amount("").is_err());
        assert!(parse_csv_amount("0.00").is_err());
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
            encoding: None,
            delimiter: None,
            has_header: Some(true),
            mapping: input.mapping.clone(),
            target_account_guid: input.target_account_guid.clone(),
            offset_account_guid: input.offset_account_guid.clone(),
            date_preference: Some("dmy".to_string()),
            selected_keys: vec![0, 1],
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

        // Linear revert removes the imported transactions.
        crate::history::revert_history_entry_inner(&conn, result.entry_id)
            .expect("revert runs");
        let tx_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tx_count, 0);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn intra_file_repeat_flags_second_occurrence() {
        let conn = csv_e2e_setup();
        let csv = "Date,Description,Amount\n2024-02-01,Shop,-5.00\n2024-02-01,Shop,-5.00\n";
        let path = write_temp_csv("intra-dup", csv);
        let bytes = read_file_bytes(&path).unwrap();
        let report = preview_inner(&conn, preview_input(&path), &bytes).expect("preview runs");
        assert_eq!(report.valid_count, 1);
        assert_eq!(report.duplicate_count, 1);
        std::fs::remove_file(&path).ok();
    }
}
