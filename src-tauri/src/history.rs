use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
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
    /// True when older entries exist beyond this page; pass the last entry's
    /// id as `beforeId` to fetch them.
    pub has_more: bool,
    /// Total entries in the log (all pages).
    pub total: i64,
}

/// One table's share of the rows a change touched. `changed` = the row still
/// exists (restore overwrites it), `removed` = it is gone (restore re-inserts).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntryTable {
    pub table_name: String,
    pub added: i64,
    pub changed: i64,
    pub removed: i64,
}

/// Detail for one change: parsed `detail_json` plus the per-table impact
/// breakdown. Never includes raw `before_json` (payload size).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntryDetail {
    pub detail: Option<serde_json::Value>,
    pub tables: Vec<ChangeEntryTable>,
}

/// Default page size for the history panel; the UI appends older pages on
/// demand so a long-lived book never ships its whole log over IPC.
const DEFAULT_HISTORY_PAGE: i64 = 100;

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
        "recurring_schedules" => {
            "SELECT guid, name, frequency, interval_mult, start_date, end_date,
                    max_occurrences, occurrences_posted, next_due_date, active,
                    template_json, created_at
             FROM recurring_schedules WHERE guid = ?1"
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
        "recurring_schedules" => stmt
            .query_row([guid], |row| {
                Ok(serde_json::json!({
                    "guid": row.get::<_, String>(0)?,
                    "name": row.get::<_, String>(1)?,
                    "frequency": row.get::<_, String>(2)?,
                    "interval_mult": row.get::<_, i64>(3)?,
                    "start_date": row.get::<_, String>(4)?,
                    "end_date": row.get::<_, Option<String>>(5)?,
                    "max_occurrences": row.get::<_, Option<i64>>(6)?,
                    "occurrences_posted": row.get::<_, i64>(7)?,
                    "next_due_date": row.get::<_, Option<String>>(8)?,
                    "active": row.get::<_, i64>(9)?,
                    "template_json": row.get::<_, String>(10)?,
                    "created_at": row.get::<_, String>(11)?,
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
        "recurring_schedules" => {
            conn.execute(
                "INSERT INTO recurring_schedules
                 (guid, name, frequency, interval_mult, start_date, end_date,
                  max_occurrences, occurrences_posted, next_due_date, active,
                  template_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(guid) DO UPDATE SET
                     name = excluded.name,
                     frequency = excluded.frequency,
                     interval_mult = excluded.interval_mult,
                     start_date = excluded.start_date,
                     end_date = excluded.end_date,
                     max_occurrences = excluded.max_occurrences,
                     occurrences_posted = excluded.occurrences_posted,
                     next_due_date = excluded.next_due_date,
                     active = excluded.active,
                     template_json = excluded.template_json,
                     created_at = excluded.created_at",
                params![
                    str_field(&v, "guid")?,
                    str_field(&v, "name")?,
                    str_field(&v, "frequency")?,
                    int_field(&v, "interval_mult")?,
                    str_field(&v, "start_date")?,
                    opt_str_field(&v, "end_date"),
                    v.get("max_occurrences").and_then(|x| x.as_i64()),
                    int_field(&v, "occurrences_posted")?,
                    opt_str_field(&v, "next_due_date"),
                    int_field(&v, "active")?,
                    str_field(&v, "template_json")?,
                    str_field(&v, "created_at")?,
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

fn list_change_entries(
    conn: &Connection,
    before_id: Option<i64>,
    page_size: i64,
) -> Result<(Vec<ChangeEntry>, bool), String> {
    let page_size = utils::clamp_page_size(page_size);
    let mut stmt = conn
        .prepare(
            "SELECT id, occurred_at, kind, label, source,
                    transactions_affected, accounts_affected, reverts_id, reverted_by,
                    CASE WHEN kind = 'import' AND source = 'import'
                         THEN json_extract(detail_json, '$.filePath') END,
                    CASE WHEN kind = 'import' AND source = 'import'
                         THEN json_extract(detail_json, '$.filename') END
             FROM change_log
             WHERE (?1 IS NULL OR id < ?1)
             ORDER BY id DESC LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let mapped = stmt
        .query_map(params![before_id, page_size + 1], |row| {
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
    let mut entries = mapped
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    // One extra row proves older entries exist; it is not part of the page.
    let has_more = entries.len() as i64 > page_size;
    if has_more {
        entries.truncate(page_size as usize);
    }
    Ok((entries, has_more))
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

/// One page of the change log, newest first, plus the global undo/redo heads.
pub(crate) fn fetch_change_history_page_inner(
    conn: &Connection,
    before_id: Option<i64>,
    page_size: Option<i64>,
) -> Result<ChangeHistory, String> {
    let page_size = utils::clamp_page_size(page_size.unwrap_or(DEFAULT_HISTORY_PAGE));
    let (entries, has_more) = list_change_entries(conn, before_id, page_size)?;
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM change_log", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    Ok(ChangeHistory {
        entries,
        undoable_id: undoable_entry_id(conn)?,
        redoable_id: redoable_entry_id(conn)?,
        has_more,
        total,
    })
}

/// First page of the change log, newest first. Unpaginated convenience used
/// by tests (import, csv, schedules and history) to read the log.
#[cfg(test)]
pub(crate) fn fetch_change_history_inner(conn: &Connection) -> Result<ChangeHistory, String> {
    fetch_change_history_page_inner(conn, None, None)
}

/// Parsed `detail_json` plus the per-table impact of one change.
fn fetch_change_entry_detail_inner(
    conn: &Connection,
    id: i64,
) -> Result<ChangeEntryDetail, AppError> {
    let detail_json: Option<String> = conn
        .query_row(
            "SELECT detail_json FROM change_log WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| {
            AppError::NotFound("Change not found — it may have been cleared".to_string())
        })?;
    let detail = match detail_json {
        Some(text) => Some(
            serde_json::from_str(&text)
                .map_err(|e| AppError::from(format!("Corrupt history detail: {e}")))?,
        ),
        None => None,
    };

    let mut tables = Vec::new();
    for table in crate::db::DATA_TABLES {
        validate_data_table(table)?;
        let added: i64 = conn.query_row(
            "SELECT COUNT(*) FROM change_snapshots
             WHERE entry_id = ?1 AND table_name = ?2 AND before_json IS NULL",
            params![id, table],
            |row| row.get(0),
        )?;
        let befores: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT row_guid FROM change_snapshots
                 WHERE entry_id = ?1 AND table_name = ?2 AND before_json IS NOT NULL",
            )?;
            let mapped = stmt.query_map(params![id, table], |row| row.get(0))?;
            mapped.collect::<Result<Vec<String>, _>>()?
        };
        if added == 0 && befores.is_empty() {
            continue;
        }
        // Rows with a before-image still in the live table were changed;
        // missing ones were removed by the change.
        let changed = count_referencing(conn, table, "guid", &befores)?;
        tables.push(ChangeEntryTable {
            table_name: table.to_string(),
            added,
            changed,
            removed: befores.len() as i64 - changed,
        });
    }
    Ok(ChangeEntryDetail { detail, tables })
}

/// Drop the whole log and its snapshots, then compact the book file. The book
/// data is untouched; undo/redo stop working until new changes are recorded.
pub(crate) fn clear_change_history_inner(conn: &Connection) -> Result<(), AppError> {
    with_tx_err(conn, || -> Result<(), AppError> {
        conn.execute("DELETE FROM change_snapshots", [])?;
        conn.execute("DELETE FROM change_log", [])?;
        Ok(())
    })?;
    // VACUUM cannot run inside a transaction; reclaim the freed pages so the
    // `.spendo` file actually shrinks.
    crate::db::checkpoint(conn);
    conn.execute_batch("VACUUM;").map_err(AppError::from)
}

// ── Undo / redo (linear history toggle) ──────────────────────────────────────

/// GUIDs an entry would delete: rows recorded as absent before the change
/// (`before_json IS NULL`) and therefore removed when it is restored.
fn deleted_row_guids(
    conn: &Connection,
    entry_id: i64,
    table: &str,
) -> Result<Vec<String>, AppError> {
    validate_data_table(table)?;
    let mut stmt = conn.prepare(
        "SELECT row_guid FROM change_snapshots
         WHERE entry_id = ?1 AND table_name = ?2 AND before_json IS NULL",
    )?;
    let guids = stmt
        .query_map(params![entry_id, table], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(guids)
}

/// Count rows in `table`.`column` referencing any of `guids`, chunked to stay
/// under SQLite's bound-parameter cap. `table`/`column` come from call-site
/// literals only — never user input.
fn count_referencing(
    conn: &Connection,
    table: &str,
    column: &str,
    guids: &[String],
) -> Result<i64, AppError> {
    let mut total = 0;
    for chunk in guids.chunks(sql::MAX_IN_PARAMS) {
        let text = format!(
            "SELECT COUNT(*) FROM {table} WHERE {column} IN ({})",
            sql::in_clause(chunk.len())
        );
        let mut stmt = conn.prepare(&text)?;
        total += stmt.query_row(sql::string_param_refs(chunk).as_slice(), |row| {
            row.get::<_, i64>(0)
        })?;
    }
    Ok(total)
}

/// Schedules store their split accounts inside `template_json` (no FK), so a
/// deleted account would leave a schedule that fails at posting time. Rows the
/// same restore deletes are not survivors.
fn count_schedules_referencing(
    conn: &Connection,
    account_guids: &[String],
    deleted_schedule_guids: &[String],
) -> Result<i64, AppError> {
    let mut stmt = conn
        .prepare("SELECT guid FROM recurring_schedules WHERE instr(template_json, ?1) > 0")?;
    let mut surviving: Vec<String> = Vec::new();
    for guid in account_guids {
        let rows = stmt.query_map([guid], |row| row.get::<_, String>(0))?;
        for row in rows {
            let schedule_guid = row?;
            if !deleted_schedule_guids.contains(&schedule_guid)
                && !surviving.contains(&schedule_guid)
            {
                surviving.push(schedule_guid);
            }
        }
    }
    Ok(surviving.len() as i64)
}

/// Undo/redo deletes rows a change added. FK cascades would silently destroy
/// data the log does not own (receipts, budgets, import rules), so refuse
/// instead of deleting something that cannot be restored.
fn guard_restore_deletes(conn: &Connection, entry_id: i64) -> Result<(), AppError> {
    let tx_guids = deleted_row_guids(conn, entry_id, "transactions")?;
    if !tx_guids.is_empty() {
        let receipts = count_referencing(conn, "attachments", "tx_guid", &tx_guids)?;
        if receipts > 0 {
            return Err(AppError::Validation(if receipts == 1 {
                "1 receipt is attached to a transaction this change added — remove it first"
                    .to_string()
            } else {
                format!(
                    "{receipts} receipts are attached to transactions this change added — remove them first"
                )
            }));
        }
    }
    let account_guids = deleted_row_guids(conn, entry_id, "accounts")?;
    if !account_guids.is_empty() {
        let budgets = count_referencing(conn, "budgets", "account_guid", &account_guids)?;
        let rules = count_referencing(conn, "import_rules", "account_guid", &account_guids)?;
        let schedule_guids = deleted_row_guids(conn, entry_id, "recurring_schedules")?;
        let schedules =
            count_schedules_referencing(conn, &account_guids, &schedule_guids)?;
        if budgets + rules + schedules > 0 {
            return Err(AppError::Validation(
                "Budgets, import rules or schedules depend on accounts this change added — remove them first"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

/// Apply an entry's snapshots: delete rows it added (children first), then
/// restore rows it changed or removed (parents first so references resolve).
fn restore_entry_snapshots(conn: &Connection, entry_id: i64) -> Result<(), AppError> {
    guard_restore_deletes(conn, entry_id)?;
    for table in ["splits", "transactions", "accounts", "commodities", "recurring_schedules"] {
        validate_data_table(table)?;
        let sql = format!(
            "DELETE FROM {table} WHERE guid IN (
                SELECT row_guid FROM change_snapshots
                WHERE entry_id = ?1 AND table_name = ?2 AND before_json IS NULL)"
        );
        conn.execute(&sql, params![entry_id, table])
            .map_err(|e| e.to_string())?;
    }
    for table in ["commodities", "accounts", "transactions", "splits", "recurring_schedules"] {
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

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_change_history(
    state: State<'_, AppState>,
    before_id: Option<i64>,
    page_size: Option<i64>,
) -> Result<ChangeHistory, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_change_history_page_inner(conn, before_id, page_size)
    })
    .await
}

#[tauri::command]
pub async fn fetch_change_entry(
    state: State<'_, AppState>,
    id: i64,
) -> Result<ChangeEntryDetail, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_change_entry_detail_inner(conn, id).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn clear_change_history(state: State<'_, AppState>) -> Result<DataSummary, String> {
    run_blocking(state.db.clone(), move |conn| {
        clear_change_history_inner(conn)?;
        get_summary(conn)
    })
    .await
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

    /// Attachments live outside the snapshots and cascade on transaction
    /// delete: undoing the create would destroy the receipt bytes forever.
    #[test]
    fn undo_blocks_when_attachment_depends_on_added_tx() {
        let conn = setup();
        let id = log_snapshots(&conn, &[("transactions", "t1"), ("splits", "s-t1")]);
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);
        conn.execute(
            "INSERT INTO attachments (guid, tx_guid, filename, mime_type, byte_size, data, created_at)
             VALUES ('a1', 't1', 'receipt.png', 'image/png', 1, X'00', '2024-01-10T00:00:00Z')",
            [],
        )
        .unwrap();

        let err = undo_history_entry_inner(&conn, id).unwrap_err();
        assert!(err.to_string().contains("receipt"), "unexpected: {err:?}");
        // Nothing was deleted: the transaction and its receipt survive.
        assert_eq!(count(&conn, "transactions"), 1);
        assert_eq!(count(&conn, "attachments"), 1);
        assert_eq!(count(&conn, "splits"), 1);
    }

    /// Same guard on the redo path: an attachment added after the undo would be
    /// cascaded away when redo deletes the restored transaction.
    #[test]
    fn redo_blocks_when_attachment_was_added_after_undo() {
        let conn = setup();
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);
        let id = log_snapshots(&conn, &[("transactions", "t1"), ("splits", "s-t1")]);
        conn.execute("DELETE FROM splits WHERE guid = 's-t1'", [])
            .unwrap();
        conn.execute("DELETE FROM transactions WHERE guid = 't1'", [])
            .unwrap();

        undo_history_entry_inner(&conn, id).unwrap();
        conn.execute(
            "INSERT INTO attachments (guid, tx_guid, filename, mime_type, byte_size, data, created_at)
             VALUES ('a1', 't1', 'receipt.png', 'image/png', 1, X'00', '2024-01-10T00:00:00Z')",
            [],
        )
        .unwrap();

        let err = redo_history_entry_inner(&conn, id).unwrap_err();
        assert!(err.to_string().contains("receipt"), "unexpected: {err:?}");
        assert_eq!(count(&conn, "transactions"), 1);
        assert_eq!(count(&conn, "attachments"), 1);
    }

    /// Budgets, import rules and schedule templates are outside the snapshots,
    /// so deleting an added account would drop them (cascade or dangling refs).
    #[test]
    fn undo_blocks_when_unlogged_config_depends_on_added_account() {
        let conn = setup();
        let id = log_snapshots(&conn, &[("accounts", "new-acct")]);
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('new-acct', 'New', 'BANK', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();

        // No dependents yet: undo goes through.
        undo_history_entry_inner(&conn, id).unwrap();
        assert_eq!(count(&conn, "accounts"), 2);
        redo_history_entry_inner(&conn, id).unwrap();

        for (setup_sql, expected) in [
            (
                "INSERT INTO budgets (guid, account_guid, year, amount, created_at)
                 VALUES ('b1', 'new-acct', 2024, 250.0, '2024-01-01T00:00:00Z')",
                "budget",
            ),
            (
                "INSERT INTO import_rules (priority, match_type, pattern, account_guid, created_at)
                 VALUES (1, 'contains', 'shop', 'new-acct', '2024-01-01T00:00:00Z')",
                "rule",
            ),
            (
                "INSERT INTO recurring_schedules
                 (guid, name, frequency, start_date, template_json, created_at)
                 VALUES ('sch1', 'Rent', 'monthly', '2024-01-01',
                         '{\"splits\":[{\"account_guid\":\"new-acct\"}]}', '2024-01-01T00:00:00Z')",
                "schedule",
            ),
        ] {
            conn.execute(setup_sql, []).unwrap();
            let err = undo_history_entry_inner(&conn, id).unwrap_err();
            assert!(
                err.to_string().contains("depend on accounts"),
                "unexpected for {expected}: {err:?}"
            );
            assert_eq!(count(&conn, "accounts"), 3);
        }
    }

    /// Undo of a scheduled post restores the pointer snapshot the caller logs,
    /// so the occurrence can be posted again (schedule.rs supplies the row).
    #[test]
    fn undo_restores_schedule_pointer_snapshot() {
        let conn = setup();
        conn.execute(
            "INSERT INTO recurring_schedules
             (guid, name, frequency, start_date, occurrences_posted, next_due_date, active,
              template_json, created_at)
             VALUES ('sch1', 'Rent', 'monthly', '2024-01-01', 0, '2024-01-01', 1,
                     '{\"splits\":[]}', '2024-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        // Post the first occurrence: snapshot the pointer + absent tx rows,
        // add the tx, advance the pointer (mirrors schedules.rs).
        let id = log_snapshots(
            &conn,
            &[
                ("recurring_schedules", "sch1"),
                ("transactions", "t1"),
                ("splits", "s-t1"),
            ],
        );
        seed_tx(&conn, "t1", "2024-01-01", "Rent", -50_000);
        conn.execute(
            "UPDATE recurring_schedules
             SET occurrences_posted = 1, next_due_date = '2024-02-01' WHERE guid = 'sch1'",
            [],
        )
        .unwrap();

        undo_history_entry_inner(&conn, id).unwrap();
        let (posted, next): (i64, Option<String>) = conn
            .query_row(
                "SELECT occurrences_posted, next_due_date FROM recurring_schedules WHERE guid = 'sch1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(posted, 0);
        assert_eq!(next.as_deref(), Some("2024-01-01"));
        assert_eq!(count(&conn, "transactions"), 0);

        redo_history_entry_inner(&conn, id).unwrap();
        let (posted, next): (i64, Option<String>) = conn
            .query_row(
                "SELECT occurrences_posted, next_due_date FROM recurring_schedules WHERE guid = 'sch1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(posted, 1);
        assert_eq!(next.as_deref(), Some("2024-02-01"));
        assert_eq!(count(&conn, "transactions"), 1);
    }

    #[test]
    fn history_pages_newest_first_with_has_more() {
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

        let first = fetch_change_history_page_inner(&conn, None, Some(2)).unwrap();
        assert_eq!(first.entries.len(), 2);
        assert!(first.has_more);
        assert_eq!(first.total, 3);
        assert!(first.entries[0].id > first.entries[1].id);

        let last_id = first.entries[1].id;
        let second = fetch_change_history_page_inner(&conn, Some(last_id), Some(2)).unwrap();
        assert_eq!(second.entries.len(), 1);
        assert!(!second.has_more);
        assert_eq!(second.total, 3);
        assert!(second.entries[0].id < last_id);
    }

    #[test]
    fn clear_history_drops_log_and_snapshots_but_keeps_data() {
        let conn = setup();
        log_snapshots(&conn, &[("transactions", "t1"), ("splits", "s-t1")]);
        seed_tx(&conn, "t1", "2024-01-10", "Groceries", 5_000);
        assert_eq!(count(&conn, "change_snapshots"), 2);

        clear_change_history_inner(&conn).unwrap();

        assert_eq!(count(&conn, "change_log"), 0);
        assert_eq!(count(&conn, "change_snapshots"), 0);
        // Book data is untouched.
        assert_eq!(count(&conn, "transactions"), 1);
        let history = fetch_change_history_inner(&conn).unwrap();
        assert_eq!(history.undoable_id, None);
        assert_eq!(history.redoable_id, None);
        assert_eq!(history.total, 0);

        // New changes still log and undo after a clear.
        let next = log_snapshots(&conn, &[("transactions", "t2"), ("splits", "s-t2")]);
        seed_tx(&conn, "t2", "2024-02-01", "New", 1_000);
        undo_history_entry_inner(&conn, next).unwrap();
        assert_eq!(count(&conn, "transactions"), 1);
    }

    #[test]
    fn entry_detail_reports_table_impact_and_parsed_detail() {
        let conn = setup();
        seed_tx(&conn, "t-changed", "2024-01-10", "Old", 100);
        seed_tx(&conn, "t-removed", "2024-01-11", "Gone", 200);
        let changed_before = snapshot_row(&conn, "transactions", "t-changed").unwrap();
        let removed_before = snapshot_row(&conn, "transactions", "t-removed").unwrap();
        conn.execute(
            "UPDATE transactions SET description = 'New' WHERE guid = 't-changed'",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM transactions WHERE guid = 't-removed'", [])
            .unwrap();
        let id = insert_change_entry(
            &conn,
            "test",
            "detail entry",
            "manual",
            1,
            0,
            Some(r#"{"foo":"bar","count":2}"#),
            &[
                Snapshot {
                    table_name: "transactions",
                    row_guid: "t-changed".to_string(),
                    before_json: changed_before,
                },
                Snapshot {
                    table_name: "transactions",
                    row_guid: "t-removed".to_string(),
                    before_json: removed_before,
                },
                Snapshot {
                    table_name: "transactions",
                    row_guid: "t-added".to_string(),
                    before_json: None,
                },
            ],
        )
        .unwrap();

        let detail = fetch_change_entry_detail_inner(&conn, id).unwrap();
        assert_eq!(detail.detail.as_ref().unwrap()["foo"], "bar");
        assert_eq!(detail.tables.len(), 1);
        let table = &detail.tables[0];
        assert_eq!(table.table_name, "transactions");
        assert_eq!(table.added, 1);
        assert_eq!(table.changed, 1);
        assert_eq!(table.removed, 1);

        assert!(fetch_change_entry_detail_inner(&conn, 9_999).is_err());
    }
}
