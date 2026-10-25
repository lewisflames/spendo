//! Payees: who a transaction was with (a shop, a person, an employer). One
//! payee per transaction at most, kept in `transaction_payees` beside the
//! GnuCash-shaped `transactions` row. Callers pass a name;
//! [`set_transaction_payee`] resolves it case-insensitively and creates
//! missing ones, so the editor never needs a separate "create payee" step.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::sql;
use crate::utils;

/// Longest payee name, in characters.
pub const MAX_PAYEE_LEN: usize = 100;

/// A payee as shown on a transaction.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PayeeRef {
    pub guid: String,
    pub name: String,
}

/// A payee with its usage, for the editor suggestions and Settings.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PayeeSummary {
    pub guid: String,
    pub name: String,
    pub transaction_count: i64,
    /// Latest post date among its transactions, `None` while unused.
    pub last_used: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePayeeInput {
    pub guid: String,
    pub name: String,
}

/// Trim and collapse inner whitespace, then check the length. A blank name
/// means "no payee" and comes back as `None`.
pub(crate) fn normalize_payee_name(raw: &str) -> Result<Option<String>, AppError> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Ok(None);
    }
    if name.chars().count() > MAX_PAYEE_LEN {
        return Err(AppError::Validation(format!(
            "Keep payee names under {MAX_PAYEE_LEN} characters"
        )));
    }
    Ok(Some(name))
}

/// GUID of the payee called `name` (ignoring case), creating it when missing.
/// `name` must already be normalized.
pub(crate) fn ensure_payee(conn: &Connection, name: &str) -> Result<String, AppError> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT guid FROM payees WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(guid) = existing {
        return Ok(guid);
    }
    let guid = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO payees (guid, name, created_at) VALUES (?1, ?2, ?3)",
        params![guid, name, utils::iso_now()],
    )?;
    Ok(guid)
}

/// Point a transaction at the payee called `name` (normalized, created on
/// demand); `None` removes its payee. Runs inside the caller's transaction.
pub(crate) fn set_transaction_payee(
    conn: &Connection,
    tx_guid: &str,
    name: Option<&str>,
) -> Result<(), AppError> {
    match name {
        Some(name) => {
            let payee_guid = ensure_payee(conn, name)?;
            conn.execute(
                "INSERT INTO transaction_payees (tx_guid, payee_guid) VALUES (?1, ?2)
                 ON CONFLICT (tx_guid) DO UPDATE SET payee_guid = excluded.payee_guid",
                params![tx_guid, payee_guid],
            )?;
        }
        None => {
            conn.execute(
                "DELETE FROM transaction_payees WHERE tx_guid = ?1",
                params![tx_guid],
            )?;
        }
    }
    Ok(())
}

/// Payee per transaction for a page of GUIDs; transactions without one are
/// absent from the map.
pub(crate) fn payees_for_txns(
    conn: &Connection,
    guids: &[String],
) -> Result<HashMap<String, PayeeRef>, AppError> {
    let mut out = HashMap::new();
    for chunk in guids.chunks(sql::MAX_IN_PARAMS) {
        let sql = format!(
            "SELECT tp.tx_guid, p.guid, p.name
             FROM transaction_payees tp
             JOIN payees p ON p.guid = tp.payee_guid
             WHERE tp.tx_guid IN ({})",
            sql::in_clause(chunk.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(sql::string_param_refs(chunk).as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                PayeeRef {
                    guid: row.get(1)?,
                    name: row.get(2)?,
                },
            ))
        })?;
        for row in rows {
            let (tx_guid, payee) = row?;
            out.insert(tx_guid, payee);
        }
    }
    Ok(out)
}

/// Name of one payee, `None` when it doesn't exist.
pub(crate) fn payee_name(conn: &Connection, guid: &str) -> Result<Option<String>, AppError> {
    conn.query_row("SELECT name FROM payees WHERE guid = ?1", [guid], |row| {
        row.get(0)
    })
    .optional()
    .map_err(AppError::from)
}

