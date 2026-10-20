use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::sql;
use crate::utils;

/// Account types with bank-statement semantics. Investments (`STOCK`/`MUTUAL`/
/// `FUND`), income/expense/equity and `ROOT` are never reconcilable; same set
/// as `CASH_LIKE_TYPES + LIABILITY_TYPES` in `utils.rs` so the two can never
/// drift (kept as an explicit list for a clear validation message).
const RECONCILABLE_TYPES: [&str; 8] = [
    "ASSET",
    "BANK",
    "CASH",
    "CURRENCY",
    "RECEIVABLE",
    "LIABILITY",
    "CREDIT",
    "PAYABLE",
];

/// Tolerance for the statement difference: half a cent, matching the display
/// `clampZero` threshold on the frontend.
const BALANCE_EPS: f64 = 0.005;

/// Cap on state changes accepted in one save/finish call.
const MAX_SPLIT_UPDATES: usize = 2000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileCandidate {
    pub split_guid: String,
    pub tx_guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub memo: Option<String>,
    /// Signed display amount (credit-normal accounts negated) in the
    /// reconciled account's commodity.
    pub amount: f64,
    /// Normalized `'n'`/`'c'` (source `NULL` reads as `'n'`).
    pub reconcile_state: String,
    /// Distinct descriptions (falling back to names) of the transaction's
    /// other splits, largest magnitude first. Helps identify a statement line
    /// when the description is empty.
    pub counter_accounts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileInfo {
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub account_type: String,
    pub commodity: Option<String>,
    pub statement_date: String,
    /// Sum of `'y'` splits at or before the statement date.
    pub starting_balance: f64,
    /// Unreconciled (`'n'`/`'c'`) splits at or before the statement date,
    /// oldest first.
    pub candidates: Vec<ReconcileCandidate>,
    /// Post date of the most recent reconciled batch (`None` when the account
    /// has never been reconciled).
    pub last_reconciled_date: Option<String>,
    /// Splits in that batch — what `reopen_last_reconciliation` returns to
    /// cleared. Zero when the account has never been reconciled.
    pub last_reconciled_count: i64,
}

/// One reconcilable account's progress, merged into the `/reconcile` index so
/// accounts with work (or saved progress) stand out without opening each one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileAccountSummary {
    pub account_guid: String,
    /// Splits still in `'n'` (never cleared) in the visibility window.
    pub unreconciled_count: i64,
    /// Splits saved as `'c'` (progress not yet finished).
    pub cleared_count: i64,
    /// Signed display total of the `'n'` splits, in the account's commodity.
    pub uncleared_total: f64,
    pub last_reconciled_date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReopenReconcileResult {
    pub reopened_count: i64,
    /// Post date of the batch before the reopened one (exclusive lower bound);
    /// `None` when the reopened batch was the first reconciliation.
    pub from_date: Option<String>,
    pub through_date: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitStateUpdate {
    pub split_guid: String,
    pub state: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkReconcileInput {
    pub account_guid: String,
    /// Statement date the candidates were loaded for; updates outside it are
    /// rejected so the write path matches what the UI offered.
    pub statement_date: String,
    pub updates: Vec<SplitStateUpdate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkReconcileResult {
    pub updated: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishReconcileInput {
    pub account_guid: String,
    pub statement_date: String,
    pub ending_balance: f64,
    /// Unsaved marks from the page, applied in the same transaction as the
    /// balance check and promotion so Finish is atomic.
    #[serde(default)]
    pub updates: Vec<SplitStateUpdate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishReconcileResult {
    pub reconciled_count: i64,
    pub starting_balance: f64,
    pub ending_balance: f64,
}

struct ReconcilableAccount {
    name: String,
    description: Option<String>,
    account_type: String,
    commodity: Option<String>,
}

/// Shared `YYYY-MM-DD` validation for statement dates: well-formed, past or
/// today (future statements would leak the visibility window).
fn validate_statement_date(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(
            "Add the statement date from your bank".to_string(),
        ));
    }
    let normalized = crate::utils::validate_iso_date(trimmed)?;
    // Local "today": the reconcile form offers the user's local today, so the
    // backend must accept it (UTC would reject it east of UTC).
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if normalized > today {
        return Err(AppError::Validation(
            "That statement date is in the future. Use today or earlier".to_string(),
        ));
    }
    Ok(normalized)
}

fn load_reconcilable_account(
    conn: &Connection,
    guid: &str,
) -> Result<ReconcilableAccount, AppError> {
    let row: Option<(ReconcilableAccount, i64)> = conn
        .query_row(
            "SELECT a.name, a.description, a.account_type, a.placeholder, c.mnemonic
             FROM accounts a LEFT JOIN commodities c ON c.guid = a.commodity_guid
             WHERE a.guid = ?1",
            params![guid],
            |row| {
                Ok((
                    ReconcilableAccount {
                        name: row.get(0)?,
                        description: row.get(1)?,
                        account_type: row.get(2)?,
                        commodity: row.get(4)?,
                    },
                    row.get(3)?,
                ))
            },
        )
        .optional()?;
    let Some((account, placeholder)) = row else {
        return Err(AppError::NotFound(
            "Account not found. It may have been removed".to_string(),
        ));
    };
    if account.account_type == "ROOT" {
        return Err(AppError::Validation(
            "The root account holds everything together. It can't be reconciled".to_string(),
        ));
    }
    if placeholder != 0 {
        return Err(AppError::Validation(
            "Placeholder accounts just group things. Pick one that holds money to reconcile"
                .to_string(),
        ));
    }
    if !RECONCILABLE_TYPES.contains(&account.account_type.as_str()) {
        return Err(AppError::Validation(format!(
            "Only cash-like accounts can be reconciled. This one is {}",
            account.account_type
        )));
    }
    Ok(account)
}

/// Sum of signed split amounts for one reconcile state up to `statement_date`
/// (half-open `< date(stmt,'+1 day')` so timed rows on the day count, plus the
/// shared visibility window so future rows never leak).
fn sum_by_state(
    conn: &Connection,
    account_guid: &str,
    state: &str,
    statement_date: &str,
) -> Result<f64, AppError> {
    let signed = utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type");
    let total: f64 = conn
        .query_row(
            &format!(
                "SELECT COALESCE(SUM({signed}), 0)
                 FROM splits s
                 JOIN transactions t ON t.guid = s.tx_guid
                 JOIN accounts a ON a.guid = s.account_guid
                 WHERE s.account_guid = ?1 AND s.reconcile_state = ?2
                   AND t.post_date < date(?3, '+1 day')
                   AND {}",
                utils::VISIBLE_TX_FILTER
            ),
            params![account_guid, state, statement_date],
            |row| row.get(0),
        )
        .map_err(AppError::from)?;
    Ok(total)
}

/// Validate a batch of `'n'`/`'c'` state updates and return the ones that
/// actually change a split, in input order (last write wins for duplicates).
/// Rejects foreign splits, `'y'` splits and anything dated after the statement
/// so callers cannot write outside the window the form showed.
fn pending_split_updates(
    conn: &Connection,
    account_guid: &str,
    statement_date: &str,
    updates: &[SplitStateUpdate],
) -> Result<Vec<(String, String)>, AppError> {
    if updates.len() > MAX_SPLIT_UPDATES {
        return Err(AppError::Validation(format!(
            "That's a lot at once. Keep it to {MAX_SPLIT_UPDATES} changes"
        )));
    }
    for u in updates {
        if u.state != "n" && u.state != "c" {
            return Err(AppError::Validation(format!(
                "Invalid reconcile state '{}'. Expected 'n' or 'c'",
                u.state
            )));
        }
        if u.split_guid.trim().is_empty() {
            return Err(AppError::Validation(
                "We’re missing a split id. Please try again".to_string(),
            ));
        }
    }
    // De-dupe by split guid, preserving first-seen order; last write wins.
    let mut deduped: Vec<(String, String)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for u in updates {
        match index.get(&u.split_guid) {
            Some(&i) => deduped[i].1 = u.state.clone(),
            None => {
                index.insert(u.split_guid.clone(), deduped.len());
                deduped.push((u.split_guid.clone(), u.state.clone()));
            }
        }
    }
    if deduped.is_empty() {
        return Ok(Vec::new());
    }
    let guids: Vec<String> = deduped.iter().map(|(g, _)| g.clone()).collect();
    // `?1` is the statement date; guids follow from `?2` on, chunked to stay
    // under SQLite's bound-parameter cap.
    let mut rows: Vec<(String, String, Option<String>, i64)> = Vec::new();
    for chunk in guids.chunks(sql::MAX_IN_PARAMS) {
        let placeholders = (2..2 + chunk.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut stmt = conn.prepare(&format!(
            "SELECT s.guid, s.account_guid, s.reconcile_state,
                    (t.post_date < date(?1, '+1 day')) AS in_window
             FROM splits s
             JOIN transactions t ON t.guid = s.tx_guid
             WHERE s.guid IN ({placeholders})"
        ))?;
        let mut bound: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::with_capacity(chunk.len() + 1);
        bound.push(Box::new(statement_date.to_string()));
        for guid in chunk {
            bound.push(Box::new(guid.clone()));
        }
        let refs = sql::param_refs(&bound);
        rows.extend(
            stmt.query_map(refs.as_slice(), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if rows.len() != guids.len() {
        return Err(AppError::NotFound(
            "A split was not found. It may have been removed".to_string(),
        ));
    }
    let by_guid: HashMap<&str, &(String, String, Option<String>, i64)> =
        rows.iter().map(|row| (row.0.as_str(), row)).collect();
    let mut pending = Vec::with_capacity(deduped.len());
    for (guid, want) in &deduped {
        let row = by_guid[guid.as_str()];
        if row.1 != account_guid {
            return Err(AppError::Validation(format!(
                "Split {guid} does not belong to this account"
            )));
        }
        if row.2.as_deref() == Some("y") {
            return Err(AppError::Validation(
                "Reconciled splits are locked here. Head back to reconcile to change them"
                    .to_string(),
            ));
        }
        if row.3 == 0 {
            return Err(AppError::Validation(
                "That split is dated after the statement. Pick a later statement date first"
                    .to_string(),
            ));
        }
        let have = row.2.clone().unwrap_or_else(|| "n".to_string());
        if have != *want {
            pending.push((guid.clone(), want.clone()));
        }
    }
    Ok(pending)
}

/// Apply validated `(split_guid, state)` pairs. Caller owns the transaction:
/// `mark` wraps this in its own `with_tx_err`, `finish` reuses the one that
/// also checks the balance and promotes.
fn apply_split_updates(conn: &Connection, pending: &[(String, String)]) -> Result<(), AppError> {
    for (guid, want) in pending {
        let reconciled = if want == "c" { 1 } else { 0 };
        conn.execute(
            "UPDATE splits SET reconcile_state = ?1, reconciled = ?2 WHERE guid = ?3",
            params![want, reconciled, guid],
        )?;
    }
    Ok(())
}

/// Legacy fallback for books finished before `reconcile_batches` existed: the
/// most recent reconciled batch inferred from the `'y'` splits dated after the
/// previous batch's last date. Returns `(from_exclusive, through, count)`.
fn legacy_last_reconciled_batch(
    conn: &Connection,
    account_guid: &str,
) -> Result<Option<(Option<String>, String, i64)>, AppError> {
    let through: Option<String> = conn
        .query_row(
            &format!(
                "SELECT MAX(t.post_date) FROM splits s
                 JOIN transactions t ON t.guid = s.tx_guid
                 WHERE s.account_guid = ?1 AND s.reconcile_state = 'y' AND {}",
                utils::VISIBLE_TX_FILTER
            ),
            params![account_guid],
            |row| row.get(0),
        )
        .map_err(AppError::from)?;
    let Some(through) = through else {
        return Ok(None);
    };
    let from_date: Option<String> = conn
        .query_row(
            &format!(
                "SELECT MAX(t.post_date) FROM splits s
                 JOIN transactions t ON t.guid = s.tx_guid
                 WHERE s.account_guid = ?1 AND s.reconcile_state = 'y'
                   AND t.post_date < ?2 AND {}",
                utils::VISIBLE_TX_FILTER
            ),
            params![account_guid, &through],
            |row| row.get(0),
        )
        .map_err(AppError::from)?;
    let count: i64 = match from_date.as_deref() {
        Some(from) => conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM splits s
                 JOIN transactions t ON t.guid = s.tx_guid
                 WHERE s.account_guid = ?1 AND s.reconcile_state = 'y'
                   AND t.post_date > ?2 AND {}",
                utils::VISIBLE_TX_FILTER
            ),
            params![account_guid, from],
            |row| row.get(0),
        )?,
        None => conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM splits s
                 JOIN transactions t ON t.guid = s.tx_guid
                 WHERE s.account_guid = ?1 AND s.reconcile_state = 'y' AND {}",
                utils::VISIBLE_TX_FILTER
            ),
            params![account_guid],
            |row| row.get(0),
        )?,
    };
    Ok(Some((from_date, through, count)))
}

/// Currency-priced `'y'` split guids in the legacy last batch (date-derived).
fn legacy_reconciled_batch_guids(
    conn: &Connection,
    account_guid: &str,
    from_date: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let sql_text = match from_date {
        Some(_) => format!(
            "SELECT s.guid FROM splits s
             JOIN transactions t ON t.guid = s.tx_guid
             WHERE s.account_guid = ?1 AND s.reconcile_state = 'y'
               AND t.post_date > ?2 AND {}",
            utils::VISIBLE_TX_FILTER
        ),
        None => format!(
            "SELECT s.guid FROM splits s
             JOIN transactions t ON t.guid = s.tx_guid
             WHERE s.account_guid = ?1 AND s.reconcile_state = 'y' AND {}",
            utils::VISIBLE_TX_FILTER
        ),
    };
    let mut stmt = conn.prepare(&sql_text)?;
    let mut guids: Vec<String> = Vec::new();
    match from_date {
        Some(from) => {
            let rows = stmt.query_map(params![account_guid, from], |row| row.get(0))?;
            for row in rows {
                guids.push(row?);
            }
        }
        None => {
            let rows = stmt.query_map(params![account_guid], |row| row.get(0))?;
            for row in rows {
                guids.push(row?);
            }
        }
    }
    Ok(guids)
}

/// Latest finished batch for an account:
/// `(batch_guid, through_date, previous_through_date)`. Ordered by finish time
/// with rowid as the tie-break so same-instant batches keep insertion order.
fn latest_batch_row(
    conn: &Connection,
    account_guid: &str,
) -> Result<Option<(String, String, Option<String>)>, AppError> {
    let row = conn
        .query_row(
            "SELECT b.guid, b.through_date,
                    (SELECT p.through_date FROM reconcile_batches p
                     WHERE p.account_guid = b.account_guid
                       AND (p.finished_at < b.finished_at
                            OR (p.finished_at = b.finished_at AND p.rowid < b.rowid))
                     ORDER BY p.finished_at DESC, p.rowid DESC LIMIT 1)
             FROM reconcile_batches b
             WHERE b.account_guid = ?1
             ORDER BY b.finished_at DESC, b.rowid DESC
             LIMIT 1",
            params![account_guid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(row)
}

/// `'y'` split guids belonging to a persisted batch; splits that were deleted
/// or already demoted are skipped.
fn batch_split_guids(conn: &Connection, batch_guid: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT k.split_guid FROM reconcile_batch_splits k
         JOIN splits s ON s.guid = k.split_guid
         WHERE k.batch_guid = ?1 AND s.reconcile_state = 'y'
         ORDER BY k.split_guid",
    )?;
    let guids = stmt
        .query_map(params![batch_guid], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(guids)
}

/// Counter-account labels for each candidate's transaction: distinct
/// descriptions (falling back to names) of splits outside the reconciled
/// account, largest magnitude first (mirrors the ledger's counter column).
fn attach_counter_accounts(
    conn: &Connection,
    account_guid: &str,
    candidates: &mut [ReconcileCandidate],
) -> Result<(), AppError> {
    if candidates.is_empty() {
        return Ok(());
    }
    let mut tx_guids: Vec<String> = Vec::new();
    let mut seen_tx: HashSet<&str> = HashSet::new();
    for c in candidates.iter() {
        if seen_tx.insert(c.tx_guid.as_str()) {
            tx_guids.push(c.tx_guid.clone());
        }
    }
    let mut labels_by_tx: HashMap<String, Vec<String>> = HashMap::new();
    let mut seen_accounts: HashMap<String, HashSet<String>> = HashMap::new();
    for chunk in tx_guids.chunks(sql::MAX_IN_PARAMS) {
        let mut stmt = conn.prepare(&format!(
            "SELECT s.tx_guid, s.account_guid, COALESCE(a.description, a.name)
             FROM splits s
             JOIN accounts a ON a.guid = s.account_guid
             WHERE s.tx_guid IN ({})
             ORDER BY s.tx_guid, ABS(s.value_num) DESC",
            sql::in_clause(chunk.len())
        ))?;
        let rows = stmt
            .query_map(sql::string_param_refs(chunk).as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (tx_guid, split_account_guid, label) in rows {
            if split_account_guid == account_guid {
                continue;
            }
            let accounts = seen_accounts.entry(tx_guid.clone()).or_default();
            if accounts.insert(split_account_guid) {
                labels_by_tx.entry(tx_guid).or_default().push(label);
            }
        }
    }
    for c in candidates.iter_mut() {
        if let Some(labels) = labels_by_tx.remove(&c.tx_guid) {
            c.counter_accounts = labels;
        }
    }
    Ok(())
}

fn fetch_reconcile_info_inner(
    conn: &Connection,
    account_guid: &str,
    statement_date: &str,
) -> Result<ReconcileInfo, AppError> {
    let statement_date = validate_statement_date(statement_date)?;
    let account = load_reconcilable_account(conn, account_guid)?;
    let starting_balance = sum_by_state(conn, account_guid, "y", &statement_date)?;

    let signed = utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type");
    let mut stmt = conn.prepare(&format!(
        "SELECT s.guid, s.tx_guid, t.post_date, t.description, t.num, s.memo,
                {signed} as amount, COALESCE(s.reconcile_state, 'n') as state
         FROM splits s
         JOIN transactions t ON t.guid = s.tx_guid
         JOIN accounts a ON a.guid = s.account_guid
         WHERE s.account_guid = ?1
           AND (s.reconcile_state IS NULL OR s.reconcile_state IN ('n', 'c'))
           AND t.post_date < date(?2, '+1 day')
           AND {}
         ORDER BY t.post_date ASC, s.guid ASC",
        utils::VISIBLE_TX_FILTER
    ))?;
    let mut candidates = stmt
        .query_map(params![account_guid, statement_date], |row| {
            Ok(ReconcileCandidate {
                split_guid: row.get(0)?,
                tx_guid: row.get(1)?,
                post_date: row.get(2)?,
                description: row.get(3)?,
                num: row.get(4)?,
                memo: row.get(5)?,
                amount: row.get(6)?,
                reconcile_state: row.get(7)?,
                counter_accounts: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    attach_counter_accounts(conn, account_guid, &mut candidates)?;

    let (last_reconciled_date, last_reconciled_count) = match latest_batch_row(conn, account_guid)?
    {
        Some((batch_guid, through, _)) => (
            Some(through),
            batch_split_guids(conn, &batch_guid)?.len() as i64,
        ),
        // Books finished before batches were persisted fall back to the
        // date-derived heuristic.
        None => match legacy_last_reconciled_batch(conn, account_guid)? {
            Some((_, through, count)) => (Some(through), count),
            None => (None, 0),
        },
    };

    Ok(ReconcileInfo {
        account_guid: account_guid.to_string(),
        account_name: account.name,
        account_description: account.description,
        account_type: account.account_type,
        commodity: account.commodity,
        statement_date,
        starting_balance,
        candidates,
        last_reconciled_date,
        last_reconciled_count,
    })
}

fn mark_reconcile_splits_inner(
    conn: &Connection,
    input: MarkReconcileInput,
) -> Result<MarkReconcileResult, AppError> {
    let statement_date = validate_statement_date(&input.statement_date)?;
    // Validate the target is a real, reconcilable account before touching rows.
    load_reconcilable_account(conn, &input.account_guid)?;
    if input.updates.is_empty() {
        return Err(AppError::Validation(
            "Nothing changed yet. Tick something off first".to_string(),
        ));
    }
    let pending =
        pending_split_updates(conn, &input.account_guid, &statement_date, &input.updates)?;
    if pending.is_empty() {
        return Ok(MarkReconcileResult { updated: 0 });
    }
    let updated = pending.len() as i64;
    with_tx_err(conn, || -> Result<(), AppError> {
        apply_split_updates(conn, &pending)?;
        Ok(())
    })?;
    Ok(MarkReconcileResult { updated })
}

fn cleared_split_guids(
    conn: &Connection,
    account_guid: &str,
    statement_date: &str,
) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT s.guid FROM splits s
         JOIN transactions t ON t.guid = s.tx_guid
         WHERE s.account_guid = ?1 AND s.reconcile_state = 'c'
           AND t.post_date < date(?2, '+1 day')
           AND {}",
        utils::VISIBLE_TX_FILTER
    ))?;
    let guids = stmt
        .query_map(params![account_guid, statement_date], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(guids)
}

fn finish_reconciliation_inner(
    conn: &Connection,
    input: FinishReconcileInput,
) -> Result<FinishReconcileResult, AppError> {
    if !input.ending_balance.is_finite() {
        return Err(AppError::Validation(
            "Enter the ending balance as a number".to_string(),
        ));
    }
    if input.ending_balance.abs() > 1_000_000_000.0 {
        return Err(AppError::Validation(
            "That ending balance is huge. Keep it under 1,000,000,000".to_string(),
        ));
    }
    let statement_date = validate_statement_date(&input.statement_date)?;
    // Validate the target is a real, reconcilable account before touching rows.
    load_reconcilable_account(conn, &input.account_guid)?;
    // Validated up front; applied inside the transaction below so an
    // out-of-balance statement leaves the saved marks untouched.
    let pending =
        pending_split_updates(conn, &input.account_guid, &statement_date, &input.updates)?;

    with_tx_err(conn, || -> Result<FinishReconcileResult, AppError> {
        apply_split_updates(conn, &pending)?;

        let starting_balance = sum_by_state(conn, &input.account_guid, "y", &statement_date)?;
        let cleared_total = sum_by_state(conn, &input.account_guid, "c", &statement_date)?;
        let diff = input.ending_balance - (starting_balance + cleared_total);
        if diff.abs() > BALANCE_EPS {
            return Err(AppError::Validation(format!(
                "Out of balance by {diff:.2}. Tick off what cleared the bank until it hits zero"
            )));
        }
        let guids = cleared_split_guids(conn, &input.account_guid, &statement_date)?;
        if guids.is_empty() {
            return Err(AppError::Validation(
                "Nothing ticked off yet. Mark what cleared first".to_string(),
            ));
        }
        let reconciled_count = guids.len() as i64;

        // Persist the batch membership before promoting: "Undo last statement"
        // reopens exactly these splits, even ones dated before the previous
        // batch's last transaction.
        let batch_guid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO reconcile_batches (guid, account_guid, finished_at, through_date)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                batch_guid,
                input.account_guid,
                utils::iso_now(),
                statement_date
            ],
        )?;
        for guid in &guids {
            conn.execute(
                "INSERT INTO reconcile_batch_splits (batch_guid, split_guid) VALUES (?1, ?2)",
                params![batch_guid, guid],
            )?;
            conn.execute(
                "UPDATE splits SET reconcile_state = 'y', reconciled = 1 WHERE guid = ?1",
                params![guid],
            )?;
        }

        Ok(FinishReconcileResult {
            reconciled_count,
            starting_balance,
            ending_balance: input.ending_balance,
        })
    })
}

fn reopen_last_reconciliation_inner(
    conn: &Connection,
    account_guid: &str,
) -> Result<ReopenReconcileResult, AppError> {
    load_reconcilable_account(conn, account_guid)?;

    // Persisted batches are authoritative; they carry exactly the splits the
    // Finish promoted and the preceding batch's through date.
    if let Some((batch_guid, through_date, from_date)) = latest_batch_row(conn, account_guid)? {
        let guids = batch_split_guids(conn, &batch_guid)?;
        if guids.is_empty() {
            // Membership lost (e.g. its transactions were swept). Drop the
            // empty batch and fall through to the legacy path.
            conn.execute(
                "DELETE FROM reconcile_batches WHERE guid = ?1",
                params![batch_guid],
            )?;
        } else {
            let reopened_count = guids.len() as i64;
            with_tx_err(conn, || -> Result<(), AppError> {
                for guid in &guids {
                    // `'c'` keeps the GnuCash `reconciled = 1` convention used
                    // by `apply_split_updates`, so the editor still warns.
                    conn.execute(
                        "UPDATE splits SET reconcile_state = 'c', reconciled = 1 WHERE guid = ?1",
                        params![guid],
                    )?;
                }
                conn.execute(
                    "DELETE FROM reconcile_batches WHERE guid = ?1",
                    params![batch_guid],
                )?;
                Ok(())
            })?;
            return Ok(ReopenReconcileResult {
                reopened_count,
                from_date,
                through_date,
            });
        }
    }

    // Legacy date-derived fallback for books finished before batches existed.
    let Some((from_date, through_date, _)) = legacy_last_reconciled_batch(conn, account_guid)?
    else {
        return Err(AppError::Validation(
            "Nothing is reconciled yet. Finish a statement first".to_string(),
        ));
    };
    let guids = legacy_reconciled_batch_guids(conn, account_guid, from_date.as_deref())?;
    if guids.is_empty() {
        return Err(AppError::Validation(
            "Nothing is reconciled yet. Finish a statement first".to_string(),
        ));
    }
    let reopened_count = guids.len() as i64;
    with_tx_err(conn, || -> Result<(), AppError> {
        for guid in &guids {
            conn.execute(
                "UPDATE splits SET reconcile_state = 'c', reconciled = 1 WHERE guid = ?1",
                params![guid],
            )?;
        }
        Ok(())
    })?;
    Ok(ReopenReconcileResult {
        reopened_count,
        from_date,
        through_date,
    })
}

fn fetch_reconcile_summaries_inner(
    conn: &Connection,
) -> Result<Vec<ReconcileAccountSummary>, AppError> {
    let signed = utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type");
    let mut summaries: HashMap<String, ReconcileAccountSummary> = HashMap::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT s.account_guid,
                    SUM(CASE WHEN COALESCE(s.reconcile_state, 'n') = 'n' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN s.reconcile_state = 'c' THEN 1 ELSE 0 END),
                    COALESCE(SUM(CASE WHEN COALESCE(s.reconcile_state, 'n') = 'n' THEN {signed} END), 0)
             FROM splits s
             JOIN transactions t ON t.guid = s.tx_guid
             JOIN accounts a ON a.guid = s.account_guid
             WHERE a.placeholder = 0
               AND a.account_type IN ({}, {})
               AND {}
             GROUP BY s.account_guid",
            utils::cash_like_types_sql(),
            utils::liability_types_sql(),
            utils::VISIBLE_TX_FILTER
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;
        for row in rows {
            let (account_guid, unreconciled_count, cleared_count, uncleared_total) = row?;
            summaries.insert(
                account_guid.clone(),
                ReconcileAccountSummary {
                    account_guid,
                    unreconciled_count,
                    cleared_count,
                    uncleared_total,
                    last_reconciled_date: None,
                },
            );
        }
    }
    {
        // Latest persisted batch per account (its through date is already a
        // plain day, unlike possibly-timed transaction dates).
        let mut stmt = conn.prepare(&format!(
            "SELECT b.account_guid, b.through_date
             FROM reconcile_batches b
             JOIN accounts a ON a.guid = b.account_guid
             WHERE a.placeholder = 0
               AND a.account_type IN ({}, {})
               AND b.rowid = (
                   SELECT b2.rowid FROM reconcile_batches b2
                   WHERE b2.account_guid = b.account_guid
                   ORDER BY b2.finished_at DESC, b2.rowid DESC
                   LIMIT 1
               )",
            utils::cash_like_types_sql(),
            utils::liability_types_sql()
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (account_guid, last) = row?;
            summaries
                .entry(account_guid.clone())
                .or_insert(ReconcileAccountSummary {
                    account_guid,
                    unreconciled_count: 0,
                    cleared_count: 0,
                    uncleared_total: 0.0,
                    last_reconciled_date: None,
                })
                .last_reconciled_date = Some(last);
        }
    }
    {
        // Legacy fallback for accounts with `'y'` splits but no batch row
        // (books finished before batches were persisted). Truncate to the day:
        // imported books carry `HH:MM:SS`.
        let mut stmt = conn.prepare(&format!(
            "SELECT s.account_guid, substr(MAX(t.post_date), 1, 10)
             FROM splits s
             JOIN transactions t ON t.guid = s.tx_guid
             JOIN accounts a ON a.guid = s.account_guid
             WHERE s.reconcile_state = 'y'
               AND a.placeholder = 0
               AND a.account_type IN ({}, {})
               AND {}
               AND NOT EXISTS (
                   SELECT 1 FROM reconcile_batches b WHERE b.account_guid = s.account_guid
               )
             GROUP BY s.account_guid",
            utils::cash_like_types_sql(),
            utils::liability_types_sql(),
            utils::VISIBLE_TX_FILTER
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (account_guid, last) = row?;
            summaries
                .entry(account_guid.clone())
                .or_insert(ReconcileAccountSummary {
                    account_guid,
                    unreconciled_count: 0,
                    cleared_count: 0,
                    uncleared_total: 0.0,
                    last_reconciled_date: None,
                })
                .last_reconciled_date = last;
        }
    }
    let mut out: Vec<ReconcileAccountSummary> = summaries.into_values().collect();
    out.sort_by(|a, b| a.account_guid.cmp(&b.account_guid));
    Ok(out)
}

#[tauri::command]
pub async fn fetch_reconcile_info(
    state: State<'_, AppState>,
    account_guid: String,
    statement_date: String,
) -> Result<ReconcileInfo, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_reconcile_info_inner(conn, &account_guid, &statement_date)
    })
    .await
}

#[tauri::command]
pub async fn mark_reconcile_splits(
    state: State<'_, AppState>,
    input: MarkReconcileInput,
) -> Result<MarkReconcileResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        mark_reconcile_splits_inner(conn, input)
    })
    .await
}

#[tauri::command]
pub async fn finish_reconciliation(
    state: State<'_, AppState>,
    input: FinishReconcileInput,
) -> Result<FinishReconcileResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        finish_reconciliation_inner(conn, input)
    })
    .await
}

