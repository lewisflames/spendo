use rusqlite::{Connection, OptionalExtension};
use std::collections::HashSet;
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::home::{get_summary, HomeDataResult};
use crate::utils;

/// Canonical root accounts every book gets: (display name, category). The
/// sidebar's ACCOUNTS group resolves one top-level account per category, and
/// these rows are app-managed: import protects them from the orphan sweep when
/// the source book has no account of that category (see `import.rs`).
pub(crate) const DEFAULT_ROOTS: [(&str, &str); 5] = [
    ("Assets", "ASSET"),
    ("Liabilities", "LIABILITY"),
    ("Equity", "EQUITY"),
    ("Income", "INCOME"),
    ("Expenses", "EXPENSE"),
];

/// Insert any missing canonical category root as a placeholder under the
/// book's primary ROOT, so every book exposes the five ACCOUNTS destinations.
/// Idempotent; returns the GUIDs it created (empty when nothing was missing).
/// A book without a ROOT (book-less placeholder, freshly cleared DB) is left
/// untouched.
pub(crate) fn ensure_default_roots(conn: &Connection) -> Result<Vec<String>, AppError> {
    let root: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT guid, commodity_guid FROM accounts WHERE account_type = 'ROOT' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((root_guid, commodity_guid)) = root else {
        return Ok(Vec::new());
    };

    // Top-level means child of *any* ROOT row: an imported book can carry the
    // source file's ROOT alongside the one created by `seed_empty_book`.
    let covered: HashSet<String> = {
        let mut stmt = conn.prepare(
            "SELECT account_type FROM accounts
             WHERE parent_guid IN (SELECT guid FROM accounts WHERE account_type = 'ROOT')",
        )?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows.iter()
            .map(|ty| utils::category_for_type(ty).to_string())
            .collect()
    };

    let mut created = Vec::new();
    for (name, category) in DEFAULT_ROOTS {
        if covered.contains(category) {
            continue;
        }
        let guid = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, 1, ?5)",
            rusqlite::params![guid, name, category, root_guid, commodity_guid],
        )?;
        created.push(guid);
    }
    Ok(created)
}

/// Validate a user-supplied currency mnemonic: exactly 3 ASCII letters.
/// Returns the normalized (trimmed, uppercased) mnemonic.
pub(crate) fn normalize_mnemonic(raw: &str) -> Result<String, AppError> {
    let mnemonic = raw.trim().to_uppercase();
    if mnemonic.len() != 3 || !mnemonic.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError::Validation(format!(
            "Use a 3-letter code like EUR, not '{raw}'"
        )));
    }
    Ok(mnemonic)
}

/// Insert the base-currency commodity + ROOT account into a fresh database.
/// Takes an already-normalized mnemonic; owns its transaction. Used by
/// `books::create_book_at`.
pub(crate) fn seed_empty_book(conn: &Connection, mnemonic: &str) -> Result<(), AppError> {
    let commodity_guid = uuid::Uuid::new_v4().to_string();
    let root_guid = uuid::Uuid::new_v4().to_string();

    with_tx_err(conn, || {
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES (?1, ?2, 'CURRENCY', ?3)",
            rusqlite::params![commodity_guid, mnemonic, mnemonic],
        )
        ?;
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid)
             VALUES (?1, 'Root Account', 'ROOT', NULL, NULL, NULL, 0, ?2)",
            rusqlite::params![root_guid, commodity_guid],
        )
        ?;
        ensure_default_roots(conn)?;
        Ok(())
    })
}

fn set_base_currency_inner(conn: &Connection, currency: &str) -> Result<HomeDataResult, AppError> {
    let mnemonic = normalize_mnemonic(currency)?;

    let root: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT guid, commodity_guid FROM accounts WHERE account_type = 'ROOT' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((root_guid, _)) = root else {
        return Err(AppError::NotFound(
            "No database yet. Create an empty book first".to_string(),
        ));
    };

    let existing_guid: Option<String> = conn
        .query_row(
            "SELECT guid FROM commodities WHERE namespace = 'CURRENCY' AND mnemonic = ?1",
            [&mnemonic],
            |row| row.get(0),
        )
        .optional()?;

    let commodity_guid = match existing_guid {
        Some(guid) => guid,
        None => {
            let guid = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES (?1, ?2, 'CURRENCY', ?3)",
                rusqlite::params![guid, mnemonic, mnemonic],
            )
            ?;
            guid
        }
    };

    conn.execute(
        "UPDATE accounts SET commodity_guid = ?1 WHERE guid = ?2",
        rusqlite::params![commodity_guid, root_guid],
    )?;

    Ok(HomeDataResult {
        has_data: true,
        summary: Some(get_summary(conn)?),
    })
}

