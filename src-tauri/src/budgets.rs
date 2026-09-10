use chrono::Datelike;
use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::utils;

/// Per-account, per-year budget. A year without a row simply has no budget.
/// Carryover is always applied (no opt-out): each year's remaining is
/// `(annual + carry-in) − actual`, where carry-in sums `(budget − actual)`
/// over earlier years that have a budget row — years without one are skipped.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub account_type: String,
    pub year: i32,
    pub amount: f64,
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
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub account_type: String,
    pub budget_amount: f64,
    pub actual_amount: f64,
    pub remaining: f64,
    pub percentage: f64,
    pub rollover_balance: f64,
    pub expected_ytd: f64,
    pub pacing_percentage: f64,
    pub delta: f64,
    pub elapsed_months: i32,
    pub effective_total: f64,
}

/// One month within a yearly budget detail: read-only pacing slice derived
/// from the year's effective total (annual + carry-in).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlySlice {
    pub month: i32,
    pub actual_amount: f64,
    pub cumulative_actual: f64,
    pub cumulative_expected: f64,
    pub delta: f64,
    pub pacing_percentage: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetYearSummary {
    pub budget_amount: f64,
    pub rollover_balance: f64,
    pub effective_total: f64,
    pub actual_amount: f64,
    pub expected_ytd: f64,
    pub remaining: f64,
    pub percentage: f64,
    pub pacing_percentage: f64,
    pub delta: f64,
    pub elapsed_months: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetDetail {
    pub budget: Budget,
    pub summary: BudgetYearSummary,
    pub monthly_entries: Vec<MonthlySlice>,
}

fn row_to_budget(row: &rusqlite::Row) -> rusqlite::Result<Budget> {
    Ok(Budget {
        guid: row.get(0)?,
        account_guid: row.get(1)?,
        account_name: row.get(2)?,
        account_description: row.get(3)?,
        account_type: row.get(4)?,
        year: row.get(5)?,
        amount: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn validate_year(year: i32) -> Result<(), String> {
    if !(1900..=2200).contains(&year) {
        return Err(format!("Budget year must be between 1900 and 2200, got {year}"));
    }
    Ok(())
}

fn validate_amount(amount: f64) -> Result<(), String> {
    if !amount.is_finite() || amount <= 0.0 {
        return Err("Budget amount must be positive and finite".to_string());
    }
    Ok(())
}

/// Months elapsed in `year` for pacing: 12 for past years, the current month
/// (1–12) for the current year, 0 for future years.
fn elapsed_months_for_year(year: i32) -> i32 {
    let now = chrono::Utc::now();
    let cy = now.year();
    let cm = now.month() as i32;
    if year < cy {
        12
    } else if year == cy {
        cm.clamp(1, 12)
    } else {
        0
    }
}

/// Expected YTD with front-loaded carry: the annual target is prorated
/// (`annual × elapsed / 12`) while the carry-in counts in full from January
/// as an unprorated buffer. Returns `(expected_ytd, pacing_pct, delta)`.
fn pacing_stats(annual: f64, carry: f64, actual: f64, elapsed: i32) -> (f64, f64, f64) {
    let expected = if elapsed <= 0 {
        0.0
    } else {
        annual * (elapsed as f64) / 12.0 + carry
    };
    let pacing = if expected > 0.0 {
        (actual.max(0.0) / expected) * 100.0
    } else {
        0.0
    };
    let delta = expected - actual;
    (expected, pacing, delta)
}

fn full_year_percentage(actual: f64, effective: f64) -> f64 {
    if effective != 0.0 {
        (actual.max(0.0) / effective) * 100.0
    } else {
        0.0
    }
}

fn budget_select_sql() -> &'static str {
    "SELECT b.guid, b.account_guid, a.name, a.description, a.account_type,
	        b.year, b.amount, b.created_at
	 FROM budgets b
	 JOIN accounts a ON a.guid = b.account_guid
	 WHERE b.account_guid = ?1 AND b.year = ?2"
}

/// Shared `WITH RECURSIVE descendants` CTE for budget actuals queries.
/// `seed_filter_sql` scopes the seed rows, e.g. `WHERE b.year = ?3` for the
/// yearly overview or empty for all-time carryover. Must be a static fragment,
/// never user input. All callers use the half-open window `>= ? AND < ?`
/// plus `VISIBLE_TX_FILTER`.
fn descendants_cte(seed_filter_sql: &str) -> String {
    format!(
        "WITH RECURSIVE
         descendants(budget_account_guid, descendant_guid) AS (
             SELECT DISTINCT b.account_guid, b.account_guid FROM budgets b {seed_filter_sql}
             UNION
             SELECT d.budget_account_guid, a.guid
             FROM descendants d JOIN accounts a ON a.parent_guid = d.descendant_guid
         )"
    )
}

fn read_budget_year(conn: &Connection, account_guid: &str, year: i32) -> Result<Budget, String> {
    let mut stmt = conn.prepare(budget_select_sql()).map_err(|e| e.to_string())?;
    stmt.query_row(rusqlite::params![account_guid, year], row_to_budget)
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No budget for this year".to_string())
}

fn assert_expense_account(conn: &Connection, account_guid: &str) -> Result<(), String> {
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
    Ok(())
}

/// Upsert a single year's budget amount.
fn set_budget_year_amount_inner(
    conn: &Connection,
    account_guid: &str,
    year: i32,
    amount: f64,
) -> Result<Budget, String> {
    assert_expense_account(conn, account_guid)?;
    validate_amount(amount)?;
    validate_year(year)?;
    conn.execute(
        "INSERT INTO budgets (guid, account_guid, year, amount, created_at)
		 VALUES (?1, ?2, ?3, ?4, ?5)
		 ON CONFLICT (account_guid, year)
		 DO UPDATE SET amount = excluded.amount",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            account_guid,
            year,
            amount,
            utils::iso_now()
        ],
    )
    .map_err(|e| e.to_string())?;
    read_budget_year(conn, account_guid, year)
}

/// Years that already have a budget for the account, ascending. Used by
/// the Add dialog to disable taken years so a new budget is always created
/// for a single free year.
fn fetch_budget_years_inner(conn: &Connection, account_guid: &str) -> Result<Vec<i32>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT year FROM budgets
			 WHERE account_guid = ?1
			 ORDER BY year",
        )
        .map_err(|e| e.to_string())?;
    let years = stmt
        .query_map([account_guid], |row| row.get::<_, i32>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(years)
}

/// Delete a single year's budget. Other years for the account are kept.
fn delete_budget_year_inner(
    conn: &Connection,
    account_guid: &str,
    year: i32,
) -> Result<(), String> {
    validate_year(year)?;
    conn.execute(
        "DELETE FROM budgets WHERE account_guid = ?1 AND year = ?2",
        rusqlite::params![account_guid, year],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// All yearly budgets as `(account_guid → year → amount)` for expense
/// accounts. Only years with a row count towards carryover.
fn yearly_budgets_map(conn: &Connection) -> Result<HashMap<String, HashMap<i32, f64>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT b.account_guid, b.year, b.amount
			 FROM budgets b
			 JOIN accounts a ON a.guid = b.account_guid
			 WHERE a.account_type = 'EXPENSE'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut map: HashMap<String, HashMap<i32, f64>> = HashMap::new();
    for (account_guid, year, amount) in rows {
        map.entry(account_guid).or_default().insert(year, amount);
    }
    Ok(map)
}

/// Sum of split amounts (converted to the report currency) for an account's
/// descendants, grouped by month index, within `[start_n, end_n)`.
/// Future-dated transactions are excluded via `VISIBLE_TX_FILTER`.
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
             UNION
             SELECT a.guid FROM descendants d JOIN accounts a ON a.parent_guid = d.descendant_guid
         )
         SELECT substr(t.post_date, 1, 7), c.mnemonic, COALESCE(SUM({}), 0)
         FROM descendants d
         JOIN splits s ON s.account_guid = d.descendant_guid
         JOIN transactions t ON t.guid = s.tx_guid
         JOIN accounts a2 ON a2.guid = s.account_guid
         JOIN commodities c ON c.guid = a2.commodity_guid
         WHERE t.post_date >= ?2 AND t.post_date < ?3 AND {}
         GROUP BY 1, 2",
        utils::signed_amount_expr(utils::RAW_AMOUNT, "a2.account_type"),
        utils::VISIBLE_TX_FILTER
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
        let converted = fx::convert_optional(total, commodity.as_deref(), &report_currency, fx_map);
        let n = utils::month_number(&ym)?;
        *actuals.entry(n).or_insert(0.0) += converted;
    }
    Ok(actuals)
}

