use crate::db::AppError;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::tree;
use crate::utils;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLossRow {
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub account_code: Option<String>,
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
    pub report_currency: String,
    pub income_rows: Vec<ProfitLossRow>,
    pub income_root_guid: Option<String>,
    pub total_income: f64,
    pub total_income_prev: f64,
    pub expense_rows: Vec<ProfitLossRow>,
    pub expense_root_guid: Option<String>,
    pub total_expenses: f64,
    pub total_expenses_prev: f64,
    pub net_income: f64,
    pub net_income_prev: f64,
    pub net_income_change_pct: Option<f64>,
    pub missing_fx: Vec<String>,
}

struct RawAccountBalance {
    guid: String,
    name: String,
    description: Option<String>,
    code: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
    balance: f64,
}

struct AccountMeta {
    name: String,
    description: Option<String>,
    code: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
}

/// Full account hierarchy for ancestor injection: every non-ROOT account with
/// its parent link. P&L buckets only carry accounts with in-window splits, so
/// without this the tree walker promotes children of inactive placeholders to
/// roots (flattened tables).
fn fetch_account_meta(conn: &Connection) -> Result<HashMap<String, AccountMeta>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT guid, name, description, code, account_type, parent_guid, placeholder
             FROM accounts WHERE account_type != 'ROOT'",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                AccountMeta {
                    name: row.get(1)?,
                    description: row.get(2)?,
                    code: row.get(3)?,
                    account_type: row.get(4)?,
                    parent_guid: row.get(5)?,
                    placeholder: row.get::<_, i64>(6)? != 0,
                },
            ))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(rows)
}

fn is_pnl_type(account_type: &str) -> bool {
    account_type == "INCOME" || account_type == "EXPENSE"
}

/// Ancestor chain of `guid` through `meta`, stopping at ROOT/missing parents.
fn ancestor_chain(guid: &str, meta: &HashMap<String, AccountMeta>) -> Vec<String> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut next = meta.get(guid).and_then(|m| m.parent_guid.clone());
    while let Some(parent) = next {
        if !seen.insert(parent.clone()) {
            break;
        }
        let Some(parent_meta) = meta.get(&parent) else {
            break;
        };
        chain.push(parent.clone());
        next = parent_meta.parent_guid.clone();
    }
    chain
}

