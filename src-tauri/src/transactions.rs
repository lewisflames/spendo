use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::fx;
use crate::sql;
use crate::tags::{self, TagRef};
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
    /// Set by the edit form once the user confirms the unreconcile warning.
    /// Skips the cleared/reconciled guard: the update re-creates every split
    /// as `'n'`, which is the unreconcile. Create ignores the flag.
    #[serde(default)]
    pub unreconcile: bool,
    /// Tag names. `None` leaves an edited transaction's tags as they are
    /// (callers that don't know about tags), `Some(vec![])` clears them.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitInfo {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
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
    /// Optional code of the split's account, shown as a chip in the detail view.
    account_code: Option<String>,
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
            amount: self.info.amount,
            quantity: self.quantity,
            commodity: self.commodity.clone(),
            memo: self.info.memo.clone(),
            reconciled: self.reconciled,
            reconcile_state: self.reconcile_state.clone(),
            account_description: self.account_description.clone(),
            account_code: self.account_code.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitDetail {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub amount: f64,
    pub quantity: f64,
    pub commodity: Option<String>,
    pub memo: Option<String>,
    pub reconciled: bool,
    pub reconcile_state: Option<String>,
    pub account_description: Option<String>,
    pub account_code: Option<String>,
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
    pub tags: Vec<TagRef>,
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
    pub tags: Vec<TagRef>,
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
    /// Only transactions carrying this tag.
    pub tag_guid: Option<String>,
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
        return Err(AppError::Validation(
            "Pick a date for this transaction".to_string(),
        ));
    }
    let normalized = utils::validate_iso_date(trimmed)?;
    // Local "today" so users east of UTC can post their current day (matches
    // the frontend's `todayStr` and the visibility window).
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if normalized > today {
        return Err(AppError::Validation(
            "Future dates stay out of balances. Use today or earlier".to_string(),
        ));
    }
    Ok(normalized)
}

fn to_value_rational(amount: f64) -> Result<(i64, i64), AppError> {
    if !amount.is_finite() {
        return Err(AppError::Validation(
            "Split amounts need to be real numbers".to_string(),
        ));
    }
    if amount.abs() > 1_000_000_000.0 {
        return Err(AppError::Validation(
            "That split amount is huge. Keep it under 1,000,000,000".to_string(),
        ));
    }
    let scaled = amount * VALUE_DENOM as f64;
    let rounded = scaled.round();
    // Reject instead of silently rounding: the caller's balance check validates
    // the doubles, so per-leg rounding could persist an unbalanced transaction.
    // The tolerance covers f64 error at the 1e9 upper bound.
    let tolerance = 1e-6 + scaled.abs() * f64::EPSILON * 4.0;
    if (scaled - rounded).abs() > tolerance {
        return Err(AppError::Validation(
            "That amount has too many decimals (4 max)".to_string(),
        ));
    }
    Ok((rounded as i64, VALUE_DENOM))
}

