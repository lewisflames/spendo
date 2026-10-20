use crate::db::AppError;
use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::tree;
use crate::utils;

/// Minimum trimmed query length before any SQL runs. The palette also gates
/// on this so single characters never trigger full-table scans.
pub const MIN_QUERY_LEN: usize = 2;
const DEFAULT_LIMIT: i64 = 5;
const MAX_LIMIT: i64 = 10;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchAccount {
    pub guid: String,
    pub name: String,
    pub full_path: String,
    pub account_type: String,
    pub code: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchTransaction {
    pub guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHolding {
    pub commodity_guid: String,
    pub mnemonic: String,
    pub full_name: Option<String>,
    pub namespace: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchBudget {
    pub account_guid: String,
    pub account_name: String,
    /// Most recent year with a budget row for the account (account-only UX).
    pub year: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalSearchResult {
    pub accounts: Vec<SearchAccount>,
    pub transactions: Vec<SearchTransaction>,
    pub holdings: Vec<SearchHolding>,
    pub budgets: Vec<SearchBudget>,
}

fn empty_result() -> GlobalSearchResult {
    GlobalSearchResult {
        accounts: vec![],
        transactions: vec![],
        holdings: vec![],
        budgets: vec![],
    }
}

fn search_accounts_inner(
    conn: &Connection,
    like: &str,
    limit: i64,
) -> Result<Vec<SearchAccount>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT guid, name, account_type, code, description
             FROM accounts
             WHERE account_type != 'ROOT'
               AND (name LIKE '%' || ?1 || '%' ESCAPE '\\'
                    OR code LIKE '%' || ?1 || '%' ESCAPE '\\'
                    OR description LIKE '%' || ?1 || '%' ESCAPE '\\')
             ORDER BY name COLLATE NOCASE
             LIMIT ?2",
    )?;
    let mut accounts: Vec<SearchAccount> = stmt
        .query_map(rusqlite::params![like, limit], |row| {
            Ok(SearchAccount {
                guid: row.get(0)?,
                name: row.get(1)?,
                full_path: String::new(),
                account_type: row.get(2)?,
                code: row.get(3)?,
                description: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    // One account index for all path walks instead of queries per ancestor.
    let metas = tree::load_account_metas(conn)?;
    for account in &mut accounts {
        account.full_path = tree::full_account_path(&account.guid, &metas, " : ");
    }
    Ok(accounts)
}

fn search_transactions_inner(
    conn: &Connection,
    like: &str,
    limit: i64,
) -> Result<Vec<SearchTransaction>, AppError> {
    let sql = format!(
        "SELECT guid, post_date, description, num
         FROM transactions t
         WHERE {}
           AND (t.description LIKE '%' || ?1 || '%' ESCAPE '\\'
                OR t.num LIKE '%' || ?1 || '%' ESCAPE '\\'
                OR t.notes LIKE '%' || ?1 || '%' ESCAPE '\\'
                OR EXISTS (SELECT 1 FROM splits m WHERE m.tx_guid = t.guid
                           AND m.memo LIKE '%' || ?1 || '%' ESCAPE '\\'))
         ORDER BY t.post_date DESC, t.guid DESC
         LIMIT ?2",
        utils::VISIBLE_TX_FILTER
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![like, limit], |row| {
            Ok(SearchTransaction {
                guid: row.get(0)?,
                post_date: row.get(1)?,
                description: row.get(2)?,
                num: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn search_holdings_inner(
    conn: &Connection,
    like: &str,
    limit: i64,
) -> Result<Vec<SearchHolding>, AppError> {
    // Mirror the portfolio universe: non-currency commodities that either
    // have investment splits or at least one stored price (sold-out /
    // watch-only rows stay reachable, anything else has no detail page).
    let mut stmt = conn.prepare(
        "SELECT c.guid, c.mnemonic, c.fullname, c.namespace
             FROM commodities c
             WHERE c.namespace != 'CURRENCY'
               AND c.namespace != 'template'
               AND (c.mnemonic LIKE '%' || ?1 || '%' ESCAPE '\\'
                    OR c.fullname LIKE '%' || ?1 || '%' ESCAPE '\\')
               AND (EXISTS (SELECT 1 FROM accounts a
                            JOIN splits s ON s.account_guid = a.guid
                            WHERE a.commodity_guid = c.guid)
                    OR EXISTS (SELECT 1 FROM prices p
                               WHERE p.commodity_guid = c.guid))
             ORDER BY c.mnemonic COLLATE NOCASE
             LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![like, limit], |row| {
            Ok(SearchHolding {
                commodity_guid: row.get(0)?,
                mnemonic: row.get(1)?,
                full_name: row.get(2)?,
                namespace: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn search_budgets_inner(
    conn: &Connection,
    like: &str,
    limit: i64,
) -> Result<Vec<SearchBudget>, AppError> {
    // Account-only UX: one row per matching budgeted account with its most
    // recent budget year for direct navigation.
    let mut stmt = conn.prepare(
        "SELECT a.guid, a.name, MAX(b.year) as year
             FROM budgets b
             JOIN accounts a ON a.guid = b.account_guid
             WHERE a.name LIKE '%' || ?1 || '%' ESCAPE '\\'
                OR a.description LIKE '%' || ?1 || '%' ESCAPE '\\'
             GROUP BY a.guid
             ORDER BY a.name COLLATE NOCASE
             LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![like, limit], |row| {
            Ok(SearchBudget {
                account_guid: row.get(0)?,
                account_name: row.get(1)?,
                year: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn search_global_inner(
    conn: &Connection,
    query: &str,
    limit_per_group: Option<i64>,
) -> Result<GlobalSearchResult, AppError> {
    let trimmed = query.trim();
    if trimmed.chars().count() < MIN_QUERY_LEN {
        return Ok(empty_result());
    }
    let limit = limit_per_group.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let like = utils::escape_like(trimmed);
    Ok(GlobalSearchResult {
        accounts: search_accounts_inner(conn, &like, limit)?,
        transactions: search_transactions_inner(conn, &like, limit)?,
        holdings: search_holdings_inner(conn, &like, limit)?,
        budgets: search_budgets_inner(conn, &like, limit)?,
    })
}

#[tauri::command]
pub async fn search_global(
    state: State<'_, AppState>,
    query: String,
    limit_per_group: Option<i64>,
) -> Result<GlobalSearchResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        search_global_inner(conn, &query, limit_per_group).map_err(String::from)
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
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES
             ('c-eur', 'EUR', 'CURRENCY', NULL),
             ('sec-aapl', 'AAPL', 'NASDAQ', 'Apple Inc')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid) VALUES
             ('root', 'Root', 'ROOT', NULL, NULL, NULL, 0, 'c-eur'),
             ('exp', 'Expenses', 'EXPENSE', 'root', NULL, NULL, 1, 'c-eur'),
             ('gro', 'Groceries', 'EXPENSE', 'exp', 'GR1', 'weekly food', 0, 'c-eur'),
             ('chk', 'Checking', 'BANK', 'root', NULL, NULL, 0, 'c-eur'),
             ('brk', 'Brokerage', 'STOCK', 'root', NULL, NULL, 0, 'sec-aapl')",
            [],
        )
        .unwrap();
        // Visible transaction matching on description.
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes)
             VALUES ('t1', '2024-01-10', 'Grocery run', 'N1', 'weekly')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, memo)
             VALUES ('s1', 't1', 'gro', 5000, 100, 5000, 100, 'food'),
                    ('s2', 't1', 'chk', -5000, 100, -5000, 100, NULL)",
            [],
        )
        .unwrap();
        // Future-dated transaction must stay invisible.
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description) VALUES ('t2', '2099-01-01', 'Grocery future')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s3', 't2', 'gro', 100, 100, 100, 100)",
            [],
        )
        .unwrap();
        // Investment split + price so AAPL is in the holding universe.
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description) VALUES ('t3', '2024-02-01', 'Buy AAPL')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s4', 't3', 'brk', 100000, 100, 10, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES ('p1', 'sec-aapl', 'c-eur', 15000, 100, '2024-03-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, year, amount, created_at)
             VALUES ('b1', 'gro', 2024, 1200, '2024-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn short_and_blank_queries_return_empty() {
        let conn = setup();
        let r = search_global_inner(&conn, "", None).unwrap();
        assert!(r.accounts.is_empty());
        let r = search_global_inner(&conn, "x", None).unwrap();
        assert!(r.transactions.is_empty());
        let r = search_global_inner(&conn, "  ", None).unwrap();
        assert!(r.holdings.is_empty());
    }

    #[test]
    fn finds_across_all_groups() {
        let conn = setup();
        // 'gro' hits the Groceries account (name + code GR1) and the budget.
        let res = search_global_inner(&conn, "gro", None).unwrap();
        assert!(res.accounts.iter().any(|a| a.guid == "gro"));
        assert_eq!(res.accounts[0].full_path, "Expenses : Groceries");
        assert!(res.budgets.iter().any(|b| b.account_guid == "gro"));
        assert_eq!(res.budgets[0].year, 2024);
        // 'grocery' hits the visible transaction but not the future one.
        let res = search_global_inner(&conn, "grocery", None).unwrap();
        assert_eq!(res.transactions.len(), 1);
        assert_eq!(res.transactions[0].guid, "t1");
        // 'aapl' hits the holding (mnemonic + fullname).
        let res = search_global_inner(&conn, "aapl", None).unwrap();
        assert!(res.holdings.iter().any(|h| h.commodity_guid == "sec-aapl"));
        let res = search_global_inner(&conn, "apple", None).unwrap();
        assert!(res.holdings.iter().any(|h| h.commodity_guid == "sec-aapl"));
    }

    #[test]
    fn like_metacharacters_match_literally() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('pct', '100% Sure', 'EXPENSE', 'exp', 0, 'c-eur')",
            [],
        )
        .unwrap();
        let res = search_global_inner(&conn, "100%", None).unwrap();
        assert!(res.accounts.iter().any(|a| a.guid == "pct"));
        // A bare wildcard must not match everything.
        let res = search_global_inner(&conn, "%_", None).unwrap();
        assert!(res.accounts.is_empty());
    }

    #[test]
    fn limits_are_clamped_per_group() {
        let conn = setup();
        for i in 0..8 {
            conn.execute(
                "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
                 VALUES (?1, ?2, 'EXPENSE', 'exp', 0, 'c-eur')",
                params![format!("lim-{i}"), format!("LimitMe {i}")],
            )
            .unwrap();
        }
        let res = search_global_inner(&conn, "LimitMe", Some(3)).unwrap();
        assert_eq!(res.accounts.len(), 3);
        // Absurd limits clamp instead of dumping the table.
        let res = search_global_inner(&conn, "LimitMe", Some(10_000)).unwrap();
        assert!(res.accounts.len() <= MAX_LIMIT as usize);
    }

    #[test]
    fn budgets_return_latest_year_per_account() {
        let conn = setup();
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, year, amount, created_at)
             VALUES ('b2', 'gro', 2025, 1300, '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let res = search_global_inner(&conn, "gro", None).unwrap();
        assert_eq!(res.budgets.len(), 1);
        assert_eq!(res.budgets[0].year, 2025);
    }
}