#[tauri::command]
pub async fn reopen_last_reconciliation(
    state: State<'_, AppState>,
    account_guid: String,
) -> Result<ReopenReconcileResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        reopen_last_reconciliation_inner(conn, &account_guid)
    })
    .await
}

#[tauri::command]
pub async fn fetch_reconcile_summaries(
    state: State<'_, AppState>,
) -> Result<Vec<ReconcileAccountSummary>, String> {
    run_blocking(state.db.clone(), |conn| {
        fetch_reconcile_summaries_inner(conn)
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
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur'),
                    ('broker', 'Broker', 'STOCK', 'root', 0, 'c-eur'),
                    ('ph', 'Parent', 'BANK', 'root', 1, 'c-eur')",
            [],
        )
        .unwrap();
        conn
    }

    fn add_split(
        conn: &Connection,
        tx: &str,
        date: &str,
        account: &str,
        value_num: i64,
        state: &str,
    ) {
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
            params![tx, date],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom,
                                 quantity_num, quantity_denom, reconcile_state, reconciled)
             VALUES (?1, ?2, ?3, ?4, 100, ?4, 100, ?5, ?6)",
            params![
                format!("s-{tx}"),
                tx,
                account,
                value_num,
                state,
                if state == "c" || state == "y" { 1 } else { 0 },
            ],
        )
        .unwrap();
    }

    fn seed(conn: &Connection) {
        // y: +100 (Jan), c: -30 (Feb), n: +50 (Mar).
        add_split(conn, "t1", "2024-01-10", "checking", 10_000, "y");
        add_split(conn, "t2", "2024-02-05", "checking", -3_000, "c");
        add_split(conn, "t3", "2024-03-01", "checking", 5_000, "n");
    }

    fn mark(
        conn: &Connection,
        split_guid: &str,
        state: &str,
        statement_date: &str,
    ) -> Result<MarkReconcileResult, AppError> {
        mark_reconcile_splits_inner(
            conn,
            MarkReconcileInput {
                account_guid: "checking".into(),
                statement_date: statement_date.into(),
                updates: vec![SplitStateUpdate {
                    split_guid: split_guid.into(),
                    state: state.into(),
                }],
            },
        )
    }

    fn finish(
        conn: &Connection,
        statement_date: &str,
        ending_balance: f64,
        updates: Vec<SplitStateUpdate>,
    ) -> Result<FinishReconcileResult, AppError> {
        finish_reconciliation_inner(
            conn,
            FinishReconcileInput {
                account_guid: "checking".into(),
                statement_date: statement_date.into(),
                ending_balance,
                updates,
            },
        )
    }

    #[test]
    fn info_returns_starting_and_candidates_capped_by_date() {
        let conn = setup();
        seed(&conn);
        let info = fetch_reconcile_info_inner(&conn, "checking", "2024-02-15").expect("info runs");
        assert_eq!(info.starting_balance, 100.0);
        assert_eq!(info.account_name, "Checking");
        // t3 (Mar) is past the statement date.
        assert_eq!(info.candidates.len(), 1);
        assert_eq!(info.candidates[0].split_guid, "s-t2");
        assert_eq!(info.candidates[0].amount, -30.0);
        assert_eq!(info.last_reconciled_date.as_deref(), Some("2024-01-10"));
        assert_eq!(info.last_reconciled_count, 1);
    }

    #[test]
    fn info_rejects_placeholders_investments_and_future_dates() {
        let conn = setup();
        seed(&conn);
        assert!(fetch_reconcile_info_inner(&conn, "ph", "2024-02-15").is_err());
        let err = fetch_reconcile_info_inner(&conn, "broker", "2024-02-15").unwrap_err();
        assert!(err.to_string().contains("cash-like"));
        assert!(fetch_reconcile_info_inner(&conn, "checking", "2999-01-01").is_err());
        assert!(fetch_reconcile_info_inner(&conn, "nope", "2024-02-15").is_err());
    }

    #[test]
    fn info_attaches_counter_accounts() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('groceries', 'Groceries', 'EXPENSE', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t9', '2024-03-02')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom,
                                 quantity_num, quantity_denom, reconcile_state, reconciled)
             VALUES ('s-t9a', 't9', 'checking', -1_000, 100, -1_000, 100, 'n', 0),
                    ('s-t9b', 't9', 'groceries', 1_000, 100, 1_000, 100, 'n', 0)",
            [],
        )
        .unwrap();
        let info = fetch_reconcile_info_inner(&conn, "checking", "2024-03-31").unwrap();
        let candidate = info.candidates.iter().find(|c| c.tx_guid == "t9").unwrap();
        assert_eq!(candidate.counter_accounts, vec!["Groceries".to_string()]);
    }

    #[test]
    fn mark_flips_n_to_c() {
        let conn = setup();
        seed(&conn);
        let out = mark(&conn, "s-t3", "c", "2024-03-31").expect("mark runs");
        assert_eq!(out.updated, 1);
        let state: String = conn
            .query_row(
                "SELECT reconcile_state FROM splits WHERE guid = 's-t3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "c");
    }

    #[test]
    fn mark_rejects_y_foreign_and_after_statement_splits() {
        let conn = setup();
        seed(&conn);
        add_split(&conn, "tx", "2024-02-06", "broker", 1_000, "n");
        let err = mark(&conn, "s-t1", "n", "2024-03-31").unwrap_err();
        assert!(err.to_string().contains("Reconciled"));
        let err = mark(&conn, "s-tx", "c", "2024-03-31").unwrap_err();
        assert!(err.to_string().contains("does not belong"));
        // t3 is dated after a February statement.
        let err = mark(&conn, "s-t3", "c", "2024-02-15").unwrap_err();
        assert!(err.to_string().contains("after the statement"));
    }

    #[test]
    fn finish_promotes_cleared_only_when_balanced() {
        let conn = setup();
        seed(&conn);
        // Starting 100 + cleared -30 = 70.
        let err = finish(&conn, "2024-02-15", 0.0, vec![]).unwrap_err();
        assert!(err.to_string().contains("Out of balance"));

        let out = finish(&conn, "2024-02-15", 70.0, vec![]).expect("finish runs");
        assert_eq!(out.reconciled_count, 1);
        let state: String = conn
            .query_row(
                "SELECT reconcile_state FROM splits WHERE guid = 's-t2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "y");
        // The untouched 'n' split stays new.
        let state: String = conn
            .query_row(
                "SELECT reconcile_state FROM splits WHERE guid = 's-t3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "n");
    }

    #[test]
    fn finish_applies_pending_updates_atomically() {
        let conn = setup();
        seed(&conn);
        // 100 - 30 + 50 = 120 with s-t3 cleared in the same call.
        let out = finish(
            &conn,
            "2024-03-31",
            120.0,
            vec![SplitStateUpdate {
                split_guid: "s-t3".into(),
                state: "c".into(),
            }],
        )
        .expect("finish runs");
        assert_eq!(out.reconciled_count, 2);
        let state: String = conn
            .query_row(
                "SELECT reconcile_state FROM splits WHERE guid = 's-t3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "y");
    }

    #[test]
    fn finish_rolls_back_pending_updates_when_out_of_balance() {
        let conn = setup();
        seed(&conn);
        let err = finish(
            &conn,
            "2024-03-31",
            0.0,
            vec![SplitStateUpdate {
                split_guid: "s-t3".into(),
                state: "c".into(),
            }],
        )
        .unwrap_err();
        assert!(err.to_string().contains("Out of balance"));
        // The update was applied inside the rolled-back transaction.
        let state: String = conn
            .query_row(
                "SELECT reconcile_state FROM splits WHERE guid = 's-t3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, "n");
    }

    #[test]
    fn reopen_reopens_a_retroactive_batch_not_the_date_latest() {
        let conn = setup();
        add_split(&conn, "t-jan", "2024-01-10", "checking", 10_000, "n");
        add_split(&conn, "t-feb", "2024-02-10", "checking", -3_000, "n");

        // February statement first: only the February split clears (-30).
        let out = finish(
            &conn,
            "2024-02-29",
            -30.0,
            vec![SplitStateUpdate {
                split_guid: "s-t-feb".into(),
                state: "c".into(),
            }],
        )
        .unwrap();
        assert_eq!(out.reconciled_count, 1);

        // Much later, the older January split is cleared on its own statement.
        let out = finish(
            &conn,
            "2024-01-31",
            100.0,
            vec![SplitStateUpdate {
                split_guid: "s-t-jan".into(),
                state: "c".into(),
            }],
        )
        .unwrap();
        assert_eq!(out.reconciled_count, 1);

        // Undo reopens the January batch (the last Finish), not the February
        // one merely because it has the latest transaction date.
        let out = reopen_last_reconciliation_inner(&conn, "checking").unwrap();
        assert_eq!(out.reopened_count, 1);
        assert_eq!(out.through_date, "2024-01-31");
        assert_eq!(out.from_date.as_deref(), Some("2024-02-29"));

        let jan: String = conn
            .query_row(
                "SELECT reconcile_state FROM splits WHERE guid = 's-t-jan'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let feb: String = conn
            .query_row(
                "SELECT reconcile_state FROM splits WHERE guid = 's-t-feb'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(jan, "c");
        assert_eq!(feb, "y");
    }

    #[test]
    fn summaries_use_the_finished_batch_through_date() {
        let conn = setup();
        add_split(
            &conn,
            "t-jan",
            "2024-01-10 09:30:00",
            "checking",
            10_000,
            "n",
        );
        finish(
            &conn,
            "2024-01-31",
            100.0,
            vec![SplitStateUpdate {
                split_guid: "s-t-jan".into(),
                state: "c".into(),
            }],
        )
        .unwrap();

        let summaries = fetch_reconcile_summaries_inner(&conn).unwrap();
        let checking = summaries
            .iter()
            .find(|s| s.account_guid == "checking")
            .unwrap();
        // Batch through date, not the timed transaction date.
        assert_eq!(checking.last_reconciled_date.as_deref(), Some("2024-01-31"));
    }

    #[test]
    fn reopen_returns_only_the_last_batch_to_cleared() {
        let conn = setup();
        seed(&conn);
        add_split(&conn, "t4", "2024-03-15", "checking", 2_000, "y");
        let out = reopen_last_reconciliation_inner(&conn, "checking").expect("reopen runs");
        assert_eq!(out.reopened_count, 1);
        assert_eq!(out.from_date.as_deref(), Some("2024-01-10"));
        assert_eq!(out.through_date, "2024-03-15");
        let reopened: String = conn
            .query_row(
                "SELECT reconcile_state FROM splits WHERE guid = 's-t4'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reopened, "c");
        let kept: String = conn
            .query_row(
                "SELECT reconcile_state FROM splits WHERE guid = 's-t1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kept, "y");
    }

    #[test]
    fn reopen_rejects_accounts_without_reconciliations() {
        let conn = setup();
        seed(&conn);
        let err = reopen_last_reconciliation_inner(&conn, "checking")
            .and_then(|_| reopen_last_reconciliation_inner(&conn, "checking"))
            .unwrap_err();
        assert!(err.to_string().contains("Nothing is reconciled"));
    }

    #[test]
    fn summaries_report_counts_and_last_reconciled() {
        let conn = setup();
        seed(&conn);
        add_split(&conn, "t4", "2024-03-15", "checking", 2_000, "y");
        let rows = fetch_reconcile_summaries_inner(&conn).expect("summaries run");
        let checking = rows.iter().find(|r| r.account_guid == "checking").unwrap();
        assert_eq!(checking.unreconciled_count, 1);
        assert_eq!(checking.cleared_count, 1);
        assert_eq!(checking.uncleared_total, 50.0);
        assert_eq!(checking.last_reconciled_date.as_deref(), Some("2024-03-15"));
        // Investment accounts are never reconciled.
        assert!(rows.iter().all(|r| r.account_guid != "broker"));
    }

    #[test]
    fn credit_normal_amounts_negate_in_starting_balance() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('card', 'Card', 'CREDIT', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        // A 40 spend on a credit card stores +40 raw but reads as -40.
        add_split(&conn, "tc", "2024-01-10", "card", 4_000, "y");
        let info = fetch_reconcile_info_inner(&conn, "card", "2024-02-01").expect("info runs");
        assert_eq!(info.starting_balance, -40.0);
    }

    #[test]
    fn reconcilable_types_match_utils_sets() {
        // `RECONCILABLE_TYPES` must stay the union of `CASH_LIKE_TYPES` and
        // `LIABILITY_TYPES` (see also `isReconcilable` in
        // `src/lib/features/reconcile/math.ts`).
        use crate::utils::{CASH_LIKE_TYPES, LIABILITY_TYPES};
        let mut actual = RECONCILABLE_TYPES.to_vec();
        actual.sort_unstable();
        let mut expected: Vec<&str> = CASH_LIKE_TYPES
            .iter()
            .chain(LIABILITY_TYPES.iter())
            .copied()
            .collect();
        expected.sort_unstable();
        assert_eq!(actual, expected);
        // Keep in sync with `RECONCILE_EPS` in `math.ts`.
        assert!((BALANCE_EPS - 0.005).abs() < 1e-12);
    }
}