fn to_quantity_rational(qty: f64) -> Result<(i64, i64), AppError> {
    if !qty.is_finite() {
        return Err(AppError::Validation(
            "Share counts need to be real numbers".to_string(),
        ));
    }
    if qty.abs() > 1_000_000_000.0 {
        return Err(AppError::Validation(
            "That share count is huge. Keep it under 1,000,000,000".to_string(),
        ));
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

/// [`validate_tx_input`] with the account lookup injected, so bulk callers
/// (e.g. CSV parsing) can memoize account facts across thousands of rows.
pub(crate) fn validate_tx_input_with(
    input: &TxInput,
    lookup: &dyn Fn(&str) -> Result<Option<SplitAccountInfo>, AppError>,
) -> Result<String, AppError> {
    let post_date = validate_post_date(&input.post_date)?;
    validate_tx_body_with(input, lookup)?;
    Ok(post_date)
}

/// The account facts split validation needs. Resolved through a lookup so the
/// GnuCash schedule preview can validate template splits against a set of
/// accounts that will exist only after the merge (source ∪ destination).
#[derive(Debug, Clone)]
pub(crate) struct SplitAccountInfo {
    pub placeholder: i64,
    pub account_type: String,
}

/// DB-backed account lookup for [`validate_tx_body_with`].
pub(crate) fn account_info_lookup(
    conn: &Connection,
    guid: &str,
) -> Result<Option<SplitAccountInfo>, AppError> {
    conn.query_row(
        "SELECT COALESCE(placeholder, 0), account_type FROM accounts WHERE guid = ?1",
        params![guid],
        |row| {
            Ok(SplitAccountInfo {
                placeholder: row.get(0)?,
                account_type: row.get(1)?,
            })
        },
    )
    .optional()
    .map_err(AppError::from)
}

/// Body-only half of [`validate_tx_input`] (no post-date check): schedule
/// templates reuse it because a template is future intent, not a posting.
/// `pub(crate)` for `schedules.rs`; the post path re-validates with the full
/// date-aware check at posting time.
pub(crate) fn validate_tx_body(conn: &Connection, input: &TxInput) -> Result<(), AppError> {
    validate_tx_body_with(input, &|guid| account_info_lookup(conn, guid))
}

/// [`validate_tx_body`] with the account lookup injected: identical rules,
/// callable without a live destination row (import preview).
pub(crate) fn validate_tx_body_with(
    input: &TxInput,
    lookup: &dyn Fn(&str) -> Result<Option<SplitAccountInfo>, AppError>,
) -> Result<(), AppError> {
    if input.splits.len() < 2 {
        return Err(AppError::Validation(
            "A transaction needs at least two splits so both sides balance".to_string(),
        ));
    }
    if input.splits.len() > 50 {
        return Err(AppError::Validation(
            "Keep it to 50 splits or fewer".to_string(),
        ));
    }
    let mut sum = 0.0;
    for (i, s) in input.splits.iter().enumerate() {
        let n = i + 1;
        if s.account_guid.trim().is_empty() {
            return Err(AppError::Validation(format!(
                "Split {n}: choose an account"
            )));
        }
        let account = lookup(s.account_guid.trim())?;
        let Some(SplitAccountInfo {
            placeholder,
            account_type,
        }) = account
        else {
            return Err(AppError::NotFound(format!(
                "Split {n}: account not found. It may have been removed"
            )));
        };
        // The internal ROOT is not a real account: posting to it corrupts the
        // balance-sheet identity (the picker never offers it).
        if account_type == "ROOT" {
            return Err(AppError::Validation(format!(
                "Split {n}: the root account just holds everything together. Pick one below it"
            )));
        }
        if placeholder != 0 {
            return Err(AppError::Validation(format!(
                "Split {n}: placeholder accounts just group things. Pick a real one"
            )));
        }
        if !s.amount.is_finite() {
            return Err(AppError::Validation(format!("Split {n}: enter an amount")));
        }
        if let Some(q) = s.quantity {
            if !q.is_finite() {
                return Err(AppError::Validation(format!(
                    "Split {n}: enter the share count as a number"
                )));
            }
        }
        if let Some(ref memo) = s.memo {
            if memo.len() > 500 {
                return Err(AppError::Validation(format!(
                    "Split {n}: keep the memo under 500 characters"
                )));
            }
        }
        sum += s.amount;
    }
    if sum.abs() > BALANCE_EPS {
        return Err(AppError::Validation(format!(
            "Splits are out of balance by {sum:.2}. Debits should equal credits"
        )));
    }
    if let Some(ref d) = input.description {
        if d.len() > 500 {
            return Err(AppError::Validation(
                "Keep the description under 500 characters".to_string(),
            ));
        }
    }
    if let Some(ref n) = input.num {
        if n.len() > 100 {
            return Err(AppError::Validation(
                "Keep the number under 100 characters".to_string(),
            ));
        }
    }
    if let Some(ref n) = input.notes {
        if n.len() > 2000 {
            return Err(AppError::Validation(
                "Keep notes under 2000 characters".to_string(),
            ));
        }
    }

    Ok(())
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
        let from = utils::validate_optional_iso_date(f.date_from.as_deref())?;
        let to = utils::validate_optional_iso_date(f.date_to.as_deref())?;
        sql::push_tx_date_search(&mut w, from.as_deref(), to.as_deref(), f.search.as_deref());
        if let Some(tag) = f.tag_guid.as_deref().filter(|g| !g.is_empty()) {
            w.add(
                "EXISTS(SELECT 1 FROM transaction_tags tt WHERE tt.tx_guid = t.guid AND tt.tag_guid = {})",
                vec![Box::new(tag.to_string())],
            );
        }
    }

    Ok(w)
}

/// Per-transaction split rows plus the FX-converted transaction total.
type SplitMaps = (
    std::collections::HashMap<String, Vec<SplitData>>,
    std::collections::HashMap<String, f64>,
);

/// Report currency + FX history shared by one list/detail command (loading the
/// full history once instead of once per helper).
struct FxContext {
    report_currency: String,
    history: fx::FxHistory,
}

fn load_fx_context(conn: &Connection) -> Result<FxContext, AppError> {
    Ok(FxContext {
        report_currency: fx::report_currency_or_empty(conn)?,
        history: fx::FxHistory::load(conn)?,
    })
}

/// SQLite caps bound parameters per statement; chunk large GUID lists so a
/// full-width transaction page (or export) can never hit "too many SQL
/// variables". Each chunk holds whole transaction GUIDs, so per-transaction
/// ordering/totals are unaffected.
const MAX_IN_PARAMS: usize = sql::MAX_IN_PARAMS;

fn fetch_splits_for_txns(conn: &Connection, guids: &[String]) -> Result<SplitMaps, AppError> {
    let ctx = load_fx_context(conn)?;
    fetch_splits_for_txns_with(conn, guids, &ctx)
}

fn fetch_splits_for_txns_with(
    conn: &Connection,
    guids: &[String],
    ctx: &FxContext,
) -> Result<SplitMaps, AppError> {
    if guids.is_empty() {
        return Ok((
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
        ));
    }

    let report_currency = &ctx.report_currency;
    let fx_history = &ctx.history;

    let mut map: std::collections::HashMap<String, Vec<SplitData>> =
        std::collections::HashMap::new();
    let mut totals: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    for chunk in guids.chunks(MAX_IN_PARAMS) {
        let sql = format!(
            "SELECT s.guid, s.tx_guid, s.account_guid, a.name as account_name,
                    s.memo,
                    {} as amount,
                    {} as signed_amount,
                    t.post_date,
                    c.mnemonic as commodity,
                    {} as quantity,
                    s.reconciled,
                    a.description as account_description,
                    s.reconcile_state,
                    a.code as account_code
             FROM splits s
             JOIN accounts a ON a.guid = s.account_guid
             LEFT JOIN commodities c ON c.guid = a.commodity_guid
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

        let mut stmt = conn.prepare(&sql)?;
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
                            amount: row.get(5)?,
                            memo: row.get(4)?,
                        },
                        commodity: row.get::<_, Option<String>>(8)?,
                        quantity: row.get(9)?,
                        reconciled: row.get::<_, i64>(10)? != 0,
                        account_description: row.get::<_, Option<String>>(11)?,
                        account_code: row.get::<_, Option<String>>(13)?,
                        reconcile_state: row.get::<_, Option<String>>(12)?,
                    },
                    row.get::<_, f64>(6)?,    // signed_amount
                    row.get::<_, String>(7)?, // post_date
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        for (tx_guid, split, signed_amount, post_date) in rows {
            // Sorted by rational magnitude desc, then signed asc (outflow leg
            // first), so the first split per transaction is deterministic: its
            // signed value is the transaction total, converted to the report
            // currency at the transaction date's FX.
            totals.entry(tx_guid.clone()).or_insert_with(|| {
                let fx_at = fx_history.map_for_date(&post_date);
                let currency = split.commodity.as_deref().unwrap_or(report_currency);
                fx::convert(signed_amount, currency, report_currency, &fx_at)
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

fn map_tx_rows(conn: &Connection, rows: Vec<TxRow>) -> Result<Vec<TransactionRow>, AppError> {
    let ctx = load_fx_context(conn)?;
    map_tx_rows_with(conn, rows, &ctx)
}

fn map_tx_rows_with(
    conn: &Connection,
    rows: Vec<TxRow>,
    ctx: &FxContext,
) -> Result<Vec<TransactionRow>, AppError> {
    let guids: Vec<String> = rows.iter().map(|r| r.guid.clone()).collect();
    let (split_map, total_map) = fetch_splits_for_txns_with(conn, &guids, ctx)?;
    let mut tag_map = tags::tags_for_txns(conn, &guids)?;

    Ok(rows
        .into_iter()
        .map(|r| TransactionRow {
            tags: tag_map.remove(&r.guid).unwrap_or_default(),
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

/// FX-converted total per transaction, matching the totals computed by
/// [`fetch_splits_for_txns_with`] (leading split = largest rational magnitude,
/// then signed ascending, then guid) but returning one row per transaction
/// instead of every split. Used to sort by amount before paginating.
fn fetch_tx_totals(
    conn: &Connection,
    where_clause: &str,
    params: &[Box<dyn rusqlite::types::ToSql>],
    ctx: &FxContext,
) -> Result<Vec<(String, f64)>, AppError> {
    let signed = utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type");
    let sql = format!(
        "SELECT guid, post_date, signed_amount, commodity FROM (
             SELECT t.guid, t.post_date,
                    {signed} AS signed_amount,
                    c.mnemonic AS commodity,
                    ROW_NUMBER() OVER (
                        PARTITION BY t.guid
                        ORDER BY ABS({raw}) DESC, {signed} ASC, s.guid ASC
                    ) AS rn
             FROM transactions t
             LEFT JOIN splits s ON s.tx_guid = t.guid
             LEFT JOIN accounts a ON a.guid = s.account_guid
             LEFT JOIN commodities c ON c.guid = a.commodity_guid
             {where_clause}
         ) WHERE rn = 1",
        raw = utils::RAW_AMOUNT,
    );
    let mut stmt = conn.prepare(&sql)?;
    let refs = sql::param_refs(params);
    let rows: Vec<(String, String, Option<f64>, Option<String>)> = stmt
        .query_map(refs.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows
        .into_iter()
        .map(|(guid, post_date, signed_amount, commodity)| {
            let amount = signed_amount.unwrap_or(0.0);
            let currency = commodity.as_deref().unwrap_or(&ctx.report_currency);
            let fx_at = ctx.history.map_for_date(&post_date);
            (
                guid,
                fx::convert(amount, currency, &ctx.report_currency, &fx_at),
            )
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
) -> Result<TransactionsResult, AppError> {
    let w = build_filter_conditions(&filters, 0)?;
    let where_clause = w.where_clause();

    let count_sql = format!("SELECT COUNT(*) as cnt FROM transactions t{where_clause}");
    let mut count_stmt = conn.prepare(&count_sql)?;
    let param_refs = sql::param_refs(&w.params);

    let total: i64 = count_stmt.query_row(param_refs.as_slice(), |row| row.get(0))?;

    let (page_size, total_pages, safe_page, offset) = utils::paginate(total, page, page_size);

    // Amount sorting needs the FX-converted totals, which are computed in
    // Rust. Fetch one lightweight total row per matching transaction, sort,
    // paginate, then enrich only the page's transactions.
    if sort_by == Some("amount") {
        let ctx = load_fx_context(conn)?;
        let mut totals = fetch_tx_totals(conn, &where_clause, &w.params, &ctx)?;

        let ascending = sort_dir == Some("asc");
        totals.sort_by(|a, b| {
            let cmp = a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0));
            if ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });

        let page_guids: Vec<String> = totals
            .into_iter()
            .skip(offset as usize)
            .take(page_size as usize)
            .map(|(guid, _)| guid)
            .collect();

        // Fetch the page's transaction headers, preserving the sorted order.
        let mut rows_by_guid: std::collections::HashMap<String, TxRow> =
            std::collections::HashMap::new();
        for chunk in page_guids.chunks(MAX_IN_PARAMS) {
            let sql = format!(
                "SELECT guid, post_date, description, num, notes
                 FROM transactions
                 WHERE guid IN ({})",
                sql::in_clause(chunk.len())
            );
            let mut stmt = conn.prepare(&sql)?;
            let param_refs = sql::string_param_refs(chunk);
            let rows = stmt
                .query_map(param_refs.as_slice(), |row| {
                    Ok(TxRow {
                        guid: row.get(0)?,
                        post_date: row.get(1)?,
                        description: row.get(2)?,
                        num: row.get(3)?,
                        notes: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for row in rows {
                rows_by_guid.insert(row.guid.clone(), row);
            }
        }
        let page_rows: Vec<TxRow> = page_guids
            .iter()
            .filter_map(|guid| rows_by_guid.remove(guid))
            .collect();

        let transactions = map_tx_rows_with(conn, page_rows, &ctx)?;

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

    let mut query_stmt = conn.prepare(&query_sql)?;
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
        })?
        .collect::<Result<Vec<_>, _>>()?;

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
) -> Result<Vec<HeatmapPoint>, AppError> {
    let w = build_filter_conditions(&filters, 0)?;
    let where_clause = w.where_clause();

    let sql = format!(
        "SELECT substr(t.post_date, 1, 10) AS day, COUNT(*) AS cnt
         FROM transactions t{where_clause}
         GROUP BY day
         ORDER BY day"
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs = sql::param_refs(&w.params);

    let points = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(HeatmapPoint {
                date: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

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
) -> Result<TransactionsResult, AppError> {
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
) -> Result<Vec<HeatmapPoint>, AppError> {
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
) -> Result<Option<String>, AppError> {
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
        .map_err(AppError::from)
}

fn fetch_transaction_inner(
    conn: &Connection,
    guid: &str,
) -> Result<Option<TransactionDetailResult>, AppError> {
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
        .optional()?;

    let Some(base) = base else {
        return Ok(None);
    };

    let guids = [base.guid.clone()];
    let (split_map, total_map) = fetch_splits_for_txns(conn, &guids)?;
    let attachment_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM attachments WHERE tx_guid = ?1",
        [&base.guid],
        |row| row.get(0),
    )?;
    let tags = tags::tags_for_txns(conn, &guids)?
        .remove(&base.guid)
        .unwrap_or_default();

    let transaction = TransactionDetail {
        tags,
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
) -> Result<Option<TransactionDetailResult>, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_transaction_inner(conn, &guid)
    })
    .await
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
                "Splits are out of balance after rounding. Please adjust the amounts".to_string(),
            ));
        }
    }

    // One prepared statement reused for every split (and across calls through
    // the connection-level cache), instead of re-preparing per leg.
    let mut insert = conn.prepare_cached(
        "INSERT INTO splits
              (guid, tx_guid, account_guid, value_num, value_denom,
               quantity_num, quantity_denom, reconcile_state, reconciled, memo)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'n', 0, ?8)",
    )?;
    let mut guids = Vec::with_capacity(splits.len());
    for (s, (value_num, value_denom, quantity_num, quantity_denom)) in splits.iter().zip(converted)
    {
        let split_guid = uuid::Uuid::new_v4().to_string();
        insert.execute(params![
            split_guid,
            tx_guid,
            s.account_guid,
            value_num,
            value_denom,
            quantity_num,
            quantity_denom,
            opt_text(&s.memo),
        ])?;
        guids.push(split_guid);
    }
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
            "Cannot {action}. This transaction has cleared or reconciled splits. Unreconcile it first"
        )));
    }
    Ok(())
}

fn create_transaction_inner(conn: &Connection, input: TxInput) -> Result<String, AppError> {
    let post_date = validate_tx_input(conn, &input)?;
    let tag_names = tags::normalize_tag_names(input.tags.as_deref().unwrap_or_default())?;
    let tx_guid = uuid::Uuid::new_v4().to_string();

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
        )?;
        insert_split_rows(conn, &tx_guid, &input.splits)?;
        tags::set_transaction_tags(conn, &tx_guid, &tag_names)?;
        Ok(tx_guid.clone())
    })
}

