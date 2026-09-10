use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::tree;
use crate::utils;

/// One CSV row per split, mirroring GnuCash's multiline export so the file can
/// be re-imported via `File → Import → Import Transactions from CSV`
/// (Multi-split checked, `GnuCash Export Settings` preset). Transaction-level
/// fields repeat on every split row — the importer tolerates repeats and it
/// keeps the CSV self-describing without blank-continuation parsing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSplitRow {
    pub tx_guid: String,
    pub split_guid: String,
    /// `YYYY-MM-DD` (date part of `post_date`).
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    /// Mnemonic of the split account's commodity.
    pub commodity: Option<String>,
    pub memo: Option<String>,
    /// Colon-joined path excluding the internal ROOT, e.g. `Assets:Bank:Checking`.
    pub full_account_name: String,
    /// Leaf account name.
    pub account_name: String,
    /// Raw native amount (`value_num/value_denom`): no FX, no sign flips.
    pub amount: f64,
    /// GnuCash reconcile state ('n'/'c'/'y') or blank.
    pub reconcile_state: Option<String>,
    /// `amount/quantity` for investments, else `1`.
    pub price: f64,
}

/// One CSV row per account in parent-before-child (pre-order DFS) order so a
/// GnuCash account import can recreate parents first. Balances are
/// deliberately omitted: they are report-currency converted snapshots and
/// would corrupt a re-import — balances rebuild from the transactions file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportAccountRow {
    pub guid: String,
    /// `None` for top-level accounts (parent is the internal ROOT or missing).
    pub parent_guid: Option<String>,
    pub account_type: String,
    /// Colon-joined path excluding the internal ROOT.
    pub full_name: String,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub commodity: Option<String>,
    pub commodity_namespace: Option<String>,
    pub placeholder: bool,
}

struct AccountMeta {
    name: String,
    parent_guid: Option<String>,
    account_type: String,
}

fn load_account_metas(conn: &Connection) -> Result<HashMap<String, AccountMeta>, String> {
    let mut stmt = conn
        .prepare("SELECT guid, name, parent_guid, account_type FROM accounts")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                AccountMeta {
                    name: row.get(1)?,
                    parent_guid: row.get(2)?,
                    account_type: row.get(3)?,
                },
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Colon-joined account path, skipping the internal ROOT ancestor. The visited
/// set guards against `parent_guid` cycles in corrupt books.
fn full_account_path(guid: &str, metas: &HashMap<String, AccountMeta>) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let mut visited: HashSet<&str> = HashSet::new();
    let mut current = Some(guid);
    while let Some(g) = current {
        if !visited.insert(g) {
            break;
        }
        let Some(meta) = metas.get(g) else {
            break;
        };
        if meta.account_type != "ROOT" {
            parts.push(meta.name.as_str());
        }
        current = meta.parent_guid.as_deref();
        // Cap the walk so a long corrupt chain can't blow up the export.
        if visited.len() > 100 {
            break;
        }
    }
    parts.reverse();
    parts.join(":")
}

fn price_for(amount: f64, quantity: f64) -> f64 {
    if quantity.abs() > 1e-12 && amount.is_finite() && quantity.is_finite() {
        amount / quantity
    } else {
        1.0
    }
}

fn fetch_export_transactions_inner(conn: &Connection) -> Result<Vec<ExportSplitRow>, String> {
    let metas = load_account_metas(conn)?;
    let mut path_cache: HashMap<String, String> = HashMap::new();
    for guid in metas.keys() {
        path_cache.insert(guid.clone(), full_account_path(guid, &metas));
    }

    let sql = format!(
        "SELECT t.guid, substr(t.post_date, 1, 10), t.description, t.num, t.notes,
                s.guid, s.account_guid, s.memo, s.reconcile_state,
                {} as amount,
                {} as quantity,
                a.name as account_name,
                c.mnemonic as commodity
         FROM splits s
         JOIN transactions t ON t.guid = s.tx_guid
         JOIN accounts a ON a.guid = s.account_guid
         LEFT JOIN commodities c ON c.guid = a.commodity_guid
         WHERE {}
         ORDER BY t.post_date DESC, t.guid DESC,
                  ABS(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)) DESC,
                  s.guid ASC",
        utils::RAW_AMOUNT,
        utils::QUANTITY_AMOUNT,
        utils::VISIBLE_TX_FILTER
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, f64>(9)?,
                row.get::<_, f64>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, Option<String>>(12)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(
                tx_guid,
                post_date,
                description,
                num,
                notes,
                split_guid,
                account_guid,
                memo,
                reconcile_state,
                amount,
                quantity,
                account_name,
                commodity,
            )| {
                let full_account_name = path_cache
                    .get(&account_guid)
                    .cloned()
                    .unwrap_or_else(|| account_name.clone());
                ExportSplitRow {
                    tx_guid,
                    split_guid,
                    post_date,
                    description,
                    num,
                    notes,
                    commodity,
                    memo,
                    full_account_name,
                    account_name,
                    amount,
                    reconcile_state,
                    price: price_for(amount, quantity),
                }
            },
        )
        .collect())
}