#[tauri::command]
pub async fn set_base_currency(
    state: State<'_, AppState>,
    currency: String,
) -> Result<HomeDataResult, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        set_base_currency_inner(conn, &currency)
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
        conn
    }

    #[test]
    fn seed_creates_root_and_currency() {
        let conn = setup();
        seed_empty_book(&conn, "EUR").unwrap();
        let summary = get_summary(&conn).unwrap();
        assert_eq!(summary.currency, "EUR");
        // ROOT + the five canonical category roots.
        assert_eq!(summary.total_accounts, 6);
        assert_eq!(summary.total_transactions, 0);

        let root_type: String = conn
            .query_row(
                "SELECT account_type FROM accounts WHERE name = 'Root Account'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root_type, "ROOT");
    }

    #[test]
    fn seed_creates_canonical_root_accounts() {
        let conn = setup();
        seed_empty_book(&conn, "EUR").unwrap();

        for (name, category) in DEFAULT_ROOTS {
            let (root_type, placeholder): (String, i64) = conn
                .query_row(
                    "SELECT a.account_type, a.placeholder
                     FROM accounts a
                     WHERE a.name = ?1
                       AND a.parent_guid IN (SELECT guid FROM accounts WHERE account_type = 'ROOT')",
                    rusqlite::params![name],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(root_type, category, "{name} must be a {category} root");
            assert_eq!(placeholder, 1, "{name} must be a placeholder");
        }
    }

    #[test]
    fn ensure_default_roots_is_idempotent_and_fills_gaps() {
        let conn = setup();
        seed_empty_book(&conn, "EUR").unwrap();
        assert!(ensure_default_roots(&conn).unwrap().is_empty());

        // Deleting one canonical root brings it back without touching the rest.
        conn.execute("DELETE FROM accounts WHERE name = 'Equity'", [])
            .unwrap();
        let created = ensure_default_roots(&conn).unwrap();
        assert_eq!(created.len(), 1);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE name = 'Equity' AND account_type = 'EQUITY'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert!(ensure_default_roots(&conn).unwrap().is_empty());
    }

    #[test]
    fn rejects_invalid_mnemonics() {
        for bad in ["", "EU", "EURO", "E1R", " us "] {
            assert!(
                normalize_mnemonic(bad).is_err(),
                "expected error for '{bad}'"
            );
        }
    }

    #[test]
    fn normalizes_case_and_whitespace() {
        assert_eq!(normalize_mnemonic("  gbp ").unwrap(), "GBP");
        assert_eq!(normalize_mnemonic("eur").unwrap(), "EUR");
    }

    #[test]
    fn set_base_currency_updates_root() {
        let conn = setup();
        seed_empty_book(&conn, "USD").unwrap();
        let result = set_base_currency_inner(&conn, "eur").unwrap();
        assert_eq!(result.summary.unwrap().currency, "EUR");
    }

    #[test]
    fn set_base_currency_reuses_existing_commodity() {
        let conn = setup();
        seed_empty_book(&conn, "USD").unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES ('c-eur', 'EUR', 'CURRENCY', 'EUR')",
            [],
        )
        .unwrap();
        set_base_currency_inner(&conn, "EUR").unwrap();
        let root_commodity: String = conn
            .query_row(
                "SELECT c.mnemonic FROM accounts a JOIN commodities c ON c.guid = a.commodity_guid WHERE a.account_type = 'ROOT'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root_commodity, "EUR");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM commodities WHERE mnemonic = 'EUR'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn set_base_currency_rejects_without_book_or_bad_code() {
        let conn = setup();
        assert!(set_base_currency_inner(&conn, "EUR").is_err());
        seed_empty_book(&conn, "USD").unwrap();
        assert!(set_base_currency_inner(&conn, "EURO").is_err());
    }
}
