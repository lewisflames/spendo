use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::prices::get_cached_prices_map;
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

const TYPE_TO_CATEGORY: [(&str, &str); 15] = [
    ("ASSET", "ASSET"),
    ("BANK", "ASSET"),
    ("CASH", "ASSET"),
    ("STOCK", "ASSET"),
    ("MUTUAL", "ASSET"),
    ("FUND", "ASSET"),
    ("CURRENCY", "ASSET"),
    ("RECEIVABLE", "ASSET"),
    ("LIABILITY", "LIABILITY"),
    ("CREDIT", "LIABILITY"),
    ("PAYABLE", "LIABILITY"),
    ("INCOME", "INCOME"),
    ("EXPENSE", "EXPENSE"),
    ("EQUITY", "EQUITY"),
    ("TRADING", "EQUITY"),
];

fn category_for_type(account_type: &str) -> &str {
    TYPE_TO_CATEGORY
        .iter()
        .find(|(ty, _)| *ty == account_type)
        .map_or(account_type, |(_, cat)| cat)
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

fn fetch_accounts_inner(
    conn: &Connection,
    as_of_date: Option<&str>,
) -> Result<AccountsResult, String> {
    // Half-open everywhere: `post_date` is `YYYY-MM-DD HH:MM:SS`, so
    // `< date(?,'+1 day')` includes the whole as-of day. `None` means current
    // state (today), matching `VISIBLE_TX_FILTER` semantics. The upper bound
    // is capped at the shared visibility window so a future as-of never leaks
    // future transactions (mirrors `reports.rs`).
    let date_expr: String = match as_of_date {
        Some(d) => d.to_string(),
        None => Utc::now().format("%Y-%m-%d").to_string(),
    };

    let sql = &format!("SELECT a.guid, a.name, a.account_type, a.parent_guid, a.code, a.description, a.placeholder,
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
                 WHERE t.post_date < min(date(?1, '+1 day'), date('now', '+1 day'))
                 GROUP BY s.account_guid
               ) b ON b.account_guid = a.guid
               WHERE a.account_type != 'ROOT'
               ORDER BY a.account_type, a.name",
        utils::RAW_AMOUNT, utils::QUANTITY_AMOUNT
    );

    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;

    let rows: Vec<AccountRow> = stmt
        .query_map(params![date_expr], |row| {
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
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let unified_prices = get_cached_prices_map(conn).unwrap_or_default();
    let report_currency = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_map = fx::get_fx_map(conn, as_of_date)?;

    let mut node_map: HashMap<String, AccountNode> = HashMap::new();
    let mut by_type: HashMap<String, TypeSummary> = HashMap::new();
    let mut missing_fx_set: HashSet<String> = HashSet::new();
    let mut missing_prices_map: HashMap<String, Option<String>> = HashMap::new();
    let mut unrealized_gains = 0.0;

    for row in &rows {
        let mut balance = if utils::is_credit_normal(&row.account_type) {
            -row.balance
        } else {
            row.balance
        };

        if utils::INVESTMENT_TYPES.contains(&row.account_type.as_str())
            && row.commodity_mnemonic.is_some()
        {
            if let Some(ref mnemonic) = row.commodity_mnemonic {
                if let Some(price) = unified_prices.get(mnemonic) {
                    let adjusted = if utils::is_credit_normal(&row.account_type) {
                        -row.quantity
                    } else {
                        row.quantity
                    };
                    let market = adjusted * price;
                    // Cost leg converts silently (never an FX warning): the
                    // holding is either priced (market dominates) or flagged
                    // via `missing_prices`.
                    let cost = fx::convert(balance, mnemonic, &report_currency, &fx_map);
                    unrealized_gains += market - cost;
                    balance = market;
                } else {
                    missing_prices_map
                        .entry(mnemonic.clone())
                        .or_insert(row.commodity_guid.clone());
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
                &report_currency,
                &fx_map,
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
            balance,
            tx_count: row.tx_count,
            children: vec![],
        };
        node_map.insert(row.guid.clone(), node);

        let category = category_for_type(&row.account_type).to_string();
        let entry = by_type.entry(category).or_insert(TypeSummary {
            count: 0,
            total_balance: 0.0,
        });
        entry.count += 1;
        entry.total_balance += balance;
    }

    // Build the parent→children index (name-sorted, orphans become roots).
    let index = {
        let triples = rows
            .iter()
            .map(|r| (r.guid.clone(), r.parent_guid.clone(), r.name.clone()));
        tree::build_index(triples)
    };

    // Recursively build tree from the adjacency map. The visited set guards
    // against `parent_guid` cycles in corrupt data (would otherwise recurse
    // until stack overflow).
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

    let mut root_nodes: Vec<AccountNode> = vec![];
    let mut visited = HashSet::new();
    for guid in &index.roots {
        if let Some(node) = build_subtree(&node_map, &index.children, guid, &mut visited) {
            root_nodes.push(node);
        }
    }

    // Propagate child balances/txns into placeholder parents.
    // Non-placeholder accounts already have their own values from direct splits.
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
    propagate_placeholder(&mut root_nodes);

    let total_accounts = rows.len() as i64;

    let total_of = |key: &str| by_type.get(key).map(|s| s.total_balance).unwrap_or(0.0);
    let retained_earnings = total_of("INCOME") - total_of("EXPENSE");
    let balance_check =
        total_of("ASSET") - (total_of("LIABILITY") + total_of("EQUITY") + unrealized_gains + retained_earnings);
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

    Ok(AccountsResult {
        trees: root_nodes,
        by_type,
        total_accounts,
        report_currency,
        retained_earnings,
        unrealized_gains,
        balance_check,
        missing_fx,
        missing_prices,
    })
}

pub fn get_subaccount_guids(conn: &Connection, guid: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE sub AS (
                SELECT guid FROM accounts WHERE guid = ?1
                UNION ALL
                SELECT a.guid FROM accounts a JOIN sub ON a.parent_guid = sub.guid
            )
            SELECT guid FROM sub",
        )
        .map_err(|e| e.to_string())?;

    let result: Vec<_> = stmt
        .query_map([guid], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
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
/// no second full-tree fetch. `as_of` is forwarded to the tree fetch so
/// detail balances agree with the accounts page for the same date.
/// Empty/`None` when the account is missing from the tree.
pub fn fetch_children_and_subtree(
    conn: &Connection,
    parent_guid: &str,
    as_of: Option<&str>,
) -> Result<AccountTreeSlice, String> {
    let result = fetch_accounts_inner(conn, as_of)?;
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
pub async fn fetch_accounts(
    state: State<'_, AppState>,
    as_of_date: Option<String>,
) -> Result<AccountsResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_accounts_inner(conn, as_of_date.as_deref())
    })
    .await
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
        let result = fetch_accounts_inner(&conn, None).unwrap();

        // Cash 500 + AAPL market 1500 + unpriced XYZ 0.
        assert_eq!(result.by_type.get("ASSET").unwrap().total_balance, 2000.0);
        // Market 1500 - cost 1000.
        assert_eq!(result.unrealized_gains, 500.0);
        assert_eq!(result.missing_prices.len(), 1);
        assert_eq!(result.missing_prices[0].mnemonic, "XYZ");

        let (_, subtree, _) = fetch_children_and_subtree(&conn, "assets", None).unwrap();
        assert_eq!(subtree.unwrap().balance, 2000.0);
    }

    #[test]
    fn investment_balances_respect_as_of() {
        let conn = setup_investments();
        let result = fetch_accounts_inner(&conn, Some("2024-01-15")).unwrap();
        assert_eq!(result.by_type.get("ASSET").unwrap().total_balance, 500.0);
    }
}
