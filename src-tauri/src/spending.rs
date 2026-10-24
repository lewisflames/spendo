//! Spending report: expense activity grouped into the categories one level
//! below a chosen expense account, month by month, against a previous window.
//! Shares the P&L monthly-bucket engine (and month-end FX), so its totals
//! always match the P&L statement for the same period.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{run_blocking, AppError, AppState};
use crate::reports::{self, AccountMeta, ConvertedBucket};
use crate::utils;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendingInput {
    pub date_from: String,
    pub date_to: String,
    pub prev_date_from: Option<String>,
    pub prev_date_to: Option<String>,
    /// Expense account whose children become the categories; `None` starts
    /// at the top of the expense tree.
    pub parent_guid: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendingCategory {
    pub account_guid: String,
    pub account_name: String,
    /// Whether the category has expense sub-accounts to drill into.
    pub has_children: bool,
    /// The parent's own splits, shown beside its children.
    pub own_splits: bool,
    /// One amount per entry of `SpendingReport::months`.
    pub monthly: Vec<f64>,
    pub total: f64,
    pub previous_total: f64,
    pub change_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendingCrumb {
    pub account_guid: String,
    pub account_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendingReport {
    pub date_from: String,
    pub date_to: String,
    pub prev_date_from: String,
    pub prev_date_to: String,
    pub report_currency: String,
    /// Expense accounts from the top level down to the current parent, empty
    /// at the top level.
    pub path: Vec<SpendingCrumb>,
    /// `YYYY-MM`, every month of the window (zero-filled).
    pub months: Vec<String>,
    /// Largest total first.
    pub categories: Vec<SpendingCategory>,
    pub total: f64,
    pub total_prev: f64,
    pub change_pct: Option<f64>,
    pub missing_fx: Vec<String>,
}

/// The category level: the account whose children are listed (`None` lists
/// the expense roots themselves) and the category GUIDs at that level.
struct Level {
    parent: Option<String>,
    categories: HashSet<String>,
}

fn is_expense(guid: &str, meta: &HashMap<String, AccountMeta>) -> bool {
    meta.get(guid).is_some_and(|m| m.account_type == "EXPENSE")
}

fn expense_children(meta: &HashMap<String, AccountMeta>) -> HashMap<&str, Vec<&str>> {
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for (guid, m) in meta {
        if m.account_type != "EXPENSE" {
            continue;
        }
        if let Some(parent) = m.parent_guid.as_deref() {
            if is_expense(parent, meta) {
                children.entry(parent).or_default().push(guid.as_str());
            }
        }
    }
    children
}

/// Expense accounts without an expense parent. A single root (GnuCash's
/// `Expenses` placeholder) is the implicit top, so the top level lists its
/// children instead of one lonely bar.
fn top_level(meta: &HashMap<String, AccountMeta>, children: &HashMap<&str, Vec<&str>>) -> Level {
    let roots: Vec<&String> = meta
        .iter()
        .filter(|(_, m)| {
            m.account_type == "EXPENSE"
                && !m
                    .parent_guid
                    .as_deref()
                    .is_some_and(|p| is_expense(p, meta))
        })
        .map(|(guid, _)| guid)
        .collect();
    if let [root] = roots.as_slice() {
        if children.contains_key(root.as_str()) {
            return level_below(root, children);
        }
    }
    Level {
        parent: None,
        categories: roots.into_iter().cloned().collect(),
    }
}

fn level_below(parent: &str, children: &HashMap<&str, Vec<&str>>) -> Level {
    Level {
        parent: Some(parent.to_string()),
        categories: children
            .get(parent)
            .map(|c| c.iter().map(|g| g.to_string()).collect())
            .unwrap_or_default(),
    }
}

/// The category a bucket's account rolls into: itself or its nearest
/// ancestor at this level, or the parent's own row. `None` = outside the level.
fn category_of(guid: &str, level: &Level, meta: &HashMap<String, AccountMeta>) -> Option<String> {
    if level.parent.as_deref() == Some(guid) || level.categories.contains(guid) {
        return Some(guid.to_string());
    }
    reports::ancestor_chain(guid, meta)
        .into_iter()
        .find(|a| level.categories.contains(a))
}

/// Per-category, per-month expense totals for one window.
fn group_buckets(
    buckets: &[ConvertedBucket],
    level: &Level,
    meta: &HashMap<String, AccountMeta>,
) -> HashMap<String, HashMap<String, f64>> {
    let mut out: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut resolved: HashMap<&str, Option<String>> = HashMap::new();
    for b in buckets {
        if b.account_type != "EXPENSE" {
            continue;
        }
        let category = resolved
            .entry(b.guid.as_str())
            .or_insert_with(|| category_of(&b.guid, level, meta));
        if let Some(category) = category {
            *out.entry(category.clone())
                .or_default()
                .entry(b.month.clone())
                .or_default() += b.amount;
        }
    }
    out
}

/// One window's expense buckets in report currency, with the missing-FX list
/// and report currency. Other account types are dropped before conversion:
/// an investment account's security mnemonic has no FX rate and would raise
/// a bogus missing-rate warning for a report that never shows it.
fn expense_buckets(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
) -> Result<(Vec<ConvertedBucket>, Vec<String>, String), AppError> {
    let mut buckets = reports::fetch_monthly_buckets(conn, date_from, date_to)?;
    buckets.retain(|b| b.account_type == "EXPENSE");
    reports::convert_buckets(conn, &buckets)
}

fn window_months(date_from: &str, date_to: &str) -> Result<Vec<String>, AppError> {
    let start = utils::month_number(date_from)?;
    let end = utils::month_number(date_to)?;
    Ok((start..=end).map(utils::format_month).collect())
}

fn fetch_spending_report_inner(
    conn: &Connection,
    input: &SpendingInput,
) -> Result<SpendingReport, AppError> {
    let date_from = utils::validate_iso_date(&input.date_from)?;
    let date_to = utils::validate_iso_date(&input.date_to)?;
    let prev_date_from = utils::validate_optional_iso_date(input.prev_date_from.as_deref())?;
    let prev_date_to = utils::validate_optional_iso_date(input.prev_date_to.as_deref())?;
    if date_from > date_to {
        return Err(AppError::Validation(
            "Start date is after end date. Flip them around".to_string(),
        ));
    }
    let (prev_from, prev_to) = reports::resolve_pnl_prev_window(
        &date_from,
        &date_to,
        prev_date_from.as_deref(),
        prev_date_to.as_deref(),
    )?;

    let meta = reports::fetch_account_meta(conn)?;
    let children = expense_children(&meta);
    let top = top_level(&meta, &children);
    let level = match input.parent_guid.as_deref() {
        None => top,
        Some(guid) => {
            if !is_expense(guid, &meta) {
                return Err(AppError::NotFound(
                    "That expense category no longer exists".to_string(),
                ));
            }
            level_below(guid, &children)
        }
    };

    // Crumbs run from the top level down to the parent; the implicit single
    // root is the top itself, so it never shows.
    let mut path = Vec::new();
    if let (Some(parent), Some(_)) = (level.parent.as_deref(), input.parent_guid.as_deref()) {
        let top_parent = top_level(&meta, &children).parent;
        let mut chain: Vec<String> = reports::ancestor_chain(parent, &meta)
            .into_iter()
            .take_while(|a| is_expense(a, &meta) && Some(a) != top_parent.as_ref())
            .collect();
        chain.reverse();
        chain.push(parent.to_string());
        path = chain
            .into_iter()
            .filter_map(|guid| {
                meta.get(&guid).map(|m| SpendingCrumb {
                    account_guid: guid.clone(),
                    account_name: m.name.clone(),
                })
            })
            .collect();
    }

    let (current, missing_cur, report_currency) = expense_buckets(conn, &date_from, &date_to)?;
    let (prev, missing_prev, _) = expense_buckets(conn, &prev_from, &prev_to)?;

    let months = window_months(&date_from, &date_to)?;
    let current = group_buckets(&current, &level, &meta);
    let prev = group_buckets(&prev, &level, &meta);

    let guids: HashSet<&String> = current.keys().chain(prev.keys()).collect();
    let mut categories: Vec<SpendingCategory> = guids
        .into_iter()
        .filter_map(|guid| {
            let m = meta.get(guid)?;
            let per_month = current.get(guid);
            let monthly: Vec<f64> = months
                .iter()
                .map(|month| per_month.and_then(|p| p.get(month)).copied().unwrap_or(0.0))
                .collect();
            let total: f64 = monthly.iter().sum();
            let previous_total: f64 = prev.get(guid).map(|p| p.values().sum()).unwrap_or(0.0);
            if total == 0.0 && previous_total == 0.0 {
                return None;
            }
            let own_splits = level.parent.as_deref() == Some(guid.as_str());
            Some(SpendingCategory {
                account_guid: guid.clone(),
                account_name: m.name.clone(),
                has_children: !own_splits && children.contains_key(guid.as_str()),
                own_splits,
                monthly,
                total,
                previous_total,
                change_pct: reports::pnl_change_pct(total, previous_total),
            })
        })
        .collect();
    categories.sort_by(|a, b| {
        b.total
            .total_cmp(&a.total)
            .then(b.previous_total.total_cmp(&a.previous_total))
            .then_with(|| a.account_name.cmp(&b.account_name))
    });

    let total: f64 = categories.iter().map(|c| c.total).sum();
    let total_prev: f64 = categories.iter().map(|c| c.previous_total).sum();
    let mut missing_fx: Vec<String> = missing_cur
        .into_iter()
        .chain(missing_prev)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    missing_fx.sort();

    Ok(SpendingReport {
        date_from,
        date_to,
        prev_date_from: prev_from,
        prev_date_to: prev_to,
        report_currency,
        path,
        months,
        categories,
        total,
        total_prev,
        change_pct: reports::pnl_change_pct(total, total_prev),
        missing_fx,
    })
}

#[tauri::command]
pub async fn fetch_spending_report(
    state: State<'_, AppState>,
    input: SpendingInput,
) -> Result<SpendingReport, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_spending_report_inner(conn, &input)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use rusqlite::params;

    fn input(from: &str, to: &str, parent: Option<&str>) -> SpendingInput {
        SpendingInput {
            date_from: from.to_string(),
            date_to: to.to_string(),
            prev_date_from: None,
            prev_date_to: None,
            parent_guid: parent.map(str::to_string),
        }
    }

    /// Expenses (placeholder) → Food (with own splits) → Groceries, Dining;
    /// Expenses → Rent. Everything paid from checking in EUR.
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
                    ('salary', 'Salary', 'INCOME', 'root', 0, 'c-eur'),
                    ('expenses', 'Expenses', 'EXPENSE', 'root', 1, 'c-eur'),
                    ('food', 'Food', 'EXPENSE', 'expenses', 0, 'c-eur'),
                    ('groceries', 'Groceries', 'EXPENSE', 'food', 0, 'c-eur'),
                    ('dining', 'Dining', 'EXPENSE', 'food', 0, 'c-eur'),
                    ('rent', 'Rent', 'EXPENSE', 'expenses', 0, 'c-eur')",
            [],
        )
        .unwrap();
        for (tag, date, debit, credit, cents) in [
            ("t1", "2024-01-05", "rent", "checking", 80_000i64),
            ("t2", "2024-01-10", "groceries", "checking", 12_000),
            ("t3", "2024-02-10", "dining", "checking", 4_000),
            ("t4", "2024-02-12", "food", "checking", 1_000),
            ("t5", "2024-02-20", "checking", "groceries", 2_000), // refund
            ("t6", "2024-02-25", "checking", "salary", 300_000),
            ("t7", "2023-12-10", "groceries", "checking", 9_000), // previous window
        ] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![tag, date],
            )
            .unwrap();
            for (side, account, amount) in [("d", debit, cents), ("c", credit, -cents)] {
                conn.execute(
                    "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                     VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                    params![format!("s-{tag}-{side}"), tag, account, amount],
                )
                .unwrap();
            }
        }
        conn
    }

    fn find<'a>(report: &'a SpendingReport, guid: &str) -> &'a SpendingCategory {
        report
            .categories
            .iter()
            .find(|c| c.account_guid == guid)
            .unwrap_or_else(|| panic!("missing category {guid}"))
    }

    #[test]
    fn top_level_skips_the_single_root_and_rolls_up_children() {
        let conn = setup();
        let r =
            fetch_spending_report_inner(&conn, &input("2024-01-01", "2024-02-29", None)).unwrap();
        assert!(r.path.is_empty());
        assert_eq!(r.months, vec!["2024-01", "2024-02"]);
        let guids: Vec<&str> = r
            .categories
            .iter()
            .map(|c| c.account_guid.as_str())
            .collect();
        assert_eq!(
            guids,
            vec!["rent", "food"],
            "largest first, income excluded"
        );
        let food = find(&r, "food");
        // 120 groceries in Jan; 40 dining + 10 own − 20 refund in Feb.
        assert_eq!(food.monthly, vec![120.0, 30.0]);
        assert!(food.has_children && !food.own_splits);
        assert_eq!(r.total, 950.0);
        // Two-month default previous window: Nov–Dec 2023.
        assert_eq!(r.prev_date_from, "2023-11-01");
        assert_eq!(food.previous_total, 90.0);
        assert_eq!(r.total_prev, 90.0);
    }

    #[test]
    fn drill_down_lists_children_and_the_parents_own_splits() {
        let conn = setup();
        let r =
            fetch_spending_report_inner(&conn, &input("2024-01-01", "2024-02-29", Some("food")))
                .unwrap();
        assert_eq!(r.path.len(), 1);
        assert_eq!(r.path[0].account_name, "Food");
        assert_eq!(find(&r, "groceries").monthly, vec![120.0, -20.0]);
        assert_eq!(find(&r, "dining").total, 40.0);
        let own = find(&r, "food");
        assert!(own.own_splits && !own.has_children);
        assert_eq!(own.total, 10.0);
        assert_eq!(r.total, 150.0, "matches the Food total one level up");
    }

    #[test]
    fn previous_only_categories_still_show() {
        let conn = setup();
        let r =
            fetch_spending_report_inner(&conn, &input("2024-03-01", "2024-03-31", Some("food")))
                .unwrap();
        // March has no spending; February (the default previous window) does.
        let groceries = find(&r, "groceries");
        assert_eq!(groceries.total, 0.0);
        assert_eq!(groceries.previous_total, -20.0);
        assert_eq!(r.months, vec!["2024-03"]);
    }

    #[test]
    fn rejects_non_expense_parents_and_bad_ranges() {
        let conn = setup();
        assert!(matches!(
            fetch_spending_report_inner(&conn, &input("2024-01-01", "2024-02-29", Some("salary"))),
            Err(AppError::NotFound(_))
        ));
        assert!(
            fetch_spending_report_inner(&conn, &input("2024-03-01", "2024-01-01", None)).is_err()
        );
        assert!(
            fetch_spending_report_inner(&conn, &input("2024-1-01", "2024-02-01", None)).is_err()
        );
    }

    #[test]
    fn ignores_missing_rates_outside_expense_accounts() {
        let conn = setup();
        // A security bought from checking, with no price: its mnemonic has no
        // rate to EUR, but it isn't an expense, so it must not warn.
        conn.execute_batch(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-etf', 'IWDA.AS', 'ETF');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('etf', 'IWDA', 'STOCK', 'root', 0, 'c-etf');
             INSERT INTO transactions (guid, post_date) VALUES ('t-etf', '2024-01-15');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-etf-d', 't-etf', 'etf', 50000, 100, 500, 100),
                    ('s-etf-c', 't-etf', 'checking', -50000, 100, -50000, 100);",
        )
        .unwrap();
        let r =
            fetch_spending_report_inner(&conn, &input("2024-01-01", "2024-02-29", None)).unwrap();
        assert!(r.missing_fx.is_empty(), "got {:?}", r.missing_fx);
        assert_eq!(r.total, 950.0);
    }

    #[test]
    fn several_expense_roots_are_the_top_level() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('taxes', 'Taxes', 'EXPENSE', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        let r =
            fetch_spending_report_inner(&conn, &input("2024-01-01", "2024-02-29", None)).unwrap();
        let guids: Vec<&str> = r
            .categories
            .iter()
            .map(|c| c.account_guid.as_str())
            .collect();
        assert_eq!(guids, vec!["expenses"]);
        assert!(find(&r, "expenses").has_children);
    }
}
