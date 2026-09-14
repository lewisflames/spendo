use rusqlite::types::ToSql;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{run_blocking, validate_data_table, with_tx_err, AppError, AppState};
use crate::home::{get_summary, DataSummary};
use crate::sql;
use crate::utils;

// ── Wire structs ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    pub id: i64,
    pub occurred_at: String,
    pub kind: String,
    pub label: String,
    pub source: String,
    pub transactions_affected: i64,
    pub accounts_affected: i64,
    pub reverts_id: Option<i64>,
    pub reverted_by: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeHistoryPage {
    pub entries: Vec<ChangeEntry>,
    pub total: i64,
    pub page_size: i64,
    pub total_pages: i64,
    /// Latest original entry eligible for linear undo: newest non-revert row
    /// with no `reverted_by`. `None` when history is empty or everything is
    /// already reverted. The frontend enables the Revert action only here.
    pub revertible_id: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertResult {
    pub reverted_id: i64,
    pub revert_id: i64,
    pub summary: DataSummary,
}

/// Arguments for `fetch_change_history`. `kind`/`source` are exact matches;
/// `search` is a literal substring over the label.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeHistoryQuery {
    pub page: i64,
    pub page_size: i64,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
}

// ── Snapshots ────────────────────────────────────────────────────────────────

/// One row captured before a mutation. `before_json` is `None` when the row
/// did not exist (it was added by the operation — revert deletes it).
pub(crate) struct Snapshot<'a> {
    pub(crate) table_name: &'a str,
    pub(crate) row_guid: String,
    pub(crate) before_json: Option<String>,
}

