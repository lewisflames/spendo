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
    /// GnuCash imports only: source path + filename for the "Re-run import"
    /// action on a history entry. `None` for CSV imports and manual edits.
    pub source_path: Option<String>,
    pub filename: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeHistory {
    pub entries: Vec<ChangeEntry>,
    /// Newest change eligible for undo: no later change is pending and it has
    /// no undo row. `None` when history is empty or nothing can be undone.
    pub undoable_id: Option<i64>,
    /// Most recently undone change still eligible for redo (LIFO). `None` once
    /// a newer change invalidates the redo stack or everything is redone.
    pub redoable_id: Option<i64>,
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
    insert_snapshot_rows(conn, entry_id, snapshots)?;
    Ok(entry_id)
}

/// Insert one snapshot row per entry. Must run inside the caller's transaction
/// alongside the log row the snapshots belong to.
fn insert_snapshot_rows(
    conn: &Connection,
    entry_id: i64,
    snapshots: &[Snapshot<'_>],
) -> Result<(), String> {
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
    Ok(())
}

// ── Fetch ────────────────────────────────────────────────────────────────────

fn list_change_entries(conn: &Connection) -> Result<Vec<ChangeEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, occurred_at, kind, label, source,
                    transactions_affected, accounts_affected, reverts_id, reverted_by,
                    CASE WHEN kind = 'import' AND source = 'import'
                         THEN json_extract(detail_json, '$.filePath') END,
                    CASE WHEN kind = 'import' AND source = 'import'
                         THEN json_extract(detail_json, '$.filename') END
             FROM change_log ORDER BY id DESC",
        )
        .map_err(|e| e.to_string())?;
    let mapped = stmt
        .query_map([], |row| {
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
                source_path: row.get(9)?,
                filename: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;
    mapped
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// Newest change eligible for undo (still active and the head of the log).
fn undoable_entry_id(conn: &Connection) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT id FROM change_log
         WHERE kind NOT IN ('revert', 'redo') AND reverted_by IS NULL
         ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Most recently undone change still eligible for redo. Picks the undone change
/// whose undo row has the highest id, provided no change was recorded after that
/// undo row (a new change invalidates the redo stack, like an editor).
fn redoable_entry_id(conn: &Connection) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT c.id FROM change_log c
         WHERE c.kind NOT IN ('revert', 'redo')
           AND c.reverted_by = (
               SELECT MAX(u.reverted_by) FROM change_log u
               WHERE u.reverted_by IS NOT NULL AND u.kind NOT IN ('revert', 'redo'))
           AND NOT EXISTS (
               SELECT 1 FROM change_log n
               WHERE n.id > c.reverted_by AND n.kind NOT IN ('revert', 'redo'))
           -- Undo rows recorded before post-state capture existed cannot be
           -- redone faithfully; ignore them (changes with no affected rows
           -- legitimately carry no snapshots).
           AND (NOT EXISTS (SELECT 1 FROM change_snapshots cs WHERE cs.entry_id = c.id)
                OR EXISTS (SELECT 1 FROM change_snapshots rs WHERE rs.entry_id = c.reverted_by))
         LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Full change log, newest first.
pub(crate) fn fetch_change_history_inner(conn: &Connection) -> Result<ChangeHistory, String> {
    Ok(ChangeHistory {
        entries: list_change_entries(conn)?,
        undoable_id: undoable_entry_id(conn)?,
        redoable_id: redoable_entry_id(conn)?,
    })
}

// ── Undo / redo (linear history toggle) ──────────────────────────────────────

/// Apply an entry's snapshots: delete rows it added (children first), then
/// restore rows it changed or removed (parents first so references resolve).
fn restore_entry_snapshots(conn: &Connection, entry_id: i64) -> Result<(), String> {
    for table in ["splits", "transactions", "accounts", "commodities"] {
        validate_data_table(table)?;
        let sql = format!(
            "DELETE FROM {table} WHERE guid IN (
                SELECT row_guid FROM change_snapshots
                WHERE entry_id = ?1 AND table_name = ?2 AND before_json IS NULL)"
        );
        conn.execute(&sql, params![entry_id, table])
            .map_err(|e| e.to_string())?;
    }
    for table in ["commodities", "accounts", "transactions", "splits"] {
        let mut stmt = conn
            .prepare(
                "SELECT before_json FROM change_snapshots
                 WHERE entry_id = ?1 AND table_name = ?2 AND before_json IS NOT NULL",
            )
            .map_err(|e| e.to_string())?;
        let befores: Vec<String> = stmt
            .query_map(params![entry_id, table], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for before in befores {
            restore_snapshot_row(conn, table, &before)?;
        }
    }
    Ok(())
}

/// Record the current state of every row `source_entry_id` touched on
/// `dest_entry_id`. Undo calls this before restoring, so its own snapshots hold
/// the post-change state that a later redo puts back.
fn carry_entry_state(
    conn: &Connection,
    source_entry_id: i64,
    dest_entry_id: i64,
) -> Result<(), String> {
    let rows: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT table_name, row_guid FROM change_snapshots WHERE entry_id = ?1")
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([source_entry_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    for (table, guid) in rows {
        let current = snapshot_row(conn, &table, &guid)?;
        conn.execute(
            "INSERT INTO change_snapshots (entry_id, table_name, row_guid, before_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![dest_entry_id, table, guid, current],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Undo the newest change: restore its before-images and record an undo row
/// (kind `revert`) carrying the post-change state for a later redo.
pub(crate) fn undo_history_entry_inner(conn: &Connection, id: i64) -> Result<(), AppError> {
    let (kind, label, reverted_by): (String, String, Option<i64>) = conn
        .query_row(
            "SELECT kind, label, reverted_by FROM change_log WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::NotFound("Change not found — it may have been cleared".to_string()))?;
    if kind == "revert" || kind == "redo" {
        return Err(AppError::Validation(
            "Only recorded changes can be undone".to_string(),
        ));
    }
    if reverted_by.is_some() {
        return Err(AppError::Validation("This change was already undone".to_string()));
    }
    let newer: i64 = conn.query_row(
        "SELECT COUNT(*) FROM change_log
         WHERE kind NOT IN ('revert', 'redo') AND reverted_by IS NULL AND id > ?1",
        [id],
        |row| row.get(0),
    )?;
    if newer > 0 {
        return Err(AppError::Validation("Undo newer changes first".to_string()));
    }

    with_tx_err(conn, || {
        let undo_id = {
            let occurred_at = utils::iso_now();
            let undo_label = format!("Undid: {label}");
            conn.execute(
                "INSERT INTO change_log
                 (occurred_at, kind, label, source, reverts_id)
                 VALUES (?1, 'revert', ?2, 'system', ?3)",
                params![occurred_at, undo_label, id],
            )?;
            conn.last_insert_rowid()
        };
        carry_entry_state(conn, id, undo_id)?;
        restore_entry_snapshots(conn, id)?;
        conn.execute(
            "UPDATE change_log SET reverted_by = ?1 WHERE id = ?2",
            params![undo_id, id],
        )?;
        Ok(())
    })
    .map_err(map_history_error)
}

/// Redo the most recently undone change: restore the post-change state captured
/// on its undo row and clear the undone mark.
pub(crate) fn redo_history_entry_inner(conn: &Connection, id: i64) -> Result<(), AppError> {
    if redoable_entry_id(conn)? != Some(id) {
        return Err(AppError::Validation(
            "This change can no longer be redone".to_string(),
        ));
    }
    let (undo_id, label): (i64, String) = conn.query_row(
        "SELECT reverted_by, label FROM change_log WHERE id = ?1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    with_tx_err(conn, || {
        restore_entry_snapshots(conn, undo_id)?;
        let occurred_at = utils::iso_now();
        let redo_label = format!("Redid: {label}");
        conn.execute(
            "INSERT INTO change_log
             (occurred_at, kind, label, source, reverts_id)
             VALUES (?1, 'redo', ?2, 'system', ?3)",
            params![occurred_at, redo_label, undo_id],
        )?;
        conn.execute("UPDATE change_log SET reverted_by = NULL WHERE id = ?1", [id])?;
        Ok(())
    })
    .map_err(map_history_error)
}

/// Untracked rows (written outside the history log) referencing rows an undo or
/// redo removes surface as FK violations — translate to plain language instead
/// of leaking a constraint error.
fn map_history_error(e: AppError) -> AppError {
    if matches!(&e, AppError::Db(db) if db.is_foreign_key()) {
        AppError::Validation("Newer data depends on this change".to_string())
    } else {
        e
    }
}

// ── Last-import pointer (feeds the re-import flow) ───────────────────────────

/// Newest GnuCash import's `(file_path, filename)` from the uniform
/// `change_log`. CSV commits also use `kind = 'import'` but their source is
/// `"csv"` — they must not feed the GnuCash re-run. `None` when this book has
/// never had a GnuCash import.
pub(crate) fn last_import_source(conn: &Connection) -> Result<Option<(String, String)>, String> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT json_extract(detail_json, '$.filePath'),
                    json_extract(detail_json, '$.filename')
             FROM change_log WHERE kind = 'import' AND source = 'import' ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match row {
        Some((Some(path), Some(filename))) if !path.is_empty() => Ok(Some((path, filename))),
        _ => Ok(None),
    }
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_change_history(state: State<'_, AppState>) -> Result<ChangeHistory, String> {
    run_blocking(state.db.clone(), move |conn| fetch_change_history_inner(conn)).await
}

#[tauri::command]
pub async fn undo_history_entry(
    state: State<'_, AppState>,
    id: i64,
) -> Result<DataSummary, String> {
    run_blocking(state.db.clone(), move |conn| {
        undo_history_entry_inner(conn, id)?;
        get_summary(conn)
    })
    .await
}

#[tauri::command]
pub async fn redo_history_entry(
    state: State<'_, AppState>,
    id: i64,
) -> Result<DataSummary, String> {
    run_blocking(state.db.clone(), move |conn| {
        redo_history_entry_inner(conn, id)?;
        get_summary(conn)
    })
    .await
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

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
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
    fn undo_create_deletes_added_rows_and_redo_reinserts() {
        let conn = setup();
        // Capture "before" (absent), then add rows; undo removes them.
        let id = log_snapshots(&conn, &[("transactions", "t1"), ("splits", "s-t1")]);
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);

        undo_history_entry_inner(&conn, id).unwrap();

        assert_eq!(count(&conn, "transactions"), 0);
        let history = fetch_change_history_inner(&conn).unwrap();
        assert_eq!(history.entries.len(), 2);
        assert_eq!(history.undoable_id, None);
        assert_eq!(history.redoable_id, Some(id));

        // Redo puts the created rows back and makes the change undoable again.
        redo_history_entry_inner(&conn, id).unwrap();
        assert_eq!(count(&conn, "transactions"), 1);
        assert_eq!(count(&conn, "splits"), 1);
        let history = fetch_change_history_inner(&conn).unwrap();
        assert_eq!(history.undoable_id, Some(id));
        assert_eq!(history.redoable_id, None);
    }

    #[test]
    fn undo_restores_before_image_and_redo_restores_after() {
        let conn = setup();
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);
        let id = log_snapshots(&conn, &[("transactions", "t1")]);
        conn.execute(
            "UPDATE transactions SET description = 'Edited' WHERE guid = 't1'",
            [],
        )
        .unwrap();
        let description = |conn: &Connection| -> String {
            conn.query_row(
                "SELECT description FROM transactions WHERE guid = 't1'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };

        undo_history_entry_inner(&conn, id).unwrap();
        assert_eq!(description(&conn), "Groceries");

        redo_history_entry_inner(&conn, id).unwrap();
        assert_eq!(description(&conn), "Edited");
    }

    #[test]
    fn undo_delete_reinserts_removed_rows_and_redo_removes_them_again() {
        let conn = setup();
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);
        let id = log_snapshots(&conn, &[("transactions", "t1"), ("splits", "s-t1")]);
        conn.execute("DELETE FROM splits WHERE guid = 's-t1'", [])
            .unwrap();
        conn.execute("DELETE FROM transactions WHERE guid = 't1'", [])
            .unwrap();

        undo_history_entry_inner(&conn, id).unwrap();
        assert_eq!(count(&conn, "transactions"), 1);
        assert_eq!(count(&conn, "splits"), 1);

        redo_history_entry_inner(&conn, id).unwrap();
        assert_eq!(count(&conn, "transactions"), 0);
        assert_eq!(count(&conn, "splits"), 0);
    }

    #[test]
    fn linear_undo_guards_out_of_order_and_double_undo() {
        let conn = setup();
        // Model two creates: snapshot the absence first, then insert.
        let first = log_snapshots(&conn, &[("transactions", "t1"), ("splits", "s-t1")]);
        seed_tx(&conn, "t1", "2024-01-10", "First", 5_000);
        let second = log_snapshots(&conn, &[("transactions", "t2"), ("splits", "s-t2")]);
        seed_tx(&conn, "t2", "2024-02-05", "Second", 1_500);

        // Older entry is blocked while a newer one is pending.
        let err = undo_history_entry_inner(&conn, first).unwrap_err();
        assert!(err.to_string().contains("newer"), "unexpected: {err:?}");

        undo_history_entry_inner(&conn, second).unwrap();
        // Double undo is blocked.
        let err = undo_history_entry_inner(&conn, second).unwrap_err();
        assert!(err.to_string().contains("already"), "unexpected: {err:?}");
        // Only the second create was undone so far.
        assert_eq!(count(&conn, "transactions"), 1);

        // Now the older entry is the head and undoes cleanly.
        undo_history_entry_inner(&conn, first).unwrap();
        assert_eq!(count(&conn, "transactions"), 0);
    }

    #[test]
    fn redo_stack_is_lifo_and_invalidated_by_new_changes() {
        let conn = setup();
        let first = log_snapshots(&conn, &[("transactions", "t1"), ("splits", "s-t1")]);
        seed_tx(&conn, "t1", "2024-01-10", "First", 5_000);
        let second = log_snapshots(&conn, &[("transactions", "t2"), ("splits", "s-t2")]);
        seed_tx(&conn, "t2", "2024-02-05", "Second", 1_500);

        undo_history_entry_inner(&conn, second).unwrap();
        undo_history_entry_inner(&conn, first).unwrap();
        assert_eq!(count(&conn, "transactions"), 0);
        // LIFO: the most recently undone change is redone first.
        assert_eq!(
            fetch_change_history_inner(&conn).unwrap().redoable_id,
            Some(first)
        );

        redo_history_entry_inner(&conn, first).unwrap();
        assert_eq!(count(&conn, "transactions"), 1);
        assert_eq!(
            fetch_change_history_inner(&conn).unwrap().redoable_id,
            Some(second)
        );
        redo_history_entry_inner(&conn, second).unwrap();
        assert_eq!(count(&conn, "transactions"), 2);
        assert_eq!(fetch_change_history_inner(&conn).unwrap().redoable_id, None);

        // A new change after an undo invalidates the redo stack.
        undo_history_entry_inner(&conn, second).unwrap();
        let third = log_snapshots(&conn, &[("transactions", "t3"), ("splits", "s-t3")]);
        seed_tx(&conn, "t3", "2024-03-01", "Third", 2_500);
        let history = fetch_change_history_inner(&conn).unwrap();
        assert_eq!(history.redoable_id, None);
        assert_eq!(history.undoable_id, Some(third));
    }

    #[test]
    fn history_toggle_entries_are_not_toggleable() {
        let conn = setup();
        seed_tx(&conn, "t1", "2024-01-10", "First", 5_000);
        let first = log_snapshots(&conn, &[("transactions", "t1")]);
        undo_history_entry_inner(&conn, first).unwrap();

        // The undo row itself cannot be undone or redone; only the change can.
        let history = fetch_change_history_inner(&conn).unwrap();
        let undo_row = &history.entries[0];
        assert_eq!(undo_row.kind, "revert");
        assert!(undo_history_entry_inner(&conn, undo_row.id).is_err());
        assert!(redo_history_entry_inner(&conn, undo_row.id).is_err());
        assert_eq!(history.undoable_id, None);
        assert_eq!(history.redoable_id, Some(first));
    }

    #[test]
    fn redo_disabled_for_undo_rows_without_captured_state() {
        let conn = setup();
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);
        let id = log_snapshots(&conn, &[("transactions", "t1")]);
        undo_history_entry_inner(&conn, id).unwrap();
        // Simulate an undo row recorded before post-state capture existed.
        conn.execute("DELETE FROM change_snapshots WHERE entry_id != ?1", [id])
            .unwrap();

        assert_eq!(fetch_change_history_inner(&conn).unwrap().redoable_id, None);
        assert!(redo_history_entry_inner(&conn, id).is_err());
    }

    #[test]
    fn history_lists_newest_first() {
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
        let history = fetch_change_history_inner(&conn).unwrap();
        assert_eq!(history.entries.len(), 3);
        assert!(history.entries[0].id > history.entries[1].id);
        assert!(history.entries[1].id > history.entries[2].id);
        assert_eq!(history.undoable_id, Some(history.entries[0].id));
    }

    #[test]
    fn undo_account_update_preserves_budgets() {
        let conn = setup();
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, year, amount, created_at)
             VALUES ('b1', 'checking', 2024, 250.0, '2024-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // Log an account_update, mutate, then undo: the upsert restore must
        // not fire the budgets FK cascade (INSERT OR REPLACE would).
        let id = log_snapshots(&conn, &[("accounts", "checking")]);
        conn.execute(
            "UPDATE accounts SET name = 'Renamed' WHERE guid = 'checking'",
            [],
        )
        .unwrap();

        undo_history_entry_inner(&conn, id).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM accounts WHERE guid = 'checking'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "Checking");
        assert_eq!(count(&conn, "budgets"), 1);
    }

    /// Regression: CSV commits share `kind = 'import'`; the re-run button must
    /// keep pointing at the newest GnuCash import.
    #[test]
    fn last_import_source_skips_csv_imports() {
        let conn = setup();
        insert_change_entry(
            &conn,
            "import",
            "Imported book.gnucash",
            "import",
            0,
            0,
            Some(r#"{"filePath":"/tmp/a.gnucash","filename":"book.gnucash"}"#),
            &[],
        )
        .unwrap();
        insert_change_entry(
            &conn,
            "import",
            "CSV import statement.csv (2 transactions)",
            "csv",
            0,
            0,
            Some(r#"{"filePath":"/tmp/statement.csv","filename":"statement.csv"}"#),
            &[],
        )
        .unwrap();

        assert_eq!(
            last_import_source(&conn).unwrap(),
            Some(("/tmp/a.gnucash".to_string(), "book.gnucash".to_string()))
        );
    }
}
