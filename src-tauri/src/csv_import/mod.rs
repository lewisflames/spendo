use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tauri::State;

use crate::db::{ensure_same_book, run_blocking, run_io, with_tx_err, AppError, AppState};
use crate::import_rules;
use crate::transactions;
use crate::utils;

mod decode;
mod fields;
mod presets;

use decode::*;
pub(crate) use fields::ensure_postable_account;
use fields::*;
use presets::*;

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
    pub label: String,
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
    /// Error row for a failure before the amount parsed.
    fn invalid(
        key: i64,
        cells: &[String],
        post_date: Option<String>,
        description: &str,
        error: String,
    ) -> Self {
        Self {
            key,
            raw: cells.to_vec(),
            post_date,
            description: Some(description.to_string()),
            amount: None,
            value_num: None,
            num: None,
            notes: None,
            offset_guid: None,
            rule_matched: false,
            error: Some(error),
        }
    }

    /// Attach the parsed amount to an error row; `value_num` mirrors `amount`
    /// so hashes match the valid path.
    fn with_amount(self, amount: f64, num: Option<String>, notes: Option<String>) -> Self {
        Self {
            amount: Some(amount),
            value_num: Some(amount_to_value_num(amount)),
            num,
            notes,
            ..self
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
            return Err(AppError::Validation(
                "Amounts can't be zero. Skip the row or add a value".to_string(),
            ));
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
        return Err(AppError::Validation(
            "That CSV has no columns. Check the delimiter and try again".to_string(),
        ));
    }
    let use_pair = validate_mapping(mapping, column_count)?;
    validate_accounts(conn, target_guid, offset_guid)?;

    let data: &[Vec<String>] = if has_header {
        records.get(1..).unwrap_or(&[])
    } else {
        records
    };

    // File-wide decimal convention, computed once: a lone dot is a thousands
    // separator only when the file demonstrably uses comma decimals.
    let dot_is_thousands = detect_dot_thousands(data, use_pair, mapping);
    // Import rules, loaded once: the first match picks the category per row,
    // the mapping step's offset account stays the fallback.
    let rules = import_rules::RuleMatcher::load(conn)?;

    // Account facts are shared by every row (target, offset, and any
    // rule-matched category); memoize lookups instead of querying per row.
    let account_cache: std::cell::RefCell<HashMap<String, Option<transactions::SplitAccountInfo>>> =
        std::cell::RefCell::new(HashMap::new());
    let account_lookup = |guid: &str| -> Result<Option<transactions::SplitAccountInfo>, AppError> {
        if let Some(cached) = account_cache.borrow().get(guid).cloned() {
            return Ok(cached);
        }
        let info = transactions::account_info_lookup(conn, guid)?;
        account_cache
            .borrow_mut()
            .insert(guid.to_string(), info.clone());
        Ok(info)
    };

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
                    e.to_string(),
                ));
                continue;
            }
        };

        let num = {
            let t = cell_at(cells, mapping.num_col).trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        };
        let notes = {
            let t = cell_at(cells, mapping.notes_col).trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        };
        let rule_match = rules.resolve(&description);
        let rule_matched = rule_match.is_some();
        let offset = rule_match.unwrap_or(offset_guid).to_string();
        // Reuse the manual-editor gate (future-date block, placeholder and
        // balance checks) so preview errors match commit errors exactly.
        let tx = transactions::TxInput {
            post_date: post_date.clone(),
            description: if description.is_empty() {
                None
            } else {
                Some(description.clone())
            },
            num: num.clone(),
            notes: notes.clone(),
            unreconcile: false,
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
        if let Err(e) = transactions::validate_tx_input_with(&tx, &account_lookup) {
            rows.push(
                ParsedRow::invalid(
                    key,
                    cells,
                    Some(post_date.clone()),
                    &description,
                    e.to_string(),
                )
                .with_amount(amount, num, notes),
            );
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
}

fn duplicate_flags(rows: &[ParsedRow], existing: &HashMap<String, i64>) -> HashMap<i64, bool> {
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
        (Some(min), Some(max)) => load_existing_counts(conn, target_guid, min, max)?,
        _ => HashMap::new(),
    };
    Ok(duplicate_flags(rows, &existing))
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn csv_detect(
    state: State<'_, AppState>,
    path: String,
) -> Result<CsvDetectResult, AppError> {
    // File read + parsing run on a blocking thread (the file may be ~10 MiB).
    let mut detect = run_io(move || csv_detect_inner(&path)).await?;
    // Saved mapping lookup is a book-scoped DB read, separate from file I/O.
    if let Some(signature) = detect.signature.clone() {
        detect.preset =
            run_blocking(state.db.clone(), move |conn| load_preset(conn, &signature)).await?;
    }
    Ok(detect)
}

fn csv_detect_inner(path: &str) -> Result<CsvDetectResult, AppError> {
    let bytes = read_file_bytes(path)?;
    let sha256 = bytes_sha256(&bytes);
    let (text, encoding) = decode_bytes(&bytes, None);
    let (_, delimiter) = parse_delimiter(None, &text)?;
    let delim_byte = delimiter.as_bytes()[0];
    let records = parse_records(&text, delim_byte)?;
    let has_header = records
        .first()
        .map(|r| looks_like_header(r))
        .unwrap_or(false);
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
        filename: filename_of(path),
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
        records
            .first()
            .map(|r| looks_like_header(r))
            .unwrap_or(false)
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
) -> Result<CsvPreviewResult, AppError> {
    // Read the file off the runtime and before the DB lock.
    let path = input.path.clone();
    let bytes = run_io(move || read_file_bytes(&path)).await?;
    run_blocking(state.db.clone(), move |conn| {
        preview_inner(conn, input, &bytes)
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
    let label = insert_staged_rows(
        conn,
        &input.path,
        &input.target_account_guid,
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
        records
            .first()
            .map(|r| looks_like_header(r))
            .unwrap_or(false)
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
            "Nothing to import. All selected rows are duplicates or errors".to_string(),
        ));
    }
    Ok(StagedSelection {
        to_insert,
        skipped_duplicates,
        skipped_errors,
    })
}

/// Insert the staged rows inside one transaction. Returns the import label.
fn insert_staged_rows(
    conn: &Connection,
    path: &str,
    target_guid: &str,
    to_insert: &[StagedRow],
) -> Result<String, AppError> {
    let filename = filename_of(path);
    let label = format!("CSV import {filename} ({} transactions)", to_insert.len());

    with_tx_err(conn, || -> Result<(), AppError> {
        let mut insert_tx = conn.prepare_cached(
            "INSERT INTO transactions (guid, post_date, description, num, notes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for row in to_insert {
            let tx_guid = uuid::Uuid::new_v4().to_string();
            let description = {
                let t = row.description.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            };
            insert_tx.execute(params![
                tx_guid,
                row.post_date,
                description,
                row.num.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()),
                row.notes
                    .as_ref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty()),
            ])?;
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
            transactions::insert_split_rows(conn, &tx_guid, &splits)?;
        }
        Ok(())
    })?;
    Ok(label)
}

