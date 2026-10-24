use crate::db::AppError;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::prices::{get_cached_prices_map, PriceHistory};
use crate::tree;
use crate::utils;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountNode {
    pub guid: String,
    pub name: String,
    pub account_type: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub placeholder: bool,
    pub commodity: Option<String>,
    pub commodity_guid: Option<String>,
    pub balance: f64,
    pub tx_count: i64,
    pub children: Vec<AccountNode>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeSummary {
    pub count: i64,
    pub total_balance: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsResult {
    pub trees: Vec<AccountNode>,
    pub by_type: HashMap<String, TypeSummary>,
    pub total_accounts: i64,
    pub report_currency: String,
    /// Cumulative income minus expenses as of the requested date.
    pub retained_earnings: f64,
    /// Market minus cost for priced investments (0 for unpriced holdings,
    /// which contribute zero and are listed in `missing_prices`).
    pub unrealized_gains: f64,
    /// `assets - (liabilities + equity + unrealized + retained)`; near zero
    /// when balanced. Mirrors the old balance-sheet check.
    pub balance_check: f64,
    pub missing_fx: Vec<String>,
    pub missing_prices: Vec<fx::MissingPrice>,
}

struct AccountRow {
    guid: String,
    name: String,
    account_type: String,
    parent_guid: Option<String>,
    code: Option<String>,
    description: Option<String>,
    placeholder: bool,
    commodity_mnemonic: Option<String>,
    commodity_guid: Option<String>,
    balance: f64,
    quantity: f64,
    tx_count: i64,
}

/// Load every account (excluding ROOT) with its balance, quantity and
/// transaction count through `as_of` (inclusive; `None` means today), always
/// capped by the shared visibility window so a future date never reveals
/// future-dated transactions.
fn load_account_rows(conn: &Connection, as_of: Option<&str>) -> Result<Vec<AccountRow>, AppError> {
    let sql = format!("SELECT a.guid, a.name, a.account_type, a.parent_guid, a.code, a.description, a.placeholder,
                      c.mnemonic as commodity_mnemonic, c.guid as commodity_guid,
                      COALESCE(b.balance, 0) as balance,
                      COALESCE(b.quantity, 0) as quantity,
                      COALESCE(b.tx_count, 0) as tx_count
               FROM accounts a
               LEFT JOIN commodities c ON c.guid = a.commodity_guid
               LEFT JOIN (
                 SELECT s.account_guid,
                        SUM({}) as balance,
                        SUM({}) as quantity,
                        COUNT(DISTINCT s.tx_guid) as tx_count
                 FROM splits s
                 JOIN transactions t ON t.guid = s.tx_guid
                 WHERE t.post_date < MIN(COALESCE(date(?1, '+1 day'), {now}), {now})
                 GROUP BY s.account_guid
               ) b ON b.account_guid = a.guid
               WHERE a.account_type != 'ROOT'
               ORDER BY a.account_type, a.name",
        utils::RAW_AMOUNT, utils::QUANTITY_AMOUNT, now = utils::VISIBLE_NOW_EXPR
    );

    let mut stmt = conn.prepare(&sql)?;

    let rows: Vec<AccountRow> = stmt
        .query_map([as_of], |row| {
            Ok(AccountRow {
                guid: row.get(0)?,
                name: row.get(1)?,
                account_type: row.get(2)?,
                parent_guid: row.get(3)?,
                code: row.get(4)?,
                description: row.get(5)?,
                placeholder: row.get::<_, i64>(6)? != 0,
                commodity_mnemonic: row.get(7)?,
                commodity_guid: row.get(8)?,
                balance: row.get(9)?,
                quantity: row.get(10)?,
                tx_count: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// Account nodes plus the aggregates accumulated while valuing each row.
struct AccountValuation {
    node_map: HashMap<String, AccountNode>,
    by_type: HashMap<String, TypeSummary>,
    missing_fx_set: HashSet<String>,
    missing_prices_map: HashMap<String, Option<String>>,
    unrealized_gains: f64,
}

/// Apply credit-normal signs, investment mark-to-market and FX conversion to
/// every row, collecting nodes, type summaries and missing-price/FX warnings.
fn value_account_rows(
    rows: &[AccountRow],
    unified_prices: &HashMap<String, f64>,
    report_currency: &str,
    fx_map: &HashMap<String, f64>,
) -> AccountValuation {
    let mut node_map: HashMap<String, AccountNode> = HashMap::new();
    let mut by_type: HashMap<String, TypeSummary> = HashMap::new();
    let mut missing_fx_set: HashSet<String> = HashSet::new();
    let mut missing_prices_map: HashMap<String, Option<String>> = HashMap::new();
    let mut unrealized_gains = 0.0;

    for row in rows {
        let mut balance = if utils::is_credit_normal(&row.account_type) {
            -row.balance
        } else {
            row.balance
        };

        if utils::INVESTMENT_TYPES.contains(&row.account_type.as_str())
            && row.commodity_mnemonic.is_some()
        {
            if let Some(ref mnemonic) = row.commodity_mnemonic {
                let price = row
                    .commodity_guid
                    .as_deref()
                    .and_then(|guid| unified_prices.get(guid))
                    .copied();
                if let Some(price) = price {
                    let adjusted = if utils::is_credit_normal(&row.account_type) {
                        -row.quantity
                    } else {
                        row.quantity
                    };
                    let market = adjusted * price;
                    // Cost leg converts silently (never an FX warning): the
                    // holding is either priced (market dominates) or flagged
                    // via `missing_prices`.
                    let cost = fx::convert(balance, mnemonic, report_currency, fx_map);
                    unrealized_gains += market - cost;
                    balance = market;
                } else {
                    // An empty holding is worth zero at any price; only flag
                    // the ones whose shares are being left out.
                    if row.quantity != 0.0 {
                        missing_prices_map
                            .entry(mnemonic.clone())
                            .or_insert(row.commodity_guid.clone());
                    }
                    // Unpriced securities contribute zero (matching home net
                    // worth and portfolio market value): cost basis is excluded
                    // until a cached price exists, and the holding is flagged
                    // via `missing_prices`.
                    balance = 0.0;
                }
            }
        } else if let Some(ref mnemonic) = row.commodity_mnemonic {
            // Cash-type (and income/expense/equity) accounts hold their own currency:
            // convert to the report currency for aggregation.
            balance = fx::convert_tracked(
                balance,
                mnemonic,
                report_currency,
                fx_map,
                Some(&mut missing_fx_set),
            );
        }

        let node = AccountNode {
            guid: row.guid.clone(),
            name: row.name.clone(),
            account_type: row.account_type.clone(),
            code: row.code.clone(),
            description: row.description.clone(),
            placeholder: row.placeholder,
            commodity: row.commodity_mnemonic.clone(),
            commodity_guid: row.commodity_guid.clone(),
            balance,
            tx_count: row.tx_count,
            children: vec![],
        };
        node_map.insert(row.guid.clone(), node);

        let category = utils::category_for_type(&row.account_type).to_string();
        let entry = by_type.entry(category).or_insert(TypeSummary {
            count: 0,
            total_balance: 0.0,
        });
        entry.count += 1;
        entry.total_balance += balance;
    }

    AccountValuation {
        node_map,
        by_type,
        missing_fx_set,
        missing_prices_map,
        unrealized_gains,
    }
}

/// Recursively clone the subtree for `guid`, stopping at already-visited nodes
/// so corrupt `parent_guid` cycles cannot recurse forever.
fn build_subtree(
    node_map: &HashMap<String, AccountNode>,
    children: &HashMap<String, Vec<String>>,
    guid: &str,
    visited: &mut HashSet<String>,
) -> Option<AccountNode> {
    if !visited.insert(guid.to_string()) {
        return None;
    }
    let mut node = node_map.get(guid)?.clone();
    if let Some(kids) = children.get(guid) {
        for child_guid in kids {
            if let Some(child) = build_subtree(node_map, children, child_guid, visited) {
                node.children.push(child);
            }
        }
    }
    Some(node)
}

/// Roll child balances and transaction counts up into placeholder parents.
/// Non-placeholder accounts keep the values from their direct splits.
fn propagate_placeholder(nodes: &mut [AccountNode]) {
    for node in nodes.iter_mut() {
        propagate_placeholder(&mut node.children);
        if node.placeholder {
            for child in &node.children {
                node.balance += child.balance;
                node.tx_count += child.tx_count;
            }
        }
    }
}

/// Assemble the name-sorted account forest from the valued node map.
fn build_account_tree(
    rows: &[AccountRow],
    node_map: &HashMap<String, AccountNode>,
) -> Vec<AccountNode> {
    // Build the parent→children index (name-sorted, orphans become roots).
    let index = {
        let triples = rows
            .iter()
            .map(|r| (r.guid.clone(), r.parent_guid.clone(), r.name.clone()));
        tree::build_index(triples)
    };

    let mut root_nodes: Vec<AccountNode> = vec![];
    let mut visited = HashSet::new();
    for guid in &index.roots {
        if let Some(node) = build_subtree(node_map, &index.children, guid, &mut visited) {
            root_nodes.push(node);
        }
    }

    propagate_placeholder(&mut root_nodes);

    root_nodes
}

/// Compute retained earnings, the balance check and the sorted missing
/// FX/price warning lists, then pack everything into the response.
fn assemble_accounts_result(
    total_accounts: i64,
    trees: Vec<AccountNode>,
    report_currency: String,
    valuation: AccountValuation,
) -> AccountsResult {
    let AccountValuation {
        by_type,
        missing_fx_set,
        missing_prices_map,
        unrealized_gains,
        ..
    } = valuation;

    let total_of = |key: &str| by_type.get(key).map(|s| s.total_balance).unwrap_or(0.0);
    let retained_earnings = total_of("INCOME") - total_of("EXPENSE");
    let balance_check = total_of("ASSET")
        - (total_of("LIABILITY") + total_of("EQUITY") + unrealized_gains + retained_earnings);
    let mut missing_fx: Vec<String> = missing_fx_set.into_iter().collect();
    missing_fx.sort();
    let mut missing_prices: Vec<fx::MissingPrice> = missing_prices_map
        .into_iter()
        .map(|(mnemonic, commodity_guid)| fx::MissingPrice {
            mnemonic,
            commodity_guid,
        })
        .collect();
    missing_prices.sort_by(|a, b| a.mnemonic.cmp(&b.mnemonic));

    AccountsResult {
        trees,
        by_type,
        total_accounts,
        report_currency,
        retained_earnings,
        unrealized_gains,
        balance_check,
        missing_fx,
        missing_prices,
    }
}

/// Report-currency price per held commodity as of `date`, with the same
/// as-of rule as the net worth trend (latest on or before the date, else the
/// earliest known price), so a month-end balance sheet matches its point.
fn prices_as_of(
    conn: &Connection,
    rows: &[AccountRow],
    date: &str,
    report_currency: &str,
) -> Result<HashMap<String, f64>, AppError> {
    let history = PriceHistory::load(conn)?;
    let fx_history = fx::FxHistory::load(conn)?;
    let mut prices = HashMap::new();
    for row in rows {
        if !utils::INVESTMENT_TYPES.contains(&row.account_type.as_str()) {
            continue;
        }
        let Some(guid) = row.commodity_guid.as_deref() else {
            continue;
        };
        if prices.contains_key(guid) {
            continue;
        }
        if let Some(price) = history.price_as_of(guid, date, report_currency, &fx_history) {
            prices.insert(guid.to_string(), price);
        }
    }
    Ok(prices)
}

fn fetch_accounts_inner(conn: &Connection) -> Result<AccountsResult, AppError> {
    accounts_as_of(conn, None)
}

/// The valued account tree through `as_of` (inclusive). `None` is "now":
/// latest cached prices and FX rates, exactly what the accounts page shows;
/// a date values holdings and currencies as of that day.
fn accounts_as_of(conn: &Connection, as_of: Option<&str>) -> Result<AccountsResult, AppError> {
    let rows = load_account_rows(conn, as_of)?;

    let report_currency = fx::report_currency_or_empty(conn)?;
    let unified_prices = match as_of {
        None => get_cached_prices_map(conn)?,
        Some(date) => prices_as_of(conn, &rows, date, &report_currency)?,
    };
    let fx_map = fx::get_fx_map(conn, as_of)?;

    let valuation = value_account_rows(&rows, &unified_prices, &report_currency, &fx_map);
    let trees = build_account_tree(&rows, &valuation.node_map);

    Ok(assemble_accounts_result(
        rows.len() as i64,
        trees,
        report_currency,
        valuation,
    ))
}

pub fn get_subaccount_guids(conn: &Connection, guid: &str) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE sub AS (
                SELECT guid FROM accounts WHERE guid = ?1
                UNION
                SELECT a.guid FROM accounts a JOIN sub ON a.parent_guid = sub.guid
            )
            SELECT guid FROM sub",
    )?;

    let result: Vec<_> = stmt
        .query_map([guid], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}

/// Direct child of one account with the same balance semantics as the
/// accounts tree (credit-normal signs, investment mark-to-market, FX
/// conversion). Carries everything the account-detail child table renders,
/// so that view needs no second full-tree fetch.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildAccountInfo {
    pub guid: String,
    pub name: String,
    pub description: Option<String>,
    pub code: Option<String>,
    pub commodity: Option<String>,
    pub commodity_guid: Option<String>,
    pub account_type: String,
    pub balance: f64,
    pub tx_count: i64,
}

/// Slim view of the parent account for the balance-composition pie: enough to
/// label it and express this account's share of its balance. `None` for
/// top-level accounts whose parent is the (excluded) ROOT node.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentAccountInfo {
    pub guid: String,
    pub name: String,
    pub description: Option<String>,
    pub balance: f64,
}

/// Find an account node together with its direct parent; the first tuple
/// element is the parent, the second the node itself.
fn find_account_with_parent<'a>(
    trees: &'a [AccountNode],
    guid: &str,
) -> (Option<&'a AccountNode>, Option<&'a AccountNode>) {
    for tree in trees {
        if tree.guid == guid {
            return (None, Some(tree));
        }
        let (parent, node) = find_account_with_parent(&tree.children, guid);
        if node.is_some() {
            return (Some(parent.unwrap_or(tree)), node);
        }
    }
    (None, None)
}

/// Tree slices of one account: direct children, the node with its subtree,
/// and its parent.
type AccountTreeSlice = (
    Vec<ChildAccountInfo>,
    Option<AccountNode>,
    Option<ParentAccountInfo>,
);

/// Direct children (for the child table), the whole subtree (for the
/// balance-composition chart) and the parent (for the pie's parent-ratio
/// label) of one account, from a single tree fetch so the detail view needs
/// no second full-tree fetch. Empty/`None` when the account is missing from
/// the tree.
pub fn fetch_children_and_subtree(
    conn: &Connection,
    parent_guid: &str,
) -> Result<AccountTreeSlice, AppError> {
    let result = fetch_accounts_inner(conn)?;
    let (parent, node) = find_account_with_parent(&result.trees, parent_guid);
    let Some(node) = node else {
        return Ok((Vec::new(), None, None));
    };
    let children = node
        .children
        .iter()
        .map(|c| ChildAccountInfo {
            guid: c.guid.clone(),
            name: c.name.clone(),
            description: c.description.clone(),
            code: c.code.clone(),
            commodity: c.commodity.clone(),
            commodity_guid: c.commodity_guid.clone(),
            account_type: c.account_type.clone(),
            balance: c.balance,
            tx_count: c.tx_count,
        })
        .collect();
    let parent_info = parent.map(|p| ParentAccountInfo {
        guid: p.guid.clone(),
        name: p.name.clone(),
        description: p.description.clone(),
        balance: p.balance,
    });
    Ok((children, Some(node.clone()), parent_info))
}

#[tauri::command]
pub async fn fetch_accounts(state: State<'_, AppState>) -> Result<AccountsResult, AppError> {
    run_blocking(state.db.clone(), fetch_accounts_inner).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSheetInput {
    /// `YYYY-MM-DD`; `None` is today, valued like the accounts page.
    pub as_of: Option<String>,
    /// Optional second date for a comparison column.
    pub compare_to: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSheetResult {
    /// The resolved valuation date (today when the input had none).
    pub as_of: String,
    pub current: AccountsResult,
    pub compare_to: Option<String>,
    pub compare: Option<AccountsResult>,
}

fn fetch_balance_sheet_inner(
    conn: &Connection,
    input: BalanceSheetInput,
) -> Result<BalanceSheetResult, AppError> {
    let as_of = input
        .as_of
        .as_deref()
        .map(utils::validate_iso_date)
        .transpose()?;
    let compare_to = input
        .compare_to
        .as_deref()
        .map(utils::validate_iso_date)
        .transpose()?;
    let current = accounts_as_of(conn, as_of.as_deref())?;
    let compare = compare_to
        .as_deref()
        .map(|date| accounts_as_of(conn, Some(date)))
        .transpose()?;
    Ok(BalanceSheetResult {
        as_of: as_of.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string()),
        current,
        compare_to,
        compare,
    })
}

#[tauri::command]
pub async fn fetch_balance_sheet(
    state: State<'_, AppState>,
    input: BalanceSheetInput,
) -> Result<BalanceSheetResult, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_balance_sheet_inner(conn, input)
    })
    .await
}

/// One ACCOUNTS-group destination: the top-level account representing a
/// category. `guid`/`name` are `None` only while a book has no account of
/// that category (healed by `init::ensure_default_roots` on book open).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavRoot {
    pub category: String,
    pub guid: Option<String>,
    pub name: Option<String>,
}

