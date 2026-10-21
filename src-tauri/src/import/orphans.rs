use super::*;

// ── Orphan helpers ──────────────────────────────────────────────────────────

/// Materialize source GUIDs into a TEMP table so orphan detection can use an
/// anti-join instead of a giant `IN (...)` list (SQLite caps bound parameters
/// at ~32k, which real books exceed). Inserts run in one txn instead of one
/// autocommit per GUID.
pub(super) fn populate_source_guids(
    conn: &Connection,
    name: &str,
    guids: &[String],
) -> Result<(), AppError> {
    conn.execute_batch(&format!(
        "DROP TABLE IF EXISTS {name};
         CREATE TEMP TABLE {name} (guid TEXT NOT NULL PRIMARY KEY);"
    ))?;
    db::with_tx_err(conn, || {
        let mut stmt = conn.prepare(&format!("INSERT OR IGNORE INTO {name} (guid) VALUES (?1)"))?;
        for guid in guids {
            stmt.execute([guid])?;
        }
        Ok(())
    })
}

pub(super) fn drop_source_guid_tables(conn: &Connection) {
    let _ = conn.execute_batch(
        "DROP TABLE IF EXISTS tmp_src_commodities;
         DROP TABLE IF EXISTS tmp_src_accounts;
         DROP TABLE IF EXISTS tmp_src_transactions;
         DROP TABLE IF EXISTS tmp_src_splits;
         DROP TABLE IF EXISTS tmp_protected_accounts;",
    );
}

pub(super) fn count_orphaned(
    conn: &Connection,
    table: &str,
    src_table: &str,
    excluded_table: Option<&str>,
) -> Result<i64, AppError> {
    db::validate_data_table(table)?;
    let mut sql =
        format!("SELECT COUNT(*) FROM {table} WHERE guid NOT IN (SELECT guid FROM {src_table})");
    if let Some(excluded) = excluded_table {
        sql.push_str(&format!(" AND guid NOT IN (SELECT guid FROM {excluded})"));
    }
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(AppError::from)
}

pub(super) fn delete_orphans_rows(
    conn: &Connection,
    table: &str,
    src_table: &str,
    excluded_table: Option<&str>,
) -> Result<i64, AppError> {
    db::validate_data_table(table)?;
    let mut sql = format!("DELETE FROM {table} WHERE guid NOT IN (SELECT guid FROM {src_table})");
    if let Some(excluded) = excluded_table {
        sql.push_str(&format!(" AND guid NOT IN (SELECT guid FROM {excluded})"));
    }
    conn.execute(&sql, [])
        .map(|n| n as i64)
        .map_err(AppError::from)
}

/// Every account that has at least one budget row. The sweep must not delete
/// these: `budgets.account_guid` cascades, so the delete would be permanent.
/// Mirrors the manual-delete guard in `accounts_mut.rs`.
pub(super) fn accounts_with_budgets(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT DISTINCT account_guid FROM budgets")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Canonical default roots the source book doesn't cover, as local GUIDs.
/// These survive the orphan sweep so re-imports stay no-ops and their GUIDs
/// stay stable; source-provided roots of the same category replace them
/// through the normal merge/sweep path. Only roots whose parent ROOT survives
/// the import are protected — the seeded ROOT of a fresh book is swept in
/// favor of the source's ROOT, taking its roots with it.
pub(super) fn protected_default_roots(
    conn: &Connection,
    src: &SourceData,
) -> Result<Vec<String>, AppError> {
    let src_roots: HashSet<&str> = src
        .accounts
        .iter()
        .filter(|a| a.account_type == "ROOT")
        .map(|a| a.guid.as_str())
        .collect();
    let covered: HashSet<&str> = src
        .accounts
        .iter()
        .filter(|a| {
            a.account_type != "ROOT"
                && a.parent_guid
                    .as_deref()
                    .is_none_or(|parent| src_roots.contains(parent))
        })
        .map(|a| utils::category_for_type(&a.account_type))
        .collect();

    let src_guids: HashSet<&str> = src.accounts.iter().map(|a| a.guid.as_str()).collect();
    let mut protected = Vec::new();
    for (name, category) in init::DEFAULT_ROOTS {
        if covered.contains(category) {
            continue;
        }
        let mut stmt = conn.prepare(
            "SELECT guid, parent_guid FROM accounts
             WHERE name = ?1 AND account_type = ?2
               AND parent_guid IN (SELECT guid FROM accounts WHERE account_type = 'ROOT')",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![name, category], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (guid, parent) in rows {
            let parent_survives = parent.as_deref().is_some_and(|p| src_guids.contains(p));
            if parent_survives && !src_guids.contains(guid.as_str()) {
                protected.push(guid);
            }
        }
    }
    Ok(protected)
}