fn fetch_export_accounts_inner(conn: &Connection) -> Result<Vec<ExportAccountRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT a.guid, a.parent_guid, a.name, a.account_type, a.code, a.description,
                    a.placeholder, c.mnemonic, c.namespace
             FROM accounts a
             LEFT JOIN commodities c ON c.guid = a.commodity_guid
             WHERE a.account_type != 'ROOT'",
        )
        .map_err(|e| e.to_string())?;
    struct Raw {
        guid: String,
        parent_guid: Option<String>,
        name: String,
        account_type: String,
        code: Option<String>,
        description: Option<String>,
        placeholder: bool,
        commodity: Option<String>,
        commodity_namespace: Option<String>,
    }
    let raws = stmt
        .query_map([], |row| {
            Ok(Raw {
                guid: row.get(0)?,
                parent_guid: row.get(1)?,
                name: row.get(2)?,
                account_type: row.get(3)?,
                code: row.get(4)?,
                description: row.get(5)?,
                placeholder: row.get::<_, i64>(6)? != 0,
                commodity: row.get(7)?,
                commodity_namespace: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    // Full paths need the complete map (including the excluded ROOT row).
    let metas = load_account_metas(conn)?;
    // Null the parent link when it points at the internal ROOT or a missing
    // row so top-level accounts import as roots instead of dangling.
    let exportable_parent = |guid: &str, parent: &Option<String>| -> Option<String> {
        let p = parent.clone()?;
        match metas.get(&p) {
            Some(meta) if meta.account_type != "ROOT" && p != guid => Some(p),
            _ => None,
        }
    };

    let mut by_guid: HashMap<String, Raw> = HashMap::new();
    for r in raws {
        by_guid.insert(r.guid.clone(), r);
    }
    // Parent→children adjacency over the exported rows only; orphans whose
    // parent isn't exported (ROOT/missing) become roots.
    let index = tree::build_index(by_guid.values().map(|r| {
        let parent = match &r.parent_guid {
            Some(p) if by_guid.contains_key(p) => Some(p.clone()),
            _ => None,
        };
        (r.guid.clone(), parent, r.name.clone())
    }));

    fn visit(
        guid: &str,
        by_guid: &HashMap<String, Raw>,
        index: &tree::TreeIndex,
        metas: &HashMap<String, AccountMeta>,
        exportable_parent: &impl Fn(&str, &Option<String>) -> Option<String>,
        out: &mut Vec<ExportAccountRow>,
        visited: &mut HashSet<String>,
    ) {
        if !visited.insert(guid.to_string()) {
            return;
        }
        if let Some(r) = by_guid.get(guid) {
            out.push(ExportAccountRow {
                parent_guid: exportable_parent(guid, &r.parent_guid),
                full_name: full_account_path(guid, metas),
                guid: r.guid.clone(),
                account_type: r.account_type.clone(),
                name: r.name.clone(),
                code: r.code.clone(),
                description: r.description.clone(),
                commodity: r.commodity.clone(),
                commodity_namespace: r.commodity_namespace.clone(),
                placeholder: r.placeholder,
            });
        }
        if let Some(kids) = index.children.get(guid) {
            for kid in kids {
                visit(kid, by_guid, index, metas, exportable_parent, out, visited);
            }
        }
    }

    let mut out = Vec::with_capacity(by_guid.len());
    let mut visited = HashSet::new();
    for root in &index.roots {
        visit(
            root,
            &by_guid,
            &index,
            &metas,
            &exportable_parent,
            &mut out,
            &mut visited,
        );
    }
    // Cycle orphans never reachable from a root: emit them by name so no
    // account silently disappears from the export.
    let mut leftover: Vec<&String> = by_guid.keys().filter(|g| !visited.contains(*g)).collect();
    leftover.sort();
    for guid in leftover {
        visit(
            guid,
            &by_guid,
            &index,
            &metas,
            &exportable_parent,
            &mut out,
            &mut visited,
        );
    }
    Ok(out)
}

#[tauri::command]
pub async fn fetch_export_transactions(
    state: State<'_, AppState>,
) -> Result<Vec<ExportSplitRow>, String> {
    run_blocking(state.db.clone(), fetch_export_transactions_inner).await
}

#[tauri::command]
pub async fn fetch_export_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<ExportAccountRow>, String> {
    run_blocking(state.db.clone(), fetch_export_accounts_inner).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES
             ('c-eur', 'EUR', 'CURRENCY'), ('c-aapl', 'AAPL', 'NASDAQ')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid, code, description) VALUES
             ('root', 'Root Account', 'ROOT', NULL, 0, 'c-eur', NULL, NULL),
             ('assets', 'Assets', 'ASSET', 'root', 1, 'c-eur', NULL, NULL),
             ('checking', 'Checking', 'BANK', 'assets', 0, 'c-eur', '1001', 'Everyday'),
             ('broker', 'Broker', 'STOCK', 'assets', 0, 'c-aapl', NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes) VALUES
             ('t1', '2024-03-01 10:00:00', 'Groceries', '12', 'weekly'),
             ('t-future', '2099-01-01 00:00:00', 'Future', NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state, memo) VALUES
             ('s1', 't1', 'checking', -5000, 100, -5000, 100, 'c', 'card'),
             ('s2', 't1', 'broker', 100000, 100, 10, 1, 'n', NULL),
             ('s-future', 't-future', 'checking', 100, 100, 100, 100, 'n', NULL)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn split_rows_carry_full_paths_and_raw_amounts() {
        let conn = setup();
        let rows = fetch_export_transactions_inner(&conn).unwrap();
        // Future-dated transaction stays out via the visibility window.
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.tx_guid == "t1"));
        // Largest-magnitude split first.
        assert_eq!(rows[0].account_name, "Broker");
        assert_eq!(rows[0].full_account_name, "Assets:Broker");
        assert_eq!(rows[0].post_date, "2024-03-01");
        assert_eq!(rows[0].amount, 1000.0);
        assert_eq!(rows[0].price, 100.0);
        assert_eq!(rows[0].commodity.as_deref(), Some("AAPL"));
        assert_eq!(rows[1].full_account_name, "Assets:Checking");
        assert_eq!(rows[1].amount, -50.0);
        assert_eq!(rows[1].price, 1.0);
        assert_eq!(rows[1].memo.as_deref(), Some("card"));
        assert_eq!(rows[1].reconcile_state.as_deref(), Some("c"));
        // Transaction-level fields repeat on every split row.
        assert!(rows
            .iter()
            .all(|r| r.description.as_deref() == Some("Groceries")));
        assert!(rows.iter().all(|r| r.num.as_deref() == Some("12")));
    }

    #[test]
    fn account_rows_emit_parents_before_children_with_full_names() {
        let conn = setup();
        let rows = fetch_export_accounts_inner(&conn).unwrap();
        assert_eq!(rows.len(), 3);
        // Pre-order DFS: parent first, children name-sorted.
        assert_eq!(
            rows.iter().map(|r| r.guid.as_str()).collect::<Vec<_>>(),
            vec!["assets", "broker", "checking"]
        );
        let assets = &rows[0];
        assert_eq!(assets.full_name, "Assets");
        assert_eq!(assets.parent_guid, None);
        assert!(assets.placeholder);
        let broker = &rows[1];
        assert_eq!(broker.full_name, "Assets:Broker");
        assert_eq!(broker.parent_guid.as_deref(), Some("assets"));
        assert_eq!(broker.commodity.as_deref(), Some("AAPL"));
        assert_eq!(broker.commodity_namespace.as_deref(), Some("NASDAQ"));
        let checking = &rows[2];
        assert_eq!(checking.full_name, "Assets:Checking");
        assert_eq!(checking.code.as_deref(), Some("1001"));
        assert_eq!(checking.description.as_deref(), Some("Everyday"));
    }
}
