use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{AppState, run_blocking};
use crate::fx;
use crate::sql;
use crate::tree;
use crate::utils;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLossRow {
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub indent: i32,
    pub current_amount: f64,
    pub previous_amount: f64,
    pub change_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLossReport {
    pub date_from: String,
    pub date_to: String,
    pub prev_date_from: String,
    pub prev_date_to: String,
    pub income_rows: Vec<ProfitLossRow>,
    pub income_root_guid: Option<String>,
    pub total_income: f64,
    pub total_income_prev: f64,
    pub expense_rows: Vec<ProfitLossRow>,
    pub expense_root_guid: Option<String>,
    pub total_expenses: f64,
    pub total_expenses_prev: f64,
    pub net_income: f64,
}

struct RawAccountBalance {
    guid: String,
    name: String,
    description: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
    commodity: Option<String>,
    balance: f64,
}

fn build_tree_adjacency(rows: &[RawAccountBalance], account_types: &[&str]) -> tree::TreeIndex {
    let included: Vec<&RawAccountBalance> = rows
        .iter()
        .filter(|r| account_types.contains(&r.account_type.as_str()))
        .collect();

    // Drop top-level placeholders whose parent is outside the filtered set:
    // they only group children and would render as empty roots otherwise.
    let guids: HashSet<&str> = included.iter().map(|r| r.guid.as_str()).collect();
    let filtered: Vec<&RawAccountBalance> = included
        .into_iter()
        .filter(|r| {
            !(r.placeholder
                && r.parent_guid
                    .as_ref()
                    .is_none_or(|p| !guids.contains(p.as_str())))
        })
        .collect();

    let triples = filtered
        .iter()
        .map(|r| (r.guid.clone(), r.parent_guid.clone(), r.name.clone()));
    tree::build_index(triples)
}

fn fetch_account_balances(
    conn: &Connection,
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> Result<Vec<RawAccountBalance>, String> {
    // Build a CASE expression that only includes split amounts when the
    // transaction's post_date falls within the range. Using CASE inside SUM
    // instead of filtering in the LEFT JOIN ensures accounts with no matching
    // transactions still appear (with balance 0). The upper bound is capped at
    // the shared visibility window so a period ending today or later never
    // includes future-dated transactions.
    let amount = utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type");
    let (date_case, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
        match (date_from, date_to) {
            (Some(f), Some(t)) => (
                format!(
                    "CASE WHEN t.post_date >= ?1 AND t.post_date <= min(?2, date('now', '+1 day')) THEN {amount} ELSE 0 END"
                ),
                vec![Box::new(f.to_string()), Box::new(t.to_string())],
            ),
            (Some(f), None) => (
                format!(
                    "CASE WHEN t.post_date >= ?1 AND {} THEN {amount} ELSE 0 END",
                    utils::VISIBLE_TX_FILTER
                ),
                vec![Box::new(f.to_string())],
            ),
            (None, Some(t)) => (
                format!("CASE WHEN t.post_date <= min(?1, date('now', '+1 day')) THEN {amount} ELSE 0 END"),
                vec![Box::new(t.to_string())],
            ),
            (None, None) => (
                format!("CASE WHEN {} THEN {amount} ELSE 0 END", utils::VISIBLE_TX_FILTER),
                vec![],
            ),
        };

    let sql = format!(
        "SELECT a.guid, a.name, a.description, a.account_type, a.parent_guid, a.placeholder,
                c.mnemonic as commodity,
                COALESCE(SUM({}), 0) as balance
         FROM accounts a
         LEFT JOIN commodities c ON c.guid = a.commodity_guid
         LEFT JOIN splits s ON s.account_guid = a.guid
         LEFT JOIN transactions t ON t.guid = s.tx_guid
         WHERE a.account_type != 'ROOT'
         GROUP BY a.guid, a.name, a.description, a.account_type, a.parent_guid, a.placeholder, c.mnemonic
         ORDER BY a.name",
        date_case,
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let param_refs = sql::param_refs(&params);
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(RawAccountBalance {
                guid: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                account_type: row.get(3)?,
                parent_guid: row.get(4)?,
                placeholder: row.get::<_, i64>(5)? != 0,
                commodity: row.get(6)?,
                balance: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Convert each account's balance from its own commodity to the report currency as of `as_of`.
fn convert_balances(
    rows: &mut [RawAccountBalance],
    conn: &Connection,
    as_of: &str,
) -> Result<(), String> {
    let report = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_map = fx::get_fx_map(conn, Some(as_of))?;
    for r in rows.iter_mut() {
        if let Some(ref commodity) = r.commodity {
            r.balance = fx::convert(r.balance, commodity, &report, &fx_map);
        }
    }
    Ok(())
}

fn walk_tree<R>(
    rows_by_guid: &HashMap<&str, &RawAccountBalance>,
    children: &HashMap<String, Vec<String>>,
    guid: &str,
    indent: i32,
    result: &mut Vec<R>,
    push_row: &impl Fn(&RawAccountBalance, i32, f64) -> R,
    visited: &mut HashSet<String>,
) -> f64 {
    if !visited.insert(guid.to_string()) {
        return 0.0;
    }
    let Some(r) = rows_by_guid.get(guid) else {
        return 0.0;
    };
    let mut total = r.balance;
    if let Some(kids) = children.get(guid) {
        for kid in kids {
            total += walk_tree(
                rows_by_guid,
                children,
                kid,
                indent + 1,
                result,
                push_row,
                visited,
            );
        }
    }
    result.push(push_row(r, indent, total));
    total
}

fn build_tree<R>(
    rows: &[RawAccountBalance],
    account_types: &[&str],
    push_row: impl Fn(&RawAccountBalance, i32, f64) -> R,
) -> Vec<R> {
    let index = build_tree_adjacency(rows, account_types);
    let rows_by_guid: HashMap<&str, &RawAccountBalance> =
        rows.iter().map(|r| (r.guid.as_str(), r)).collect();
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    for root in &index.roots {
        walk_tree(
            &rows_by_guid,
            &index.children,
            root,
            0,
            &mut result,
            &push_row,
            &mut visited,
        );
    }
    result
}

pub fn generate_pnl_report_inner(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
    prev_date_from: Option<&str>,
    prev_date_to: Option<&str>,
) -> Result<ProfitLossReport, String> {
    let mut current_rows = fetch_account_balances(conn, Some(date_from), Some(date_to))?;
    convert_balances(&mut current_rows, conn, date_to)?;

    let push_pnl = |r: &RawAccountBalance, indent: i32, total: f64| -> ProfitLossRow {
        ProfitLossRow {
            account_guid: r.guid.clone(),
            account_name: r.name.clone(),
            account_description: r.description.clone(),
            indent,
            current_amount: total,
            previous_amount: 0.0,
            change_pct: None,
        }
    };
    let income_rows = build_tree(&current_rows, &["INCOME"], push_pnl);
    let expense_rows = build_tree(&current_rows, &["EXPENSE"], push_pnl);

    let income_total: f64 = income_rows
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.current_amount)
        .sum();
    let expense_total: f64 = expense_rows
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.current_amount)
        .sum();

    // Previous period — subtract span to get same-length prior window
    let (prev_date_from_owned, prev_date_to_owned) = match (prev_date_from, prev_date_to) {
        (Some(pf), Some(pt)) => (pf.to_string(), pt.to_string()),
        (Some(pf), None) => (pf.to_string(), date_from.to_string()),
        (None, Some(pt)) => (date_from.to_string(), pt.to_string()),
        (None, None) => {
            let f = utils::month_number(date_from)?;
            let t = utils::month_number(date_to)?;
            let span = t - f;
            let prev_start = utils::format_month(f - span - 1);
            let prev_end = utils::format_month(f - 1);
            (prev_start, prev_end)
        }
    };

    let mut prev_rows =
        fetch_account_balances(conn, Some(&prev_date_from_owned), Some(&prev_date_to_owned))?;
    convert_balances(&mut prev_rows, conn, &prev_date_to_owned)?;

    let prev_income_rows = build_tree(&prev_rows, &["INCOME"], push_pnl);
    let prev_expense_rows = build_tree(&prev_rows, &["EXPENSE"], push_pnl);

    let mut prev_income_map: HashMap<String, f64> = HashMap::new();
    for r in &prev_income_rows {
        prev_income_map.insert(r.account_guid.clone(), r.current_amount);
    }
    let mut prev_expense_map: HashMap<String, f64> = HashMap::new();
    for r in &prev_expense_rows {
        prev_expense_map.insert(r.account_guid.clone(), r.current_amount);
    }

    fn merge_prev_amount(mut r: ProfitLossRow, prev_map: &HashMap<String, f64>) -> ProfitLossRow {
        r.previous_amount = *prev_map.get(&r.account_guid).unwrap_or(&0.0);
        r.change_pct = if r.previous_amount != 0.0 {
            Some((r.current_amount - r.previous_amount) / r.previous_amount * 100.0)
        } else if r.current_amount != 0.0 {
            Some(100.0)
        } else {
            None
        };
        r
    }

    let income_rows: Vec<ProfitLossRow> = income_rows
        .into_iter()
        .map(|r| merge_prev_amount(r, &prev_income_map))
        .collect();

    let expense_rows: Vec<ProfitLossRow> = expense_rows
        .into_iter()
        .map(|r| merge_prev_amount(r, &prev_expense_map))
        .collect();

    let income_prev_total: f64 = income_rows
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.previous_amount)
        .sum();
    let expense_prev_total: f64 = expense_rows
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.previous_amount)
        .sum();

    let income_root_guid = income_rows
        .iter()
        .find(|r| r.indent == 0)
        .map(|r| r.account_guid.clone());
    let expense_root_guid = expense_rows
        .iter()
        .find(|r| r.indent == 0)
        .map(|r| r.account_guid.clone());

    Ok(ProfitLossReport {
        date_from: date_from.to_string(),
        date_to: date_to.to_string(),
        prev_date_from: prev_date_from_owned,
        prev_date_to: prev_date_to_owned,
        income_rows,
        income_root_guid,
        total_income: income_total,
        total_income_prev: income_prev_total,
        expense_rows,
        expense_root_guid,
        total_expenses: expense_total,
        total_expenses_prev: expense_prev_total,
        net_income: income_total - expense_total,
    })
}

#[tauri::command]
pub async fn generate_pnl_report(
    state: State<'_, AppState>,
    date_from: String,
    date_to: String,
    prev_date_from: Option<String>,
    prev_date_to: Option<String>,
) -> Result<ProfitLossReport, String> {
    run_blocking(state.db.clone(), move |conn| {
        generate_pnl_report_inner(
            conn,
            &date_from,
            &date_to,
            prev_date_from.as_deref(),
            prev_date_to.as_deref(),
        )
    })
    .await
}
