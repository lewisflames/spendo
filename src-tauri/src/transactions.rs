use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::fx;
use crate::history;
use crate::sql;
use crate::utils;
use crate::utils::opt_text;

/// Fixed denominators for manual-entry rational storage. `commodities` carries
/// no SCU/fraction column, so per-commodity precision is out of scope for the
/// MVP: 4dp covers all fiat + sub-cent FX rounding, 8dp covers fractional
/// shares (GnuCash default).
pub const VALUE_DENOM: i64 = 10_000;
pub const QUANTITY_DENOM: i64 = 100_000_000;
/// Balance tolerance for the zero-sum check on display/raw amounts.
const BALANCE_EPS: f64 = 1e-6;

/// One split as entered in the manual editor. `amount` is the raw native
/// amount exactly as displayed in the detail view (debits/credits sum to
/// zero); `quantity` defaults to `amount` for cash accounts and carries share
/// counts for investments.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitInput {
    pub account_guid: String,
    pub amount: f64,
    pub quantity: Option<f64>,
    pub memo: Option<String>,
}

/// Manual transaction payload (create + update share this shape).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxInput {
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    pub splits: Vec<SplitInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitInfo {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub account_type: String,
    pub amount: f64,
    pub memo: Option<String>,
}

/// Split row enriched with fields only the transaction-detail view renders.
#[derive(Debug, Clone)]
struct SplitData {
    info: SplitInfo,
    /// Raw signed quantity (`quantity_num/denom`) — share counts for
    /// investment accounts; equals `amount` for plain currency accounts.
    quantity: f64,
    reconciled: bool,
    /// GnuCash reconcile state ('n' new, 'c' cleared, 'y' reconciled).
    reconcile_state: Option<String>,
    /// Description of the split's account (falls back to its name in the UI).
    account_description: Option<String>,
    /// Mnemonic of the split account's commodity. Drives FX conversion of the
    /// transaction total and quantity display on the detail page.
    commodity: Option<String>,
}