fn update_transaction_inner(conn: &Connection, guid: &str, input: TxInput) -> Result<(), AppError> {
    let post_date = validate_tx_input(conn, &input)?;
    let tag_names = input
        .tags
        .as_deref()
        .map(tags::normalize_tag_names)
        .transpose()?;
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM transactions WHERE guid = ?1",
            params![guid],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Err(AppError::NotFound(
            "Transaction not found. It may have been deleted or replaced".to_string(),
        ));
    }
    // Manual edits delete + re-insert splits as `'n'`, so saving a cleared or
    // reconciled transaction wipes its reconciliation state. The edit form
    // confirms that consequence and sets `unreconcile`; callers that skip the
    // confirmation stay blocked.
    if !input.unreconcile {
        reject_when_reconciled(conn, guid, "edit")?;
    }

    with_tx_err(conn, || {
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
        )?;
        conn.execute("DELETE FROM splits WHERE tx_guid = ?1", params![guid])?;
        insert_split_rows(conn, guid, &input.splits)?;
        if let Some(ref names) = tag_names {
            tags::set_transaction_tags(conn, guid, names)?;
        }
        Ok(())
    })
}

fn delete_transaction_inner(
    conn: &Connection,
    guid: &str,
    unreconcile: bool,
) -> Result<(), AppError> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM transactions WHERE guid = ?1",
            params![guid],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Err(AppError::NotFound(
            "Transaction not found. It may have been deleted or replaced".to_string(),
        ));
    }
    if !unreconcile {
        reject_when_reconciled(conn, guid, "delete")?;
    }

    with_tx_err(conn, || {
        conn.execute("DELETE FROM splits WHERE tx_guid = ?1", params![guid])?;
        conn.execute("DELETE FROM transactions WHERE guid = ?1", params![guid])?;
        Ok(())
    })
}

