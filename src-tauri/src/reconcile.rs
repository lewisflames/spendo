use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::history;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileCandidate {
    pub split_guid: String,
    pub tx_guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub memo: Option<String>,
    /// Signed display amount (credit-normal accounts negated).
    pub amount: f64,
    /// Normalized `'n'`/`'c'` (source `NULL` reads as `'n'`).
    pub reconcile_state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileInfo {
    pub account_guid: String,
    pub account_name: String,
    pub account_type: String,
    pub commodity: Option<String>,
    pub statement_date: String,
    /// Sum of `'y'` splits at or before the statement date.
    pub starting_balance: f64,
    /// Unreconciled (`'n'`/`'c'`) splits at or before the statement date,
    /// oldest first.
    pub candidates: Vec<ReconcileCandidate>,
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
    account_type: String,
    commodity: Option<String>,
}

/// Shared `YYYY-MM-DD` validation for statement dates: well-formed, past or
/// today (future statements would leak the visibility window).
fn validate_statement_date(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Statement date is required".to_string()));
    }
    let parsed = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .map_err(|_| AppError::Validation(format!("Invalid date (expected YYYY-MM-DD): {trimmed}")))?;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    if parsed.format("%Y-%m-%d").to_string() > today {
        return Err(AppError::Validation("Statement date cannot be in the future".to_string()));
    }
    Ok(parsed.format("%Y-%m-%d").to_string())
}

