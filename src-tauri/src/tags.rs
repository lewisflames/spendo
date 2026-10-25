//! Transaction tags: free-form labels that cut across the account tree (a
//! trip, a project, tax-deductible). A tag belongs to the whole transaction,
//! not a split. Callers pass tag names; [`set_transaction_tags`] resolves them
//! case-insensitively and creates missing ones, so the editor never needs a
//! separate "create tag" step.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::sql;
use crate::utils;

/// Longest tag name, in characters.
pub const MAX_TAG_LEN: usize = 40;
/// Most tags one transaction can carry.
pub const MAX_TAGS_PER_TX: usize = 20;

/// A tag as shown on a transaction.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagRef {
    pub guid: String,
    pub name: String,
}

/// A tag with its usage, for the editor suggestions and Settings.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagSummary {
    pub guid: String,
    pub name: String,
    pub transaction_count: i64,
    /// Latest post date among its transactions, `None` while unused.
    pub last_used: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameTagInput {
    pub guid: String,
    pub name: String,
}

/// Trim and collapse inner whitespace, then check length and characters.
/// Commas are refused because the tag input commits a tag on comma.
pub(crate) fn normalize_tag_name(raw: &str) -> Result<String, AppError> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(AppError::Validation("Tags need a name".to_string()));
    }
    if name.chars().count() > MAX_TAG_LEN {
        return Err(AppError::Validation(format!(
            "Keep tags under {MAX_TAG_LEN} characters"
        )));
    }
    if name.contains(',') {
        return Err(AppError::Validation(
            "Tags can't contain commas".to_string(),
        ));
    }
    Ok(name)
}

/// Normalized names, deduplicated ignoring case (first spelling wins).
pub(crate) fn normalize_tag_names(raw: &[String]) -> Result<Vec<String>, AppError> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for name in raw {
        let name = normalize_tag_name(name)?;
        if seen.insert(name.to_lowercase()) {
            out.push(name);
        }
    }
    if out.len() > MAX_TAGS_PER_TX {
        return Err(AppError::Validation(format!(
            "Keep it to {MAX_TAGS_PER_TX} tags or fewer"
        )));
    }
    Ok(out)
}

/// GUID of the tag called `name` (ignoring case), creating it when missing.
/// `name` must already be normalized.
pub(crate) fn ensure_tag(conn: &Connection, name: &str) -> Result<String, AppError> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT guid FROM tags WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(guid) = existing {
        return Ok(guid);
    }
    let guid = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO tags (guid, name, created_at) VALUES (?1, ?2, ?3)",
        params![guid, name, utils::iso_now()],
    )?;
    Ok(guid)
}

/// Replace a transaction's tags with `names` (validated, created on demand).
/// Runs inside the caller's transaction.
pub(crate) fn set_transaction_tags(
    conn: &Connection,
    tx_guid: &str,
    names: &[String],
) -> Result<(), AppError> {
    let names = normalize_tag_names(names)?;
    conn.execute(
        "DELETE FROM transaction_tags WHERE tx_guid = ?1",
        params![tx_guid],
    )?;
    let mut insert = conn.prepare_cached(
        "INSERT OR IGNORE INTO transaction_tags (tx_guid, tag_guid) VALUES (?1, ?2)",
    )?;
    for name in &names {
        let tag_guid = ensure_tag(conn, name)?;
        insert.execute(params![tx_guid, tag_guid])?;
    }
    Ok(())
}

/// Tags per transaction for a page of GUIDs, each list sorted by name.
pub(crate) fn tags_for_txns(
    conn: &Connection,
    guids: &[String],
) -> Result<HashMap<String, Vec<TagRef>>, AppError> {
    let mut out: HashMap<String, Vec<TagRef>> = HashMap::new();
    for chunk in guids.chunks(sql::MAX_IN_PARAMS) {
        let sql = format!(
            "SELECT tt.tx_guid, g.guid, g.name
             FROM transaction_tags tt
             JOIN tags g ON g.guid = tt.tag_guid
             WHERE tt.tx_guid IN ({})
             ORDER BY g.name COLLATE NOCASE, g.guid",
            sql::in_clause(chunk.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(sql::string_param_refs(chunk).as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                TagRef {
                    guid: row.get(1)?,
                    name: row.get(2)?,
                },
            ))
        })?;
        for row in rows {
            let (tx_guid, tag) = row?;
            out.entry(tx_guid).or_default().push(tag);
        }
    }
    Ok(out)
}

