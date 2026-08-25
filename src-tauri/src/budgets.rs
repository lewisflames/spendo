use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::utils;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub account_type: String,
    pub period_type: String,
    pub amount: f64,
    pub rollover: bool,
    pub rollover_start: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetDateRange {
    pub earliest_date: Option<String>,
    pub latest_date: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetActualRow {
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub account_type: String,
    pub budget_amount: f64,
    pub actual_amount: f64,
    pub remaining: f64,
    pub percentage: f64,
    pub rollover_balance: f64,
}

fn row_to_budget(row: &rusqlite::Row) -> rusqlite::Result<Budget> {
    Ok(Budget {
        guid: row.get(0)?,
        account_guid: row.get(1)?,
        account_name: row.get(2)?,
        account_description: row.get(3)?,
        account_type: row.get(4)?,
        period_type: row.get(5)?,
        amount: row.get(6)?,
        rollover: row.get::<_, i64>(7)? != 0,
        rollover_start: row.get(8)?,
        notes: row.get(9)?,
        created_at: row.get(10)?,
    })
}

pub fn list_budgets_inner(conn: &Connection) -> Result<Vec<Budget>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT b.guid, b.account_guid, a.name, a.description, a.account_type,
			        b.period_type, b.amount, b.rollover, b.rollover_start, b.notes, b.created_at
			 FROM budgets b
			 JOIN accounts a ON a.guid = b.account_guid
			 WHERE a.account_type = 'EXPENSE'
			 ORDER BY a.name",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], row_to_budget)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn set_budget_inner(
    conn: &Connection,
    account_guid: &str,
    amount: f64,
    period_type: &str,
    rollover: bool,
    rollover_start: Option<&str>,
    notes: Option<&str>,
) -> Result<Budget, String> {
    if !matches!(period_type, "MONTHLY" | "YEARLY") {
        return Err("Period type must be MONTHLY or YEARLY".to_string());
    }
    if !amount.is_finite() {
        return Err("Budget amount must be finite".to_string());
    }
    if let Some(rs) = rollover_start {
        // Slicing `rollover_start[..7]` elsewhere requires a well-formed date.
        utils::validate_date(rs)?;
    }
    let acct_type: String = conn
        .query_row(
            "SELECT account_type FROM accounts WHERE guid = ?1",
            [account_guid],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Account not found: {account_guid}"))?;
    if acct_type != "EXPENSE" {
        return Err("Only expense accounts can have budgets".to_string());
    }

    // Atomic check-then-write: SELECT existing GUID, UPDATE or INSERT, then
    // re-SELECT the stored row — a failure mid-way must not half-update.
    conn.execute_batch("BEGIN")
        .map_err(|e| e.to_string())?;
    let write_result = (|| -> Result<String, String> {
        let existing: Option<String> = conn
            .query_row(
                "SELECT guid FROM budgets WHERE account_guid = ?1",
                [account_guid],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        Ok(if let Some(g) = existing {
            conn.execute(
                "UPDATE budgets SET amount = ?1, period_type = ?2, rollover = ?3, rollover_start = ?4, notes = ?5
			     WHERE guid = ?6",
                rusqlite::params![amount, period_type, rollover as i64, rollover_start, notes, g],
            )
            .map_err(|e| e.to_string())?;
            g
        } else {
            let g = Uuid::new_v4().to_string();
            conn.execute(
			    "INSERT INTO budgets (guid, account_guid, period_type, amount, rollover, rollover_start, notes, created_at)
			     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
			    rusqlite::params![g, account_guid, period_type, amount, rollover as i64, rollover_start, notes, utils::iso_now()],
		    )
		    .map_err(|e| e.to_string())?;
            g
        })
    })();

    match write_result {
        Ok(guid) => {
            conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            read_budget(conn, &guid)
        }
        Err(e) => {
            conn.execute_batch("ROLLBACK").ok();
            Err(e)
        }
    }
}

fn budget_select_sql() -> &'static str {
    "SELECT b.guid, b.account_guid, a.name, a.description, a.account_type,
	        b.period_type, b.amount, b.rollover, b.rollover_start, b.notes, b.created_at
	 FROM budgets b
	 JOIN accounts a ON a.guid = b.account_guid
	 WHERE b.guid = ?1"
}

fn read_budget(conn: &Connection, guid: &str) -> Result<Budget, String> {
    let mut stmt = conn.prepare(budget_select_sql()).map_err(|e| e.to_string())?;
    stmt.query_row([guid], row_to_budget)
        .map_err(|e| e.to_string())
}

pub fn delete_budget_inner(conn: &Connection, guid: &str) -> Result<(), String> {
    conn.execute("DELETE FROM budgets WHERE guid = ?1", [guid])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn months_between(start_ym: &str, end_ym: &str) -> Result<i32, String> {
    let start_n = utils::month_number(start_ym)?;
    let end_n = utils::month_number(end_ym)?;
    Ok((end_n - start_n) as i32)
}

fn period_range(year: i32, month: i32, period: &str) -> (String, String) {
    if period == "YEARLY" {
        (
            format!("{:04}-01-01", year),
            format!("{:04}-01-01", year + 1),
        )
    } else {
        utils::month_range(year, month)
    }
}

fn effective_budget_expr(period: &str) -> &'static str {
    if period == "YEARLY" {
        "CASE WHEN b.period_type = 'YEARLY' THEN b.amount ELSE b.amount * 12.0 END"
    } else {
        "CASE WHEN b.period_type = 'YEARLY' THEN b.amount / 12.0 ELSE b.amount END"
    }
}

fn compute_cumulative_carryovers(
    conn: &Connection,
    period_start: &str,
) -> Result<HashMap<String, f64>, String> {
    let sql = format!(
        "WITH RECURSIVE
         descendants(budget_account_guid, descendant_guid) AS (
             SELECT b.account_guid, b.account_guid
             FROM budgets b
             JOIN accounts a ON a.guid = b.account_guid
             WHERE b.rollover = 1 AND a.account_type = 'EXPENSE'
             UNION ALL
             SELECT d.budget_account_guid, a.guid
             FROM descendants d
             JOIN accounts a ON a.parent_guid = d.descendant_guid
         ),
         b AS (
             SELECT b.account_guid,
                    CASE WHEN b.period_type = 'YEARLY' THEN b.amount / 12.0 ELSE b.amount END as monthly_budget,
                    COALESCE(b.rollover_start, date(b.created_at, 'start of month')) as budget_start
             FROM budgets b
             WHERE b.rollover = 1
         )
         SELECT b.account_guid, b.monthly_budget, b.budget_start, c.mnemonic,
                COALESCE(SUM(
                    CASE WHEN t.guid IS NOT NULL THEN {} ELSE 0.0 END
                ), 0) as total_actual
         FROM b
         LEFT JOIN descendants d ON d.budget_account_guid = b.account_guid
         LEFT JOIN splits s ON s.account_guid = d.descendant_guid
         LEFT JOIN accounts a2 ON a2.guid = s.account_guid
         LEFT JOIN commodities c ON c.guid = a2.commodity_guid
         LEFT JOIN transactions t ON t.guid = s.tx_guid
              AND t.post_date >= b.budget_start AND t.post_date < ?1
         GROUP BY b.account_guid, b.monthly_budget, b.budget_start, c.mnemonic",
        utils::signed_amount_expr(utils::RAW_AMOUNT, "a2.account_type")
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let rows: Vec<(String, f64, String, Option<String>, f64)> = stmt
        .query_map(rusqlite::params![period_start], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let report_currency = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_map = fx::get_fx_map(conn, Some(period_start))?;

    let period_ym = &period_start[..7];
    let mut carryovers: HashMap<String, f64> = HashMap::new();
    for (account_guid, monthly_budget, budget_start, commodity, total_actual) in rows {
        let converted_actual = match commodity {
            Some(mn) => fx::convert(total_actual, &mn, &report_currency, &fx_map),
            None => total_actual,
        };
        let start_ym = &budget_start[..7];
        // `rollover_start` is validated at write time; a malformed value here
        // means DB tampering/corruption and must surface, not silently
        // corrupt the carryover via a bogus month count.
        let num_months = months_between(start_ym, period_ym)?;
        let carryover = if num_months <= 0 {
            0.0
        } else {
            monthly_budget * num_months as f64 - converted_actual
        };
        *carryovers.entry(account_guid).or_insert(0.0) += carryover;
    }

    Ok(carryovers)
}

pub fn get_budget_vs_actual_inner(
    conn: &Connection,
    year: i32,
    month: i32,
    period: &str,
) -> Result<Vec<BudgetActualRow>, String> {
    let (period_start, period_end) = period_range(year, month, period);
    let effective_budget = effective_budget_expr(period);

    let mut stmt = conn
        .prepare(&format!(
            "WITH RECURSIVE
			 descendants(budget_account_guid, descendant_guid) AS (
				 SELECT a2.guid, a2.guid FROM accounts a2
				 WHERE a2.guid IN (SELECT account_guid FROM budgets)
				 AND a2.account_type = 'EXPENSE'
				 UNION ALL
				 SELECT d.budget_account_guid, a.guid
				 FROM descendants d
				 JOIN accounts a ON a.parent_guid = d.descendant_guid
			 )
			 SELECT b.account_guid, a.name, a.description, a.account_type, b.period_type,
			        {} as effective_budget,
			        c.mnemonic,
			        COALESCE(SUM(ps.value), 0) as actual_amount
			  FROM budgets b
			  JOIN accounts a ON a.guid = b.account_guid
			  LEFT JOIN descendants d ON d.budget_account_guid = b.account_guid
			  LEFT JOIN (
			      SELECT s.account_guid, a2.commodity_guid,
			             {} as value
			      FROM splits s
			      JOIN accounts a2 ON a2.guid = s.account_guid
			      JOIN transactions t ON t.guid = s.tx_guid
			      WHERE t.post_date >= ?1 AND t.post_date < ?2
			  ) ps ON ps.account_guid = d.descendant_guid
			  LEFT JOIN commodities c ON c.guid = ps.commodity_guid
			  WHERE a.account_type = 'EXPENSE'
			  GROUP BY b.guid, c.mnemonic
			  ORDER BY a.name",
            effective_budget,
            utils::RAW_AMOUNT
        ))
        .map_err(|e| e.to_string())?;

    /// (guid, name, description, type, period_type, budget, commodity, actual)
    type RawBudgetRow = (
        String,
        String,
        Option<String>,
        String,
        String,
        f64,
        Option<String>,
        f64,
    );
    let raw_rows: Vec<RawBudgetRow> = stmt
        .query_map(rusqlite::params![&period_start, &period_end], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, f64>(7)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let report_currency = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_map = fx::get_fx_map(conn, Some(&period_end))?;

    // Aggregate actuals per budget, converting each currency to the report currency.
    #[derive(Clone)]
    struct BudgetAgg {
        account_guid: String,
        account_name: String,
        account_description: Option<String>,
        account_type: String,
        budget_amount: f64,
        actual_amount: f64,
    }

    let mut aggs: HashMap<String, BudgetAgg> = HashMap::new();
    let mut agg_order: Vec<String> = vec![];
    for (
        account_guid,
        account_name,
        account_description,
        account_type,
        _period_type,
        budget_amount,
        commodity,
        actual_amount,
    ) in raw_rows
    {
        let converted = match commodity {
            Some(mn) => fx::convert(actual_amount, &mn, &report_currency, &fx_map),
            None => actual_amount,
        };
        match aggs.get_mut(&account_guid) {
            Some(agg) => agg.actual_amount += converted,
            None => {
                agg_order.push(account_guid.clone());
                aggs.insert(
                    account_guid.clone(),
                    BudgetAgg {
                        account_guid,
                        account_name,
                        account_description,
                        account_type,
                        budget_amount,
                        actual_amount: converted,
                    },
                );
            }
        }
    }

    let mut rows: Vec<BudgetActualRow> = agg_order
        .iter()
        .map(|guid| {
            let agg = &aggs[guid];
            let remaining = agg.budget_amount - agg.actual_amount;
            let percentage = if agg.budget_amount != 0.0 {
                (agg.actual_amount.max(0.0) / agg.budget_amount) * 100.0
            } else {
                0.0
            };
            BudgetActualRow {
                account_guid: agg.account_guid.clone(),
                account_name: agg.account_name.clone(),
                account_description: agg.account_description.clone(),
                account_type: agg.account_type.clone(),
                budget_amount: agg.budget_amount,
                actual_amount: agg.actual_amount,
                remaining,
                percentage,
                rollover_balance: 0.0,
            }
        })
        .collect();

    let carryovers = compute_cumulative_carryovers(conn, &period_start)?;
    for row in &mut rows {
        if let Some(&carryover) = carryovers.get(&row.account_guid) {
            row.rollover_balance = carryover;
            let effective_total = row.budget_amount + row.rollover_balance;
            row.remaining = effective_total - row.actual_amount;
            row.percentage = if effective_total != 0.0 {
                (row.actual_amount.max(0.0) / effective_total) * 100.0
            } else {
                0.0
            };
        }
    }

    Ok(rows)
}

/// Book-wide earliest visible transaction date (`YYYY-MM-DD`). Shared by the
/// global query and the budget date range so both answers can never diverge.
fn earliest_visible_tx_date(conn: &Connection) -> Result<Option<String>, String> {
    conn.query_row(
        &format!(
            "SELECT substr(MIN(t.post_date), 1, 10) FROM transactions t WHERE {}",
            utils::VISIBLE_TX_FILTER
        ),
        [],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

pub fn get_global_earliest_transaction_date_inner(
    conn: &Connection,
) -> Result<Option<String>, String> {
    earliest_visible_tx_date(conn)
}

pub fn get_budget_date_range_inner(conn: &Connection) -> Result<BudgetDateRange, String> {
    let earliest = earliest_visible_tx_date(conn)?;

    let latest: Option<String> = conn
        .query_row(
            &format!(
                "SELECT substr(MAX(t.post_date), 1, 10) FROM transactions t WHERE {}",
                utils::VISIBLE_TX_FILTER
            ),
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let latest_date = latest.map(|l| if l >= today { l } else { today.clone() });

    Ok(BudgetDateRange {
        earliest_date: earliest,
        latest_date,
    })
}

pub fn get_earliest_transaction_date_inner(
    conn: &Connection,
    account_guid: &str,
) -> Result<Option<String>, String> {
    let date: Option<String> = conn
        .query_row(
            &format!(
                "WITH RECURSIVE
                 descendants(descendant_guid) AS (
                     SELECT ?1
                     UNION ALL
                     SELECT a.guid
                     FROM descendants d
                     JOIN accounts a ON a.parent_guid = d.descendant_guid
                 )
                 SELECT MIN(t.post_date)
                 FROM transactions t
                 JOIN splits s ON s.tx_guid = t.guid
                 WHERE s.account_guid IN (SELECT descendant_guid FROM descendants)
                 AND {}",
                utils::VISIBLE_TX_FILTER
            ),
            [account_guid],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(date)
}

#[tauri::command]
pub async fn list_budgets(state: State<'_, AppState>) -> Result<Vec<Budget>, String> {
    run_blocking(state.db.clone(), list_budgets_inner).await
}

#[tauri::command]
pub async fn set_budget(
    state: State<'_, AppState>,
    account_guid: String,
    amount: f64,
    period_type: String,
    rollover: bool,
    rollover_start: Option<String>,
    notes: Option<String>,
) -> Result<Budget, String> {
    run_blocking(state.db.clone(), move |conn| {
        set_budget_inner(
            conn,
            &account_guid,
            amount,
            &period_type,
            rollover,
            rollover_start.as_deref(),
            notes.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn delete_budget(state: State<'_, AppState>, guid: String) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| delete_budget_inner(conn, &guid)).await
}

#[tauri::command]
pub async fn get_budget_vs_actual(
    state: State<'_, AppState>,
    year: i32,
    month: i32,
    period: String,
) -> Result<Vec<BudgetActualRow>, String> {
    run_blocking(state.db.clone(), move |conn| {
        get_budget_vs_actual_inner(conn, year, month, &period)
    })
    .await
}

#[tauri::command]
pub async fn get_global_earliest_transaction_date(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    run_blocking(state.db.clone(), move |conn| {
        get_global_earliest_transaction_date_inner(conn)
    })
    .await
}

#[tauri::command]
pub async fn get_earliest_transaction_date(
    state: State<'_, AppState>,
    account_guid: String,
) -> Result<Option<String>, String> {
    run_blocking(state.db.clone(), move |conn| {
        get_earliest_transaction_date_inner(conn, &account_guid)
    })
    .await
}

#[tauri::command]
pub async fn get_budget_date_range(state: State<'_, AppState>) -> Result<BudgetDateRange, String> {
    run_blocking(state.db.clone(), get_budget_date_range_inner).await
}
