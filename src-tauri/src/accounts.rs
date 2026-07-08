use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use tauri::State;

use crate::db::{get_conn, AppState};
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
}

const TYPE_CATEGORY: [(&str, &str); 14] = [
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
];

fn get_category(account_type: &str) -> &str {
    for (ty, cat) in &TYPE_CATEGORY {
        if *ty == account_type {
            return cat;
        }
    }
    account_type
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
    balance: f64,
    quantity: f64,
    tx_count: i64,
}

pub fn fetch_accounts_inner(
    conn: &Connection,
    as_of_date: Option<&str>,
) -> Result<AccountsResult, String> {
    let date_expr: String = match as_of_date {
        Some(d) => d.to_string(),
        None => Utc::now().format("%Y-%m-%d").to_string(),
    };

    let sql = &format!("SELECT a.guid, a.name, a.account_type, a.parent_guid, a.code, a.description, a.placeholder,
                      c.mnemonic as commodity_mnemonic,
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
                 WHERE t.post_date < ?1
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
                balance: row.get(8)?,
                quantity: row.get(9)?,
                tx_count: row.get(10)?,
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
                    balance = adjusted * price;
                }
            }
        } else if let Some(ref mnemonic) = row.commodity_mnemonic {
            // Cash-type (and income/expense/equity) accounts hold their own currency:
            // convert to the report currency for aggregation.
            balance = fx::convert(balance, mnemonic, &report_currency, &fx_map);
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

        let category = get_category(&row.account_type).to_string();
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
    fn propagate_placeholder(nodes: &mut Vec<AccountNode>) {
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

    Ok(AccountsResult {
        trees: root_nodes,
        by_type,
        total_accounts,
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

#[tauri::command]
pub async fn fetch_accounts(
    state: State<'_, AppState>,
    as_of_date: Option<String>,
) -> Result<AccountsResult, String> {
    let conn = get_conn(&state)?;
    fetch_accounts_inner(&conn, as_of_date.as_deref())
}
