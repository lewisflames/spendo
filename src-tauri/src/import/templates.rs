use super::*;

// ── Source scheduled-transaction plumbing ────────────────────────────────────
//
// GnuCash keeps a scheduled transaction's template in the regular account and
// transaction tables: the SX points at a template account (a child of the
// book's template root, named after the SX GUID) whose splits are template
// rows. Schedules aren't imported, so these rows are only identified here and
// excluded from the ordinary entity merge.

fn table_exists(conn: &Connection, name: &str) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .map_err(AppError::from)
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, AppError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names.iter().any(|n| n == column))
}

/// Template account of every source SX. Sources without the SX table
/// (hand-made fixtures) have none.
fn sx_template_accounts(source: &Connection) -> Result<Vec<String>, AppError> {
    if !table_exists(source, "schedxactions")? {
        return Ok(Vec::new());
    }
    let mut stmt = source.prepare("SELECT template_act_guid FROM schedxactions")?;
    let guids = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(guids)
}

/// Number of scheduled transactions in the source, reported by the preview so
/// skipping them isn't silent.
pub(super) fn count_source_schedules(source: &Connection) -> Result<i64, AppError> {
    Ok(sx_template_accounts(source)?.len() as i64)
}

/// Accounts that are template plumbing: the template root recorded in `books`
/// (or its fixed name for books without the `books` table) plus every
/// descendant, including the per-SX template accounts.
pub(super) fn excluded_template_accounts(
    source: &Connection,
    accounts: &[Account],
) -> Result<HashSet<String>, AppError> {
    let mut excluded: HashSet<String> = sx_template_accounts(source)?.into_iter().collect();

    let mut found_root = false;
    if table_exists(source, "books")? && table_has_column(source, "books", "root_template_guid")? {
        let root: Option<String> = source
            .query_row("SELECT root_template_guid FROM books LIMIT 1", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .optional()?
            .flatten();
        if let Some(root) = root {
            excluded.insert(root);
            found_root = true;
        }
    }
    if !found_root {
        // GnuCash's fixed template-root name; the fallback only fires for
        // sources without a `books` table (hand-made fixtures).
        for a in accounts {
            if a.account_type == "ROOT" && a.name == "Template Root" {
                excluded.insert(a.guid.clone());
            }
        }
    }

    // Close over descendants so orphaned template accounts come along too.
    loop {
        let before = excluded.len();
        for a in accounts {
            if let Some(parent) = &a.parent_guid {
                if excluded.contains(parent) {
                    excluded.insert(a.guid.clone());
                }
            }
        }
        if excluded.len() == before {
            break;
        }
    }
    Ok(excluded)
}