/// Every payee, most used first. Counts and dates honor the visibility window
/// like the transaction list the counts link to.
fn fetch_payees_inner(conn: &Connection) -> Result<Vec<PayeeSummary>, AppError> {
    let sql = format!(
        "SELECT p.guid, p.name, COUNT(t.guid), MAX(t.post_date)
         FROM payees p
         LEFT JOIN transaction_payees tp ON tp.payee_guid = p.guid
         LEFT JOIN transactions t ON t.guid = tp.tx_guid AND {}
         GROUP BY p.guid, p.name
         ORDER BY COUNT(t.guid) DESC, p.name COLLATE NOCASE, p.guid",
        utils::VISIBLE_TX_FILTER
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |row| {
            let last: Option<String> = row.get(3)?;
            Ok(PayeeSummary {
                guid: row.get(0)?,
                name: row.get(1)?,
                transaction_count: row.get(2)?,
                last_used: last.map(|d| d.get(..10).unwrap_or(&d).to_string()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Rename a payee. Renaming onto another payee's name (ignoring case) merges
/// the two: its transactions move over and the renamed payee disappears.
fn rename_payee_inner(conn: &Connection, input: &RenamePayeeInput) -> Result<String, AppError> {
    let Some(name) = normalize_payee_name(&input.name)? else {
        return Err(AppError::Validation("Payees need a name".to_string()));
    };
    if payee_name(conn, &input.guid)?.is_none() {
        return Err(AppError::NotFound(
            "That payee is gone. It may have been deleted".to_string(),
        ));
    }
    let other: Option<String> = conn
        .query_row(
            "SELECT guid FROM payees WHERE name = ?1 AND guid != ?2",
            params![name, input.guid],
            |row| row.get(0),
        )
        .optional()?;
    with_tx_err(conn, || match other {
        Some(target) => {
            conn.execute(
                "UPDATE transaction_payees SET payee_guid = ?1 WHERE payee_guid = ?2",
                params![target, input.guid],
            )?;
            conn.execute("DELETE FROM payees WHERE guid = ?1", params![input.guid])?;
            Ok(target)
        }
        None => {
            conn.execute(
                "UPDATE payees SET name = ?1 WHERE guid = ?2",
                params![name, input.guid],
            )?;
            Ok(input.guid.clone())
        }
    })
}

/// Delete a payee; its transactions stay and lose their payee (FK cascade).
fn delete_payee_inner(conn: &Connection, guid: &str) -> Result<(), AppError> {
    let deleted = conn.execute("DELETE FROM payees WHERE guid = ?1", params![guid])?;
    if deleted == 0 {
        return Err(AppError::NotFound(
            "That payee is gone. It may have been deleted".to_string(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn fetch_payees(state: State<'_, AppState>) -> Result<Vec<PayeeSummary>, AppError> {
    run_blocking(state.db.clone(), fetch_payees_inner).await
}

/// Returns the surviving payee's GUID (the merge target when names collide).
#[tauri::command]
pub async fn rename_payee(
    state: State<'_, AppState>,
    input: RenamePayeeInput,
) -> Result<String, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        rename_payee_inner(conn, &input)
    })
    .await
}

#[tauri::command]
pub async fn delete_payee(state: State<'_, AppState>, guid: String) -> Result<(), AppError> {
    run_blocking(state.db.clone(), move |conn| {
        delete_payee_inner(conn, &guid)
    })
    .await
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

    fn payee_of(conn: &Connection, tx: &str) -> Option<String> {
        payees_for_txns(conn, &[tx.to_string()])
            .unwrap()
            .remove(tx)
            .map(|p| p.name)
    }

    #[test]
    fn normalizes_and_rejects_bad_names() {
        assert_eq!(
            normalize_payee_name("  Corte   Inglés ")
                .unwrap()
                .as_deref(),
            Some("Corte Inglés")
        );
        assert_eq!(normalize_payee_name("   ").unwrap(), None);
        assert!(normalize_payee_name(&"x".repeat(MAX_PAYEE_LEN + 1)).is_err());
    }

    #[test]
    fn set_reuses_payees_ignoring_case_and_replaces_the_link() {
        let conn = setup();
        set_transaction_payee(&conn, "t1", Some("Mercadona")).unwrap();
        set_transaction_payee(&conn, "t2", Some("MERCADONA")).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM payees", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "'MERCADONA' reuses 'Mercadona'");
        assert_eq!(payee_of(&conn, "t2").as_deref(), Some("Mercadona"));

        set_transaction_payee(&conn, "t1", Some("Renfe")).unwrap();
        assert_eq!(payee_of(&conn, "t1").as_deref(), Some("Renfe"));
        set_transaction_payee(&conn, "t1", None).unwrap();
        assert_eq!(payee_of(&conn, "t1"), None);
    }

    #[test]
    fn summary_counts_visible_transactions_only() {
        let conn = setup();
        for tx in ["t1", "t2", "t3"] {
            set_transaction_payee(&conn, tx, Some("Renfe")).unwrap();
        }
        ensure_payee(&conn, "Unused").unwrap();

        let payees = fetch_payees_inner(&conn).unwrap();
        assert_eq!(payees[0].name, "Renfe");
        assert_eq!(
            payees[0].transaction_count, 2,
            "the 2099 row is outside the window"
        );
        assert_eq!(payees[0].last_used.as_deref(), Some("2024-02-05"));
        assert_eq!(payees[1].name, "Unused");
        assert_eq!(payees[1].transaction_count, 0);
        assert_eq!(payees[1].last_used, None);
    }

    #[test]
    fn rename_onto_existing_name_merges() {
        let conn = setup();
        set_transaction_payee(&conn, "t1", Some("Corte Ingles")).unwrap();
        set_transaction_payee(&conn, "t2", Some("Corte Inglés")).unwrap();
        let typo = ensure_payee(&conn, "Corte Ingles").unwrap();
        let right = ensure_payee(&conn, "Corte Inglés").unwrap();

        let survivor = rename_payee_inner(
            &conn,
            &RenamePayeeInput {
                guid: typo.clone(),
                name: "corte inglés".into(),
            },
        )
        .unwrap();
        assert_eq!(survivor, right);
        assert_eq!(payee_of(&conn, "t1").as_deref(), Some("Corte Inglés"));
        assert_eq!(payee_name(&conn, &typo).unwrap(), None);

        let renamed = rename_payee_inner(
            &conn,
            &RenamePayeeInput {
                guid: right.clone(),
                name: "El Corte Inglés".into(),
            },
        )
        .unwrap();
        assert_eq!(renamed, right);
        assert_eq!(payee_of(&conn, "t2").as_deref(), Some("El Corte Inglés"));
        assert!(rename_payee_inner(
            &conn,
            &RenamePayeeInput {
                guid: right,
                name: "  ".into(),
            },
        )
        .is_err());
    }

    #[test]
    fn links_die_with_their_transaction_or_payee() {
        let conn = setup();
        set_transaction_payee(&conn, "t1", Some("Renfe")).unwrap();
        set_transaction_payee(&conn, "t2", Some("Ouigo")).unwrap();
        conn.execute("DELETE FROM transactions WHERE guid = 't1'", [])
            .unwrap();
        let ouigo = ensure_payee(&conn, "Ouigo").unwrap();
        delete_payee_inner(&conn, &ouigo).unwrap();
        let links: i64 = conn
            .query_row("SELECT COUNT(*) FROM transaction_payees", [], |r| r.get(0))
            .unwrap();
        assert_eq!(links, 0);
        let txns: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(txns, 2, "deleting a payee keeps its transactions");
        assert!(matches!(
            delete_payee_inner(&conn, &ouigo),
            Err(AppError::NotFound(_))
        ));
    }
}