fn load_reconcilable_account(conn: &Connection, guid: &str) -> Result<ReconcilableAccount, AppError> {
    let row: Option<(String, String, i64, Option<String>)> = conn
        .query_row(
            "SELECT a.name, a.account_type, a.placeholder, c.mnemonic
             FROM accounts a LEFT JOIN commodities c ON c.guid = a.commodity_guid
             WHERE a.guid = ?1",
            params![guid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((name, account_type, placeholder, commodity)) = row else {
        return Err(AppError::NotFound("Account not found — it may have been removed".to_string()));
    };
    if account_type == "ROOT" {
        return Err(AppError::Validation("The root account cannot be reconciled".to_string()));
    }
    if placeholder != 0 {
        return Err(AppError::Validation("Placeholder accounts cannot be reconciled".to_string()));
    }
    if !RECONCILABLE_TYPES.contains(&account_type.as_str()) {
        return Err(AppError::Validation(format!(
            "Only cash-like accounts can be reconciled (not {account_type})"
        )));
    }
    Ok(ReconcilableAccount { name, account_type, commodity })
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
    let candidates = stmt
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
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ReconcileInfo {
        account_guid: account_guid.to_string(),
        account_name: account.name,
        account_type: account.account_type,
        commodity: account.commodity,
        statement_date,
        starting_balance,
        candidates,
    })
}

fn mark_reconcile_splits_inner(
    conn: &Connection,
    input: MarkReconcileInput,
) -> Result<MarkReconcileResult, AppError> {
    let account = load_reconcilable_account(conn, &input.account_guid)?;
    if input.updates.is_empty() {
        return Err(AppError::Validation("No changes to save".to_string()));
    }
    if input.updates.len() > 2000 {
        return Err(AppError::Validation("Too many changes at once (max 2000)".to_string()));
    }
    for u in &input.updates {
        if u.state != "n" && u.state != "c" {
            return Err(AppError::Validation(format!(
                "Invalid reconcile state '{}' — expected 'n' or 'c'",
                u.state
            )));
        }
        if u.split_guid.trim().is_empty() {
            return Err(AppError::Validation("Split id is required".to_string()));
        }
    }
    // De-dupe by split guid; last write wins for repeated entries.
    let mut deduped: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for u in input.updates {
        deduped.insert(u.split_guid, u.state);
    }
    let guids: Vec<String> = deduped.keys().cloned().collect();

    let select_sql = format!(
        "SELECT guid, account_guid, tx_guid, reconcile_state FROM splits WHERE guid IN ({})",
        sql::in_clause(guids.len())
    );
    let mut select_stmt = conn.prepare(&select_sql)?;
    let select_refs = sql::string_param_refs(&guids);
    let rows: Vec<(String, String, String, Option<String>)> = select_stmt
        .query_map(select_refs.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() != guids.len() {
        return Err(AppError::NotFound("A split was not found — it may have been removed".to_string()));
    }
    for (guid, owner, _, current) in &rows {
        if owner != &input.account_guid {
            return Err(AppError::Validation(format!(
                "Split {guid} does not belong to this account"
            )));
        }
        if current.as_deref() == Some("y") {
            return Err(AppError::Validation(
                "Reconciled splits cannot be changed here — un-reconcile them first".to_string(),
            ));
        }
    }
    // No-op when every target already has the requested state.
    let pending: Vec<(String, String)> = rows
        .iter()
        .filter_map(|(guid, _, _, current)| {
            let want = deduped.get(guid).cloned().unwrap_or_default();
            let have = current.clone().unwrap_or_else(|| "n".to_string());
            (have != want).then(|| (guid.clone(), want))
        })
        .collect();
    if pending.is_empty() {
        return Ok(MarkReconcileResult { updated: 0 });
    }
    let tx_guids: HashSet<String> = rows.iter().map(|(_, _, tx, _)| tx.clone()).collect();
    let marked_cleared = pending.iter().filter(|(_, s)| s == "c").count() as i64;
    let marked_new = pending.len() as i64 - marked_cleared;
    let label = format!(
        "Reconcile {} — marked {marked_cleared} cleared",
        account.name.trim()
    );
    let detail = serde_json::json!({
        "accountGuid": input.account_guid,
        "markedCleared": marked_cleared,
        "markedNew": marked_new,
    })
    .to_string();

    let updated = pending.len() as i64;
    with_tx_err(conn, || -> Result<(), AppError> {
        let mut snapshots = Vec::with_capacity(pending.len());
        for (guid, _) in &pending {
            snapshots.push(history::Snapshot {
                table_name: "splits",
                row_guid: guid.clone(),
                before_json: history::snapshot_row(conn, "splits", guid)?,
            });
        }
        for (guid, want) in &pending {
            let reconciled = if want == "c" { 1 } else { 0 };
            conn.execute(
                "UPDATE splits SET reconcile_state = ?1, reconciled = ?2 WHERE guid = ?3",
                params![want, reconciled, guid],
            )?;
        }
        history::insert_change_entry(
            conn,
            "reconcile",
            &label,
            "manual",
            tx_guids.len() as i64,
            1,
            Some(&detail),
            &snapshots,
        )?;
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
        return Err(AppError::Validation("Ending balance must be a number".to_string()));
    }
    if input.ending_balance.abs() > 1_000_000_000.0 {
        return Err(AppError::Validation("Ending balance is too large".to_string()));
    }
    let statement_date = validate_statement_date(&input.statement_date)?;
    let account = load_reconcilable_account(conn, &input.account_guid)?;

    let starting_balance = sum_by_state(conn, &input.account_guid, "y", &statement_date)?;
    let cleared_total = sum_by_state(conn, &input.account_guid, "c", &statement_date)?;
    let diff = input.ending_balance - (starting_balance + cleared_total);
    if diff.abs() > BALANCE_EPS {
        return Err(AppError::Validation(format!(
            "Out of balance by {diff:.2} — check off cleared transactions until the difference is zero"
        )));
    }
    let guids = cleared_split_guids(conn, &input.account_guid, &statement_date)?;
    if guids.is_empty() {
        return Err(AppError::Validation("Nothing cleared to reconcile".to_string()));
    }
    let tx_count: i64 = {
        let sql = format!(
            "SELECT COUNT(DISTINCT tx_guid) FROM splits WHERE guid IN ({})",
            sql::in_clause(guids.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let refs = sql::string_param_refs(&guids);
        stmt.query_row(refs.as_slice(), |row| row.get(0))?
    };
    let label = format!(
        "Reconciled {} to {:.2}",
        account.name.trim(),
        input.ending_balance
    );
    let detail = serde_json::json!({
        "accountGuid": input.account_guid,
        "statementDate": statement_date,
        "endingBalance": input.ending_balance,
        "startingBalance": starting_balance,
        "clearedCount": guids.len(),
    })
    .to_string();
    let reconciled_count = guids.len() as i64;

    with_tx_err(conn, || -> Result<(), AppError> {
        let mut snapshots = Vec::with_capacity(guids.len());
        for guid in &guids {
            snapshots.push(history::Snapshot {
                table_name: "splits",
                row_guid: guid.clone(),
                before_json: history::snapshot_row(conn, "splits", guid)?,
            });
        }
        for guid in &guids {
            conn.execute(
                "UPDATE splits SET reconcile_state = 'y', reconciled = 1 WHERE guid = ?1",
                params![guid],
            )?;
        }
        history::insert_change_entry(
            conn,
            "reconcile",
            &label,
            "manual",
            tx_count,
            1,
            Some(&detail),
            &snapshots,
        )?;
        Ok(())
    })?;

    Ok(FinishReconcileResult {
        reconciled_count,
        starting_balance,
        ending_balance: input.ending_balance,
    })
}

#[tauri::command]
pub async fn fetch_reconcile_info(
    state: State<'_, AppState>,
    account_guid: String,
    statement_date: String,
) -> Result<ReconcileInfo, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_reconcile_info_inner(conn, &account_guid, &statement_date).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn mark_reconcile_splits(
    state: State<'_, AppState>,
    input: MarkReconcileInput,
) -> Result<MarkReconcileResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        mark_reconcile_splits_inner(conn, input).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn finish_reconciliation(
    state: State<'_, AppState>,
    input: FinishReconcileInput,
) -> Result<FinishReconcileResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        finish_reconciliation_inner(conn, input).map_err(String::from)
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

    #[test]
    fn info_returns_starting_and_candidates_capped_by_date() {
        let conn = setup();
        seed(&conn);
        let info =
            fetch_reconcile_info_inner(&conn, "checking", "2024-02-15").expect("info runs");
        assert_eq!(info.starting_balance, 100.0);
        assert_eq!(info.account_name, "Checking");
        // t3 (Mar) is past the statement date.
        assert_eq!(info.candidates.len(), 1);
        assert_eq!(info.candidates[0].split_guid, "s-t2");
        assert_eq!(info.candidates[0].amount, -30.0);
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
    fn mark_flips_n_to_c_and_logs_history() {
        let conn = setup();
        seed(&conn);
        let out = mark_reconcile_splits_inner(
            &conn,
            MarkReconcileInput {
                account_guid: "checking".into(),
                updates: vec![SplitStateUpdate {
                    split_guid: "s-t3".into(),
                    state: "c".into(),
                }],
            },
        )
        .expect("mark runs");
        assert_eq!(out.updated, 1);
        let state: String = conn
            .query_row("SELECT reconcile_state FROM splits WHERE guid = 's-t3'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "c");
        let kinds: Vec<String> = {
            let mut stmt = conn.prepare("SELECT kind FROM change_log").unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(kinds, vec!["reconcile"]);
    }

    #[test]
    fn mark_rejects_y_and_foreign_splits() {
        let conn = setup();
        seed(&conn);
        add_split(&conn, "tx", "2024-02-06", "broker", 1_000, "n");
        let err = mark_reconcile_splits_inner(
            &conn,
            MarkReconcileInput {
                account_guid: "checking".into(),
                updates: vec![SplitStateUpdate {
                    split_guid: "s-t1".into(),
                    state: "n".into(),
                }],
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("Reconciled"));
        let err = mark_reconcile_splits_inner(
            &conn,
            MarkReconcileInput {
                account_guid: "checking".into(),
                updates: vec![SplitStateUpdate {
                    split_guid: "s-tx".into(),
                    state: "c".into(),
                }],
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not belong"));
    }

    #[test]
    fn finish_promotes_cleared_only_when_balanced() {
        let conn = setup();
        seed(&conn);
        // Starting 100 + cleared -30 = 70.
        let err = finish_reconciliation_inner(
            &conn,
            FinishReconcileInput {
                account_guid: "checking".into(),
                statement_date: "2024-02-15".into(),
                ending_balance: 0.0,
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("Out of balance"));

        let out = finish_reconciliation_inner(
            &conn,
            FinishReconcileInput {
                account_guid: "checking".into(),
                statement_date: "2024-02-15".into(),
                ending_balance: 70.0,
            },
        )
        .expect("finish runs");
        assert_eq!(out.reconciled_count, 1);
        let state: String = conn
            .query_row("SELECT reconcile_state FROM splits WHERE guid = 's-t2'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "y");
        // The untouched 'n' split stays new.
        let state: String = conn
            .query_row("SELECT reconcile_state FROM splits WHERE guid = 's-t3'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "n");
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
}