/// Expand P&L row sets so inactive placeholder parents render with rolled-up
/// totals instead of flattening children to roots. Only ancestors of active
/// INCOME/EXPENSE accounts are added (zero-only branches stay hidden). Both
/// windows are expanded to the union of active guids so prev-only accounts
/// still render with a zero current amount (and vice versa); per-window
/// balances stay intact and root sums are unchanged.
fn augment_pnl_rows(
    current: Vec<RawAccountBalance>,
    prev: Vec<RawAccountBalance>,
    meta: &HashMap<String, AccountMeta>,
) -> (Vec<RawAccountBalance>, Vec<RawAccountBalance>) {
    let mut wanted: HashSet<String> = HashSet::new();
    for r in current.iter().chain(prev.iter()) {
        if is_pnl_type(&r.account_type) {
            wanted.insert(r.guid.clone());
        }
    }
    for guid in wanted.clone() {
        wanted.extend(ancestor_chain(&guid, meta));
    }

    let mut cur_map: HashMap<String, RawAccountBalance> =
        current.into_iter().map(|r| (r.guid.clone(), r)).collect();
    let mut prev_map: HashMap<String, RawAccountBalance> =
        prev.into_iter().map(|r| (r.guid.clone(), r)).collect();
    for guid in &wanted {
        for map in [&mut cur_map, &mut prev_map] {
            if !map.contains_key(guid) {
                if let Some(m) = meta.get(guid) {
                    map.insert(
                        guid.clone(),
                        RawAccountBalance {
                            guid: guid.clone(),
                            name: m.name.clone(),
                            description: m.description.clone(),
                            code: m.code.clone(),
                            account_type: m.account_type.clone(),
                            parent_guid: m.parent_guid.clone(),
                            placeholder: m.placeholder,
                            balance: 0.0,
                        },
                    );
                }
            }
        }
    }

    let mut current: Vec<RawAccountBalance> = cur_map.into_values().collect();
    let mut prev: Vec<RawAccountBalance> = prev_map.into_values().collect();
    current.sort_by(|a, b| a.name.cmp(&b.name));
    prev.sort_by(|a, b| a.name.cmp(&b.name));
    (current, prev)
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

/// One account's signed activity in one calendar month. Sign follows
/// `signed_amount_expr` (credit-normal types negated, so INCOME reads
/// positive like everywhere else).
struct MonthlyBucket {
    guid: String,
    name: String,
    description: Option<String>,
    code: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
    commodity: Option<String>,
    month: String,
    raw: f64,
}

/// Per-account activity per calendar month within `[date_from, date_to]`
/// (half-open, visibility-capped like every money query). Only accounts with
/// in-window splits appear — zero-activity accounts contribute nothing to P&L
/// trees, so omitting them just drops €0 rows.
fn fetch_monthly_buckets(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
) -> Result<Vec<MonthlyBucket>, AppError> {
    let amount = utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type");
    let visible_now = utils::VISIBLE_NOW_EXPR;
    let mut stmt = conn.prepare(&format!(
        "SELECT a.guid, a.name, a.description, a.code, a.account_type, a.parent_guid,
                    a.placeholder, c.mnemonic as commodity, strftime('%Y-%m', t.post_date) as month,
                    SUM({amount}) as total
             FROM splits s
             JOIN transactions t ON t.guid = s.tx_guid
             JOIN accounts a ON a.guid = s.account_guid
             LEFT JOIN commodities c ON c.guid = a.commodity_guid
             WHERE a.account_type != 'ROOT'
               AND t.post_date >= ?1
               AND t.post_date < min(date(?2, '+1 day'), {visible_now})
             GROUP BY a.guid, a.name, a.description, a.code, a.account_type, a.parent_guid,
                      a.placeholder, c.mnemonic, month
             ORDER BY month, a.name"
    ))?;
    let rows = stmt
        .query_map([date_from, date_to], |row| {
            Ok(MonthlyBucket {
                guid: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                code: row.get(3)?,
                account_type: row.get(4)?,
                parent_guid: row.get(5)?,
                placeholder: row.get::<_, i64>(6)? != 0,
                commodity: row.get(7)?,
                month: row.get(8)?,
                raw: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// A monthly bucket converted to report currency at its month-end rate.
struct ConvertedBucket {
    guid: String,
    name: String,
    description: Option<String>,
    code: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
    commodity: Option<String>,
    month: String,
    amount: f64,
}

/// Convert buckets at month-end rates via a single `FxHistory` load (one rate
/// map per distinct month). Returns converted buckets, the sorted missing-FX
/// list and report currency. Zero buckets convert to zero at any rate and
/// never warn.
fn convert_buckets(
    conn: &Connection,
    buckets: &[MonthlyBucket],
) -> Result<(Vec<ConvertedBucket>, Vec<String>, String), AppError> {
    let report = fx::report_currency_or_empty(conn)?;
    let fx_history = fx::FxHistory::load(conn)?;
    let mut missing = HashSet::new();
    let mut maps: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut out = Vec::with_capacity(buckets.len());
    for b in buckets {
        if b.raw == 0.0 {
            continue;
        }
        if !maps.contains_key(&b.month) {
            let end = utils::month_end(&b.month)?;
            maps.insert(b.month.clone(), fx_history.map_for_date(&end));
        }
        let fx_at = &maps[&b.month];
        let amount = match b.commodity.as_deref() {
            Some(m) => fx::convert_tracked(b.raw, m, &report, fx_at, Some(&mut missing)),
            None => b.raw,
        };
        out.push(ConvertedBucket {
            guid: b.guid.clone(),
            name: b.name.clone(),
            description: b.description.clone(),
            code: b.code.clone(),
            account_type: b.account_type.clone(),
            parent_guid: b.parent_guid.clone(),
            placeholder: b.placeholder,
            commodity: b.commodity.clone(),
            month: b.month.clone(),
            amount,
        });
    }
    let mut missing: Vec<String> = missing.into_iter().collect();
    missing.sort();
    Ok((out, missing, report))
}

/// Roll converted buckets up to per-account totals for the tree walker,
/// ordered by account name like the old balance query.
fn roll_up_buckets(buckets: &[ConvertedBucket]) -> Vec<RawAccountBalance> {
    struct Acc<'a> {
        name: &'a str,
        description: Option<&'a str>,
        code: Option<&'a str>,
        account_type: &'a str,
        parent_guid: Option<&'a str>,
        placeholder: bool,
        total: f64,
    }
    let mut map: HashMap<&str, Acc> = HashMap::new();
    for b in buckets {
        let entry = map.entry(b.guid.as_str()).or_insert(Acc {
            name: b.name.as_str(),
            description: b.description.as_deref(),
            code: b.code.as_deref(),
            account_type: b.account_type.as_str(),
            parent_guid: b.parent_guid.as_deref(),
            placeholder: b.placeholder,
            total: 0.0,
        });
        entry.total += b.amount;
    }
    let mut rows: Vec<RawAccountBalance> = map
        .into_iter()
        .map(|(guid, a)| RawAccountBalance {
            guid: guid.to_string(),
            name: a.name.to_string(),
            description: a.description.map(|d| d.to_string()),
            code: a.code.map(|c| c.to_string()),
            account_type: a.account_type.to_string(),
            parent_guid: a.parent_guid.map(|p| p.to_string()),
            placeholder: a.placeholder,
            balance: a.total,
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

fn walk_tree<T>(
    rows_by_guid: &HashMap<&str, &RawAccountBalance>,
    children: &HashMap<String, Vec<String>>,
    guid: &str,
    indent: i32,
    result: &mut Vec<T>,
    push_row: &impl Fn(&RawAccountBalance, i32, f64) -> T,
    visited: &mut HashSet<String>,
) -> f64 {
    if !visited.insert(guid.to_string()) {
        return 0.0;
    }
    let Some(r) = rows_by_guid.get(guid) else {
        return 0.0;
    };
    // Pre-order: parents precede children so the frontend can rebuild the
    // tree in one forward pass (`report-tree.ts:buildTree` contract). Children
    // accumulate into a scratch vec, then the rolled-up parent is emitted
    // first followed by its subtree.
    let mut total = r.balance;
    let mut child_rows = Vec::new();
    if let Some(kids) = children.get(guid) {
        for kid in kids {
            total += walk_tree(
                rows_by_guid,
                children,
                kid,
                indent + 1,
                &mut child_rows,
                push_row,
                visited,
            );
        }
    }
    result.push(push_row(r, indent, total));
    result.extend(child_rows);
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

/// Percent change for P&L rows. `None` when both periods are zero; a new
/// account with no prior balance reports 100% (matches the frontend
/// `changePct` in `pnl-utils.ts` — keep the two in sync).
fn pnl_change_pct(current: f64, previous: f64) -> Option<f64> {
    if previous != 0.0 {
        Some((current - previous) / previous * 100.0)
    } else if current != 0.0 {
        Some(100.0)
    } else {
        None
    }
}

/// One P&L window in report currency: converted monthly buckets, the
/// per-account rollup for the tree walker, missing-FX mnemonics and the
/// window's report currency.
struct PnlWindow {
    rows: Vec<RawAccountBalance>,
    converted: Vec<ConvertedBucket>,
    missing: Vec<String>,
    report_currency: String,
}

/// Load one P&L window: monthly buckets, month-end conversion and the
/// per-account rollup consumed by the tree walker.
fn fetch_pnl_window(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
) -> Result<PnlWindow, AppError> {
    let buckets = fetch_monthly_buckets(conn, date_from, date_to)?;
    let (converted, missing, report_currency) = convert_buckets(conn, &buckets)?;
    let rows = roll_up_buckets(&converted);
    Ok(PnlWindow {
        rows,
        converted,
        missing,
        report_currency,
    })
}

fn resolve_pnl_prev_window(
    date_from: &str,
    date_to: &str,
    prev_date_from: Option<&str>,
    prev_date_to: Option<&str>,
) -> Result<(String, String), AppError> {
    Ok(match (prev_date_from, prev_date_to) {
        (Some(pf), Some(pt)) => (pf.to_string(), pt.to_string()),
        (Some(pf), None) => (pf.to_string(), date_from.to_string()),
        (None, Some(pt)) => (date_from.to_string(), pt.to_string()),
        (None, None) => {
            let f = utils::month_number(date_from)?;
            let t = utils::month_number(date_to)?;
            let span = t - f;
            let prev_start = utils::format_month(f - span - 1);
            let prev_end = utils::format_month(f - 1);
            // Full dates: `date('2024-01', '+1 day')` is NULL in SQLite, which
            // would silently empty the previous window (every change = 100%).
            (format!("{prev_start}-01"), utils::month_end(&prev_end)?)
        }
    })
}

fn merge_prev_amount(mut r: ProfitLossRow, prev_map: &HashMap<String, f64>) -> ProfitLossRow {
    r.previous_amount = *prev_map.get(&r.account_guid).unwrap_or(&0.0);
    r.change_pct = pnl_change_pct(r.current_amount, r.previous_amount);
    r
}

/// Merge previous-window row totals into current rows by account guid.
fn merge_pnl_prev_amounts(
    current_rows: Vec<ProfitLossRow>,
    prev_rows: &[ProfitLossRow],
) -> Vec<ProfitLossRow> {
    let mut prev_map: HashMap<String, f64> = HashMap::new();
    for r in prev_rows {
        prev_map.insert(r.account_guid.clone(), r.current_amount);
    }
    current_rows
        .into_iter()
        .map(|r| merge_prev_amount(r, &prev_map))
        .collect()
}

fn fetch_pnl_report_inner(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
    prev_date_from: Option<&str>,
    prev_date_to: Option<&str>,
) -> Result<ProfitLossReport, AppError> {
    let date_from = utils::validate_iso_date(date_from)?;
    let date_to = utils::validate_iso_date(date_to)?;
    let prev_date_from = utils::validate_optional_iso_date(prev_date_from)?;
    let prev_date_to = utils::validate_optional_iso_date(prev_date_to)?;
    if date_from > date_to {
        return Err(AppError::Validation(
            "Start date is after end date. Flip them around".to_string(),
        ));
    }
    let current = fetch_pnl_window(conn, &date_from, &date_to)?;

    let push_pnl = |r: &RawAccountBalance, indent: i32, total: f64| -> ProfitLossRow {
        ProfitLossRow {
            account_guid: r.guid.clone(),
            account_name: r.name.clone(),
            account_description: r.description.clone(),
            account_code: r.code.clone(),
            indent,
            current_amount: total,
            previous_amount: 0.0,
            change_pct: None,
        }
    };

    // Previous period — subtract span to get same-length prior window
    let (prev_date_from_owned, prev_date_to_owned) = resolve_pnl_prev_window(
        &date_from,
        &date_to,
        prev_date_from.as_deref(),
        prev_date_to.as_deref(),
    )?;
    let prev = fetch_pnl_window(conn, &prev_date_from_owned, &prev_date_to_owned)?;

    // Re-attach inactive placeholder parents (and union prev-only accounts)
    // so the trees keep their hierarchy instead of promoting children to
    // roots. Totals are unaffected: injected rows carry zero balances.
    let meta = fetch_account_meta(conn)?;
    let (current_rows, prev_rows) = augment_pnl_rows(current.rows, prev.rows, &meta);

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

    // Union of currencies missing in either window; either window converts at
    // 1.0 parity, so either can skew its totals. Only currencies on
    // INCOME/EXPENSE buckets matter here: the trees below drop non-P&L types,
    // so their mnemonics must not raise a warning.
    let mut missing_set: HashSet<String> = current.missing.into_iter().collect();
    missing_set.extend(prev.missing);
    let pnl_currencies: HashSet<&str> = current
        .converted
        .iter()
        .chain(prev.converted.iter())
        .filter(|b| b.account_type == "INCOME" || b.account_type == "EXPENSE")
        .filter_map(|b| b.commodity.as_deref())
        .collect();
    let mut missing_fx: Vec<String> = missing_set
        .into_iter()
        .filter(|m| pnl_currencies.contains(m.as_str()))
        .collect();
    missing_fx.sort();
    let report_currency = if current.report_currency.is_empty() {
        prev.report_currency
    } else {
        current.report_currency
    };

    let prev_income_rows = build_tree(&prev_rows, &["INCOME"], push_pnl);
    let prev_expense_rows = build_tree(&prev_rows, &["EXPENSE"], push_pnl);

    let income_rows = merge_pnl_prev_amounts(income_rows, &prev_income_rows);
    let expense_rows = merge_pnl_prev_amounts(expense_rows, &prev_expense_rows);

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

    let net_income = income_total - expense_total;
    let net_income_prev = income_prev_total - expense_prev_total;
    Ok(ProfitLossReport {
        date_from: date_from.to_string(),
        date_to: date_to.to_string(),
        prev_date_from: prev_date_from_owned,
        prev_date_to: prev_date_to_owned,
        report_currency,
        income_rows,
        income_root_guid,
        total_income: income_total,
        total_income_prev: income_prev_total,
        expense_rows,
        expense_root_guid,
        total_expenses: expense_total,
        total_expenses_prev: expense_prev_total,
        net_income,
        net_income_prev,
        net_income_change_pct: pnl_change_pct(net_income, net_income_prev),
        missing_fx,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlTrendPoint {
    pub month: String,
    pub income: f64,
    pub expenses: f64,
}

/// Monthly income/expense totals over `[date_from, date_to]` for the P&L
/// trend chart. Shares the monthly-bucket engine (and month-end FX) with the
/// statement, so chart and table always agree. Months without activity are
/// zero-filled for a continuous series.
fn fetch_pnl_trend_inner(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
) -> Result<Vec<PnlTrendPoint>, AppError> {
    let date_from = utils::validate_iso_date(date_from)?;
    let date_to = utils::validate_iso_date(date_to)?;
    if date_from > date_to {
        return Err(AppError::Validation(
            "Start date is after end date. Flip them around".to_string(),
        ));
    }
    let buckets = fetch_monthly_buckets(conn, &date_from, &date_to)?;
    let (converted, _, _) = convert_buckets(conn, &buckets)?;
    let mut per_month: HashMap<String, (f64, f64)> = HashMap::new();
    for b in &converted {
        if b.account_type == "INCOME" {
            per_month.entry(b.month.clone()).or_default().0 += b.amount;
        } else if b.account_type == "EXPENSE" {
            per_month.entry(b.month.clone()).or_default().1 += b.amount;
        }
    }
    let start = utils::month_number(&date_from)?;
    let end = utils::month_number(&date_to)?;
    let mut points = Vec::with_capacity((end - start + 1).max(0) as usize);
    for m in start..=end {
        let month = utils::format_month(m);
        let (income, expenses) = per_month.get(&month).copied().unwrap_or((0.0, 0.0));
        points.push(PnlTrendPoint {
            month,
            income,
            expenses,
        });
    }
    Ok(points)
}

#[tauri::command]
pub async fn fetch_pnl_report(
    state: State<'_, AppState>,
    date_from: String,
    date_to: String,
    prev_date_from: Option<String>,
    prev_date_to: Option<String>,
) -> Result<ProfitLossReport, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_pnl_report_inner(
            conn,
            &date_from,
            &date_to,
            prev_date_from.as_deref(),
            prev_date_to.as_deref(),
        )
        .map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn fetch_pnl_trend(
    state: State<'_, AppState>,
    date_from: String,
    date_to: String,
) -> Result<Vec<PnlTrendPoint>, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_pnl_trend_inner(conn, &date_from, &date_to).map_err(String::from)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use rusqlite::params;

    /// Regression: the `(None, None)` fallback returned month-only strings
    /// (`2023-12`), which SQLite's `date(?, '+1 day')` turns into NULL — the
    /// whole previous window silently read as zero.
    #[test]
    fn prev_window_fallback_uses_full_dates() {
        let (from, to) = resolve_pnl_prev_window("2024-03-01", "2024-05-31", None, None).unwrap();
        assert_eq!(from, "2023-12-01");
        assert_eq!(to, "2024-02-29");
        // The bounds must survive SQLite date math (non-NULL).
        let conn = Connection::open_in_memory().unwrap();
        let advanced: Option<String> = conn
            .query_row("SELECT date(?1, '+1 day')", params![to], |r| r.get(0))
            .unwrap();
        assert!(advanced.is_some(), "month-end must be a full date");
    }

    /// Regression: the P&L commands did no date validation, so `2024-1-5`
    /// compared lexicographically / emptied the window silently.
    #[test]
    fn pnl_rejects_malformed_dates() {
        let conn = setup_pnl_fixture();
        assert!(fetch_pnl_trend_inner(&conn, "2024-1-5", "2024-02-01").is_err());
        assert!(fetch_pnl_report_inner(&conn, "2024-01-01", "2024-2-01", None, None).is_err());
    }

    fn setup_pnl_fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES
             ('c-eur', 'EUR', 'CURRENCY'), ('c-usd', 'USD', 'CURRENCY'),
             ('c-aapl', 'AAPL', 'NASDAQ')",
            [],
        )
        .unwrap();
        // ROOT commodity sets the report currency to EUR.
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur'),
                    ('loan', 'Loan', 'LIABILITY', 'root', 0, 'c-eur'),
                    ('equity', 'Opening', 'EQUITY', 'root', 0, 'c-eur'),
                    ('salary', 'Salary', 'INCOME', 'root', 0, 'c-eur'),
                    ('rent', 'Rent', 'EXPENSE', 'root', 0, 'c-eur'),
                    ('broker', 'Broker', 'STOCK', 'root', 0, 'c-aapl')",
            [],
        )
        .unwrap();
        // Opening equity 1000 -> checking; salary 2000 -> checking; rent 500.
        for (tag, date, debit, credit, amount) in [
            ("t0", "2024-01-01", "checking", "equity", 100_000i64),
            ("t1", "2024-02-01", "checking", "salary", 200_000i64),
            ("t2", "2024-03-01", "rent", "checking", 50_000i64),
        ] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![tag, date],
            )
            .unwrap();
            // Double entry like GnuCash: debit positive, credit negative.
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                params![format!("s-{tag}-d"), tag, debit, amount],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                params![format!("s-{tag}-c"), tag, credit, -amount],
            )
            .unwrap();
        }
        // Buy 10 AAPL @ 100 (value +1000, quantity +10) funded from checking.
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t3', '2024-04-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-t3-d', 't3', 'broker', 100000, 100, 1000, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-t3-c', 't3', 'checking', -100000, 100, -100000, 100)",
            [],
        )
        .unwrap();
        // AAPL marked at 120: 10 shares -> market 1200, cost 1000.
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES ('c-aapl-2024-06-01', 'c-aapl', 'c-eur', 12000, 100, '2024-06-01')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn pnl_rejects_inverted_range() {
        let conn = setup_pnl_fixture();
        assert!(fetch_pnl_report_inner(&conn, "2024-04-01", "2024-01-01", None, None).is_err());
    }

    #[test]
    fn pnl_converts_each_month_at_its_month_end_rate() {
        let conn = setup_pnl_fixture();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('consult', 'Consulting', 'INCOME', 'root', 0, 'c-usd')",
            [],
        )
        .unwrap();
        for (tag, date) in [("t-jan", "2024-01-15"), ("t-feb", "2024-02-15")] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![tag, date],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, 'checking', 10000, 100, 10000, 100)",
                params![format!("s-{tag}-d"), tag],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, 'consult', -10000, 100, -10000, 100)",
                params![format!("s-{tag}-c"), tag],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES ('p-usd-jan', 'c-usd', 'c-eur', 50, 100, '2024-01-31'),
                    ('p-usd-feb', 'c-usd', 'c-eur', 100, 100, '2024-02-28')",
            [],
        )
        .unwrap();
        // January's 100 USD converts at 0.5, February's at 1.0. A single
        // period-end rate (1.0) would report 2200 with the fixture salary;
        // month-end bucketing reports 2150. Fixture salary adds 2000 EUR in
        // February on top of the 150 USD consulting income.
        let report = fetch_pnl_report_inner(
            &conn,
            "2024-01-01",
            "2024-02-29",
            Some("2023-12-01"),
            Some("2023-12-31"),
        )
        .unwrap();
        assert_eq!(report.total_income, 2150.0);
        let consult = report
            .income_rows
            .iter()
            .find(|r| r.account_name == "Consulting")
            .expect("consulting line");
        assert_eq!(consult.current_amount, 150.0);
        assert!(report.missing_fx.is_empty());
    }

    #[test]
    fn pnl_trend_buckets_months_and_zero_fills() {
        let conn = setup_pnl_fixture();
        let points = fetch_pnl_trend_inner(&conn, "2024-01-01", "2024-03-31").unwrap();
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].month, "2024-01");
        assert_eq!(points[0].income, 0.0);
        assert_eq!(points[0].expenses, 0.0);
        assert_eq!(points[1].month, "2024-02");
        assert_eq!(points[1].income, 2000.0);
        assert_eq!(points[2].month, "2024-03");
        assert_eq!(points[2].expenses, 500.0);
    }

    #[test]
    fn pnl_trend_rejects_inverted_range() {
        let conn = setup_pnl_fixture();
        assert!(fetch_pnl_trend_inner(&conn, "2024-04-01", "2024-01-01").is_err());
    }

    #[test]
    fn pnl_change_pct_matches_frontend_convention() {
        assert_eq!(pnl_change_pct(150.0, 100.0), Some(50.0));
        assert_eq!(pnl_change_pct(0.0, 0.0), None);
        // New account with no prior balance reports 100%.
        assert_eq!(pnl_change_pct(200.0, 0.0), Some(100.0));
    }

    #[test]
    fn pnl_totals_and_prev_merge() {
        let conn = setup_pnl_fixture();
        let report = fetch_pnl_report_inner(
            &conn,
            "2024-02-01",
            "2024-03-31",
            Some("2024-01-01"),
            Some("2024-01-31"),
        )
        .unwrap();
        // Salary 2000 in Feb, rent 500 in Mar; January has neither.
        assert_eq!(report.total_income, 2000.0);
        assert_eq!(report.total_expenses, 500.0);
        assert_eq!(report.net_income, 1500.0);
        assert_eq!(report.total_income_prev, 0.0);
        assert_eq!(report.total_expenses_prev, 0.0);
        assert_eq!(report.net_income_prev, 0.0);
        assert_eq!(report.net_income_change_pct, Some(100.0));
        assert_eq!(report.income_rows.len(), 1);
        assert_eq!(report.income_rows[0].current_amount, 2000.0);
        assert_eq!(report.income_rows[0].previous_amount, 0.0);
        assert_eq!(report.income_rows[0].change_pct, Some(100.0));
        assert_eq!(report.expense_rows[0].change_pct, Some(100.0));
        assert_eq!(report.report_currency, "EUR");
        assert!(report.missing_fx.is_empty());
    }

    #[test]
    fn pnl_reports_missing_fx_from_either_window() {
        let conn = setup_pnl_fixture();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('consult', 'Consulting', 'INCOME', 'root', 0, 'c-usd')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t-usd', '2024-02-15')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-t-usd-d', 't-usd', 'checking', 10000, 100, 10000, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-t-usd-c', 't-usd', 'consult', -10000, 100, -10000, 100)",
            [],
        )
        .unwrap();
        let report = fetch_pnl_report_inner(
            &conn,
            "2024-02-01",
            "2024-03-31",
            Some("2024-01-01"),
            Some("2024-01-31"),
        )
        .unwrap();
        // No USD->EUR price exists, so the book degrades to parity and flags it.
        assert!(report.missing_fx.contains(&"USD".to_string()));
    }

    #[test]
    fn pnl_keeps_placeholder_ancestors_and_prev_only_accounts() {
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
                    ('inc-top', 'Income Top', 'INCOME', 'root', 1, 'c-eur'),
                    ('wages', 'Wages', 'INCOME', 'inc-top', 1, 'c-eur'),
                    ('salary', 'Salary', 'INCOME', 'wages', 0, 'c-eur'),
                    ('bonus', 'Bonus', 'INCOME', 'wages', 0, 'c-eur'),
                    ('other-top', 'Other Top', 'INCOME', 'root', 1, 'c-eur'),
                    ('other-leaf', 'Other Leaf', 'INCOME', 'other-top', 0, 'c-eur'),
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        // Salary in the current window (Feb), bonus only in the previous
        // window (Jan). Other Leaf has no activity in either window.
        for (tag, date, amount, income) in [
            ("p1", "2024-02-10", 200_000i64, "salary"),
            ("p0", "2024-01-10", 50_000i64, "bonus"),
        ] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![tag, date],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, 'checking', ?3, 100, ?3, 100)",
                params![format!("s-{tag}-d"), tag, amount],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                params![format!("s-{tag}-c"), tag, income, -amount],
            )
            .unwrap();
        }
        let report = fetch_pnl_report_inner(
            &conn,
            "2024-02-01",
            "2024-02-29",
            Some("2024-01-01"),
            Some("2024-01-31"),
        )
        .unwrap();
        // Intermediate placeholder renders as the root (the top grouping row
        // is covered by the Income total row) with rolled-up totals.
        assert_eq!(report.income_rows.len(), 3);
        assert_eq!(report.income_rows[0].account_name, "Wages");
        assert_eq!(report.income_rows[0].indent, 0);
        assert_eq!(report.income_rows[0].current_amount, 2000.0);
        assert_eq!(report.income_rows[0].previous_amount, 500.0);
        // Name-sorted children: Bonus before Salary.
        assert_eq!(report.income_rows[1].account_name, "Bonus");
        assert_eq!(report.income_rows[1].indent, 1);
        assert_eq!(report.income_rows[1].current_amount, 0.0);
        assert_eq!(report.income_rows[1].previous_amount, 500.0);
        assert_eq!(report.income_rows[2].account_name, "Salary");
        assert_eq!(report.income_rows[2].indent, 1);
        assert_eq!(report.income_rows[2].current_amount, 2000.0);
        assert_eq!(report.income_rows[2].previous_amount, 0.0);
        assert_eq!(report.total_income, 2000.0);
        assert_eq!(report.total_income_prev, 500.0);
        // Zero-only branches stay hidden.
        assert!(report
            .income_rows
            .iter()
            .all(|r| r.account_name != "Other Leaf" && r.account_name != "Other Top"));
    }
}