#[tauri::command]
pub async fn csv_commit(
    state: State<'_, AppState>,
    input: CsvCommitInput,
) -> Result<CsvCommitResult, AppError> {
    // Capture the active book before reading/parsing the file so a switch
    // during preparation aborts instead of writing into the new book.
    let generation = state
        .book_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    let path = input.path.clone();
    let bytes = run_io(move || read_file_bytes(&path)).await?;
    let generation_handle = state.book_generation.clone();
    let app_data_dir = state.app_data_dir.clone();
    run_blocking(state.db.clone(), move |conn| {
        ensure_same_book(&generation_handle, generation)?;
        crate::backups::before_change(
            conn,
            &app_data_dir,
            crate::backups::BackupReason::BeforeImport,
        )?;
        commit_inner(conn, input, &bytes)
    })
    .await
}

/// Forget a saved mapping (the wizard's "Forget" action).
#[tauri::command]
pub async fn delete_csv_preset(
    state: State<'_, AppState>,
    signature: String,
) -> Result<(), AppError> {
    run_blocking(state.db.clone(), move |conn| {
        delete_preset(conn, &signature)
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
        assert!(detect_dot_thousands(
            &rows(&["1,23", "1.234"]),
            false,
            &mapping()
        ));
        assert!(!detect_dot_thousands(
            &rows(&["1,23", "1.23"]),
            false,
            &mapping()
        ));
        assert!(!detect_dot_thousands(
            &rows(&["1.23", "1.234"]),
            false,
            &mapping()
        ));
    }

    /// The internal ROOT must never be a CSV target/offset, even though the
    /// backend would otherwise accept it (placeholder = 0).
    #[test]
    fn validate_accounts_rejects_root() {
        let conn = csv_e2e_setup();
        let err = validate_accounts(&conn, "root", "groceries").unwrap_err();
        assert!(
            err.to_string().contains("root account"),
            "unexpected: {err}"
        );
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
        p.push(format!(
            "spendo-{name}-{}-{}.csv",
            std::process::id(),
            rand_suffix()
        ));
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

        // Second preview of the same file flags both rows as duplicates.
        let second = preview_inner(&conn, input, &bytes).expect("re-preview runs");
        assert_eq!(second.valid_count, 0);
        assert_eq!(second.duplicate_count, 2);

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
        assert!(
            replay.to_string().contains("duplicates or errors"),
            "{replay}"
        );

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
        let preset = load_preset(&conn, &signature)
            .unwrap()
            .expect("preset saved");
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

        let csv =
            "Date,Description,Amount\n2024-01-05,Morning Coffee,-3.50\n2024-01-06,Bakery,-12.00\n";
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

        std::fs::remove_file(&path).ok();
    }
}
