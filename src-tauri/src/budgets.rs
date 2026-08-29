use chrono::Datelike;
use std::cmp::Ordering;
use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::utils;

/// Per-account budget definition. `amount` is the monthly default target that
/// applies to every month; only overridden months get a row in
/// `budget_amounts`. `rollover`/`rollover_start` are dormant leftovers of the
/// pre-entries schema.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub account_type: String,
    pub amount: f64,
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
    pub has_budget: bool,
    pub budget_amount: f64,
    pub actual_amount: f64,
    pub remaining: f64,
    pub percentage: f64,
    pub rollover_balance: f64,
}

/// One `budget_amounts` row with the actuals/rollover math applied to its month.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetEntry {
    pub year: i32,
    pub month: i32,
    pub amount: f64,
    pub actual_amount: f64,
    pub remaining: f64,
    pub percentage: f64,
    pub rollover_balance: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetPeriodSummary {
    pub budget_amount: f64,
    pub actual_amount: f64,
    pub remaining: f64,
    pub percentage: f64,
    pub rollover_balance: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetDetail {
    pub budget: Budget,
    pub entries: Vec<BudgetEntry>,
    pub current_period: BudgetPeriodSummary,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

fn row_to_budget(row: &rusqlite::Row) -> rusqlite::Result<Budget> {
    Ok(Budget {
        guid: row.get(0)?,
        account_guid: row.get(1)?,
        account_name: row.get(2)?,
        account_description: row.get(3)?,
        account_type: row.get(4)?,
        amount: row.get(5)?,
        notes: row.get(6)?,
        created_at: row.get(7)?,
    })
}

pub fn list_budgets_inner(conn: &Connection) -> Result<Vec<Budget>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT b.guid, b.account_guid, a.name, a.description, a.account_type,
			        b.amount, b.notes, b.created_at
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
    notes: Option<&str>,
    amount: f64,
) -> Result<Budget, String> {
    if !amount.is_finite() || amount <= 0.0 {
        return Err("Budget amount must be positive and finite".to_string());
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

    let existing: Option<String> = conn
        .query_row(
            "SELECT guid FROM budgets WHERE account_guid = ?1",
            [account_guid],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let guid = if let Some(g) = existing {
        conn.execute(
            "UPDATE budgets SET notes = ?1, amount = ?2 WHERE guid = ?3",
            rusqlite::params![notes, amount, g],
        )
        .map_err(|e| e.to_string())?;
        g
    } else {
        let g = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, period_type, amount, rollover, notes, created_at)
			 VALUES (?1, ?2, 'MONTHLY', ?3, 0, ?4, ?5)",
            rusqlite::params![g, account_guid, amount, notes, utils::iso_now()],
        )
        .map_err(|e| e.to_string())?;
        g
    };
    read_budget(conn, &guid)
}

/// Upsert a single month's override amount.
pub fn set_budget_amount_inner(
    conn: &Connection,
    account_guid: &str,
    year: i32,
    month: i32,
    amount: f64,
) -> Result<(), String> {
    let budget = read_budget_by_account(conn, account_guid)?;
    if !amount.is_finite() || amount <= 0.0 {
        return Err("Budget amount must be positive and finite".to_string());
    }
    if !(1..=12).contains(&month) {
        return Err("Budget months must be between 1 and 12".to_string());
    }
    conn.execute(
        "INSERT INTO budget_amounts (budget_guid, year, month, amount)
		 VALUES (?1, ?2, ?3, ?4)
		 ON CONFLICT (budget_guid, year, month) DO UPDATE SET amount = ?4",
        rusqlite::params![budget.guid, year, month, amount],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Delete a single month's override so it falls back to the default target.
pub fn reset_budget_amount_inner(
    conn: &Connection,
    account_guid: &str,
    year: i32,
    month: i32,
) -> Result<(), String> {
    let budget = read_budget_by_account(conn, account_guid)?;
    if !(1..=12).contains(&month) {
        return Err("Budget months must be between 1 and 12".to_string());
    }
    conn.execute(
        "DELETE FROM budget_amounts WHERE budget_guid = ?1 AND year = ?2 AND month = ?3",
        rusqlite::params![budget.guid, year, month],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_budget_inner(conn: &Connection, guid: &str) -> Result<(), String> {
    conn.execute("DELETE FROM budgets WHERE guid = ?1", [guid])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn budget_select_sql() -> &'static str {
    "SELECT b.guid, b.account_guid, a.name, a.description, a.account_type,
	        b.amount, b.notes, b.created_at
	 FROM budgets b
	 JOIN accounts a ON a.guid = b.account_guid
	 WHERE b.guid = ?1"
}

fn read_budget(conn: &Connection, guid: &str) -> Result<Budget, String> {
    let mut stmt = conn.prepare(budget_select_sql()).map_err(|e| e.to_string())?;
    stmt.query_row([guid], row_to_budget)
        .map_err(|e| e.to_string())
}

fn read_budget_by_account(conn: &Connection, account_guid: &str) -> Result<Budget, String> {
    let mut stmt = conn
        .prepare(
            "SELECT b.guid, b.account_guid, a.name, a.description, a.account_type,
			        b.amount, b.notes, b.created_at
			 FROM budgets b
			 JOIN accounts a ON a.guid = b.account_guid
			 WHERE b.account_guid = ?1",
        )
        .map_err(|e| e.to_string())?;
    stmt.query_row([account_guid], row_to_budget)
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No budget for this account".to_string())
}

/// Effective budget for a month: a per-month override wins, otherwise the
/// budget's monthly default target (`budgets.amount`) applies to every month.
fn effective_budget_expr(year: i32, month: i32) -> String {
    format!(
        "COALESCE((SELECT ba.amount FROM budget_amounts ba
                   WHERE ba.budget_guid = b.guid AND ba.year = {year} AND ba.month = {month}),
                  b.amount, 0)"
    )
}

/// Whether a month has an effective budget (override or default target).
fn has_budget_expr(year: i32, month: i32) -> String {
    format!(
        "(EXISTS(SELECT 1 FROM budget_amounts ba
                WHERE ba.budget_guid = b.guid AND ba.year = {year} AND ba.month = {month})
          OR b.amount > 0)"
    )
}

fn actual_for_period(month_actuals: &HashMap<i64, f64>, start_n: i64, months: i64) -> f64 {
    (start_n..start_n + months)
        .map(|n| month_actuals.get(&n).copied().unwrap_or(0.0))
        .sum()
}

/// Cumulative carryover for a period ending at month index `end_n`: the
/// (monthly budget − actual) per month from `start_n` (the account's first
/// transaction month) up to `end_n`. Every month in the window counts: a
/// per-month override wins, otherwise the budget's default target applies.
/// An empty or inverted window yields 0.
fn carryover_for_window(
    overrides: &HashMap<(i32, i32), f64>,
    target: f64,
    month_actuals: &HashMap<i64, f64>,
    start_n: i64,
    end_n: i64,
) -> f64 {
    (start_n..end_n)
        .map(|n| {
            let y = (n / 12) as i32;
            let m = (n % 12) as i32 + 1;
            let budget = overrides.get(&(y, m)).copied().unwrap_or(target);
            budget - month_actuals.get(&n).copied().unwrap_or(0.0)
        })
        .sum()
}

/// Sum of split amounts (converted to the report currency) for an account's
/// descendants, grouped by month index, within `[start_n, end_n)`.
fn fetch_monthly_actuals(
    conn: &Connection,
    account_guid: &str,
    start_n: i64,
    end_n: i64,
    fx_map: &HashMap<String, f64>,
) -> Result<HashMap<i64, f64>, String> {
    let sql = format!(
        "WITH RECURSIVE
         descendants(descendant_guid) AS (
             SELECT ?1
             UNION ALL
             SELECT a.guid FROM descendants d JOIN accounts a ON a.parent_guid = d.descendant_guid
         )
         SELECT substr(t.post_date, 1, 7), c.mnemonic, COALESCE(SUM({}), 0)
         FROM descendants d
         JOIN splits s ON s.account_guid = d.descendant_guid
         JOIN transactions t ON t.guid = s.tx_guid
         JOIN accounts a2 ON a2.guid = s.account_guid
         JOIN commodities c ON c.guid = a2.commodity_guid
         WHERE t.post_date >= ?2 AND t.post_date < ?3
         GROUP BY 1, 2",
        utils::signed_amount_expr(utils::RAW_AMOUNT, "a2.account_type")
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let start = format!("{}-01", utils::format_month(start_n));
    let end = format!("{}-01", utils::format_month(end_n));
    let rows: Vec<(String, Option<String>, f64)> = stmt
        .query_map(rusqlite::params![account_guid, start, end], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let report_currency = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let mut actuals: HashMap<i64, f64> = HashMap::new();
    for (ym, commodity, total) in rows {
        let converted = match commodity {
            Some(mn) => fx::convert(total, &mn, &report_currency, fx_map),
            None => total,
        };
        let n = utils::month_number(&ym)?;
        *actuals.entry(n).or_insert(0.0) += converted;
    }
    Ok(actuals)
}

/// Month index of the account's earliest visible transaction (descendants
/// included). `None` when the account has never been used.
fn earliest_account_tx_n(conn: &Connection, account_guid: &str) -> Result<Option<i64>, String> {
    let sql = format!(
        "WITH RECURSIVE
         descendants(descendant_guid) AS (
             SELECT ?1
             UNION ALL
             SELECT a.guid FROM descendants d JOIN accounts a ON a.parent_guid = d.descendant_guid
         )
         SELECT MIN(substr(t.post_date, 1, 7))
         FROM descendants d
         JOIN splits s ON s.account_guid = d.descendant_guid
         JOIN transactions t ON t.guid = s.tx_guid
         WHERE {}",
        utils::VISIBLE_TX_FILTER
    );
    let ym: Option<String> = conn
        .query_row(&sql, [account_guid], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    ym.map(|ym| utils::month_number(&ym)).transpose()
}

/// Carryover balances for every budget at `period_start`: the cumulative
/// (budget − actual) per month from each account's first transaction month
/// (descendants included) up to the period start. Carryover is always applied.
fn compute_cumulative_carryovers(
    conn: &Connection,
    period_start: &str,
) -> Result<HashMap<String, f64>, String> {
    let end_n = utils::month_number(&period_start[..7])?;

    // Earliest visible transaction month per budget account (incl. descendants).
    let sql = format!(
        "WITH RECURSIVE
         descendants(budget_account_guid, descendant_guid) AS (
             SELECT b.account_guid, b.account_guid FROM budgets b
             UNION ALL
             SELECT d.budget_account_guid, a.guid
             FROM descendants d JOIN accounts a ON a.parent_guid = d.descendant_guid
         )
         SELECT d.budget_account_guid, MIN(substr(t.post_date, 1, 7))
         FROM descendants d
         JOIN budgets b ON b.account_guid = d.budget_account_guid
         JOIN accounts a ON a.guid = b.account_guid
         JOIN splits s ON s.account_guid = d.descendant_guid
         JOIN transactions t ON t.guid = s.tx_guid
         WHERE a.account_type = 'EXPENSE' AND {}
         GROUP BY d.budget_account_guid",
        utils::VISIBLE_TX_FILTER
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut first_tx: HashMap<String, i64> = HashMap::new();
    for (guid, ym) in rows {
        if let Some(ym) = ym {
            first_tx.insert(guid, utils::month_number(&ym)?);
        }
    }

    let mut targets: HashMap<String, f64> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT b.account_guid, b.amount
				 FROM budgets b
				 JOIN accounts a ON a.guid = b.account_guid
				 WHERE a.account_type = 'EXPENSE'",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (account_guid, amount) in rows {
            targets.insert(account_guid, amount);
        }
    }

    let mut overrides: HashMap<String, HashMap<(i32, i32), f64>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT b.account_guid, ba.year, ba.month, ba.amount
				 FROM budget_amounts ba
				 JOIN budgets b ON b.guid = ba.budget_guid
				 JOIN accounts a ON a.guid = b.account_guid
				 WHERE a.account_type = 'EXPENSE'",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i32>(1)?,
                    row.get::<_, i32>(2)?,
                    row.get::<_, f64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (account_guid, year, month, amount) in rows {
            overrides
                .entry(account_guid)
                .or_default()
                .insert((year, month), amount);
        }
    }

    let start_n = first_tx.values().copied().min().unwrap_or(end_n).min(end_n);

    let sql = format!(
        "WITH RECURSIVE
         descendants(budget_account_guid, descendant_guid) AS (
             SELECT b.account_guid, b.account_guid FROM budgets b
             UNION ALL
             SELECT d.budget_account_guid, a.guid
             FROM descendants d JOIN accounts a ON a.parent_guid = d.descendant_guid
         )
         SELECT d.budget_account_guid, substr(t.post_date, 1, 7), c.mnemonic,
                COALESCE(SUM({}), 0)
         FROM descendants d
         JOIN splits s ON s.account_guid = d.descendant_guid
         JOIN transactions t ON t.guid = s.tx_guid
         JOIN accounts a2 ON a2.guid = s.account_guid
         JOIN commodities c ON c.guid = a2.commodity_guid
         WHERE t.post_date >= ?1 AND t.post_date < ?2
         GROUP BY d.budget_account_guid, 2, 3",
        utils::signed_amount_expr(utils::RAW_AMOUNT, "a2.account_type")
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let start = format!("{}-01", utils::format_month(start_n));
    let rows: Vec<(String, String, Option<String>, f64)> = stmt
        .query_map(rusqlite::params![start, period_start], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let report_currency = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_map = fx::get_fx_map(conn, Some(period_start))?;
    let mut month_actuals: HashMap<String, HashMap<i64, f64>> = HashMap::new();
    for (account_guid, ym, commodity, total) in rows {
        let converted = match commodity {
            Some(mn) => fx::convert(total, &mn, &report_currency, &fx_map),
            None => total,
        };
        let n = utils::month_number(&ym)?;
        *month_actuals
            .entry(account_guid)
            .or_default()
            .entry(n)
            .or_insert(0.0) += converted;
    }

    let mut carryovers: HashMap<String, f64> = HashMap::new();
    for (guid, first_n) in first_tx {
        if first_n >= end_n {
            continue;
        }
        let o = overrides.get(&guid).cloned().unwrap_or_default();
        let target = targets.get(&guid).copied().unwrap_or(0.0);
        let ma = month_actuals.get(&guid).cloned().unwrap_or_default();
        let total = carryover_for_window(&o, target, &ma, first_n, end_n);
        carryovers.insert(guid, total);
    }
    Ok(carryovers)
}

pub fn get_budget_vs_actual_inner(
    conn: &Connection,
    year: i32,
    month: i32,
) -> Result<Vec<BudgetActualRow>, String> {
    let (period_start, period_end) = utils::month_range(year, month);
    let effective_budget = effective_budget_expr(year, month);
    let has_budget = has_budget_expr(year, month);

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
			 SELECT b.account_guid, a.name, a.description, a.account_type,
			        {} as effective_budget,
			        {} as has_budget,
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
            has_budget,
            utils::RAW_AMOUNT
        ))
        .map_err(|e| e.to_string())?;

    /// (guid, name, description, type, budget, has_budget, commodity, actual)
    type RawBudgetRow = (
        String,
        String,
        Option<String>,
        String,
        f64,
        bool,
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
                row.get::<_, f64>(4)?,
                row.get::<_, bool>(5)?,
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
        has_budget: bool,
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
        budget_amount,
        has_budget,
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
                        has_budget,
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
                has_budget: agg.has_budget,
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

pub fn get_budget_detail_inner(
    conn: &Connection,
    account_guid: &str,
    page: i64,
    page_size: i64,
    sort_by: Option<&str>,
    sort_dir: Option<&str>,
) -> Result<BudgetDetail, String> {
    let budget = read_budget_by_account(conn, account_guid)?;
    let target = budget.amount;

    let mut stmt = conn
        .prepare(
            "SELECT year, month, amount FROM budget_amounts
			 WHERE budget_guid = ?1
			 ORDER BY year, month",
        )
        .map_err(|e| e.to_string())?;
    let raw_entries: Vec<(i32, i32, f64)> = stmt
        .query_map([&budget.guid], |row| {
            Ok((
                row.get::<_, i32>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let overrides: HashMap<(i32, i32), f64> = raw_entries
        .iter()
        .map(|(y, m, a)| ((*y, *m), *a))
        .collect();

    let now = chrono::Utc::now();
    let current_year = now.year();
    let current_month = now.month() as i32;
    let current_start_n = current_year as i64 * 12 + (current_month - 1) as i64;
    let current_end_n = current_start_n + 1;

    // The table spans the account's carryover window (from its first
    // transaction month) through the current month, plus any override months
    // beyond that. The default target applies to every month in between.
    let first_tx_n = earliest_account_tx_n(conn, account_guid)?;
    let mut series_start = current_start_n.min(first_tx_n.unwrap_or(current_start_n));
    let mut series_end = current_end_n.max(series_start + 1);
    for (y, m) in overrides.keys() {
        let n = *y as i64 * 12 + *m as i64 - 1;
        series_start = series_start.min(n);
        series_end = series_end.max(n + 1);
    }

    let fx_map = fx::get_fx_map(conn, Some(&format!("{}-01", utils::format_month(series_end))))?;
    let month_actuals = fetch_monthly_actuals(conn, account_guid, series_start, series_end, &fx_map)?;

    let mut entries = Vec::with_capacity((series_end - series_start) as usize);
    for n in series_start..series_end {
        let year = (n / 12) as i32;
        let month = (n % 12) as i32 + 1;
        let amount = overrides.get(&(year, month)).copied().unwrap_or(target);
        let actual_amount = actual_for_period(&month_actuals, n, 1);
        let rollover_balance = carryover_for_window(&overrides, target, &month_actuals, series_start, n);
        let effective = amount + rollover_balance;
        let remaining = effective - actual_amount;
        let percentage = if effective != 0.0 {
            (actual_amount.max(0.0) / effective) * 100.0
        } else {
            0.0
        };
        entries.push(BudgetEntry {
            year,
            month,
            amount,
            actual_amount,
            remaining,
            percentage,
            rollover_balance,
        });
    }

    let page_size = utils::clamp_page_size(page_size);
    let total = entries.len() as i64;
    let total_pages = utils::total_pages(total, page_size);
    let safe_page = utils::clamp_page(page, total_pages);
    let offset = (safe_page - 1) * page_size;

    // Chronological (oldest first) unless an explicit sort key flips it.
    let ascending = match sort_by {
        None => true,
        _ => sort_dir == Some("asc"),
    };
    entries.sort_by(|a, b| {
        let cmp = match sort_by {
            Some("amount") => a.amount.partial_cmp(&b.amount).unwrap_or(Ordering::Equal),
            Some("spent") => a
                .actual_amount
                .partial_cmp(&b.actual_amount)
                .unwrap_or(Ordering::Equal),
            Some("remaining") => a
                .remaining
                .partial_cmp(&b.remaining)
                .unwrap_or(Ordering::Equal),
            _ => (a.year * 12 + a.month).cmp(&(b.year * 12 + b.month)),
        };
        // Tie-break chronologically so the order stays deterministic.
        let cmp = cmp.then_with(|| (a.year * 12 + a.month).cmp(&(b.year * 12 + b.month)));
        if ascending {
            cmp
        } else {
            cmp.reverse()
        }
    });
    let entries = entries
        .into_iter()
        .skip(offset as usize)
        .take(page_size as usize)
        .collect();

    let budget_amount = overrides
        .get(&(current_year, current_month))
        .copied()
        .unwrap_or(target);
    let actual_amount = actual_for_period(&month_actuals, current_start_n, 1);
    let rollover_balance = carryover_for_window(
        &overrides,
        target,
        &month_actuals,
        series_start,
        current_start_n,
    );
    let effective = budget_amount + rollover_balance;
    let remaining = effective - actual_amount;
    let percentage = if effective != 0.0 {
        (actual_amount.max(0.0) / effective) * 100.0
    } else {
        0.0
    };
    let current_period = BudgetPeriodSummary {
        budget_amount,
        actual_amount,
        remaining,
        percentage,
        rollover_balance,
    };

    Ok(BudgetDetail {
        budget,
        entries,
        current_period,
        total,
        page: safe_page,
        page_size,
        total_pages,
    })
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

#[tauri::command]
pub async fn list_budgets(state: State<'_, AppState>) -> Result<Vec<Budget>, String> {
    run_blocking(state.db.clone(), list_budgets_inner).await
}

#[tauri::command]
pub async fn set_budget(
    state: State<'_, AppState>,
    account_guid: String,
    notes: Option<String>,
    amount: f64,
) -> Result<Budget, String> {
    run_blocking(state.db.clone(), move |conn| {
        set_budget_inner(conn, &account_guid, notes.as_deref(), amount)
    })
    .await
}

#[tauri::command]
pub async fn set_budget_amount(
    state: State<'_, AppState>,
    account_guid: String,
    year: i32,
    month: i32,
    amount: f64,
) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        set_budget_amount_inner(conn, &account_guid, year, month, amount)
    })
    .await
}

#[tauri::command]
pub async fn reset_budget_amount(
    state: State<'_, AppState>,
    account_guid: String,
    year: i32,
    month: i32,
) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        reset_budget_amount_inner(conn, &account_guid, year, month)
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
) -> Result<Vec<BudgetActualRow>, String> {
    run_blocking(state.db.clone(), move |conn| {
        get_budget_vs_actual_inner(conn, year, month)
    })
    .await
}

#[tauri::command]
pub async fn get_budget_detail(
    state: State<'_, AppState>,
    account_guid: String,
    page: i64,
    page_size: i64,
    sort_by: Option<String>,
    sort_dir: Option<String>,
) -> Result<BudgetDetail, String> {
    run_blocking(state.db.clone(), move |conn| {
        get_budget_detail_inner(
            conn,
            &account_guid,
            page,
            page_size,
            sort_by.as_deref(),
            sort_dir.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_budget_date_range(state: State<'_, AppState>) -> Result<BudgetDateRange, String> {
    run_blocking(state.db.clone(), get_budget_date_range_inner).await
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
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
             ('expenses', 'Expenses', 'EXPENSE', 'root', 1, 'c-eur'),
             ('food',     'Food',     'EXPENSE', 'expenses', 0, 'c-eur'),
             ('dining',   'Dining',   'EXPENSE', 'root', 1, 'c-eur'),
             ('restaurant', 'Restaurant', 'EXPENSE', 'dining', 0, 'c-eur'),
             ('unused',   'Unused',   'EXPENSE', 'expenses', 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn
    }

    /// A spend of `amount` on `date` in `account` (expense splits are positive).
    fn add_spend(conn: &Connection, tag: &str, date: &str, account: &str, amount: f64) {
        let num = (amount * 100.0) as i64;
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
            params![tag, date],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
            params![format!("s-{tag}"), tag, account, num],
        )
        .unwrap();
    }

    fn assert_close(a: f64, b: f64) {
        assert!(
            (a - b).abs() < 0.001,
            "expected {b}, got {a}"
        );
    }

    fn find_entry<'a>(detail: &'a BudgetDetail, year: i32, month: i32) -> &'a BudgetEntry {
        detail
            .entries
            .iter()
            .find(|e| e.year == year && e.month == month)
            .unwrap_or_else(|| panic!("no entry for {year}-{month}"))
    }

    #[test]
    fn carryover_window_starts_at_first_transaction() {
        let conn = setup();
        // The default target applies to every month since the account's first
        // transaction, well before the budget is created.
        set_budget_inner(&conn, "food", None, 100.0).unwrap();
        // First transaction ever: May 2022, spending 50.
        add_spend(&conn, "t1", "2022-05-10", "food", 50.0);

        // June 2023: window = May 2022..June 2023 (13 months) at 100/mo minus 50.
        let rows = get_budget_vs_actual_inner(&conn, 2023, 6).unwrap();
        assert_eq!(rows.len(), 1);
        assert_close(rows[0].budget_amount, 100.0);
        assert_close(rows[0].actual_amount, 0.0);
        assert_close(rows[0].rollover_balance, 1250.0);
        assert_close(rows[0].remaining, 1350.0);

        // February 2023: window = May 2022..Feb 2023 (9 months) at 100/mo minus 50.
        let rows = get_budget_vs_actual_inner(&conn, 2023, 2).unwrap();
        assert_close(rows[0].rollover_balance, 850.0);
        assert_close(rows[0].remaining, 950.0);
    }

    #[test]
    fn carryover_includes_descendant_transactions() {
        let conn = setup();
        set_budget_inner(&conn, "dining", None, 200.0).unwrap();
        // First transaction is on the child account, March 2023, spending 30.
        add_spend(&conn, "t1", "2023-03-15", "restaurant", 30.0);

        let rows = get_budget_vs_actual_inner(&conn, 2023, 6).unwrap();
        assert_eq!(rows.len(), 1);
        assert_close(rows[0].budget_amount, 200.0);
        // Window = Mar..Jun 2023 (3 months) at 200/mo minus 30 spent in March.
        assert_close(rows[0].rollover_balance, 570.0);
        assert_close(rows[0].remaining, 770.0);
    }

    #[test]
    fn overspending_reduces_carryover() {
        let conn = setup();
        set_budget_inner(&conn, "food", None, 100.0).unwrap();
        // Overspend in the first month: 120 against a 100 budget.
        add_spend(&conn, "t1", "2024-01-05", "food", 120.0);

        let rows = get_budget_vs_actual_inner(&conn, 2024, 3).unwrap();
        // Window = Jan..Mar 2024 (2 months): 200 budgeted - 120 spent.
        assert_close(rows[0].rollover_balance, 80.0);
        assert_close(rows[0].actual_amount, 0.0);
        assert_close(rows[0].remaining, 180.0);
    }

    #[test]
    fn no_transactions_yields_zero_carryover() {
        let conn = setup();
        set_budget_inner(&conn, "unused", None, 80.0).unwrap();

        let rows = get_budget_vs_actual_inner(&conn, 2024, 3).unwrap();
        assert_close(rows[0].budget_amount, 80.0);
        assert_close(rows[0].actual_amount, 0.0);
        assert_close(rows[0].rollover_balance, 0.0);
        assert_close(rows[0].remaining, 80.0);

        let detail = get_budget_detail_inner(&conn, "unused", 1, 10000, None, None).unwrap();
        // Without transactions the series is just the current month.
        assert_eq!(detail.entries.len(), 1);
        for e in &detail.entries {
            assert_close(e.amount, 80.0);
            assert_close(e.rollover_balance, 0.0);
        }
        assert_close(detail.current_period.rollover_balance, 0.0);
    }

    #[test]
    fn overrides_win_over_default_target() {
        let conn = setup();
        set_budget_inner(&conn, "food", None, 100.0).unwrap();
        // January is overridden down to 50; every other month uses the target.
        set_budget_amount_inner(&conn, "food", 2024, 1, 50.0).unwrap();
        // Spend 50 in the overridden month and 120 in a target month.
        add_spend(&conn, "t1", "2024-01-05", "food", 50.0);
        add_spend(&conn, "t2", "2024-02-10", "food", 120.0);

        // March 2024: window = Jan..Mar: Jan 50-50=0, Feb 100-120=-20.
        let rows = get_budget_vs_actual_inner(&conn, 2024, 3).unwrap();
        assert!(rows[0].has_budget);
        assert_close(rows[0].budget_amount, 100.0);
        assert_close(rows[0].rollover_balance, -20.0);
        assert_close(rows[0].actual_amount, 0.0);
        assert_close(rows[0].remaining, 80.0);

        // Detail: January shows the override, February the default target.
        let detail = get_budget_detail_inner(&conn, "food", 1, 10000, None, None).unwrap();
        assert_close(find_entry(&detail, 2024, 1).amount, 50.0);
        assert_close(find_entry(&detail, 2024, 2).amount, 100.0);
        assert_close(find_entry(&detail, 2024, 3).amount, 100.0);
    }

    #[test]
    fn reset_override_falls_back_to_default_target() {
        let conn = setup();
        set_budget_inner(&conn, "food", None, 100.0).unwrap();
        set_budget_amount_inner(&conn, "food", 2024, 1, 50.0).unwrap();
        add_spend(&conn, "t1", "2024-01-05", "food", 50.0);

        // March 2024: window Jan..Mar: Jan 50-50=0, Feb 100-0=100.
        let rows = get_budget_vs_actual_inner(&conn, 2024, 3).unwrap();
        assert_close(rows[0].rollover_balance, 100.0);

        // Reset the January override: every month now uses the target.
        reset_budget_amount_inner(&conn, "food", 2024, 1).unwrap();
        let rows = get_budget_vs_actual_inner(&conn, 2024, 3).unwrap();
        // Window Jan..Mar: Jan 100-50=50, Feb 100-0=100.
        assert_close(rows[0].rollover_balance, 150.0);

        let detail = get_budget_detail_inner(&conn, "food", 1, 10000, None, None).unwrap();
        assert_close(find_entry(&detail, 2024, 1).amount, 100.0);
    }

    #[test]
    fn detail_entries_carry_over_from_first_transaction() {
        let conn = setup();
        set_budget_inner(&conn, "food", None, 100.0).unwrap();
        add_spend(&conn, "t1", "2022-05-10", "food", 50.0);

        let detail = get_budget_detail_inner(&conn, "food", 1, 10000, None, None).unwrap();

        // The series starts at the account's first transaction month.
        assert_eq!(detail.entries.first().map(|e| (e.year, e.month)), Some((2022, 5)));

        // June 2023 entry: window May 2022..Jun 2023 (13 months) minus 50 spent.
        let jun = find_entry(&detail, 2023, 6);
        assert_close(jun.amount, 100.0);
        assert_close(jun.actual_amount, 0.0);
        assert_close(jun.rollover_balance, 1250.0);
        assert_close(jun.remaining, 1350.0);

        // January 2023 entry: window May 2022..Jan 2023 (8 months) minus 50 spent.
        let jan = find_entry(&detail, 2023, 1);
        assert_close(jan.rollover_balance, 750.0);

        // The current period always carries over too (window predates it).
        assert!(detail.current_period.rollover_balance > 0.0);
    }

    #[test]
    fn detail_paginates_and_sorts() {
        let conn = setup();
        set_budget_inner(&conn, "food", None, 100.0).unwrap();
        add_spend(&conn, "t1", "2022-05-10", "food", 50.0);
        // Push the series far into the future with an override so the entry
        // count is independent of the current date.
        set_budget_amount_inner(&conn, "food", 2099, 1, 100.0).unwrap();
        set_budget_amount_inner(&conn, "food", 2023, 1, 500.0).unwrap();

        // Page 2 of 12 skips the first 12 months: May 2022 + 12 = May 2023.
        let detail = get_budget_detail_inner(&conn, "food", 2, 12, None, None).unwrap();
        assert_eq!(detail.page, 2);
        assert_eq!(detail.entries.len(), 12);
        assert_eq!(
            detail.entries.first().map(|e| (e.year, e.month)),
            Some((2023, 5))
        );
        assert_eq!(
            detail.total_pages,
            (detail.total as f64 / 12.0).ceil() as i64
        );

        // Out-of-range pages clamp to the last page.
        let last = get_budget_detail_inner(&conn, "food", 999, 12, None, None).unwrap();
        assert_eq!(last.page, last.total_pages);
        assert_eq!(last.entries.len(), ((last.total - 1) % 12 + 1) as usize);

        // Amount sort: the 500 override (2023-01) leads descending; ascending
        // falls back to the earliest chronological month on ties.
        let desc =
            get_budget_detail_inner(&conn, "food", 1, 10000, Some("amount"), Some("desc")).unwrap();
        assert_eq!(
            desc.entries.first().map(|e| (e.year, e.month, e.amount)),
            Some((2023, 1, 500.0))
        );
        let asc =
            get_budget_detail_inner(&conn, "food", 1, 10000, Some("amount"), Some("asc")).unwrap();
        assert_eq!(
            asc.entries.first().map(|e| (e.year, e.month, e.amount)),
            Some((2022, 5, 100.0))
        );
    }
}