/// Resolve the ACCOUNTS sidebar entries: one top-level account (child of any
/// ROOT) per category, preferring the canonical name when several share a
/// category. Always returns the five categories in balance-sheet order.
fn nav_roots_inner(conn: &Connection) -> Result<Vec<NavRoot>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT guid, name, account_type FROM accounts
             WHERE parent_guid IN (SELECT guid FROM accounts WHERE account_type = 'ROOT')
               AND account_type IN ({})
             ORDER BY name",
        utils::category_types_all_sql()
    ))?;

    // category -> (guid, name), preferring the canonical default-root name.
    let mut best: HashMap<String, (String, String)> = HashMap::new();
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (guid, name, account_type) = row?;
        let category = utils::category_for_type(&account_type).to_string();
        let canonical_name = crate::init::DEFAULT_ROOTS
            .iter()
            .find(|(_, cat)| *cat == category)
            .map(|(canonical, _)| *canonical);
        // Prefer the canonical root name; more than one top-level account of
        // the same category is possible in imported books.
        let replace = match best.get(&category) {
            None => true,
            Some((_, current)) => {
                let current_is_canonical = canonical_name == Some(current.as_str());
                let candidate_is_canonical = canonical_name == Some(name.as_str());
                !current_is_canonical && candidate_is_canonical
            }
        };
        if replace {
            best.insert(category, (guid, name));
        }
    }

    Ok(utils::CATEGORY_ORDER
        .iter()
        .map(|category| {
            let found = best.get(*category);
            NavRoot {
                category: category.to_string(),
                guid: found.map(|(guid, _)| guid.clone()),
                name: found.map(|(_, name)| name.clone()),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn fetch_nav_roots(state: State<'_, AppState>) -> Result<Vec<NavRoot>, AppError> {
    run_blocking(state.db.clone(), nav_roots_inner).await
}

/// Book currencies for the account commodity picker. Thin wrapper over the
/// existing `fx::list_book_currencies` helper (previously backend-only).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookCurrency {
    pub guid: String,
    pub mnemonic: String,
}

fn fetch_book_currencies_inner(conn: &Connection) -> Result<Vec<BookCurrency>, AppError> {
    Ok(fx::list_book_currencies(conn)?
        .into_iter()
        .map(|(guid, mnemonic)| BookCurrency { guid, mnemonic })
        .collect())
}

#[tauri::command]
pub async fn fetch_book_currencies(
    state: State<'_, AppState>,
) -> Result<Vec<BookCurrency>, AppError> {
    run_blocking(state.db.clone(), fetch_book_currencies_inner).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn setup_investments() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES
             ('c-eur', 'EUR', 'CURRENCY'),
             ('c-aapl', 'AAPL', 'NASDAQ'),
             ('c-xyz', 'XYZ', 'NASDAQ')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
             ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
             ('assets', 'Assets', 'ASSET', 'root', 1, 'c-eur'),
             ('checking', 'Checking', 'BANK', 'assets', 0, 'c-eur'),
             ('broker', 'Broker', 'STOCK', 'assets', 0, 'c-aapl'),
             ('vault', 'Vault', 'STOCK', 'assets', 0, 'c-xyz')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES
             ('t-cash', '2024-01-10'), ('t-aapl', '2024-02-01'), ('t-xyz', '2024-02-15')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom) VALUES
             ('s-cash', 't-cash', 'checking', 50000, 100, 50000, 100),
             ('s-aapl', 't-aapl', 'broker', 100000, 100, 10, 1),
             ('s-xyz', 't-xyz', 'vault', 70000, 100, 5, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES ('c-aapl-2024-03-01', 'c-aapl', 'c-eur', 15000, 100, '2024-03-01')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn priced_investments_use_market_and_unpriced_contribute_zero() {
        let conn = setup_investments();
        let result = fetch_accounts_inner(&conn).unwrap();

        // Cash 500 + AAPL market 1500 + unpriced XYZ 0.
        assert_eq!(result.by_type.get("ASSET").unwrap().total_balance, 2000.0);
        // Market 1500 - cost 1000.
        assert_eq!(result.unrealized_gains, 500.0);
        assert_eq!(result.missing_prices.len(), 1);
        assert_eq!(result.missing_prices[0].mnemonic, "XYZ");

        let (_, subtree, _) = fetch_children_and_subtree(&conn, "assets").unwrap();
        assert_eq!(subtree.unwrap().balance, 2000.0);
    }

    #[test]
    fn balance_sheet_values_each_date_with_its_own_splits_and_prices() {
        let conn = setup_investments();
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES ('c-aapl-2024-02-10', 'c-aapl', 'c-eur', 12000, 100, '2024-02-10')",
            [],
        )
        .unwrap();
        let sheet = fetch_balance_sheet_inner(
            &conn,
            BalanceSheetInput {
                as_of: Some("2024-02-20".into()),
                compare_to: Some("2024-01-31".into()),
            },
        )
        .unwrap();

        // 20 Feb: cash 500 + 10 AAPL at the 10 Feb price (120); XYZ unpriced.
        assert_eq!(sheet.as_of, "2024-02-20");
        let asset = |r: &AccountsResult| r.by_type.get("ASSET").unwrap().total_balance;
        assert_eq!(asset(&sheet.current), 1700.0);
        assert_eq!(sheet.current.unrealized_gains, 200.0);
        // 31 Jan: only the cash deposit had happened.
        let compare = sheet.compare.unwrap();
        assert_eq!(sheet.compare_to.as_deref(), Some("2024-01-31"));
        assert_eq!(asset(&compare), 500.0);
        assert!(compare.missing_prices.is_empty());
        // No date: today, latest price (150).
        let today = fetch_balance_sheet_inner(
            &conn,
            BalanceSheetInput {
                as_of: None,
                compare_to: None,
            },
        )
        .unwrap();
        assert_eq!(asset(&today.current), 2000.0);
        assert!(today.compare.is_none());
    }

    #[test]
    fn balance_sheet_never_shows_future_transactions() {
        let conn = setup_investments();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t-future', '2999-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-future', 't-future', 'checking', 10000, 100, 10000, 100)",
            [],
        )
        .unwrap();
        let sheet = fetch_balance_sheet_inner(
            &conn,
            BalanceSheetInput {
                as_of: Some("2999-12-31".into()),
                compare_to: None,
            },
        )
        .unwrap();
        let checking = sheet.current.trees[0]
            .children
            .iter()
            .find(|c| c.guid == "checking")
            .unwrap();
        assert_eq!(checking.balance, 500.0);
    }

    #[test]
    fn balance_sheet_rejects_malformed_dates() {
        let conn = setup_investments();
        let err = fetch_balance_sheet_inner(
            &conn,
            BalanceSheetInput {
                as_of: Some("2024-13-01".into()),
                compare_to: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn nav_roots_resolve_one_account_per_category() {
        let conn = setup_investments();
        let roots = nav_roots_inner(&conn).unwrap();
        assert_eq!(roots.len(), 5);

        let asset = roots.iter().find(|r| r.category == "ASSET").unwrap();
        assert_eq!(asset.name.as_deref(), Some("Assets"));
        assert_eq!(asset.guid.as_deref(), Some("assets"));

        // Categories the book doesn't cover stay represented but unresolved.
        let equity = roots.iter().find(|r| r.category == "EQUITY").unwrap();
        assert!(equity.guid.is_none());
    }

    #[test]
    fn nav_roots_prefer_canonical_name() {
        let conn = setup_investments();
        // A non-canonical top-level ASSET account that sorts before "Assets".
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('investments', 'A Investments', 'ASSET', 'root', 1, 'c-eur')",
            [],
        )
        .unwrap();

        let roots = nav_roots_inner(&conn).unwrap();
        let asset = roots.iter().find(|r| r.category == "ASSET").unwrap();
        assert_eq!(asset.guid.as_deref(), Some("assets"));
    }
}
