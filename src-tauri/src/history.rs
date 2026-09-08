use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::db::{run_blocking, validate_data_table, with_tx_err, AppError, AppState};
use crate::home::{get_summary, DataSummary};
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

/// Re-insert (or overwrite) a row from its before-image. Callers order phases:
/// parents before children so FK references resolve.
fn restore_snapshot_row(conn: &Connection, table: &str, before: &str) -> Result<(), String> {
    validate_data_table(table)?;
    let v: serde_json::Value =
        serde_json::from_str(before).map_err(|e| format!("Corrupt history snapshot: {e}"))?;
    match table {
        "commodities" => {
            conn.execute(
                "INSERT OR REPLACE INTO commodities (guid, mnemonic, namespace, fullname)
                 VALUES (?1, ?2, ?3, ?4)",
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
                "INSERT OR REPLACE INTO accounts
                 (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
                "INSERT OR REPLACE INTO transactions (guid, post_date, description, num, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
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
                "INSERT OR REPLACE INTO splits
                 (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom,
                  reconcile_state, reconciled, memo)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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

fn get_change_entries(
    conn: &Connection,
    limit: i64,
    offset: i64,
) -> Result<Vec<ChangeEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, occurred_at, kind, label, source,
                    transactions_affected, accounts_affected, reverts_id, reverted_by
             FROM change_log ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| e.to_string())?;
    let mapped = stmt
        .query_map(params![limit, offset], |row| {
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

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

fn count_change_entries(conn: &Connection) -> Result<i64, String> {
    if !table_exists(conn, "change_log") {
        return Ok(0);
    }
    conn.query_row("SELECT COUNT(*) FROM change_log", [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

/// Newest original entry still eligible for linear undo.
fn revertible_entry_id(conn: &Connection) -> Result<Option<i64>, String> {
    if !table_exists(conn, "change_log") {
        return Ok(None);
    }
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
) -> Result<ChangeHistoryPage, String> {
    let total = count_change_entries(conn)?;
    let (page_size, total_pages, safe_page, _) = utils::paginate(total, page, page_size);
    let entries = if table_exists(conn, "change_log") {
        get_change_entries(conn, page_size, (safe_page - 1) * page_size)?
    } else {
        vec![]
    };
    Ok(ChangeHistoryPage {
        entries,
        total,
        page_size,
        total_pages,
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

/// Newest import's `(file_path, filename)`. Reads the uniform `change_log`
/// first; falls back to the legacy `import_log` table so books imported
/// before the history rewrite can still reimport.
pub(crate) fn last_import_source(conn: &Connection) -> Result<(String, String), String> {
    if table_exists(conn, "change_log") {
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
        if let Some((Some(path), Some(filename))) = row {
            if !path.is_empty() {
                return Ok((path, filename));
            }
        }
    }
    if table_exists(conn, "import_log") {
        return conn
            .query_row(
                "SELECT file_path, filename FROM import_log
                 WHERE file_path IS NOT NULL ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(|_| "No previous import to reimport".to_string());
    }
    Err("No previous import to reimport".to_string())
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_change_history(
    state: State<'_, AppState>,
    page: i64,
    page_size: i64,
) -> Result<ChangeHistoryPage, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_change_history_inner(conn, page, page_size)
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
        let page = fetch_change_history_inner(&conn, 1, 50).unwrap();
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
        assert!(fetch_change_history_inner(&conn, 1, 50)
            .unwrap()
            .revertible_id
            .is_none());
    }

    #[test]
    fn revert_maps_only_fk_violations_to_blocked() {
        use crate::db::DbError;
        // Synthetic FK failure (unreachable end-to-end with the current
        // schema — the only FK onto data tables cascades — so cover the
        // mapping directly).
        let fk = AppError::Db(DbError {
            message: "FOREIGN KEY constraint failed".to_string(),
            extended_code: Some(rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY),
        });
        let mapped = map_revert_error(fk);
        assert!(matches!(mapped, AppError::Validation(ref m) if m.contains("depends")));
        assert_eq!(
            String::from(mapped),
            "Cannot revert — newer data depends on this change"
        );
        // Other Db failures (e.g. UNIQUE) and guards pass through untouched.
        let unique = AppError::Db(DbError {
            message: "UNIQUE constraint failed".to_string(),
            extended_code: Some(rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE),
        });
        assert!(matches!(map_revert_error(unique), AppError::Db(_)));
        let guard = AppError::Validation("Revert newer changes first".to_string());
        assert!(matches!(map_revert_error(guard), AppError::Validation(_)));
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
        let page = fetch_change_history_inner(&conn, 1, 2).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.total_pages, 2);
        assert_eq!(page.entries.len(), 2);
        assert!(page.entries[0].id > page.entries[1].id);
        assert_eq!(page.revertible_id, Some(page.entries[0].id));
    }
}
