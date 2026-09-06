use rusqlite::Connection;
use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::home::{get_summary, HomeDataResult};

/// Validate a user-supplied currency mnemonic: exactly 3 ASCII letters.
/// Returns the normalized (trimmed, uppercased) mnemonic.
pub(crate) fn normalize_mnemonic(raw: &str) -> Result<String, String> {
    let mnemonic = raw.trim().to_uppercase();
    if mnemonic.len() != 3 || !mnemonic.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(format!("Invalid currency code: '{raw}' (expected 3 letters, e.g. EUR)"));
    }
    Ok(mnemonic)
}

/// Insert the base-currency commodity + ROOT account into a fresh database.
/// Takes an already-normalized mnemonic; owns its transaction. Shared by
/// `create_empty_book` (active book) and `books::create_book_at` (new file).
pub(crate) fn seed_empty_book(conn: &Connection, mnemonic: &str) -> Result<(), String> {
    let commodity_guid = uuid::Uuid::new_v4().to_string();
    let root_guid = uuid::Uuid::new_v4().to_string();

    conn.execute_batch("BEGIN TRANSACTION")
        .map_err(|e| e.to_string())?;
    let tx_result = (|| -> Result<(), String> {
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES (?1, ?2, 'CURRENCY', ?3)",
            rusqlite::params![commodity_guid, mnemonic, mnemonic],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid)
             VALUES (?1, 'Root Account', 'ROOT', NULL, NULL, NULL, 0, ?2)",
            rusqlite::params![root_guid, commodity_guid],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = tx_result {
        conn.execute_batch("ROLLBACK").ok();
        return Err(e);
    }
    conn.execute_batch("COMMIT").map_err(|e| e.to_string())
}

fn create_empty_book_inner(conn: &Connection, currency: &str) -> Result<HomeDataResult, String> {
    let mnemonic = normalize_mnemonic(currency)?;

    let existing: i64 = conn
        .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if existing > 0 {
        return Err("Database already initialized — clear all data first".to_string());
    }

    seed_empty_book(conn, &mnemonic)?;

    Ok(HomeDataResult {
        has_data: true,
        summary: Some(get_summary(conn)?),
    })
}

#[tauri::command]
pub async fn create_empty_book(
    state: State<'_, AppState>,
    currency: String,
) -> Result<HomeDataResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        create_empty_book_inner(conn, &currency)
    })
    .await
}

fn set_base_currency_inner(conn: &Connection, currency: &str) -> Result<HomeDataResult, String> {
    let mnemonic = normalize_mnemonic(currency)?;

    let root: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT guid, commodity_guid FROM accounts WHERE account_type = 'ROOT' LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    let Some((root_guid, _)) = root else {
        return Err("No database yet — create an empty book first".to_string());
    };

    let existing_guid: Option<String> = conn
        .query_row(
            "SELECT guid FROM commodities WHERE namespace = 'CURRENCY' AND mnemonic = ?1",
            [&mnemonic],
            |row| row.get(0),
        )
        .ok();

    let commodity_guid = match existing_guid {
        Some(guid) => guid,
        None => {
            let guid = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES (?1, ?2, 'CURRENCY', ?3)",
                rusqlite::params![guid, mnemonic, mnemonic],
            )
            .map_err(|e| e.to_string())?;
            guid
        }
    };

    conn.execute(
        "UPDATE accounts SET commodity_guid = ?1 WHERE guid = ?2",
        rusqlite::params![commodity_guid, root_guid],
    )
    .map_err(|e| e.to_string())?;

    Ok(HomeDataResult {
        has_data: true,
        summary: Some(get_summary(conn)?),
    })
}

#[tauri::command]
pub async fn set_base_currency(
    state: State<'_, AppState>,
    currency: String,
) -> Result<HomeDataResult, String> {
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
    fn creates_root_and_currency() {
        let conn = setup();
        let result = create_empty_book_inner(&conn, "eur").unwrap();
        assert!(result.has_data);
        let summary = result.summary.unwrap();
        assert_eq!(summary.currency, "EUR");
        assert_eq!(summary.total_accounts, 1);
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
    fn rejects_second_init_without_clear() {
        let conn = setup();
        create_empty_book_inner(&conn, "USD").unwrap();
        let err = match create_empty_book_inner(&conn, "EUR") {
            Ok(_) => panic!("expected already-initialized error"),
            Err(e) => e,
        };
        assert!(err.contains("already initialized"));
    }

    #[test]
    fn rejects_invalid_mnemonics() {
        let conn = setup();
        for bad in ["", "EU", "EURO", "E1R", " us "] {
            assert!(
                create_empty_book_inner(&conn, bad).is_err(),
                "expected error for '{bad}'"
            );
        }
        // Failed attempts must not have seeded anything.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn normalizes_case_and_whitespace() {
        let conn = setup();
        let result = create_empty_book_inner(&conn, "  gbp ").unwrap();
        assert_eq!(result.summary.unwrap().currency, "GBP");
    }

    #[test]
    fn set_base_currency_updates_root() {
        let conn = setup();
        create_empty_book_inner(&conn, "USD").unwrap();
        let result = set_base_currency_inner(&conn, "eur").unwrap();
        assert_eq!(result.summary.unwrap().currency, "EUR");
    }

    #[test]
    fn set_base_currency_reuses_existing_commodity() {
        let conn = setup();
        create_empty_book_inner(&conn, "USD").unwrap();
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
        create_empty_book_inner(&conn, "USD").unwrap();
        assert!(set_base_currency_inner(&conn, "EURO").is_err());
    }
}