/// Cumulative carryover into Jan 1 of `year` for every budgeted account: the
/// `(budget − actual)` per year from each account's first budget year up to
/// `year − 1`. Only years with a budget row count — years without one are
/// skipped entirely. Carryover is always applied — there is no opt-out. An
/// empty or inverted window yields 0.
fn compute_cumulative_carryovers(
    conn: &Connection,
    year: i32,
) -> Result<HashMap<String, f64>, String> {
    let period_start = format!("{year:04}-01-01");

    let yearly = yearly_budgets_map(conn)?;
    let start_year = yearly
        .values()
        .flat_map(|years| years.keys().copied())
        .min()
        .unwrap_or(year)
        .min(year);
    if start_year >= year {
        return Ok(HashMap::new());
    }

    let sql = format!(
        "{}
         SELECT d.budget_account_guid, substr(t.post_date, 1, 4), c.mnemonic,
                COALESCE(SUM({}), 0)
         FROM descendants d
         JOIN splits s ON s.account_guid = d.descendant_guid
         JOIN transactions t ON t.guid = s.tx_guid
         JOIN accounts a2 ON a2.guid = s.account_guid
         JOIN commodities c ON c.guid = a2.commodity_guid
         WHERE t.post_date >= ?1 AND t.post_date < ?2 AND {}
         GROUP BY d.budget_account_guid, 2, 3",
        descendants_cte(""),
        utils::signed_amount_expr(utils::RAW_AMOUNT, "a2.account_type"),
        utils::VISIBLE_TX_FILTER
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let start = format!("{start_year:04}-01-01");
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
    let fx_map = fx::get_fx_map(conn, Some(&period_start))?;
    let mut yearly_actuals: HashMap<String, HashMap<i32, f64>> = HashMap::new();
    for (account_guid, y, commodity, total) in rows {
        let converted = fx::convert_optional(total, commodity.as_deref(), &report_currency, &fx_map);
        if let Ok(yi) = y.parse::<i32>() {
            *yearly_actuals
                .entry(account_guid)
                .or_default()
                .entry(yi)
                .or_insert(0.0) += converted;
        }
    }

    let mut carryovers: HashMap<String, f64> = HashMap::new();
    for (guid, years) in &yearly {
        let first = match years.keys().copied().min() {
            Some(f) if f < year => f,
            _ => continue,
        };
        let ya = yearly_actuals.get(guid).cloned().unwrap_or_default();
        let mut total = 0.0;
        for y in first..year {
            if let Some(budget) = years.get(&y) {
                total += budget - ya.get(&y).copied().unwrap_or(0.0);
            }
        }
        carryovers.insert(guid.clone(), total);
    }
    Ok(carryovers)
}

fn fetch_budget_vs_actual_inner(
    conn: &Connection,
    year: i32,
) -> Result<Vec<BudgetActualRow>, String> {
    validate_year(year)?;
    let period_start = format!("{year:04}-01-01");
    let period_end = format!("{:04}-01-01", year + 1);

    let mut stmt = conn
        .prepare(&format!(
            "{}
			 SELECT b.guid, b.account_guid, a.name, a.description, a.account_type,
			        b.amount,
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
			      WHERE t.post_date >= ?1 AND t.post_date < ?2 AND {}
			  ) ps ON ps.account_guid = d.descendant_guid
			  LEFT JOIN commodities c ON c.guid = ps.commodity_guid
			  WHERE a.account_type = 'EXPENSE' AND b.year = ?3
			  GROUP BY b.guid, c.mnemonic
			  ORDER BY a.name",
            descendants_cte("WHERE b.year = ?3"),
            utils::RAW_AMOUNT,
            utils::VISIBLE_TX_FILTER
        ))
        .map_err(|e| e.to_string())?;

    /// (guid, account_guid, name, description, type, budget, commodity, actual)
    type RawBudgetRow = (
        String,
        String,
        String,
        Option<String>,
        String,
        f64,
        Option<String>,
        f64,
    );
    let raw_rows: Vec<RawBudgetRow> = stmt
        .query_map(
            rusqlite::params![&period_start, &period_end, year],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, f64>(7)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let report_currency = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_map = fx::get_fx_map(conn, Some(&period_end))?;

    // Aggregate actuals per budget, converting each currency to the report currency.
    #[derive(Clone)]
    struct BudgetAgg {
        guid: String,
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
        guid,
        account_guid,
        account_name,
        account_description,
        account_type,
        budget_amount,
        commodity,
        actual_amount,
    ) in raw_rows
    {
        let converted =
            fx::convert_optional(actual_amount, commodity.as_deref(), &report_currency, &fx_map);
        match aggs.get_mut(&account_guid) {
            Some(agg) => agg.actual_amount += converted,
            None => {
                agg_order.push(account_guid.clone());
                aggs.insert(
                    account_guid.clone(),
                    BudgetAgg {
                        guid,
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

    let carryovers = compute_cumulative_carryovers(conn, year)?;
    let elapsed = elapsed_months_for_year(year);

    let mut rows: Vec<BudgetActualRow> = agg_order
        .iter()
        .map(|guid| {
            let agg = &aggs[guid];
            let carry = carryovers.get(guid).copied().unwrap_or(0.0);
            let effective = agg.budget_amount + carry;
            let remaining = effective - agg.actual_amount;
            let percentage = full_year_percentage(agg.actual_amount, effective);
            let (expected_ytd, pacing_percentage, delta) =
                pacing_stats(agg.budget_amount, carry, agg.actual_amount, elapsed);
            BudgetActualRow {
                guid: agg.guid.clone(),
                account_guid: agg.account_guid.clone(),
                account_name: agg.account_name.clone(),
                account_description: agg.account_description.clone(),
                account_type: agg.account_type.clone(),
                budget_amount: agg.budget_amount,
                actual_amount: agg.actual_amount,
                remaining,
                percentage,
                rollover_balance: carry,
                expected_ytd,
                pacing_percentage,
                delta,
                elapsed_months: elapsed,
                effective_total: effective,
            }
        })
        .collect();

    // Deterministic order (already ordered by account name in SQL).
    rows.sort_by(|a, b| a.account_name.cmp(&b.account_name));
    Ok(rows)
}

fn fetch_budget_detail_inner(
    conn: &Connection,
    account_guid: &str,
    year: i32,
) -> Result<BudgetDetail, String> {
    validate_year(year)?;
    let budget = read_budget_year(conn, account_guid, year)?;
    let annual_amount = budget.amount;
    let period_start = format!("{year:04}-01-01");
    let period_end = format!("{:04}-01-01", year + 1);

    let mut stmt = conn
        .prepare(
            "SELECT year, amount FROM budgets
			 WHERE account_guid = ?1
			 ORDER BY year",
        )
        .map_err(|e| e.to_string())?;
    let raw_entries: Vec<(i32, f64)> = stmt
        .query_map([account_guid], |row| {
            Ok((row.get::<_, i32>(0)?, row.get::<_, f64>(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let yearly: HashMap<i32, f64> = raw_entries.iter().map(|(y, a)| (*y, *a)).collect();

    // Carry-in at Jan 1 of the selected year (always applied). Only years
    // with a budget row count; earlier years without one are skipped.
    let first_year = yearly.keys().copied().min();
    let mut carry = 0.0;
    if let Some(fy) = first_year {
        if fy < year {
            let fx_carry = fx::get_fx_map(conn, Some(&period_start))?;
            let start_n = fy as i64 * 12;
            let end_n = year as i64 * 12;
            let month_actuals =
                fetch_monthly_actuals(conn, account_guid, start_n, end_n, &fx_carry)?;
            for y in fy..year {
                if let Some(amount) = yearly.get(&y) {
                    let mut actual_y = 0.0;
                    for m in 0..12 {
                        actual_y += month_actuals.get(&(y as i64 * 12 + m)).copied().unwrap_or(0.0);
                    }
                    carry += amount - actual_y;
                }
            }
        }
    }

    // Monthly actuals for the selected year (visible transactions only).
    let fx_year = fx::get_fx_map(conn, Some(&period_end))?;
    let start_n = year as i64 * 12;
    let end_n = (year as i64 + 1) * 12;
    let month_actuals = fetch_monthly_actuals(conn, account_guid, start_n, end_n, &fx_year)?;

    let mut monthly: Vec<(i32, f64)> = Vec::with_capacity(12);
    for m in 1..=12 {
        let n = year as i64 * 12 + (m - 1) as i64;
        monthly.push((m, month_actuals.get(&n).copied().unwrap_or(0.0)));
    }
    let actual_amount: f64 = monthly.iter().map(|(_, a)| *a).sum();

    let effective = annual_amount + carry;
    let remaining = effective - actual_amount;
    let percentage = full_year_percentage(actual_amount, effective);
    let elapsed = elapsed_months_for_year(year);
    let (expected_ytd, pacing_percentage, delta) =
        pacing_stats(annual_amount, carry, actual_amount, elapsed);

    let mut monthly_entries = Vec::with_capacity(12);
    let mut cum = 0.0;
    for (m, actual_m) in monthly {
        cum += actual_m;
        // Reuse the front-loaded pacing rule: annual prorated, carry in full.
        let (cum_expected, pacing, delta) = pacing_stats(annual_amount, carry, cum, m);
        monthly_entries.push(MonthlySlice {
            month: m,
            actual_amount: actual_m,
            cumulative_actual: cum,
            cumulative_expected: cum_expected,
            delta,
            pacing_percentage: pacing,
        });
    }

    Ok(BudgetDetail {
        budget,
        summary: BudgetYearSummary {
            budget_amount: annual_amount,
            rollover_balance: carry,
            effective_total: effective,
            actual_amount,
            expected_ytd,
            remaining,
            percentage,
            pacing_percentage,
            delta,
            elapsed_months: elapsed,
        },
        monthly_entries,
    })
}

/// Book-wide visible transaction window (`YYYY-MM-DD`). Single `MIN,MAX`
/// query so earliest/latest can never diverge.
fn visible_tx_window(conn: &Connection) -> Result<(Option<String>, Option<String>), String> {
    conn.query_row(
        &format!(
            "SELECT substr(MIN(t.post_date), 1, 10), substr(MAX(t.post_date), 1, 10) FROM transactions t WHERE {}",
            utils::VISIBLE_TX_FILTER
        ),
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|e| e.to_string())
}

fn fetch_budget_date_range_inner(conn: &Connection) -> Result<BudgetDateRange, String> {
    let (earliest, latest) = visible_tx_window(conn)?;

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    // Clamp the upper bound to today so an empty book still offers the
    // current year in the picker.
    let latest_date = match latest {
        Some(l) if l >= today => Some(l),
        _ => Some(today.clone()),
    };

    Ok(BudgetDateRange {
        earliest_date: earliest,
        latest_date,
    })
}

#[tauri::command]
pub async fn set_budget_year_amount(
    state: State<'_, AppState>,
    account_guid: String,
    year: i32,
    amount: f64,
) -> Result<Budget, String> {
    run_blocking(state.db.clone(), move |conn| {
        set_budget_year_amount_inner(conn, &account_guid, year, amount)
    })
    .await
}

#[tauri::command]
pub async fn fetch_budget_years(
    state: State<'_, AppState>,
    account_guid: String,
) -> Result<Vec<i32>, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_budget_years_inner(conn, &account_guid)
    })
    .await
}

#[tauri::command]
pub async fn delete_budget_year(
    state: State<'_, AppState>,
    account_guid: String,
    year: i32,
) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        delete_budget_year_inner(conn, &account_guid, year)
    })
    .await
}

#[tauri::command]
pub async fn fetch_budget_vs_actual(
    state: State<'_, AppState>,
    year: i32,
) -> Result<Vec<BudgetActualRow>, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_budget_vs_actual_inner(conn, year)
    })
    .await
}

#[tauri::command]
pub async fn fetch_budget_detail(
    state: State<'_, AppState>,
    account_guid: String,
    year: i32,
) -> Result<BudgetDetail, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_budget_detail_inner(conn, &account_guid, year)
    })
    .await
}