impl SplitData {
    fn to_detail(&self) -> SplitDetail {
        SplitDetail {
            guid: self.info.guid.clone(),
            account_guid: self.info.account_guid.clone(),
            account_name: self.info.account_name.clone(),
            account_type: self.info.account_type.clone(),
            amount: self.info.amount,
            quantity: self.quantity,
            commodity: self.commodity.clone(),
            memo: self.info.memo.clone(),
            reconciled: self.reconciled,
            reconcile_state: self.reconcile_state.clone(),
            account_description: self.account_description.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitDetail {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub account_type: String,
    pub amount: f64,
    pub quantity: f64,
    pub commodity: Option<String>,
    pub memo: Option<String>,
    pub reconciled: bool,
    pub reconcile_state: Option<String>,
    pub account_description: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRow {
    pub guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    pub splits: Vec<SplitInfo>,
    pub total_amount: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetail {
    pub guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    pub splits: Vec<SplitDetail>,
    pub total_amount: f64,
    /// Number of attached receipts/documents. Cheap COUNT so the editor can
    /// warn that deleting the transaction deletes them permanently.
    pub attachment_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetailResult {
    pub transaction: TransactionDetail,
    /// Neighbor following the ledger list ordering (post_date DESC, guid
    /// DESC): `prev` is the row displayed above (more recent), `next` the row
    /// below (older). Both respect the same visibility window as the list.
    pub prev_guid: Option<String>,
    pub next_guid: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    pub search: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsResult {
    pub transactions: Vec<TransactionRow>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

/// Per-day transaction count for the activity heatmap. `date` is `YYYY-MM-DD`.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapPoint {
    pub date: String,
    pub count: i64,
}

// ── Manual editor: validation + rational conversion ─────────────────────────

/// Validate `YYYY-MM-DD`, reject empty/malformed and future dates (hard-block
/// preserves the `VISIBLE_TX_FILTER` invariant: manually created rows are
/// always visible in balances immediately).
fn validate_post_date(post_date: &str) -> Result<String, AppError> {
    let trimmed = post_date.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Date is required".to_string()));
    }
    let normalized = utils::validate_iso_date(trimmed).map_err(AppError::Validation)?;
    // Local "today" so users east of UTC can post their current day (matches
    // the frontend's `todayStr` and the visibility window).
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if normalized > today {
        return Err(AppError::Validation("Future dates are not allowed".to_string()));
    }
    Ok(normalized)
}

fn to_value_rational(amount: f64) -> Result<(i64, i64), AppError> {
    if !amount.is_finite() {
        return Err(AppError::Validation("Split amounts must be finite numbers".to_string()));
    }
    if amount.abs() > 1_000_000_000.0 {
        return Err(AppError::Validation("Split amount is too large".to_string()));
    }
    let scaled = amount * VALUE_DENOM as f64;
    let rounded = scaled.round();
    // Reject instead of silently rounding: the caller's balance check validates
    // the doubles, so per-leg rounding could persist an unbalanced transaction.
    // The tolerance covers f64 error at the 1e9 upper bound.
    let tolerance = 1e-6 + scaled.abs() * f64::EPSILON * 4.0;
    if (scaled - rounded).abs() > tolerance {
        return Err(AppError::Validation(
            "Split amount cannot be represented (max 4 decimal places)".to_string(),
        ));
    }
    Ok((rounded as i64, VALUE_DENOM))
}

fn to_quantity_rational(qty: f64) -> Result<(i64, i64), AppError> {
    if !qty.is_finite() {
        return Err(AppError::Validation("Split quantities must be finite numbers".to_string()));
    }
    if qty.abs() > 1_000_000_000.0 {
        return Err(AppError::Validation("Split quantity is too large".to_string()));
    }
    let num = (qty * QUANTITY_DENOM as f64).round() as i64;
    Ok((num, QUANTITY_DENOM))
}

/// Shared validation for create/update: ≥2 splits, accounts exist and are not
/// placeholders, finite amounts, zero-sum balance. Returns the normalized
/// post-date string.
/// Full manual validation including the future-date block. `pub(crate)` for
/// `schedules.rs`: posting a due occurrence must pass the same gate as a
/// hand-entered transaction (due dates are always ≤ today by construction).
pub(crate) fn validate_tx_input(conn: &Connection, input: &TxInput) -> Result<String, AppError> {
    let post_date = validate_post_date(&input.post_date)?;
    validate_tx_body(conn, input)?;
    Ok(post_date)
}

/// Body-only half of [`validate_tx_input`] (no post-date check): schedule
/// templates reuse it because a template is future intent, not a posting.
/// `pub(crate)` for `schedules.rs`; the post path re-validates with the full
/// date-aware check at posting time.
pub(crate) fn validate_tx_body(conn: &Connection, input: &TxInput) -> Result<(), AppError> {
    if input.splits.len() < 2 {
        return Err(AppError::Validation("A transaction needs at least two splits".to_string()));
    }
    if input.splits.len() > 50 {
        return Err(AppError::Validation("Too many splits (max 50)".to_string()));
    }
    let mut sum = 0.0;
    for (i, s) in input.splits.iter().enumerate() {
        let n = i + 1;
        if s.account_guid.trim().is_empty() {
            return Err(AppError::Validation(format!("Split {n}: account is required")));
        }
        let account: Option<(i64, String)> = conn
            .query_row(
                "SELECT COALESCE(placeholder, 0), account_type FROM accounts WHERE guid = ?1",
                params![s.account_guid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((placeholder, account_type)) = account else {
            return Err(AppError::NotFound(format!("Split {n}: account not found")));
        };
        // The internal ROOT is not a real account: posting to it corrupts the
        // balance-sheet identity (the picker never offers it).
        if account_type == "ROOT" {
            return Err(AppError::Validation(format!(
                "Split {n}: the root account cannot hold splits"
            )));
        }
        if placeholder != 0 {
            return Err(AppError::Validation(format!("Split {n}: placeholder accounts cannot hold splits")));
        }
        if !s.amount.is_finite() {
            return Err(AppError::Validation(format!("Split {n}: amount must be a number")));
        }
        if let Some(q) = s.quantity {
            if !q.is_finite() {
                return Err(AppError::Validation(format!("Split {n}: quantity must be a number")));
            }
        }
        if let Some(ref memo) = s.memo {
            if memo.len() > 500 {
                return Err(AppError::Validation(format!("Split {n}: memo is too long (max 500)")));
            }
        }
        sum += s.amount;
    }
    if sum.abs() > BALANCE_EPS {
        return Err(AppError::Validation(format!(
            "Splits are out of balance by {sum:.2} — debits must equal credits"
        )));
    }
    if let Some(ref d) = input.description {
        if d.len() > 500 {
            return Err(AppError::Validation("Description is too long (max 500)".to_string()));
        }
    }
    if let Some(ref n) = input.num {
        if n.len() > 100 {
            return Err(AppError::Validation("Number is too long (max 100)".to_string()));
        }
    }
    if let Some(ref n) = input.notes {
        if n.len() > 2000 {
            return Err(AppError::Validation("Notes are too long (max 2000)".to_string()));
        }
    }

    Ok(())
}

fn tx_label(input: &TxInput) -> String {
    match opt_text(&input.description) {
        Some(d) => utils::truncate_chars(&d, 80),
        None => "Manual transaction".to_string(),
    }
}

fn tx_detail_json(post_date: &str) -> String {
    serde_json::json!({ "postDate": post_date }).to_string()
}

/// Shared transaction-list WHERE builder. `param_offset` reserves leading
/// placeholder indices already used by the surrounding query. Date window and
/// search come from `sql::push_tx_date_search`; date bounds are validated so a
/// malformed value can't silently NULL out the comparison.
fn build_filter_conditions(
    filters: &Option<Filters>,
    param_offset: usize,
) -> Result<sql::WhereBuilder, AppError> {
    let mut w = sql::WhereBuilder::new(param_offset);
    w.push_raw(utils::VISIBLE_TX_FILTER);

    if let Some(ref f) = filters {
        let from = utils::validate_optional_iso_date(f.date_from.as_deref())
            .map_err(AppError::Validation)?;
        let to = utils::validate_optional_iso_date(f.date_to.as_deref())
            .map_err(AppError::Validation)?;
        sql::push_tx_date_search(&mut w, from.as_deref(), to.as_deref(), f.search.as_deref());
    }

    Ok(w)
}

/// Per-transaction split rows plus the FX-converted transaction total.
type SplitMaps = (std::collections::HashMap<String, Vec<SplitData>>, std::collections::HashMap<String, f64>);

/// SQLite caps bound parameters per statement; chunk large GUID lists so a
/// full-width transaction page (or export) can never hit "too many SQL
/// variables". Each chunk holds whole transaction GUIDs, so per-transaction
/// ordering/totals are unaffected.
const MAX_IN_PARAMS: usize = sql::MAX_IN_PARAMS;

fn fetch_splits_for_txns(conn: &Connection, guids: &[String]) -> Result<SplitMaps, String> {
    if guids.is_empty() {
        return Ok((
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
        ));
    }

    let report_currency = fx::get_report_currency_mnemonic(conn)?.unwrap_or_default();
    let fx_history = fx::FxHistory::load(conn)?;

    let mut map: std::collections::HashMap<String, Vec<SplitData>> =
        std::collections::HashMap::new();
    let mut totals: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    for chunk in guids.chunks(MAX_IN_PARAMS) {
        let sql = format!(
            "SELECT s.guid, s.tx_guid, s.account_guid, a.name as account_name, a.account_type,
                    s.memo,
                    {} as amount,
                    {} as signed_amount,
                    t.post_date,
                    c.mnemonic as commodity,
                    {} as quantity,
                    s.reconciled,
                    a.description as account_description,
                    s.reconcile_state
             FROM splits s
             JOIN accounts a ON a.guid = s.account_guid
             JOIN commodities c ON c.guid = a.commodity_guid
             JOIN transactions t ON t.guid = s.tx_guid
             WHERE s.tx_guid IN ({})
             ORDER BY s.tx_guid,
                      ABS(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)) DESC,
                      signed_amount ASC,
                      s.guid ASC",
            utils::RAW_AMOUNT,
            utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type"),
            utils::QUANTITY_AMOUNT,
            sql::in_clause(chunk.len())
        );

        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let param_refs = sql::string_param_refs(chunk);

        let rows: Vec<(String, SplitData, f64, String)> = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(1)?, // tx_guid
                    SplitData {
                        info: SplitInfo {
                            guid: row.get(0)?,
                            account_guid: row.get(2)?,
                            account_name: row.get(3)?,
                            account_type: row.get(4)?,
                            amount: row.get(6)?,
                            memo: row.get(5)?,
                        },
                        commodity: row.get::<_, Option<String>>(9)?,
                        quantity: row.get(10)?,
                        reconciled: row.get::<_, i64>(11)? != 0,
                        account_description: row.get::<_, Option<String>>(12)?,
                        reconcile_state: row.get::<_, Option<String>>(13)?,
                    },
                    row.get::<_, f64>(7)?,    // signed_amount
                    row.get::<_, String>(8)?, // post_date
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        for (tx_guid, split, signed_amount, post_date) in rows {
            // Sorted by rational magnitude desc, then signed asc (outflow leg
            // first), so the first split per transaction is deterministic: its
            // signed value is the transaction total, converted to the report
            // currency at the transaction date's FX.
            totals.entry(tx_guid.clone()).or_insert_with(|| {
                let fx_at = fx_history.map_for_date(&post_date);
                let currency = split.commodity.as_deref().unwrap_or(&report_currency);
                fx::convert(signed_amount, currency, &report_currency, &fx_at)
            });
            map.entry(tx_guid).or_default().push(split);
        }
    }
    Ok((map, totals))
}

#[derive(Debug)]
struct TxRow {
    guid: String,
    post_date: String,
    description: Option<String>,
    num: Option<String>,
    notes: Option<String>,
}

fn map_tx_rows(conn: &Connection, rows: Vec<TxRow>) -> Result<Vec<TransactionRow>, String> {
    let guids: Vec<String> = rows.iter().map(|r| r.guid.clone()).collect();
    let (split_map, total_map) = fetch_splits_for_txns(conn, &guids)?;

    Ok(rows
        .into_iter()
        .map(|r| TransactionRow {
            splits: split_map
                .get(&r.guid)
                .map(|v| v.iter().map(|d| d.info.clone()).collect())
                .unwrap_or_default(),
            total_amount: total_map.get(&r.guid).copied().unwrap_or(0.0),
            guid: r.guid,
            post_date: r.post_date,
            description: r.description,
            num: r.num,
            notes: r.notes,
        })
        .collect())
}

fn fetch_transactions_inner(
    conn: &Connection,
    page: i64,
    page_size: i64,
    filters: Option<Filters>,
    sort_by: Option<&str>,
    sort_dir: Option<&str>,
) -> Result<TransactionsResult, String> {
    let w = build_filter_conditions(&filters, 0)?;
    let where_clause = w.where_clause();

    let count_sql = format!("SELECT COUNT(*) as cnt FROM transactions t{where_clause}");
    let mut count_stmt = conn.prepare(&count_sql).map_err(|e| e.to_string())?;
    let param_refs = sql::param_refs(&w.params);

    let total: i64 = count_stmt
        .query_row(param_refs.as_slice(), |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let (page_size, total_pages, safe_page, offset) = utils::paginate(total, page, page_size);

    // Amount sorting needs the FX-converted totals, which are computed in Rust,
    // so fetch all matching rows, sort, then paginate.
    if sort_by == Some("amount") {
        let query_sql = format!(
            "SELECT guid, post_date, description, num, notes FROM transactions t{where_clause} ORDER BY post_date DESC"
        );
        let mut query_stmt = conn.prepare(&query_sql).map_err(|e| e.to_string())?;
        let all_refs = sql::param_refs(&w.params);
        let rows: Vec<TxRow> = query_stmt
            .query_map(all_refs.as_slice(), |row| {
                Ok(TxRow {
                    guid: row.get(0)?,
                    post_date: row.get(1)?,
                    description: row.get(2)?,
                    num: row.get(3)?,
                    notes: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let mut all_txns = map_tx_rows(conn, rows)?;
        let ascending = sort_dir == Some("asc");
        all_txns.sort_by(|a, b| {
            let cmp = a
                .total_amount
                .total_cmp(&b.total_amount)
                .then_with(|| a.guid.cmp(&b.guid));
            if ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });

        let transactions = all_txns
            .into_iter()
            .skip(offset as usize)
            .take(page_size as usize)
            .collect();

        return Ok(TransactionsResult {
            transactions,
            total,
            page: safe_page,
            page_size,
            total_pages,
        });
    }

    let order_clause = match (sort_by, sort_dir) {
        (Some("description"), Some("asc")) => {
            "ORDER BY COALESCE(LOWER(description), '') ASC, guid ASC"
        }
        (Some("description"), _) => "ORDER BY COALESCE(LOWER(description), '') DESC, guid DESC",
        (Some("date"), Some("asc")) => "ORDER BY post_date ASC, guid ASC",
        _ => "ORDER BY post_date DESC, guid DESC",
    };

    let query_sql = format!(
        "SELECT guid, post_date, description, num, notes FROM transactions t{where_clause} {order_clause} LIMIT ? OFFSET ?"
    );

    let mut query_stmt = conn.prepare(&query_sql).map_err(|e| e.to_string())?;
    let all_params: Vec<Box<dyn rusqlite::types::ToSql>> = w
        .params
        .into_iter()
        .chain(vec![
            Box::new(page_size) as Box<dyn rusqlite::types::ToSql>,
            Box::new(offset) as Box<dyn rusqlite::types::ToSql>,
        ])
        .collect();
    let all_refs = sql::param_refs(&all_params);

    let rows: Vec<TxRow> = query_stmt
        .query_map(all_refs.as_slice(), |row| {
            Ok(TxRow {
                guid: row.get(0)?,
                post_date: row.get(1)?,
                description: row.get(2)?,
                num: row.get(3)?,
                notes: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let transactions = map_tx_rows(conn, rows)?;

    Ok(TransactionsResult {
        transactions,
        total,
        page: safe_page,
        page_size,
        total_pages,
    })
}

/// Per-day transaction counts for the activity heatmap, honoring the same
/// filters and visibility window as the ledger list.
fn fetch_transaction_heatmap_inner(
    conn: &Connection,
    filters: Option<Filters>,
) -> Result<Vec<HeatmapPoint>, String> {
    let w = build_filter_conditions(&filters, 0)?;
    let where_clause = w.where_clause();

    let sql = format!(
        "SELECT substr(t.post_date, 1, 10) AS day, COUNT(*) AS cnt
         FROM transactions t{where_clause}
         GROUP BY day
         ORDER BY day"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let param_refs = sql::param_refs(&w.params);

    let points = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(HeatmapPoint {
                date: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(points)
}

#[tauri::command]
pub async fn fetch_transactions(
    state: State<'_, AppState>,
    page: i64,
    page_size: i64,
    filters: Option<Filters>,
    sort_by: Option<String>,
    sort_dir: Option<String>,
) -> Result<TransactionsResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_transactions_inner(
            conn,
            page,
            page_size,
            filters,
            sort_by.as_deref(),
            sort_dir.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn fetch_transaction_heatmap(
    state: State<'_, AppState>,
    filters: Option<Filters>,
) -> Result<Vec<HeatmapPoint>, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_transaction_heatmap_inner(conn, filters)
    })
    .await
}

/// Nearest neighbor of a transaction in the ledger list ordering
/// (`post_date DESC, guid DESC`), restricted to the same visibility window as
/// the list. `above = true` finds the row displayed above (more recent),
/// `false` the row below (older).
fn neighbor_guid(
    conn: &Connection,
    post_date: &str,
    guid: &str,
    above: bool,
) -> Result<Option<String>, String> {
    let (op, order) = if above {
        (">", "ORDER BY t.post_date ASC, t.guid ASC")
    } else {
        ("<", "ORDER BY t.post_date DESC, t.guid DESC")
    };
    let sql = format!(
        // Alias the table: VISIBLE_TX_FILTER qualifies its column as `t`.
        "SELECT t.guid FROM transactions t
         WHERE {}
           AND (t.post_date {op} ?1 OR (t.post_date = ?1 AND t.guid {op} ?2))
         {order}
         LIMIT 1",
        utils::VISIBLE_TX_FILTER
    );
    conn.query_row(&sql, rusqlite::params![post_date, guid], |row| row.get(0))
        .optional()
        .map_err(|e| e.to_string())
}

fn fetch_transaction_inner(
    conn: &Connection,
    guid: &str,
) -> Result<Option<TransactionDetailResult>, String> {
    let base = conn
        .query_row(
            &format!(
                "SELECT t.guid, t.post_date, t.description, t.num, t.notes
                 FROM transactions t
                 WHERE t.guid = ?1 AND {}",
                utils::VISIBLE_TX_FILTER
            ),
            [guid],
            |row| {
                Ok(TxRow {
                    guid: row.get(0)?,
                    post_date: row.get(1)?,
                    description: row.get(2)?,
                    num: row.get(3)?,
                    notes: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let Some(base) = base else {
        return Ok(None);
    };

    let guids = [base.guid.clone()];
    let (split_map, total_map) = fetch_splits_for_txns(conn, &guids)?;
    let attachment_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM attachments WHERE tx_guid = ?1",
            [&base.guid],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let transaction = TransactionDetail {
        guid: base.guid.clone(),
        post_date: base.post_date.clone(),
        description: base.description,
        num: base.num,
        notes: base.notes,
        splits: split_map
            .get(&base.guid)
            .map(|v| v.iter().map(|d| d.to_detail()).collect())
            .unwrap_or_default(),
        total_amount: total_map.get(&base.guid).copied().unwrap_or(0.0),
        attachment_count,
    };

    let prev_guid = neighbor_guid(conn, &base.post_date, &base.guid, true)?;
    let next_guid = neighbor_guid(conn, &base.post_date, &base.guid, false)?;

    Ok(Some(TransactionDetailResult {
        transaction,
        prev_guid,
        next_guid,
    }))
}

/// `None` when no transaction with that GUID exists (the frontend shows a
/// not-found state instead of an error card).
#[tauri::command]
pub async fn fetch_transaction(
    state: State<'_, AppState>,
    guid: String,
) -> Result<Option<TransactionDetailResult>, String> {
    run_blocking(state.db.clone(), move |conn| fetch_transaction_inner(conn, &guid)).await
}

// ── Manual editor: create / update / delete ─────────────────────────────────

/// `pub(crate)` for `schedules.rs` (posting a due occurrence reuses the exact
/// manual-entry split encoding: fixed denominators, `'n'` reconcile state).
pub(crate) fn insert_split_rows(
    conn: &Connection,
    tx_guid: &str,
    splits: &[SplitInput],
) -> Result<Vec<String>, AppError> {
    // Convert every leg first and require the stored (scaled integer) values
    // to sum to zero: rounding each leg independently could otherwise persist
    // an unbalanced transaction that the double-based check accepted.
    let converted: Vec<(i64, i64, i64, i64)> = splits
        .iter()
        .map(|s| {
            let (value_num, value_denom) = to_value_rational(s.amount)?;
            let qty = s.quantity.unwrap_or(s.amount);
            let (quantity_num, quantity_denom) = to_quantity_rational(qty)?;
            Ok((value_num, value_denom, quantity_num, quantity_denom))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    if converted.len() > 1 {
        let sum: i64 = converted.iter().map(|(num, _, _, _)| *num).sum();
        if sum != 0 {
            return Err(AppError::Validation(
                "Splits are out of balance after rounding — adjust the amounts".to_string(),
            ));
        }
    }

    let mut guids = Vec::with_capacity(splits.len());
    for (s, (value_num, value_denom, quantity_num, quantity_denom)) in
        splits.iter().zip(converted)
    {
        let split_guid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO splits
              (guid, tx_guid, account_guid, value_num, value_denom,
               quantity_num, quantity_denom, reconcile_state, reconciled, memo)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'n', 0, ?8)",
            params![
                split_guid,
                tx_guid,
                s.account_guid,
                value_num,
                value_denom,
                quantity_num,
                quantity_denom,
                opt_text(&s.memo),
            ],
        )
        ?;
        guids.push(split_guid);
    }
    Ok(guids)
}

fn existing_split_guids(conn: &Connection, tx_guid: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn
        .prepare("SELECT guid FROM splits WHERE tx_guid = ?1")
        ?;
    let rows = stmt
        .query_map(params![tx_guid], |row| row.get(0))
        ?;
    let guids = rows
        .collect::<Result<Vec<String>, _>>()
        ?;
    Ok(guids)
}

/// Distinct account GUIDs currently touched by a transaction's splits, for
/// the history entry's `accounts_affected` metadata.
fn affected_account_guids(conn: &Connection, tx_guid: &str) -> Result<HashSet<String>, AppError> {
    let mut stmt = conn.prepare("SELECT DISTINCT account_guid FROM splits WHERE tx_guid = ?1")?;
    let rows = stmt.query_map(params![tx_guid], |row| row.get(0))?;
    let guids = rows.collect::<Result<HashSet<String>, _>>()?;
    Ok(guids)
}

/// Manual edits re-create splits, so they must not touch cleared/reconciled
/// rows — those transitions belong to the reconcile workflow.
fn reject_when_reconciled(conn: &Connection, tx_guid: &str, action: &str) -> Result<(), AppError> {
    let locked: i64 = conn.query_row(
        "SELECT COUNT(*) FROM splits
         WHERE tx_guid = ?1 AND reconcile_state IN ('c', 'y')",
        params![tx_guid],
        |row| row.get(0),
    )?;
    if locked > 0 {
        return Err(AppError::Validation(format!(
            "Cannot {action} — this transaction has cleared or reconciled splits. Adjust them on the reconcile page first"
        )));
    }
    Ok(())
}

fn create_transaction_inner(conn: &Connection, input: TxInput) -> Result<String, AppError> {

    let post_date = validate_tx_input(conn, &input)?;
    let tx_guid = uuid::Uuid::new_v4().to_string();
    let label = tx_label(&input);
    let detail = tx_detail_json(&post_date);
    let accounts_affected = input
        .splits
        .iter()
        .map(|s| s.account_guid.clone())
        .collect::<HashSet<_>>()
        .len() as i64;

    with_tx_err(conn, || {
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                tx_guid,
                post_date,
                opt_text(&input.description),
                opt_text(&input.num),
                opt_text(&input.notes),
            ],
        )
        ?;
        let split_guids = insert_split_rows(conn, &tx_guid, &input.splits)?;
        let mut snapshots = vec![history::Snapshot {
            table_name: "transactions",
            row_guid: tx_guid.clone(),
            before_json: None,
        }];
        for g in &split_guids {
            snapshots.push(history::Snapshot {
                table_name: "splits",
                row_guid: g.clone(),
                before_json: None,
            });
        }
        history::insert_change_entry(
            conn,
            "tx_create",
            &label,
            "manual",
            1,
            accounts_affected,
            Some(&detail),
            &snapshots,
        )?;
        Ok(tx_guid.clone())
    })
}

fn update_transaction_inner(
    conn: &Connection,
    guid: &str,
    input: TxInput,
) -> Result<(), AppError> {
    let post_date = validate_tx_input(conn, &input)?;
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM transactions WHERE guid = ?1",
            params![guid],
            |_| Ok(()),
        )
        .optional()
        ?
        .is_some();
    if !exists {
        return Err(AppError::NotFound("Transaction not found — it may have been removed".to_string()));
    }
    // Manual edits delete + re-insert splits as `'n'`, which would silently
    // wipe reconciliation state. Block edits while any split is cleared or
    // reconciled — the reconcile page owns those transitions.
    reject_when_reconciled(conn, guid, "edit")?;
    let old_splits = existing_split_guids(conn, guid)?;
    let mut affected_accounts = affected_account_guids(conn, guid)?;
    affected_accounts.extend(input.splits.iter().map(|s| s.account_guid.clone()));
    let accounts_affected = affected_accounts.len() as i64;
    let label = tx_label(&input);
    let detail = tx_detail_json(&post_date);

    with_tx_err(conn, || {
        let mut snapshots = vec![history::Snapshot {
            table_name: "transactions",
            row_guid: guid.to_string(),
            before_json: history::snapshot_row(conn, "transactions", guid)?,
        }];
        for g in &old_splits {
            snapshots.push(history::Snapshot {
                table_name: "splits",
                row_guid: g.clone(),
                before_json: history::snapshot_row(conn, "splits", g)?,
            });
        }
        conn.execute(
            "UPDATE transactions SET post_date = ?1, description = ?2, num = ?3, notes = ?4
             WHERE guid = ?5",
            params![
                post_date,
                opt_text(&input.description),
                opt_text(&input.num),
                opt_text(&input.notes),
                guid,
            ],
        )
        ?;
        conn.execute("DELETE FROM splits WHERE tx_guid = ?1", params![guid])
            ?;
        let new_guids = insert_split_rows(conn, guid, &input.splits)?;
        for g in &new_guids {
            snapshots.push(history::Snapshot {
                table_name: "splits",
                row_guid: g.clone(),
                before_json: None,
            });
        }
        history::insert_change_entry(
            conn,
            "tx_update",
            &label,
            "manual",
            1,
            accounts_affected,
            Some(&detail),
            &snapshots,
        )?;
        Ok(())
    })
}

fn delete_transaction_inner(conn: &Connection, guid: &str) -> Result<(), AppError> {
    let before: Option<String> = conn
        .query_row(
            "SELECT description FROM transactions WHERE guid = ?1",
            params![guid],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    let exists = conn
        .query_row(
            "SELECT 1 FROM transactions WHERE guid = ?1",
            params![guid],
            |_| Ok(()),
        )
        .optional()
        ?
        .is_some();
    if !exists {
        return Err(AppError::NotFound("Transaction not found — it may have been removed".to_string()));
    }
    reject_when_reconciled(conn, guid, "delete")?;
    let old_splits = existing_split_guids(conn, guid)?;
    let accounts_affected = affected_account_guids(conn, guid)?.len() as i64;
    let label = before
        .as_deref()
        .map(|d| {
            let t = d.trim();
            if t.is_empty() {
                "Delete manual transaction".to_string()
            } else {
                format!("Delete {}", utils::truncate_chars(t, 80))
            }
        })
        .unwrap_or_else(|| "Delete manual transaction".to_string());

    with_tx_err(conn, || {
        let mut snapshots = vec![history::Snapshot {
            table_name: "transactions",
            row_guid: guid.to_string(),
            before_json: history::snapshot_row(conn, "transactions", guid)?,
        }];
        for g in &old_splits {
            snapshots.push(history::Snapshot {
                table_name: "splits",
                row_guid: g.clone(),
                before_json: history::snapshot_row(conn, "splits", g)?,
            });
        }
        conn.execute("DELETE FROM splits WHERE tx_guid = ?1", params![guid])
            ?;
        conn.execute("DELETE FROM transactions WHERE guid = ?1", params![guid])
            ?;
        history::insert_change_entry(
            conn,
            "tx_delete",
            &label,
            "manual",
            1,
            accounts_affected,
            None,
            &snapshots,
        )?;
        Ok(())
    })
}

#[tauri::command]
pub async fn create_transaction(
    state: State<'_, AppState>,
    input: TxInput,
) -> Result<String, String> {
    run_blocking(state.db.clone(), move |conn| {
        create_transaction_inner(conn, input).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn update_transaction(
    state: State<'_, AppState>,
    guid: String,
    input: TxInput,
) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        update_transaction_inner(conn, &guid, input).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn delete_transaction(state: State<'_, AppState>, guid: String) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        delete_transaction_inner(conn, &guid).map_err(String::from)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use rusqlite::params;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        for (tag, date, value) in [("t1", "2024-01-10", 5_000i64), ("t2", "2024-02-05", -1_500)] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![tag, date],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, 'checking', ?3, 100, ?3, 100)",
                params![format!("s-{tag}"), tag, value],
            )
            .unwrap();
        }
        conn
    }

    /// Exercises neighbor_guid's visibility-window query, which must alias
    /// `transactions` as `t` to match the shared filter fragment.
    #[test]
    fn detail_returns_neighbors_in_ledger_order() {
        let conn = setup();

        // Ledger order is newest-first: `prev` is the more recent row.
        let oldest = fetch_transaction_inner(&conn, "t1")
            .expect("query runs")
            .expect("tx exists");
        assert_eq!(oldest.prev_guid.as_deref(), Some("t2"));
        assert_eq!(oldest.next_guid, None);

        let newest = fetch_transaction_inner(&conn, "t2")
            .expect("query runs")
            .expect("tx exists");
        assert_eq!(newest.prev_guid, None);
        assert_eq!(newest.next_guid.as_deref(), Some("t1"));
    }

    /// The heatmap must reuse the shared visibility-window filter: future-dated
    /// transactions stay out of the per-day counts.
    #[test]
    fn heatmap_counts_visible_transactions_per_day() {
        let conn = setup();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t3', '2099-01-01')",
            [],
        )
        .unwrap();

        let points = fetch_transaction_heatmap_inner(&conn, None).expect("query runs");
        assert_eq!(
            points,
            vec![
                HeatmapPoint {
                    date: "2024-01-10".into(),
                    count: 1
                },
                HeatmapPoint {
                    date: "2024-02-05".into(),
                    count: 1
                }
            ]
        );
    }

    fn setup_two_accounts() -> Connection {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('groceries', 'Groceries', 'EXPENSE', 'root', 0, 'c-eur'),
                    ('placeholder', 'Parent', 'EXPENSE', 'root', 1, 'c-eur')",
            [],
        )
        .unwrap();
        conn
    }

    fn balanced_input() -> TxInput {
        TxInput {
            post_date: "2024-03-01".to_string(),
            description: Some("Manual groceries".to_string()),
            num: None,
            notes: Some("test".to_string()),
            splits: vec![
                SplitInput {
                    account_guid: "checking".to_string(),
                    amount: -42.10,
                    quantity: None,
                    memo: None,
                },
                SplitInput {
                    account_guid: "groceries".to_string(),
                    amount: 42.10,
                    quantity: None,
                    memo: Some("food".to_string()),
                },
            ],
        }
    }

    #[test]
    fn create_balanced_transaction_persists_and_logs_history() {
        let conn = setup_two_accounts();
        let guid = create_transaction_inner(&conn, balanced_input()).expect("create runs");
        let detail = fetch_transaction_inner(&conn, &guid)
            .expect("query runs")
            .expect("tx exists");
        assert_eq!(detail.transaction.splits.len(), 2);
        assert_eq!(detail.transaction.post_date, "2024-03-01");
        // Raw amounts round-trip with fixed-denominator precision.
        let mut amounts: Vec<f64> = detail
            .transaction
            .splits
            .iter()
            .map(|s| s.amount)
            .collect();
        amounts.sort_by(|a, b| a.total_cmp(b));
        assert!((amounts[0] + 42.10).abs() < 1e-6);
        assert!((amounts[1] - 42.10).abs() < 1e-6);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM change_log WHERE kind = 'tx_create'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn tx_label_truncates_multibyte_descriptions_safely() {
        // Regression: `&s[..80]` used to panic (process abort in release) when
        // byte 80 landed inside a multibyte character.
        let mut input = balanced_input();
        input.description = Some("漢".repeat(27));
        let label = tx_label(&input);
        assert_eq!(label, format!("{}…", "漢".repeat(26)));
        // Descriptions at or under the cap pass through untouched.
        let mut short = balanced_input();
        short.description = Some("é".repeat(40));
        assert_eq!(tx_label(&short), "é".repeat(40));
    }

    #[test]
    fn validate_post_date_accepts_local_today() {
        // Canary: backend "today" must match the frontend's local `todayStr`,
        // not UTC, or users east of UTC cannot enter today's transactions.
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert_eq!(validate_post_date(&today).unwrap(), today);
    }

    /// Regression: the total used an unstable `ABS(value_num)` tie-break, so a
    /// balanced purchase could total +42.10 (income-colored) depending on the
    /// order its splits were inserted.
    #[test]
    fn transaction_total_is_deterministic_for_balanced_splits() {
        let conn = setup_two_accounts();

        let guid = create_transaction_inner(&conn, balanced_input()).unwrap();
        let detail = fetch_transaction_inner(&conn, &guid).unwrap().unwrap();
        assert!((detail.transaction.total_amount + 42.10).abs() < 1e-6);

        // Same transaction, splits inserted in reverse order: same total sign.
        let mut reversed = balanced_input();
        reversed.splits.reverse();
        let guid2 = create_transaction_inner(&conn, reversed).unwrap();
        let detail2 = fetch_transaction_inner(&conn, &guid2).unwrap().unwrap();
        assert!((detail2.transaction.total_amount + 42.10).abs() < 1e-6);
    }

    #[test]
    fn value_rational_rejects_sub_minor_unit_amounts() {
        // Regression: the old drift check could never fire, so `0.12345678`
        // was silently stored as `0.1235`.
        assert!(to_value_rational(0.12345678).is_err());
        assert_eq!(to_value_rational(0.1234).unwrap(), (1234, VALUE_DENOM));
        assert_eq!(to_value_rational(-42.1).unwrap(), (-421_000, VALUE_DENOM));
        // Near the upper bound, f64 error must not cause false rejections.
        assert!(to_value_rational(999_999_999.9999).is_ok());
    }

    /// Regression: malformed filter dates used to make `date(?, '+1 day')`
    /// NULL and silently return zero rows instead of an error.
    #[test]
    fn list_rejects_malformed_date_filters() {
        let conn = setup();
        let err = fetch_transactions_inner(
            &conn,
            1,
            50,
            Some(Filters {
                search: None,
                date_from: Some("2024-1-5".to_string()),
                date_to: None,
            }),
            None,
            None,
        )
        .unwrap_err();
        assert!(err.contains("Invalid date"), "unexpected: {err}");

        let err = fetch_transaction_heatmap_inner(
            &conn,
            Some(Filters {
                search: None,
                date_from: None,
                date_to: Some("01/02/2024".to_string()),
            }),
        )
        .unwrap_err();
        assert!(err.contains("Invalid date"), "unexpected: {err}");
    }

    #[test]
    fn create_rejects_splits_on_the_root_account() {
        let conn = setup_two_accounts();
        let mut input = balanced_input();
        input.splits[0].account_guid = "root".to_string();
        let err = create_transaction_inner(&conn, input).unwrap_err();
        assert!(err.to_string().contains("root account"), "unexpected: {err}");
    }

    #[test]
    fn create_rejects_unbalanced_placeholder_future_and_single_split() {
        let conn = setup_two_accounts();
        // Unbalanced.
        let mut bad = balanced_input();
        bad.splits[1].amount = 42.11;
        assert!(create_transaction_inner(&conn, bad).is_err());
        // Placeholder account.
        let mut bad = balanced_input();
        bad.splits[1].account_guid = "placeholder".to_string();
        assert!(create_transaction_inner(&conn, bad).unwrap_err().to_string().contains("placeholder"));
        // Future date hard-block.
        let mut bad = balanced_input();
        bad.post_date = "2099-01-01".to_string();
        assert!(create_transaction_inner(&conn, bad).unwrap_err().to_string().contains("Future"));
        // Single split.
        let mut bad = balanced_input();
        bad.splits.truncate(1);
        assert!(create_transaction_inner(&conn, bad).unwrap_err().to_string().contains("two splits"));
        // Unknown account.
        let mut bad = balanced_input();
        bad.splits[0].account_guid = "nope".to_string();
        assert!(create_transaction_inner(&conn, bad).unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn update_replaces_splits_and_delete_removes_with_history() {
        let conn = setup_two_accounts();
        let guid = create_transaction_inner(&conn, balanced_input()).expect("create runs");
        let mut edited = balanced_input();
        edited.description = Some("Edited".to_string());
        edited.splits[0].amount = -10.0;
        edited.splits[1].amount = 10.0;
        update_transaction_inner(&conn, &guid, edited).expect("update runs");
        let detail = fetch_transaction_inner(&conn, &guid)
            .expect("query runs")
            .expect("tx exists");
        assert_eq!(detail.transaction.description.as_deref(), Some("Edited"));
        assert_eq!(detail.transaction.splits.len(), 2);

        delete_transaction_inner(&conn, &guid).expect("delete runs");
        assert!(fetch_transaction_inner(&conn, &guid).expect("query runs").is_none());
        let kinds: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT kind FROM change_log ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(kinds, vec!["tx_create", "tx_update", "tx_delete"]);
        // Every history entry reports the real number of affected accounts.
        let affected: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT accounts_affected FROM change_log ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(affected, vec![2, 2, 2]);
    }

    #[test]
    fn list_filters_sorts_and_paginates_visible_transactions() {
        let conn = setup_two_accounts();
        conn.execute(
            "UPDATE transactions SET description = 'Coffee' WHERE guid = 't1'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE transactions SET description = 'Rent' WHERE guid = 't2'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description) VALUES ('t3', '2099-01-01', 'Future')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-t3', 't3', 'checking', 100, 100, 100, 100)",
            [],
        )
        .unwrap();

        // Future-dated rows stay hidden from the list.
        let all = fetch_transactions_inner(&conn, 1, 50, None, None, None).unwrap();
        assert_eq!(all.total, 2);
        assert_eq!(
            all.transactions
                .iter()
                .map(|t| t.guid.as_str())
                .collect::<Vec<_>>(),
            vec!["t2", "t1"]
        );

        // Search and date range combine with AND.
        let filtered = fetch_transactions_inner(
            &conn,
            1,
            50,
            Some(Filters {
                search: Some("coffee".into()),
                date_from: Some("2024-02-01".into()),
                date_to: None,
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(filtered.total, 0);

        let search = fetch_transactions_inner(
            &conn,
            1,
            50,
            Some(Filters {
                search: Some("coffee".into()),
                date_from: None,
                date_to: None,
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(search.total, 1);
        assert_eq!(search.transactions[0].guid, "t1");

        // Page 2 of size 1 holds the older transaction.
        let page2 = fetch_transactions_inner(&conn, 2, 1, None, None, None).unwrap();
        assert_eq!(page2.page, 2);
        assert_eq!(page2.total_pages, 2);
        assert_eq!(page2.transactions.len(), 1);
        assert_eq!(page2.transactions[0].guid, "t1");

        // Amount sorting uses the FX-converted transaction total.
        let asc =
            fetch_transactions_inner(&conn, 1, 50, None, Some("amount"), Some("asc")).unwrap();
        assert_eq!(asc.transactions[0].guid, "t2");
        let desc =
            fetch_transactions_inner(&conn, 1, 50, None, Some("amount"), Some("desc")).unwrap();
        assert_eq!(desc.transactions[0].guid, "t1");
    }

    #[test]
    fn reconciled_or_cleared_splits_block_edit_and_delete() {
        let conn = setup_two_accounts();
        let guid = create_transaction_inner(&conn, balanced_input()).unwrap();
        let set_state = |state: &str| {
            conn.execute("UPDATE splits SET reconcile_state = ?1", params![state])
                .unwrap();
        };

        for state in ["c", "y"] {
            set_state(state);
            let err = update_transaction_inner(&conn, &guid, balanced_input()).unwrap_err();
            assert!(err.to_string().contains("reconcile"), "unexpected: {err}");
            let err = delete_transaction_inner(&conn, &guid).unwrap_err();
            assert!(err.to_string().contains("reconcile"), "unexpected: {err}");
        }

        set_state("n");
        update_transaction_inner(&conn, &guid, balanced_input()).expect("unlocked edit runs");
        delete_transaction_inner(&conn, &guid).expect("unlocked delete runs");
    }
}