/// Read a live row and serialize it for `change_snapshots.before_json`.
/// Returns `Ok(None)` when the row does not exist (an add — revert deletes).
/// Keys mirror column names so `restore_snapshot_row` can map them back.
pub(crate) fn snapshot_row(
    conn: &Connection,
    table: &str,
    guid: &str,
) -> Result<Option<String>, String> {
    validate_data_table(table)?;
    let sql = match table {
        "commodities" => {
            "SELECT guid, mnemonic, namespace, fullname FROM commodities WHERE guid = ?1"
        }
        "accounts" => {
            "SELECT guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid
             FROM accounts WHERE guid = ?1"
        }
        "transactions" => {
            "SELECT guid, post_date, description, num, notes FROM transactions WHERE guid = ?1"
        }
        "splits" => {
            "SELECT guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom,
                    reconcile_state, reconciled, memo FROM splits WHERE guid = ?1"
        }
        _ => return Err(format!("Invalid table name: {table}")),
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let value: Option<serde_json::Value> = match table {
        "commodities" => stmt
            .query_row([guid], |row| {
                Ok(serde_json::json!({
                    "guid": row.get::<_, String>(0)?,
                    "mnemonic": row.get::<_, String>(1)?,
                    "namespace": row.get::<_, String>(2)?,
                    "fullname": row.get::<_, Option<String>>(3)?,
                }))
            })
            .optional()
            .map_err(|e| e.to_string())?,
        "accounts" => stmt
            .query_row([guid], |row| {
                Ok(serde_json::json!({
                    "guid": row.get::<_, String>(0)?,
                    "name": row.get::<_, String>(1)?,
                    "account_type": row.get::<_, String>(2)?,
                    "parent_guid": row.get::<_, Option<String>>(3)?,
                    "code": row.get::<_, Option<String>>(4)?,
                    "description": row.get::<_, Option<String>>(5)?,
                    "placeholder": row.get::<_, i64>(6)?,
                    "commodity_guid": row.get::<_, Option<String>>(7)?,
                }))
            })
            .optional()
            .map_err(|e| e.to_string())?,
        "transactions" => stmt
            .query_row([guid], |row| {
                Ok(serde_json::json!({
                    "guid": row.get::<_, String>(0)?,
                    "post_date": row.get::<_, String>(1)?,
                    "description": row.get::<_, Option<String>>(2)?,
                    "num": row.get::<_, Option<String>>(3)?,
                    "notes": row.get::<_, Option<String>>(4)?,
                }))
            })
            .optional()
            .map_err(|e| e.to_string())?,
        _ => stmt
            .query_row([guid], |row| {
                Ok(serde_json::json!({
                    "guid": row.get::<_, String>(0)?,
                    "tx_guid": row.get::<_, String>(1)?,
                    "account_guid": row.get::<_, String>(2)?,
                    "value_num": row.get::<_, i64>(3)?,
                    "value_denom": row.get::<_, i64>(4)?,
                    "quantity_num": row.get::<_, i64>(5)?,
                    "quantity_denom": row.get::<_, i64>(6)?,
                    "reconcile_state": row.get::<_, Option<String>>(7)?,
                    "reconciled": row.get::<_, i64>(8)?,
                    "memo": row.get::<_, Option<String>>(9)?,
                }))
            })
            .optional()
            .map_err(|e| e.to_string())?,
    };
    Ok(value.map(|v| v.to_string()))
}

fn str_field(v: &serde_json::Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Snapshot is missing '{key}'"))
}

fn opt_str_field(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

fn int_field(v: &serde_json::Value, key: &str) -> Result<i64, String> {
    v.get(key)
        .and_then(|x| x.as_i64())
        .ok_or_else(|| format!("Snapshot is missing '{key}'"))
}

/// Re-insert (or overwrite) a row from its before-image. Uses an upsert rather
/// than `INSERT OR REPLACE`: REPLACE deletes the conflicting row first, and
/// with foreign keys enabled that fires `ON DELETE CASCADE` (e.g. wiping
/// `budgets` when restoring an account edit). Callers order phases: parents
/// before children so FK references resolve.
fn restore_snapshot_row(conn: &Connection, table: &str, before: &str) -> Result<(), String> {
    validate_data_table(table)?;
    let v: serde_json::Value =
        serde_json::from_str(before).map_err(|e| format!("Corrupt history snapshot: {e}"))?;
    match table {
        "commodities" => {
            conn.execute(
                "INSERT INTO commodities (guid, mnemonic, namespace, fullname)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(guid) DO UPDATE SET
                     mnemonic = excluded.mnemonic,
                     namespace = excluded.namespace,
                     fullname = excluded.fullname",
                params![
                    str_field(&v, "guid")?,
                    str_field(&v, "mnemonic")?,
                    str_field(&v, "namespace")?,
                    opt_str_field(&v, "fullname"),
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        "accounts" => {
            conn.execute(
                "INSERT INTO accounts
                 (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(guid) DO UPDATE SET
                     name = excluded.name,
                     account_type = excluded.account_type,
                     parent_guid = excluded.parent_guid,
                     code = excluded.code,
                     description = excluded.description,
                     placeholder = excluded.placeholder,
                     commodity_guid = excluded.commodity_guid",
                params![
                    str_field(&v, "guid")?,
                    str_field(&v, "name")?,
                    str_field(&v, "account_type")?,
                    opt_str_field(&v, "parent_guid"),
                    opt_str_field(&v, "code"),
                    opt_str_field(&v, "description"),
                    int_field(&v, "placeholder")?,
                    opt_str_field(&v, "commodity_guid"),
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        "transactions" => {
            conn.execute(
                "INSERT INTO transactions (guid, post_date, description, num, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(guid) DO UPDATE SET
                     post_date = excluded.post_date,
                     description = excluded.description,
                     num = excluded.num,
                     notes = excluded.notes",
                params![
                    str_field(&v, "guid")?,
                    str_field(&v, "post_date")?,
                    opt_str_field(&v, "description"),
                    opt_str_field(&v, "num"),
                    opt_str_field(&v, "notes"),
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        _ => {
            conn.execute(
                "INSERT INTO splits
                 (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom,
                  reconcile_state, reconciled, memo)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(guid) DO UPDATE SET
                     tx_guid = excluded.tx_guid,
                     account_guid = excluded.account_guid,
                     value_num = excluded.value_num,
                     value_denom = excluded.value_denom,
                     quantity_num = excluded.quantity_num,
                     quantity_denom = excluded.quantity_denom,
                     reconcile_state = excluded.reconcile_state,
                     reconciled = excluded.reconciled,
                     memo = excluded.memo",
                params![
                    str_field(&v, "guid")?,
                    str_field(&v, "tx_guid")?,
                    str_field(&v, "account_guid")?,
                    int_field(&v, "value_num")?,
                    int_field(&v, "value_denom")?,
                    int_field(&v, "quantity_num")?,
                    int_field(&v, "quantity_denom")?,
                    opt_str_field(&v, "reconcile_state"),
                    int_field(&v, "reconciled")?,
                    opt_str_field(&v, "memo"),
                ],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ── Entry writer (shared by import + future manual editors) ──────────────────

/// Insert one `change_log` row plus its snapshots. Must be called inside the
/// caller's transaction so log and data commit atomically.
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_change_entry(
    conn: &Connection,
    kind: &str,
    label: &str,
    source: &str,
    transactions_affected: i64,
    accounts_affected: i64,
    detail_json: Option<&str>,
    snapshots: &[Snapshot<'_>],
) -> Result<i64, String> {
    let occurred_at = utils::iso_now();
    conn.execute(
        "INSERT INTO change_log
         (occurred_at, kind, label, source, transactions_affected, accounts_affected, detail_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            occurred_at,
            kind,
            label,
            source,
            transactions_affected,
            accounts_affected,
            detail_json,
        ],
    )
    .map_err(|e| e.to_string())?;
    let entry_id = conn.last_insert_rowid();
    {
        let mut stmt = conn
            .prepare(
                "INSERT INTO change_snapshots (entry_id, table_name, row_guid, before_json)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .map_err(|e| e.to_string())?;
        for s in snapshots {
            validate_data_table(s.table_name)?;
            stmt.execute(params![entry_id, s.table_name, s.row_guid, s.before_json])
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(entry_id)
}

// ── Fetch ────────────────────────────────────────────────────────────────────

/// Optional `change_log` predicates shared by the count and page queries.
#[derive(Debug, Default)]
pub(crate) struct HistoryFilter<'a> {
    kind: Option<&'a str>,
    source: Option<&'a str>,
    search: Option<&'a str>,
}

impl<'a> HistoryFilter<'a> {
    /// Trim and drop empty values so `?kind=` / whitespace search behave as
    /// "no filter" instead of matching nothing.
    fn from_query(query: &'a ChangeHistoryQuery) -> Self {
        Self {
            kind: non_empty(query.kind.as_deref()),
            source: non_empty(query.source.as_deref()),
            search: non_empty(query.search.as_deref()),
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

/// Build the shared WHERE clause + bound params for the filter predicates.
/// `kind`/`source` are exact matches; `search` is a literal LIKE over the label
/// (metacharacters escaped). Placeholder indices start at 1 so callers can
/// append their own placeholders after `params.len()`.
fn history_where(filter: &HistoryFilter) -> (String, Vec<Box<dyn ToSql>>) {
    let mut w = sql::WhereBuilder::new(0);
    if let Some(kind) = filter.kind {
        w.add("kind = {}", vec![Box::new(kind.to_string())]);
    }
    if let Some(source) = filter.source {
        w.add("source = {}", vec![Box::new(source.to_string())]);
    }
    if let Some(search) = filter.search {
        w.add(
            "label LIKE '%' || {} || '%' ESCAPE '\\'",
            vec![Box::new(utils::escape_like(search))],
        );
    }
    (w.where_clause(), w.params)
}

fn get_change_entries(
    conn: &Connection,
    limit: i64,
    offset: i64,
    filter: &HistoryFilter,
) -> Result<Vec<ChangeEntry>, String> {
    let (where_clause, mut params) = history_where(filter);
    let limit_idx = params.len() + 1;
    let offset_idx = limit_idx + 1;
    let sql_text = format!(
        "SELECT id, occurred_at, kind, label, source,
                transactions_affected, accounts_affected, reverts_id, reverted_by
         FROM change_log{where_clause} ORDER BY id DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
    );
    params.push(Box::new(limit));
    params.push(Box::new(offset));
    let mut stmt = conn.prepare(&sql_text).map_err(|e| e.to_string())?;
    let mapped = stmt
        .query_map(sql::param_refs(&params).as_slice(), |row| {
            Ok(ChangeEntry {
                id: row.get(0)?,
                occurred_at: row.get(1)?,
                kind: row.get(2)?,
                label: row.get(3)?,
                source: row.get(4)?,
                transactions_affected: row.get(5)?,
                accounts_affected: row.get(6)?,
                reverts_id: row.get(7)?,
                reverted_by: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;
    mapped
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn count_change_entries(conn: &Connection, filter: &HistoryFilter) -> Result<i64, String> {
    let (where_clause, params) = history_where(filter);
    let sql_text = format!("SELECT COUNT(*) FROM change_log{where_clause}");
    conn.query_row(&sql_text, sql::param_refs(&params).as_slice(), |row| {
        row.get(0)
    })
    .map_err(|e| e.to_string())
}

/// Newest original entry still eligible for linear undo.
fn revertible_entry_id(conn: &Connection) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT id FROM change_log
         WHERE kind != 'revert' AND reverted_by IS NULL
         ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub(crate) fn fetch_change_history_inner(
    conn: &Connection,
    page: i64,
    page_size: i64,
    filter: &HistoryFilter,
) -> Result<ChangeHistoryPage, String> {
    let total = count_change_entries(conn, filter)?;
    let (page_size, total_pages, safe_page, _) = utils::paginate(total, page, page_size);
    let entries = get_change_entries(conn, page_size, (safe_page - 1) * page_size, filter)?;
    Ok(ChangeHistoryPage {
        entries,
        total,
        page_size,
        total_pages,
        // Undo eligibility is global, not filtered: the Revert action stays
        // correct even while the newest entry is hidden by a filter.
        revertible_id: revertible_entry_id(conn)?,
    })
}

// ── Revert (linear undo) ─────────────────────────────────────────────────────

pub(crate) fn revert_history_entry_inner(
    conn: &Connection,
    id: i64,
) -> Result<(i64, i64), AppError> {
    let (kind, label, reverted_by): (String, String, Option<i64>) = conn
        .query_row(
            "SELECT kind, label, reverted_by FROM change_log WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("Change not found — it may have been cleared".to_string()))?;
    if kind == "revert" {
        return Err(AppError::Validation("Undo entries cannot be reverted".to_string()));
    }
    if reverted_by.is_some() {
        return Err(AppError::Validation("This change was already reverted".to_string()));
    }
    let newer: i64 = conn.query_row(
        "SELECT COUNT(*) FROM change_log
         WHERE kind != 'revert' AND reverted_by IS NULL AND id > ?1",
        [id],
        |row| row.get(0),
    )?;
    if newer > 0 {
        return Err(AppError::Validation("Revert newer changes first".to_string()));
    }

    let tx_result = with_tx_err(conn, || {
        // Phase 1: drop rows the operation added (children before parents).
        for table in ["splits", "transactions", "accounts", "commodities"] {
            validate_data_table(table)?;
            let sql = format!(
                "DELETE FROM {table} WHERE guid IN (
                    SELECT row_guid FROM change_snapshots
                    WHERE entry_id = ?1 AND table_name = ?2 AND before_json IS NULL)"
            );
            conn.execute(&sql, params![id, table])?;
        }

        // Phase 2: restore rows the operation changed or removed
        // (parents before children so references resolve).
        for table in ["commodities", "accounts", "transactions", "splits"] {
            let mut stmt = conn.prepare(
                "SELECT before_json FROM change_snapshots
                 WHERE entry_id = ?1 AND table_name = ?2 AND before_json IS NOT NULL",
            )?;
            let befores: Vec<String> = stmt
                .query_map(params![id, table], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for before in befores {
                restore_snapshot_row(conn, table, &before)?;
            }
        }

        let revert_id = {
            let occurred_at = utils::iso_now();
            let revert_label = format!("Reverted: {label}");
            conn.execute(
                "INSERT INTO change_log
                 (occurred_at, kind, label, source, reverts_id)
                 VALUES (?1, 'revert', ?2, 'system', ?3)",
                params![occurred_at, revert_label, id],
            )?;
            conn.last_insert_rowid()
        };
        conn.execute(
            "UPDATE change_log SET reverted_by = ?1 WHERE id = ?2",
            params![revert_id, id],
        )?;

        Ok(revert_id)
    });
    match tx_result {
        Ok(revert_id) => Ok((id, revert_id)),
        Err(e) => Err(map_revert_error(e)),
    }
}

/// Untracked rows (written outside the history log) referencing rows this
/// revert removes surface as FK violations — translate to the same "blocked"
/// language as the ordering guard. Matched on the extended result code so
/// other constraint failures stay loud instead of masquerading as ordering.
fn map_revert_error(e: AppError) -> AppError {
    if matches!(&e, AppError::Db(db) if db.is_foreign_key()) {
        AppError::Validation("Cannot revert — newer data depends on this change".to_string())
    } else {
        e
    }
}

// ── Last-import pointer (feeds `reimport_last`) ──────────────────────────────

/// Newest import's `(file_path, filename)` from the uniform `change_log`.
pub(crate) fn last_import_source(conn: &Connection) -> Result<(String, String), String> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT json_extract(detail_json, '$.filePath'),
                    json_extract(detail_json, '$.filename')
             FROM change_log WHERE kind = 'import' ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match row {
        Some((Some(path), Some(filename))) if !path.is_empty() => Ok((path, filename)),
        _ => Err("No previous import to reimport".to_string()),
    }
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_change_history(
    state: State<'_, AppState>,
    query: ChangeHistoryQuery,
) -> Result<ChangeHistoryPage, String> {
    run_blocking(state.db.clone(), move |conn| {
        let filter = HistoryFilter::from_query(&query);
        fetch_change_history_inner(conn, query.page, query.page_size, &filter)
    })
    .await
}

#[tauri::command]
pub async fn revert_history_entry(
    state: State<'_, AppState>,
    id: i64,
) -> Result<RevertResult, String> {
    let (reverted_id, revert_id, summary) = run_blocking(state.db.clone(), move |conn| {
        let (reverted_id, revert_id) = revert_history_entry_inner(conn, id)?;
        let summary = get_summary(conn)?;
        Ok::<_, String>((reverted_id, revert_id, summary))
    })
    .await?;
    Ok(RevertResult {
        reverted_id,
        revert_id,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

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
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn
    }

    fn seed_tx(conn: &Connection, guid: &str, date: &str, desc: &str, value: i64) {
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description) VALUES (?1, ?2, ?3)",
            params![guid, date, desc],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom,
                                 quantity_num, quantity_denom)
             VALUES (?1, ?2, 'checking', ?3, 100, ?3, 100)",
            params![format!("s-{guid}"), guid, value],
        )
        .unwrap();
    }

    fn log_snapshots(conn: &Connection, pairs: &[(&str, &str)]) -> i64 {
        let snaps: Vec<Snapshot> = pairs
            .iter()
            .map(|(table, guid)| Snapshot {
                table_name: table,
                row_guid: (*guid).to_string(),
                before_json: snapshot_row(conn, table, guid).unwrap(),
            })
            .collect();
        insert_change_entry(conn, "test", "test entry", "manual", 0, 0, None, &snaps).unwrap()
    }

    #[test]
    fn snapshot_row_round_trips_missing_and_present() {
        let conn = setup();
        assert_eq!(snapshot_row(&conn, "transactions", "nope").unwrap(), None);
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);
        let json = snapshot_row(&conn, "transactions", "t1")
            .unwrap()
            .expect("row must snapshot");
        assert!(json.contains("Groceries"));
        assert!(snapshot_row(&conn, "nope", "t1").is_err());
    }

    #[test]
    fn revert_create_deletes_added_rows() {
        let conn = setup();
        // Capture "before" (absent), then add rows, then revert removes them.
        let id = log_snapshots(&conn, &[("transactions", "t1"), ("splits", "s-t1")]);
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);

        revert_history_entry_inner(&conn, id).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let page = fetch_change_history_inner(&conn, 1, 50, &HistoryFilter::default()).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.revertible_id, None);
    }

    #[test]
    fn revert_update_restores_before_image() {
        let conn = setup();
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);
        let id = log_snapshots(&conn, &[("transactions", "t1")]);
        conn.execute(
            "UPDATE transactions SET description = 'Edited' WHERE guid = 't1'",
            [],
        )
        .unwrap();

        revert_history_entry_inner(&conn, id).unwrap();

        let desc: String = conn
            .query_row(
                "SELECT description FROM transactions WHERE guid = 't1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(desc, "Groceries");
    }

    #[test]
    fn revert_delete_reinserts_removed_rows() {
        let conn = setup();
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);
        let id = log_snapshots(
            &conn,
            &[("transactions", "t1"), ("splits", "s-t1")],
        );
        conn.execute("DELETE FROM splits WHERE guid = 's-t1'", [])
            .unwrap();
        conn.execute("DELETE FROM transactions WHERE guid = 't1'", [])
            .unwrap();

        revert_history_entry_inner(&conn, id).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let splits: i64 = conn
            .query_row("SELECT COUNT(*) FROM splits", [], |r| r.get(0))
            .unwrap();
        assert_eq!(splits, 1);
    }

    #[test]
    fn linear_undo_guards_out_of_order_and_double_revert() {
        let conn = setup();
        // Model two creates: snapshot the absence first, then insert.
        let first = log_snapshots(&conn, &[("transactions", "t1"), ("splits", "s-t1")]);
        seed_tx(&conn, "t1", "2024-01-10", "First", 5_000);
        let second = log_snapshots(&conn, &[("transactions", "t2"), ("splits", "s-t2")]);
        seed_tx(&conn, "t2", "2024-02-05", "Second", 1_500);

        // Older entry is blocked while a newer one is pending.
        let err = revert_history_entry_inner(&conn, first).unwrap_err();
        assert!(err.to_string().contains("newer"), "unexpected: {err:?}");

        revert_history_entry_inner(&conn, second).unwrap();
        // Double revert is blocked.
        let err = revert_history_entry_inner(&conn, second).unwrap_err();
        assert!(err.to_string().contains("already"), "unexpected: {err:?}");
        // Only the second create was undone so far.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // Now the older entry is the head and reverts cleanly.
        revert_history_entry_inner(&conn, first).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn revert_entries_are_not_revertible() {
        let conn = setup();
        seed_tx(&conn, "t1", "2024-01-10", "First", 5_000);
        let first = log_snapshots(&conn, &[("transactions", "t1")]);
        let (_, revert_id) = revert_history_entry_inner(&conn, first).unwrap();
        assert!(revert_history_entry_inner(&conn, revert_id).is_err());
        assert!(fetch_change_history_inner(&conn, 1, 50, &HistoryFilter::default())
            .unwrap()
            .revertible_id
            .is_none());
    }

    #[test]
    fn history_paginates_newest_first() {
        let conn = setup();
        for i in 0..3 {
            insert_change_entry(
                &conn,
                "test",
                &format!("entry {i}"),
                "manual",
                0,
                0,
                None,
                &[],
            )
            .unwrap();
        }
        let page = fetch_change_history_inner(&conn, 1, 2, &HistoryFilter::default()).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.total_pages, 2);
        assert_eq!(page.entries.len(), 2);
        assert!(page.entries[0].id > page.entries[1].id);
        assert_eq!(page.revertible_id, Some(page.entries[0].id));
    }

    #[test]
    fn revert_account_update_preserves_budgets() {
        let conn = setup();
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, year, amount, created_at)
             VALUES ('b1', 'checking', 2024, 250.0, '2024-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // Log an account_update, mutate, then revert: the upsert restore must
        // not fire the budgets FK cascade (INSERT OR REPLACE would).
        let id = log_snapshots(&conn, &[("accounts", "checking")]);
        conn.execute(
            "UPDATE accounts SET name = 'Renamed' WHERE guid = 'checking'",
            [],
        )
        .unwrap();

        revert_history_entry_inner(&conn, id).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM accounts WHERE guid = 'checking'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "Checking");
        let budgets: i64 = conn
            .query_row("SELECT COUNT(*) FROM budgets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(budgets, 1);
    }

    #[test]
    fn history_filters_by_kind_source_and_search() {
        let conn = setup();
        insert_change_entry(&conn, "import", "Imported a.gnucash", "import", 3, 2, None, &[]).unwrap();
        insert_change_entry(&conn, "tx_create", "Groceries", "manual", 1, 1, None, &[]).unwrap();
        insert_change_entry(&conn, "tx_create", "Rent", "schedule", 1, 1, None, &[]).unwrap();
        insert_change_entry(&conn, "reconcile", "Reconciled Checking", "manual", 5, 1, None, &[])
            .unwrap();

        let imports = HistoryFilter {
            kind: Some("import"),
            ..Default::default()
        };
        let page = fetch_change_history_inner(&conn, 1, 50, &imports).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.entries[0].label, "Imported a.gnucash");

        let scheduled = HistoryFilter {
            source: Some("schedule"),
            ..Default::default()
        };
        let page = fetch_change_history_inner(&conn, 1, 50, &scheduled).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.entries[0].label, "Rent");

        let search = HistoryFilter {
            search: Some("recon"),
            ..Default::default()
        };
        let page = fetch_change_history_inner(&conn, 1, 50, &search).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.entries[0].kind, "reconcile");

        // Filters combine with AND and keep pagination totals consistent.
        let none = HistoryFilter {
            kind: Some("import"),
            source: Some("schedule"),
            ..Default::default()
        };
        let page = fetch_change_history_inner(&conn, 1, 50, &none).unwrap();
        assert_eq!((page.total, page.total_pages, page.entries.len()), (0, 1, 0));
        // Undo eligibility stays global (unfiltered).
        assert_eq!(page.revertible_id, Some(4));
    }

    #[test]
    fn history_search_escapes_like_metacharacters() {
        let conn = setup();
        insert_change_entry(&conn, "test", "100% rent", "manual", 0, 0, None, &[]).unwrap();
        insert_change_entry(&conn, "test", "100x rent", "manual", 0, 0, None, &[]).unwrap();

        let percent = HistoryFilter {
            search: Some("100%"),
            ..Default::default()
        };
        assert_eq!(
            fetch_change_history_inner(&conn, 1, 50, &percent).unwrap().total,
            1
        );
        // Unescaped `_` would match any single character; escaped it matches
        // nothing here.
        let underscore = HistoryFilter {
            search: Some("100_"),
            ..Default::default()
        };
        assert_eq!(
            fetch_change_history_inner(&conn, 1, 50, &underscore)
                .unwrap()
                .total,
            0
        );
    }
}