#[tauri::command]
pub async fn fetch_budget_date_range(state: State<'_, AppState>) -> Result<BudgetDateRange, String> {
    run_blocking(state.db.clone(), fetch_budget_date_range_inner).await
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
            (a - b).abs() < 0.02,
            "expected {b}, got {a}"
        );
    }

    #[test]
    fn carryover_skips_years_without_budget() {
        let conn = setup();
        // Only 2022 and 2024 have budgets; 2023 has none and is skipped.
        set_budget_year_amount_inner(&conn, "food", 2022, 1200.0).unwrap();
        set_budget_year_amount_inner(&conn, "food", 2024, 1200.0).unwrap();
        add_spend(&conn, "t1", "2022-05-10", "food", 600.0);
        add_spend(&conn, "t2", "2023-06-10", "food", 9999.0);

        // 2024: carry = (2022: 1200 − 600); 2023 is skipped despite spending.
        let rows = fetch_budget_vs_actual_inner(&conn, 2024).unwrap();
        assert_eq!(rows.len(), 1);
        assert_close(rows[0].budget_amount, 1200.0);
        assert_close(rows[0].actual_amount, 0.0);
        assert_close(rows[0].rollover_balance, 600.0);
        assert_close(rows[0].remaining, 1800.0);
        assert_close(rows[0].effective_total, 1800.0);

        // 2023 has no budget rows at all: nothing is listed.
        let rows = fetch_budget_vs_actual_inner(&conn, 2023).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn carryover_window_starts_at_first_budget_year() {
        let conn = setup();
        // Transactions in 2021 predate the first budget and are ignored.
        set_budget_year_amount_inner(&conn, "food", 2022, 1200.0).unwrap();
        set_budget_year_amount_inner(&conn, "food", 2023, 1200.0).unwrap();
        add_spend(&conn, "t0", "2021-05-10", "food", 5000.0);
        add_spend(&conn, "t1", "2022-05-10", "food", 600.0);

        // 2023: carry = (2022: 1200 − 600) = 600.
        let rows = fetch_budget_vs_actual_inner(&conn, 2023).unwrap();
        assert_eq!(rows.len(), 1);
        assert_close(rows[0].budget_amount, 1200.0);
        assert_close(rows[0].actual_amount, 0.0);
        assert_close(rows[0].rollover_balance, 600.0);
        assert_close(rows[0].remaining, 1800.0);
        assert_close(rows[0].effective_total, 1800.0);
    }

    #[test]
    fn carryover_includes_descendant_transactions() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "dining", 2023, 2400.0).unwrap();
        set_budget_year_amount_inner(&conn, "dining", 2024, 2400.0).unwrap();
        // First transaction is on the child account, March 2023, spending 300.
        add_spend(&conn, "t1", "2023-03-15", "restaurant", 300.0);

        // 2024: carry = 2023: 2400 − 300 = 2100.
        let rows = fetch_budget_vs_actual_inner(&conn, 2024).unwrap();
        assert_eq!(rows.len(), 1);
        assert_close(rows[0].budget_amount, 2400.0);
        assert_close(rows[0].rollover_balance, 2100.0);
        assert_close(rows[0].remaining, 4500.0);
    }

    #[test]
    fn overspending_reduces_carryover() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "food", 2024, 1200.0).unwrap();
        set_budget_year_amount_inner(&conn, "food", 2025, 1200.0).unwrap();
        // Overspend in the first year: 1500 against a 1200 budget.
        add_spend(&conn, "t1", "2024-01-05", "food", 1500.0);

        // 2025: carry = 1200 − 1500 = −300.
        let rows = fetch_budget_vs_actual_inner(&conn, 2025).unwrap();
        assert_close(rows[0].rollover_balance, -300.0);
        assert_close(rows[0].remaining, 900.0);
    }

    #[test]
    fn no_transactions_yields_zero_carryover() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "unused", 2024, 960.0).unwrap();

        let rows = fetch_budget_vs_actual_inner(&conn, 2024).unwrap();
        assert_close(rows[0].budget_amount, 960.0);
        assert_close(rows[0].actual_amount, 0.0);
        assert_close(rows[0].rollover_balance, 0.0);
        assert_close(rows[0].remaining, 960.0);

        let detail = fetch_budget_detail_inner(&conn, "unused", 2024).unwrap();
        assert_eq!(detail.monthly_entries.len(), 12);
        assert_close(detail.summary.rollover_balance, 0.0);
        assert_close(detail.budget.amount, 960.0);
    }

    #[test]
    fn budgets_are_per_year() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "food", 2024, 600.0).unwrap();
        set_budget_year_amount_inner(&conn, "food", 2025, 1200.0).unwrap();
        add_spend(&conn, "t1", "2024-01-05", "food", 600.0);
        add_spend(&conn, "t2", "2025-02-10", "food", 1400.0);

        // 2025: carry = 2024: 600 − 600 = 0.
        let rows = fetch_budget_vs_actual_inner(&conn, 2025).unwrap();
        assert_eq!(rows.len(), 1);
        assert_close(rows[0].budget_amount, 1200.0);
        assert_close(rows[0].rollover_balance, 0.0);
        assert_close(rows[0].actual_amount, 1400.0);
        assert_close(rows[0].remaining, -200.0);

        // 2026 has no budget: the account is not listed. Carry would be
        // 2024: 0 + 2025: 1200 − 1400 = −200 once 2026 is created.
        let rows = fetch_budget_vs_actual_inner(&conn, 2026).unwrap();
        assert!(rows.is_empty());
        set_budget_year_amount_inner(&conn, "food", 2026, 1200.0).unwrap();
        let rows = fetch_budget_vs_actual_inner(&conn, 2026).unwrap();
        assert_close(rows[0].rollover_balance, -200.0);

        // Detail shows each year's own amount.
        let d2024 = fetch_budget_detail_inner(&conn, "food", 2024).unwrap();
        assert_close(d2024.budget.amount, 600.0);
        let d2025 = fetch_budget_detail_inner(&conn, "food", 2025).unwrap();
        assert_close(d2025.budget.amount, 1200.0);
    }

    #[test]
    fn upsert_overwrites_same_year() {
        let conn = setup();
        let created = set_budget_year_amount_inner(&conn, "food", 2024, 600.0).unwrap();
        assert_eq!(created.year, 2024);
        assert_close(created.amount, 600.0);

        let updated = set_budget_year_amount_inner(&conn, "food", 2024, 900.0).unwrap();
        assert_close(updated.amount, 900.0);

        // Still a single row for the year.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM budgets WHERE account_guid = 'food' AND year = 2024",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_close(
            fetch_budget_detail_inner(&conn, "food", 2024).unwrap().budget.amount,
            900.0,
        );
    }

    #[test]
    fn budget_years_lists_taken_years_ascending() {
        let conn = setup();
        assert!(fetch_budget_years_inner(&conn, "food").unwrap().is_empty());
        set_budget_year_amount_inner(&conn, "food", 2026, 900.0).unwrap();
        set_budget_year_amount_inner(&conn, "food", 2024, 600.0).unwrap();
        assert_eq!(
            fetch_budget_years_inner(&conn, "food").unwrap(),
            vec![2024, 2026]
        );
        assert!(fetch_budget_years_inner(&conn, "unused").unwrap().is_empty());
    }

    #[test]
    fn delete_year_removes_only_that_year() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "food", 2024, 600.0).unwrap();
        set_budget_year_amount_inner(&conn, "food", 2025, 1200.0).unwrap();
        add_spend(&conn, "t1", "2024-01-05", "food", 600.0);

        // 2025 carry with the 2024 row: 600 − 600 = 0.
        let rows = fetch_budget_vs_actual_inner(&conn, 2025).unwrap();
        assert_close(rows[0].rollover_balance, 0.0);

        delete_budget_year_inner(&conn, "food", 2024).unwrap();

        // 2024 is gone but 2025 is untouched; with 2024 skipped the carry is 0.
        assert!(fetch_budget_detail_inner(&conn, "food", 2024).is_err());
        let rows = fetch_budget_vs_actual_inner(&conn, 2024).unwrap();
        assert!(rows.is_empty());
        let rows = fetch_budget_vs_actual_inner(&conn, 2025).unwrap();
        assert_eq!(rows.len(), 1);
        assert_close(rows[0].budget_amount, 1200.0);
        assert_close(rows[0].rollover_balance, 0.0);
    }

    #[test]
    fn detail_missing_year_errors() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "food", 2024, 600.0).unwrap();
        let err = fetch_budget_detail_inner(&conn, "food", 2025).unwrap_err();
        assert_eq!(err, "No budget for this year");
    }

    #[test]
    fn detail_has_twelve_monthly_slices_with_pacing() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "food", 2022, 1200.0).unwrap();
        set_budget_year_amount_inner(&conn, "food", 2023, 1200.0).unwrap();
        add_spend(&conn, "t1", "2022-05-10", "food", 600.0);
        add_spend(&conn, "t2", "2023-01-15", "food", 100.0);
        add_spend(&conn, "t3", "2023-02-15", "food", 100.0);

        // 2023: carry = 2022: 1200 − 600 = 600, effective = 1800.
        let detail = fetch_budget_detail_inner(&conn, "food", 2023).unwrap();
        assert_eq!(detail.budget.year, 2023);
        assert_eq!(detail.monthly_entries.len(), 12);
        assert_close(detail.summary.rollover_balance, 600.0);
        assert_close(detail.summary.effective_total, 1800.0);
        assert_close(detail.summary.actual_amount, 200.0);
        assert_close(detail.summary.remaining, 1600.0);

        // January cumulative: 100 vs front-loaded expected 1200/12 + 600 = 700.
        let jan = &detail.monthly_entries[0];
        assert_eq!(jan.month, 1);
        assert_close(jan.actual_amount, 100.0);
        assert_close(jan.cumulative_actual, 100.0);
        assert_close(jan.cumulative_expected, 700.0);
        // February cumulative: 200 vs 2400/12 + 600 = 800.
        let feb = &detail.monthly_entries[1];
        assert_close(feb.cumulative_actual, 200.0);
        assert_close(feb.cumulative_expected, 800.0);
    }

    #[test]
    fn pacing_uses_expected_ytd() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "food", 2023, 1200.0).unwrap();
        // No spending: pacing must be 0 and delta equals expected YTD.
        // Front-loaded: annual × elapsed/12 + carry (carry is 0 here).
        let rows = fetch_budget_vs_actual_inner(&conn, 2023).unwrap();
        let elapsed = rows[0].elapsed_months;
        assert!((0..=12).contains(&elapsed));
        let expected = rows[0].budget_amount * (elapsed as f64) / 12.0 + rows[0].rollover_balance;
        assert_close(rows[0].expected_ytd, expected);
        assert_close(rows[0].delta, expected - rows[0].actual_amount);
    }

    #[test]
    fn front_loaded_carry_counts_in_full() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "food", 2022, 1200.0).unwrap();
        set_budget_year_amount_inner(&conn, "food", 2023, 1200.0).unwrap();
        add_spend(&conn, "t1", "2022-05-10", "food", 600.0);

        // 2023: annual 1200, carry 600. Front-loaded January expects
        // 1200/12 + 600 = 700 (not 1800/12 = 150).
        let detail = fetch_budget_detail_inner(&conn, "food", 2023).unwrap();
        assert_close(detail.summary.rollover_balance, 600.0);
        assert_close(detail.monthly_entries[0].cumulative_expected, 700.0);
        assert_close(detail.monthly_entries[11].cumulative_expected, 1800.0);
    }

    #[test]
    fn invalid_year_and_amount_rejected() {
        let conn = setup();
        set_budget_year_amount_inner(&conn, "food", 2024, 1200.0).unwrap();
        assert!(set_budget_year_amount_inner(&conn, "food", 1800, 500.0).is_err());
        assert!(set_budget_year_amount_inner(&conn, "food", 2024, 0.0).is_err());
        assert!(set_budget_year_amount_inner(&conn, "food", 2025, 500.0).is_ok());
        assert!(fetch_budget_vs_actual_inner(&conn, 1800).is_err());
        assert!(fetch_budget_detail_inner(&conn, "food", 1800).is_err());
    }

    #[test]
    fn non_expense_accounts_rejected() {
        let conn = setup();
        assert!(set_budget_year_amount_inner(&conn, "root", 2024, 500.0).is_err());
        assert!(set_budget_year_amount_inner(&conn, "missing", 2024, 500.0).is_err());
    }
}