/// Name of one tag, `None` when it doesn't exist.
pub(crate) fn tag_name(conn: &Connection, guid: &str) -> Result<Option<String>, AppError> {
    conn.query_row("SELECT name FROM tags WHERE guid = ?1", [guid], |row| {
        row.get(0)
    })
    .optional()
    .map_err(AppError::from)
}

/// Every tag, most used first. Counts and dates honor the visibility window
/// like the transaction list the counts link to.
fn fetch_tags_inner(conn: &Connection) -> Result<Vec<TagSummary>, AppError> {
    let sql = format!(
        "SELECT g.guid, g.name, COUNT(t.guid), MAX(t.post_date)
         FROM tags g
         LEFT JOIN transaction_tags tt ON tt.tag_guid = g.guid
         LEFT JOIN transactions t ON t.guid = tt.tx_guid AND {}
         GROUP BY g.guid, g.name
         ORDER BY COUNT(t.guid) DESC, g.name COLLATE NOCASE, g.guid",
        utils::VISIBLE_TX_FILTER
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |row| {
            let last: Option<String> = row.get(3)?;
            Ok(TagSummary {
                guid: row.get(0)?,
                name: row.get(1)?,
                transaction_count: row.get(2)?,
                last_used: last.map(|d| d.get(..10).unwrap_or(&d).to_string()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Rename a tag. Renaming onto another tag's name (ignoring case) merges the
/// two: its transactions move over and the renamed tag disappears.
fn rename_tag_inner(conn: &Connection, input: &RenameTagInput) -> Result<String, AppError> {
    let name = normalize_tag_name(&input.name)?;
    if tag_name(conn, &input.guid)?.is_none() {
        return Err(AppError::NotFound(
            "That tag is gone. It may have been deleted".to_string(),
        ));
    }
    let other: Option<String> = conn
        .query_row(
            "SELECT guid FROM tags WHERE name = ?1 AND guid != ?2",
            params![name, input.guid],
            |row| row.get(0),
        )
        .optional()?;
    with_tx_err(conn, || match other {
        Some(target) => {
            conn.execute(
                "INSERT OR IGNORE INTO transaction_tags (tx_guid, tag_guid)
                 SELECT tx_guid, ?1 FROM transaction_tags WHERE tag_guid = ?2",
                params![target, input.guid],
            )?;
            conn.execute("DELETE FROM tags WHERE guid = ?1", params![input.guid])?;
            Ok(target)
        }
        None => {
            conn.execute(
                "UPDATE tags SET name = ?1 WHERE guid = ?2",
                params![name, input.guid],
            )?;
            Ok(input.guid.clone())
        }
    })
}

/// Delete a tag; its links go with it (FK cascade), the transactions stay.
fn delete_tag_inner(conn: &Connection, guid: &str) -> Result<(), AppError> {
    let deleted = conn.execute("DELETE FROM tags WHERE guid = ?1", params![guid])?;
    if deleted == 0 {
        return Err(AppError::NotFound(
            "That tag is gone. It may have been deleted".to_string(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn fetch_tags(state: State<'_, AppState>) -> Result<Vec<TagSummary>, AppError> {
    run_blocking(state.db.clone(), fetch_tags_inner).await
}

/// Returns the surviving tag's GUID (the merge target when names collide).
#[tauri::command]
pub async fn rename_tag(
    state: State<'_, AppState>,
    input: RenameTagInput,
) -> Result<String, AppError> {
    run_blocking(state.db.clone(), move |conn| rename_tag_inner(conn, &input)).await
}

#[tauri::command]
pub async fn delete_tag(state: State<'_, AppState>, guid: String) -> Result<(), AppError> {
    run_blocking(state.db.clone(), move |conn| delete_tag_inner(conn, &guid)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        init_db(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO transactions (guid, post_date) VALUES
                ('t1', '2024-01-10'), ('t2', '2024-02-05'), ('t3', '2099-01-01');",
        )
        .unwrap();
        conn
    }

    fn names(conn: &Connection, tx: &str) -> Vec<String> {
        tags_for_txns(conn, &[tx.to_string()])
            .unwrap()
            .remove(tx)
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    #[test]
    fn normalizes_and_rejects_bad_names() {
        assert_eq!(
            normalize_tag_name("  Summer   trip ").unwrap(),
            "Summer trip"
        );
        assert!(normalize_tag_name("   ").is_err());
        assert!(normalize_tag_name("a,b").is_err());
        assert!(normalize_tag_name(&"x".repeat(MAX_TAG_LEN + 1)).is_err());
        let deduped =
            normalize_tag_names(&["Trip".into(), "trip".into(), " work ".into()]).unwrap();
        assert_eq!(deduped, vec!["Trip".to_string(), "work".to_string()]);
        let too_many: Vec<String> = (0..=MAX_TAGS_PER_TX).map(|i| format!("t{i}")).collect();
        assert!(normalize_tag_names(&too_many).is_err());
    }

    #[test]
    fn set_reuses_tags_ignoring_case_and_replaces_links() {
        let conn = setup();
        set_transaction_tags(&conn, "t1", &["Trip".into(), "Work".into()]).unwrap();
        set_transaction_tags(&conn, "t2", &["trip".into()]).unwrap();
        let tag_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tag_count, 2, "'trip' reuses 'Trip'");
        assert_eq!(names(&conn, "t2"), vec!["Trip"]);

        set_transaction_tags(&conn, "t1", &["Work".into()]).unwrap();
        assert_eq!(names(&conn, "t1"), vec!["Work"]);
        set_transaction_tags(&conn, "t1", &[]).unwrap();
        assert!(names(&conn, "t1").is_empty());
    }

    #[test]
    fn summary_counts_visible_transactions_only() {
        let conn = setup();
        for tx in ["t1", "t2", "t3"] {
            set_transaction_tags(&conn, tx, &["trip".into()]).unwrap();
        }
        set_transaction_tags(&conn, "t1", &["trip".into(), "unused-later".into()]).unwrap();
        set_transaction_tags(&conn, "t1", &["trip".into()]).unwrap();

        let tags = fetch_tags_inner(&conn).unwrap();
        assert_eq!(tags[0].name, "trip");
        assert_eq!(
            tags[0].transaction_count, 2,
            "the 2099 row is outside the window"
        );
        assert_eq!(tags[0].last_used.as_deref(), Some("2024-02-05"));
        assert_eq!(tags[1].name, "unused-later");
        assert_eq!(tags[1].transaction_count, 0);
        assert_eq!(tags[1].last_used, None);
    }

    #[test]
    fn rename_onto_existing_name_merges() {
        let conn = setup();
        set_transaction_tags(&conn, "t1", &["Holiday".into(), "Trip".into()]).unwrap();
        set_transaction_tags(&conn, "t2", &["Holiday".into()]).unwrap();
        let holiday = ensure_tag(&conn, "Holiday").unwrap();
        let trip = ensure_tag(&conn, "Trip").unwrap();

        let survivor = rename_tag_inner(
            &conn,
            &RenameTagInput {
                guid: holiday.clone(),
                name: "trip".into(),
            },
        )
        .unwrap();
        assert_eq!(survivor, trip);
        assert_eq!(names(&conn, "t1"), vec!["Trip"]);
        assert_eq!(names(&conn, "t2"), vec!["Trip"]);
        assert_eq!(tag_name(&conn, &holiday).unwrap(), None);

        let renamed = rename_tag_inner(
            &conn,
            &RenameTagInput {
                guid: trip.clone(),
                name: "Travel".into(),
            },
        )
        .unwrap();
        assert_eq!(renamed, trip);
        assert_eq!(names(&conn, "t1"), vec!["Travel"]);
    }

    #[test]
    fn links_die_with_their_transaction_or_tag() {
        let conn = setup();
        set_transaction_tags(&conn, "t1", &["trip".into()]).unwrap();
        set_transaction_tags(&conn, "t2", &["trip".into(), "work".into()]).unwrap();
        conn.execute("DELETE FROM transactions WHERE guid = 't1'", [])
            .unwrap();
        let work = ensure_tag(&conn, "work").unwrap();
        delete_tag_inner(&conn, &work).unwrap();
        let links: i64 = conn
            .query_row("SELECT COUNT(*) FROM transaction_tags", [], |r| r.get(0))
            .unwrap();
        assert_eq!(links, 1);
        assert!(matches!(
            delete_tag_inner(&conn, &work),
            Err(AppError::NotFound(_))
        ));
    }
}