#[tauri::command]
pub async fn create_transaction(
    state: State<'_, AppState>,
    input: TxInput,
) -> Result<String, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        create_transaction_inner(conn, input)
    })
    .await
}

#[tauri::command]
pub async fn update_transaction(
    state: State<'_, AppState>,
    guid: String,
    input: TxInput,
) -> Result<(), AppError> {
    run_blocking(state.db.clone(), move |conn| {
        update_transaction_inner(conn, &guid, input)
    })
    .await
}

#[tauri::command]
pub async fn delete_transaction(
    state: State<'_, AppState>,
    guid: String,
    unreconcile: Option<bool>,
) -> Result<(), AppError> {
    let unreconcile = unreconcile.unwrap_or(false);
    run_blocking(state.db.clone(), move |conn| {
        delete_transaction_inner(conn, &guid, unreconcile)
    })
    .await
}

// ── Bulk edit ───────────────────────────────────────────────────────────────

/// Which transactions a bulk edit touches: an explicit selection, or every row
/// matching the list filters ("Select all matching"). `expected_count` is the
/// total the user saw; a mismatch means the list changed underneath them
/// (another edit, an import, a book switch) and the edit is refused.
#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BulkTarget {
    Guids {
        guids: Vec<String>,
    },
    Matching {
        filters: Option<Filters>,
        expected_count: i64,
    },
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BulkAction {
    /// Delete whole transactions; attachments go with them (FK cascade).
    Delete,
    /// Repoint the splits posted to `from_account_guid` at `to_account_guid`
    /// (e.g. recategorize imported rows). Amounts stay as they are, so both
    /// accounts must share a commodity.
    MoveSplits {
        from_account_guid: String,
        to_account_guid: String,
    },
    /// Overwrite the description. Splits are untouched, so cleared and
    /// reconciled transactions are renamed too.
    SetDescription { description: String },
    /// Add a tag by name (created when missing). Tags live on the
    /// transaction, so cleared and reconciled ones are tagged too.
    AddTag { name: String },
    /// Remove one tag; the tag itself stays for other transactions.
    RemoveTag { tag_guid: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkEditInput {
    pub target: BulkTarget,
    pub action: BulkAction,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BulkEditResult {
    /// Transactions the action changed.
    pub changed: i64,
    /// Transactions left alone because the splits the action would rewrite
    /// are cleared or reconciled (that state belongs to the reconcile flow).
    pub skipped_locked: i64,
}

fn resolve_bulk_guids(conn: &Connection, target: &BulkTarget) -> Result<Vec<String>, AppError> {
    let guids = match target {
        BulkTarget::Guids { guids } => {
            let mut guids = guids.clone();
            guids.sort();
            guids.dedup();
            guids
        }
        BulkTarget::Matching {
            filters,
            expected_count,
        } => {
            let w = build_filter_conditions(filters, 0)?;
            let sql = format!("SELECT t.guid FROM transactions t{}", w.where_clause());
            let mut stmt = conn.prepare(&sql)?;
            let guids = stmt
                .query_map(sql::param_refs(&w.params).as_slice(), |row| row.get(0))?
                .collect::<Result<Vec<String>, _>>()?;
            if guids.len() as i64 != *expected_count {
                return Err(AppError::Validation(
                    "The list changed since you selected it. Check the selection and try again"
                        .to_string(),
                ));
            }
            guids
        }
    };
    if guids.is_empty() {
        return Err(AppError::Validation(
            "Select at least one transaction".to_string(),
        ));
    }
    Ok(guids)
}

/// Transactions among `guids` with a cleared or reconciled split, limited to
/// splits in `account_guid` when given.
fn locked_tx_guids(
    conn: &Connection,
    guids: &[String],
    account_guid: Option<&str>,
) -> Result<std::collections::HashSet<String>, AppError> {
    let mut locked = std::collections::HashSet::new();
    for chunk in guids.chunks(MAX_IN_PARAMS) {
        let mut sql = format!(
            "SELECT DISTINCT tx_guid FROM splits
             WHERE tx_guid IN ({}) AND reconcile_state IN ('c', 'y')",
            sql::in_clause(chunk.len())
        );
        let mut params = sql::string_param_refs(chunk);
        if let Some(ref account) = account_guid {
            sql.push_str(&format!(" AND account_guid = ?{}", chunk.len() + 1));
            params.push(account);
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params.as_slice(), |row| row.get::<_, String>(0))?;
        for row in rows {
            locked.insert(row?);
        }
    }
    Ok(locked)
}

/// Commodity of a postable account, with the same rules the editor applies
/// (no ROOT, no placeholders).
fn postable_account_commodity(
    conn: &Connection,
    guid: &str,
    role: &str,
) -> Result<Option<String>, AppError> {
    let row = conn
        .query_row(
            "SELECT COALESCE(placeholder, 0), account_type, commodity_guid
             FROM accounts WHERE guid = ?1",
            params![guid],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((placeholder, account_type, commodity)) = row else {
        return Err(AppError::NotFound(format!(
            "The {role} account wasn't found. It may have been removed"
        )));
    };
    if account_type == "ROOT" || placeholder != 0 {
        return Err(AppError::Validation(format!(
            "The {role} account just groups others. Pick a real one"
        )));
    }
    Ok(commodity)
}

fn bulk_delete(conn: &Connection, guids: &[String]) -> Result<BulkEditResult, AppError> {
    let locked = locked_tx_guids(conn, guids, None)?;
    let open: Vec<String> = guids
        .iter()
        .filter(|g| !locked.contains(*g))
        .cloned()
        .collect();
    let mut changed = 0;
    for chunk in open.chunks(MAX_IN_PARAMS) {
        let in_list = sql::in_clause(chunk.len());
        let params = sql::string_param_refs(chunk);
        conn.execute(
            &format!("DELETE FROM splits WHERE tx_guid IN ({in_list})"),
            params.as_slice(),
        )?;
        changed += conn.execute(
            &format!("DELETE FROM transactions WHERE guid IN ({in_list})"),
            params.as_slice(),
        )? as i64;
    }
    Ok(BulkEditResult {
        changed,
        skipped_locked: locked.len() as i64,
    })
}

fn bulk_move_splits(
    conn: &Connection,
    guids: &[String],
    from: &str,
    to: &str,
) -> Result<BulkEditResult, AppError> {
    if from == to {
        return Err(AppError::Validation(
            "Pick a different account to move to".to_string(),
        ));
    }
    // The source may be a placeholder (imported books post to them); only the
    // target has to accept new postings.
    let from_commodity: Option<String> = conn
        .query_row(
            "SELECT commodity_guid FROM accounts WHERE guid = ?1",
            params![from],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            AppError::NotFound(
                "The account to move from wasn't found. It may have been removed".to_string(),
            )
        })?;
    let to_commodity = postable_account_commodity(conn, to, "target")?;
    if from_commodity != to_commodity {
        return Err(AppError::Validation(
            "Both accounts need the same currency or commodity".to_string(),
        ));
    }

    let locked = locked_tx_guids(conn, guids, Some(from))?;
    let open: Vec<String> = guids
        .iter()
        .filter(|g| !locked.contains(*g))
        .cloned()
        .collect();
    let mut changed = 0;
    for chunk in open.chunks(MAX_IN_PARAMS) {
        let in_list = sql::in_clause(chunk.len());
        let n = chunk.len();
        let mut params = sql::string_param_refs(chunk);
        params.push(&from);
        changed += conn.query_row(
            &format!(
                "SELECT COUNT(DISTINCT tx_guid) FROM splits
                 WHERE tx_guid IN ({in_list}) AND account_guid = ?{}",
                n + 1
            ),
            params.as_slice(),
            |row| row.get::<_, i64>(0),
        )?;
        params.push(&to);
        conn.execute(
            &format!(
                "UPDATE splits SET account_guid = ?{}
                 WHERE tx_guid IN ({in_list}) AND account_guid = ?{}",
                n + 2,
                n + 1
            ),
            params.as_slice(),
        )?;
    }
    Ok(BulkEditResult {
        changed,
        skipped_locked: locked.len() as i64,
    })
}

fn bulk_set_description(
    conn: &Connection,
    guids: &[String],
    description: &str,
) -> Result<BulkEditResult, AppError> {
    let description = description.trim();
    if description.is_empty() {
        return Err(AppError::Validation("Enter a description".to_string()));
    }
    if description.len() > 500 {
        return Err(AppError::Validation(
            "Keep the description under 500 characters".to_string(),
        ));
    }
    let mut changed = 0;
    for chunk in guids.chunks(MAX_IN_PARAMS) {
        let mut params = sql::string_param_refs(chunk);
        params.push(&description);
        changed += conn.execute(
            &format!(
                "UPDATE transactions SET description = ?{} WHERE guid IN ({})",
                chunk.len() + 1,
                sql::in_clause(chunk.len())
            ),
            params.as_slice(),
        )? as i64;
    }
    Ok(BulkEditResult {
        changed,
        skipped_locked: 0,
    })
}

fn bulk_add_tag(
    conn: &Connection,
    guids: &[String],
    name: &str,
) -> Result<BulkEditResult, AppError> {
    let name = tags::normalize_tag_name(name)?;
    let tag_guid = tags::ensure_tag(conn, &name)?;
    // Same cap the editor enforces, counted against the tags each
    // transaction would end up with.
    let mut full = 0;
    for chunk in guids.chunks(MAX_IN_PARAMS) {
        let mut params = sql::string_param_refs(chunk);
        params.push(&tag_guid);
        full += conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM (
                     SELECT tx_guid FROM transaction_tags
                     WHERE tx_guid IN ({}) GROUP BY tx_guid
                     HAVING COUNT(*) >= {} AND SUM(tag_guid = ?{}) = 0
                 )",
                sql::in_clause(chunk.len()),
                tags::MAX_TAGS_PER_TX,
                chunk.len() + 1
            ),
            params.as_slice(),
            |row| row.get::<_, i64>(0),
        )?;
    }
    if full > 0 {
        return Err(AppError::Validation(format!(
            "{full} of these already carry {} tags, the most one can have",
            tags::MAX_TAGS_PER_TX
        )));
    }
    let mut changed = 0;
    for chunk in guids.chunks(MAX_IN_PARAMS) {
        let mut params = sql::string_param_refs(chunk);
        params.push(&tag_guid);
        changed += conn.execute(
            &format!(
                "INSERT OR IGNORE INTO transaction_tags (tx_guid, tag_guid)
                 SELECT guid, ?{} FROM transactions WHERE guid IN ({})",
                chunk.len() + 1,
                sql::in_clause(chunk.len())
            ),
            params.as_slice(),
        )? as i64;
    }
    Ok(BulkEditResult {
        changed,
        skipped_locked: 0,
    })
}

fn bulk_remove_tag(
    conn: &Connection,
    guids: &[String],
    tag_guid: &str,
) -> Result<BulkEditResult, AppError> {
    if tags::tag_name(conn, tag_guid)?.is_none() {
        return Err(AppError::NotFound(
            "That tag is gone. It may have been deleted".to_string(),
        ));
    }
    let mut changed = 0;
    for chunk in guids.chunks(MAX_IN_PARAMS) {
        let mut params = sql::string_param_refs(chunk);
        params.push(&tag_guid);
        changed += conn.execute(
            &format!(
                "DELETE FROM transaction_tags WHERE tx_guid IN ({}) AND tag_guid = ?{}",
                sql::in_clause(chunk.len()),
                chunk.len() + 1
            ),
            params.as_slice(),
        )? as i64;
    }
    Ok(BulkEditResult {
        changed,
        skipped_locked: 0,
    })
}

fn bulk_edit_transactions_inner(
    conn: &Connection,
    input: BulkEditInput,
) -> Result<BulkEditResult, AppError> {
    let guids = resolve_bulk_guids(conn, &input.target)?;
    with_tx_err(conn, || match &input.action {
        BulkAction::Delete => bulk_delete(conn, &guids),
        BulkAction::MoveSplits {
            from_account_guid,
            to_account_guid,
        } => bulk_move_splits(conn, &guids, from_account_guid, to_account_guid),
        BulkAction::SetDescription { description } => {
            bulk_set_description(conn, &guids, description)
        }
        BulkAction::AddTag { name } => bulk_add_tag(conn, &guids, name),
        BulkAction::RemoveTag { tag_guid } => bulk_remove_tag(conn, &guids, tag_guid),
    })
}

#[tauri::command]
pub async fn bulk_edit_transactions(
    state: State<'_, AppState>,
    input: BulkEditInput,
) -> Result<BulkEditResult, AppError> {
    let app_data_dir = state.app_data_dir.clone();
    run_blocking(state.db.clone(), move |conn| {
        crate::backups::before_change(
            conn,
            &app_data_dir,
            crate::backups::BackupReason::BeforeBulkEdit,
        )?;
        bulk_edit_transactions_inner(conn, input)
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
            tags: None,
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
            unreconcile: false,
        }
    }

    #[test]
    fn create_balanced_transaction_persists() {
        let conn = setup_two_accounts();
        let guid = create_transaction_inner(&conn, balanced_input()).expect("create runs");
        let detail = fetch_transaction_inner(&conn, &guid)
            .expect("query runs")
            .expect("tx exists");
        assert_eq!(detail.transaction.splits.len(), 2);
        assert_eq!(detail.transaction.post_date, "2024-03-01");
        // Raw amounts round-trip with fixed-denominator precision.
        let mut amounts: Vec<f64> = detail.transaction.splits.iter().map(|s| s.amount).collect();
        amounts.sort_by(|a, b| a.total_cmp(b));
        assert!((amounts[0] + 42.10).abs() < 1e-6);
        assert!((amounts[1] - 42.10).abs() < 1e-6);
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
        assert!(to_value_rational(999_999_999.999_9).is_ok());
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
                tag_guid: None,
            }),
            None,
            None,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Invalid date"),
            "unexpected: {err}"
        );

        let err = fetch_transaction_heatmap_inner(
            &conn,
            Some(Filters {
                search: None,
                date_from: None,
                date_to: Some("01/02/2024".to_string()),
                tag_guid: None,
            }),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Invalid date"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn create_rejects_splits_on_the_root_account() {
        let conn = setup_two_accounts();
        let mut input = balanced_input();
        input.splits[0].account_guid = "root".to_string();
        let err = create_transaction_inner(&conn, input).unwrap_err();
        assert!(
            err.to_string().contains("root account"),
            "unexpected: {err}"
        );
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
        assert!(create_transaction_inner(&conn, bad)
            .unwrap_err()
            .to_string()
            .contains("placeholder"));
        // Future date hard-block.
        let mut bad = balanced_input();
        bad.post_date = "2099-01-01".to_string();
        assert!(create_transaction_inner(&conn, bad)
            .unwrap_err()
            .to_string()
            .contains("Future"));
        // Single split.
        let mut bad = balanced_input();
        bad.splits.truncate(1);
        assert!(create_transaction_inner(&conn, bad)
            .unwrap_err()
            .to_string()
            .contains("two splits"));
        // Unknown account.
        let mut bad = balanced_input();
        bad.splits[0].account_guid = "nope".to_string();
        assert!(create_transaction_inner(&conn, bad)
            .unwrap_err()
            .to_string()
            .contains("not found"));
    }

    #[test]
    fn update_replaces_splits_and_delete_removes() {
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

        delete_transaction_inner(&conn, &guid, false).expect("delete runs");
        assert!(fetch_transaction_inner(&conn, &guid)
            .expect("query runs")
            .is_none());
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
                tag_guid: None,
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
                tag_guid: None,
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

        // Amount-sorted pagination slices the sorted list and still enriches
        // only the page's transactions with their totals.
        let amount_page_1 =
            fetch_transactions_inner(&conn, 1, 1, None, Some("amount"), Some("desc")).unwrap();
        assert_eq!(amount_page_1.total_pages, 2);
        assert_eq!(amount_page_1.transactions[0].guid, "t1");
        assert_eq!(amount_page_1.transactions[0].total_amount, 50.0);
        let amount_page_2 =
            fetch_transactions_inner(&conn, 2, 1, None, Some("amount"), Some("desc")).unwrap();
        assert_eq!(amount_page_2.transactions[0].guid, "t2");
        assert_eq!(amount_page_2.transactions[0].total_amount, -15.0);
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
            let err = delete_transaction_inner(&conn, &guid, false).unwrap_err();
            assert!(err.to_string().contains("reconcile"), "unexpected: {err}");
        }

        set_state("n");
        update_transaction_inner(&conn, &guid, balanced_input()).expect("unlocked edit runs");
        delete_transaction_inner(&conn, &guid, false).expect("unlocked delete runs");
    }

    #[test]
    fn confirmed_unreconcile_save_resets_split_states() {
        let conn = setup_two_accounts();
        let guid = create_transaction_inner(&conn, balanced_input()).unwrap();
        conn.execute(
            "UPDATE splits SET reconcile_state = 'y', reconciled = 1 WHERE tx_guid = ?1",
            params![guid],
        )
        .unwrap();

        let mut edited = balanced_input();
        edited.description = Some("Fixed description".to_string());
        edited.unreconcile = true;
        update_transaction_inner(&conn, &guid, edited).expect("confirmed edit runs");

        let detail = fetch_transaction_inner(&conn, &guid)
            .expect("query runs")
            .expect("tx exists");
        assert_eq!(
            detail.transaction.description.as_deref(),
            Some("Fixed description")
        );
        for split in &detail.transaction.splits {
            assert_eq!(split.reconcile_state.as_deref(), Some("n"));
            assert!(!split.reconciled);
        }
        let cached: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM splits WHERE tx_guid = ?1 AND reconciled != 0",
                params![guid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cached, 0);

        // Delete takes the same confirmation flag.
        conn.execute(
            "UPDATE splits SET reconcile_state = 'c', reconciled = 1 WHERE tx_guid = ?1",
            params![guid],
        )
        .unwrap();
        delete_transaction_inner(&conn, &guid, true).expect("confirmed delete runs");
    }

    #[test]
    fn failed_confirmed_save_keeps_reconciliation_state() {
        let conn = setup_two_accounts();
        let guid = create_transaction_inner(&conn, balanced_input()).unwrap();
        conn.execute(
            "UPDATE splits SET reconcile_state = 'y', reconciled = 1 WHERE tx_guid = ?1",
            params![guid],
        )
        .unwrap();

        let mut edited = balanced_input();
        edited.unreconcile = true;
        // Break the balance check so the save fails before writing anything.
        edited.splits[0].amount = -1.0;
        update_transaction_inner(&conn, &guid, edited).unwrap_err();

        let locks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM splits WHERE tx_guid = ?1 AND reconcile_state = 'y'",
                params![guid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(locks, 2);
    }

    fn setup_bulk() -> Connection {
        let conn = setup_two_accounts();
        conn.execute_batch(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-usd', 'USD', 'CURRENCY');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('dining', 'Dining', 'EXPENSE', 'root', 0, 'c-eur'),
                    ('travel-usd', 'Travel', 'EXPENSE', 'root', 0, 'c-usd');",
        )
        .unwrap();
        conn
    }

    fn guids_target(guids: &[&str]) -> BulkTarget {
        BulkTarget::Guids {
            guids: guids.iter().map(|g| g.to_string()).collect(),
        }
    }

    fn lock_tx(conn: &Connection, tx_guid: &str, account: &str) {
        conn.execute(
            "UPDATE splits SET reconcile_state = 'y', reconciled = 1
             WHERE tx_guid = ?1 AND account_guid = ?2",
            params![tx_guid, account],
        )
        .unwrap();
    }

    fn split_accounts(conn: &Connection, tx_guid: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT account_guid FROM splits WHERE tx_guid = ?1 ORDER BY account_guid")
            .unwrap();
        stmt.query_map(params![tx_guid], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<String>, _>>()
            .unwrap()
    }

    #[test]
    fn bulk_input_deserializes_from_ipc_shape() {
        let input: BulkEditInput = serde_json::from_value(serde_json::json!({
            "target": { "kind": "matching", "filters": { "search": "x" }, "expectedCount": 3 },
            "action": { "kind": "moveSplits", "fromAccountGuid": "a", "toAccountGuid": "b" }
        }))
        .unwrap();
        assert!(matches!(
            input.target,
            BulkTarget::Matching {
                expected_count: 3,
                ..
            }
        ));
        assert!(matches!(
            input.action,
            BulkAction::MoveSplits { ref from_account_guid, .. } if from_account_guid == "a"
        ));
    }

    #[test]
    fn bulk_delete_skips_locked_transactions() {
        let conn = setup_bulk();
        let a = create_transaction_inner(&conn, balanced_input()).unwrap();
        let b = create_transaction_inner(&conn, balanced_input()).unwrap();
        lock_tx(&conn, &b, "checking");

        let result = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: guids_target(&[&a, &b, &a]),
                action: BulkAction::Delete,
            },
        )
        .unwrap();
        assert_eq!(
            result,
            BulkEditResult {
                changed: 1,
                skipped_locked: 1
            }
        );
        assert!(fetch_transaction_inner(&conn, &a).unwrap().is_none());
        assert!(fetch_transaction_inner(&conn, &b).unwrap().is_some());
        let orphans: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM splits WHERE tx_guid = ?1",
                params![a],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn bulk_move_repoints_only_open_splits_in_the_source_account() {
        let conn = setup_bulk();
        let a = create_transaction_inner(&conn, balanced_input()).unwrap();
        let b = create_transaction_inner(&conn, balanced_input()).unwrap();
        // Locked on the *other* side: the groceries split still moves.
        lock_tx(&conn, &b, "checking");
        let c = create_transaction_inner(&conn, balanced_input()).unwrap();
        lock_tx(&conn, &c, "groceries");

        let result = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: guids_target(&[&a, &b, &c, "t1"]),
                action: BulkAction::MoveSplits {
                    from_account_guid: "groceries".into(),
                    to_account_guid: "dining".into(),
                },
            },
        )
        .unwrap();
        // t1 has no groceries split, so it is neither changed nor skipped.
        assert_eq!(
            result,
            BulkEditResult {
                changed: 2,
                skipped_locked: 1
            }
        );
        assert_eq!(split_accounts(&conn, &a), ["checking", "dining"]);
        assert_eq!(split_accounts(&conn, &b), ["checking", "dining"]);
        assert_eq!(split_accounts(&conn, &c), ["checking", "groceries"]);
    }

    #[test]
    fn bulk_move_rejects_bad_targets() {
        let conn = setup_bulk();
        let a = create_transaction_inner(&conn, balanced_input()).unwrap();
        for to in ["groceries", "placeholder", "root", "travel-usd", "missing"] {
            let err = bulk_edit_transactions_inner(
                &conn,
                BulkEditInput {
                    target: guids_target(&[&a]),
                    action: BulkAction::MoveSplits {
                        from_account_guid: "groceries".into(),
                        to_account_guid: to.into(),
                    },
                },
            )
            .unwrap_err();
            let expected_kind = if to == "missing" {
                "notFound"
            } else {
                "validation"
            };
            assert_eq!(err.kind(), expected_kind, "{to}: {err}");
        }
        assert_eq!(split_accounts(&conn, &a), ["checking", "groceries"]);
    }

    #[test]
    fn bulk_set_description_renames_locked_transactions_too() {
        let conn = setup_bulk();
        let a = create_transaction_inner(&conn, balanced_input()).unwrap();
        lock_tx(&conn, &a, "checking");
        let result = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: guids_target(&[&a, "t1"]),
                action: BulkAction::SetDescription {
                    description: "  Supermarket ".into(),
                },
            },
        )
        .unwrap();
        assert_eq!(result.changed, 2);
        let detail = fetch_transaction_inner(&conn, &a).unwrap().unwrap();
        assert_eq!(
            detail.transaction.description.as_deref(),
            Some("Supermarket")
        );

        let err = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: guids_target(&[&a]),
                action: BulkAction::SetDescription {
                    description: "   ".into(),
                },
            },
        )
        .unwrap_err();
        assert_eq!(err.kind(), "validation");
    }

    #[test]
    fn bulk_matching_target_uses_list_filters_and_expected_count() {
        let conn = setup_bulk();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('future', '2099-01-01')",
            [],
        )
        .unwrap();
        let filters = || {
            Some(Filters {
                search: None,
                date_from: Some("2024-01-01".into()),
                date_to: Some("2024-01-31".into()),
                tag_guid: None,
            })
        };
        // Stale count: refuse instead of touching a different set of rows.
        let err = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: BulkTarget::Matching {
                    filters: filters(),
                    expected_count: 2,
                },
                action: BulkAction::Delete,
            },
        )
        .unwrap_err();
        assert_eq!(err.kind(), "validation");

        let result = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: BulkTarget::Matching {
                    filters: filters(),
                    expected_count: 1,
                },
                action: BulkAction::Delete,
            },
        )
        .unwrap();
        assert_eq!(result.changed, 1);
        assert!(fetch_transaction_inner(&conn, "t1").unwrap().is_none());
        assert!(fetch_transaction_inner(&conn, "t2").unwrap().is_some());

        // No filters still respects the visibility window: `future` is not
        // part of "all matching".
        let result = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: BulkTarget::Matching {
                    filters: None,
                    expected_count: 1,
                },
                action: BulkAction::Delete,
            },
        )
        .unwrap();
        assert_eq!(result.changed, 1);
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 1);
    }

    #[test]
    fn bulk_edit_requires_a_selection() {
        let conn = setup_bulk();
        let err = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: guids_target(&[]),
                action: BulkAction::Delete,
            },
        )
        .unwrap_err();
        assert_eq!(err.kind(), "validation");
    }

    fn tag_names(conn: &Connection, guid: &str) -> Vec<String> {
        fetch_transaction_inner(conn, guid)
            .unwrap()
            .unwrap()
            .transaction
            .tags
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    fn tagged_input(tags: &[&str]) -> TxInput {
        TxInput {
            tags: Some(tags.iter().map(|t| t.to_string()).collect()),
            ..balanced_input()
        }
    }

    #[test]
    fn create_and_update_carry_tags() {
        let conn = setup_bulk();
        let guid = create_transaction_inner(&conn, tagged_input(&["Trip", "work"])).unwrap();
        assert_eq!(tag_names(&conn, &guid), vec!["Trip", "work"]);

        // `None` leaves tags alone; `Some` replaces them.
        update_transaction_inner(&conn, &guid, balanced_input()).unwrap();
        assert_eq!(tag_names(&conn, &guid), vec!["Trip", "work"]);
        update_transaction_inner(&conn, &guid, tagged_input(&["trip"])).unwrap();
        assert_eq!(tag_names(&conn, &guid), vec!["Trip"]);

        // A bad tag fails the whole save.
        let err = create_transaction_inner(&conn, tagged_input(&["a,b"])).unwrap_err();
        assert_eq!(err.kind(), "validation");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3, "two seeded rows plus the one created above");
    }

    #[test]
    fn list_filters_by_tag_and_searches_tag_names() {
        let conn = setup_bulk();
        let a = create_transaction_inner(&conn, tagged_input(&["Summer trip"])).unwrap();
        create_transaction_inner(&conn, tagged_input(&["work"])).unwrap();
        let trip = tags::ensure_tag(&conn, "Summer trip").unwrap();

        let by_tag = fetch_transactions_inner(
            &conn,
            1,
            50,
            Some(Filters {
                search: None,
                date_from: None,
                date_to: None,
                tag_guid: Some(trip),
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(by_tag.total, 1);
        assert_eq!(by_tag.transactions[0].guid, a);
        assert_eq!(by_tag.transactions[0].tags[0].name, "Summer trip");

        let by_search = fetch_transactions_inner(
            &conn,
            1,
            50,
            Some(Filters {
                search: Some("summer".into()),
                date_from: None,
                date_to: None,
                tag_guid: None,
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(by_search.total, 1);
    }

    #[test]
    fn bulk_tags_apply_to_locked_transactions_too() {
        let conn = setup_bulk();
        let a = create_transaction_inner(&conn, tagged_input(&["work"])).unwrap();
        let b = create_transaction_inner(&conn, balanced_input()).unwrap();
        lock_tx(&conn, &b, "checking");

        let added = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: guids_target(&[&a, &b]),
                action: BulkAction::AddTag {
                    name: " Deductible ".into(),
                },
            },
        )
        .unwrap();
        assert_eq!(
            added,
            BulkEditResult {
                changed: 2,
                skipped_locked: 0
            }
        );
        assert_eq!(tag_names(&conn, &a), vec!["Deductible", "work"]);
        assert_eq!(tag_names(&conn, &b), vec!["Deductible"]);

        let work = tags::ensure_tag(&conn, "work").unwrap();
        let removed = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: guids_target(&[&a, &b]),
                action: BulkAction::RemoveTag { tag_guid: work },
            },
        )
        .unwrap();
        assert_eq!(removed.changed, 1, "only `a` carried it");
        assert_eq!(tag_names(&conn, &a), vec!["Deductible"]);
    }

    #[test]
    fn bulk_add_tag_respects_the_per_transaction_cap() {
        let conn = setup_bulk();
        let full: Vec<String> = (0..tags::MAX_TAGS_PER_TX)
            .map(|i| format!("t{i}"))
            .collect();
        let a = create_transaction_inner(
            &conn,
            TxInput {
                tags: Some(full),
                ..balanced_input()
            },
        )
        .unwrap();
        let err = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: guids_target(&[&a]),
                action: BulkAction::AddTag {
                    name: "one more".into(),
                },
            },
        )
        .unwrap_err();
        assert_eq!(err.kind(), "validation");
        // Re-adding a tag it already has is fine.
        let again = bulk_edit_transactions_inner(
            &conn,
            BulkEditInput {
                target: guids_target(&[&a]),
                action: BulkAction::AddTag { name: "t0".into() },
            },
        )
        .unwrap();
        assert_eq!(again.changed, 0);
    }
}
