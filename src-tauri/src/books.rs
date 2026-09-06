use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::State;

use crate::db::{self, AppState};
use crate::home::{get_summary, HomeDataResult};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveBook {
    pub path: Option<String>,
}

/// Guidance returned when the user points Open at a GnuCash source file.
/// Those share table names with our schema, so the error must steer toward
/// Import instead of just saying "invalid".
fn gnucash_guidance() -> String {
    "This looks like a GnuCash book, not a Spendo (.spendo) book. \
     Open or create a Spendo book first, then bring its data in via Import from GnuCash."
        .to_string()
}

/// Check that `path` is an openable Spendo book: stamped `application_id`, or
/// — for pre-stamp `spendo.db` files — the Spendo-specific `import_log` table
/// on top of the shared core tables. GnuCash sources and random SQLite/text
/// files are rejected with actionable errors.
fn validate_spendo_db_inner(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!("File not found: {}", path.display()));
    }
    // Extension hint first: a valid Spendo book renamed to .gnucash is still
    // opened via Import, never via Open.
    if path.extension().and_then(|e| e.to_str()) == Some("gnucash") {
        return Err(gnucash_guidance());
    }
    let probe = db::connect_source_db(&path.to_path_buf())
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let app_id: Result<i32, _> = probe.query_row("PRAGMA application_id", [], |row| row.get(0));
    match app_id {
        Ok(id) if id == db::SPENDO_APP_ID => return Ok(()),
        // Fall through to the table check for unstamped (legacy) books.
        Ok(_) => {}
        // Not a SQLite database at all.
        Err(_) => return Err(format!("Not a Spendo database: {}", path.display())),
    }

    let tables: HashSet<String> = {
        let mut stmt = probe
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .map_err(|_| format!("Not a Spendo database: {}", path.display()))?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .map_err(|_| format!("Not a Spendo database: {}", path.display()))?;
        rows
            .collect::<Result<HashSet<String>, _>>()
            .map_err(|_| format!("Not a Spendo database: {}", path.display()))?
    };
    if tables.contains("slots") {
        return Err(gnucash_guidance());
    }
    let has_core = ["accounts", "commodities", "transactions", "splits"]
        .iter()
        .all(|t| tables.contains(*t));
    if has_core && tables.contains("import_log") {
        return Ok(());
    }
    if has_core {
        return Err(gnucash_guidance());
    }
    Err(format!("Not a Spendo database: {}", path.display()))
}

/// Open + upgrade (idempotent `init_db` for pre-stamp files) + stamp.
/// File I/O happens here, before the shared lock is touched.
fn open_book_inner(path: &Path) -> Result<rusqlite::Connection, String> {
    validate_spendo_db_inner(path)?;
    let conn =
        db::connect_db(&path.to_path_buf()).map_err(|e| format!("Failed to open book: {}", e))?;
    db::init_db(&conn).map_err(|e| format!("Failed to init book: {}", e))?;
    db::stamp_app_id(&conn);
    Ok(conn)
}

/// Create (or replace) a `.spendo` file and seed it as an empty book.
/// `clear_db` on the fresh connection keeps this correct when the target is
/// the currently open file too (deleting an open SQLite file fails on
/// Windows, dropping tables does not).
fn create_book_at_inner(path: &Path, currency: &str) -> Result<rusqlite::Connection, String> {
    let mnemonic = crate::init::normalize_mnemonic(currency)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create folder: {}", e))?;
        }
    }
    let conn =
        db::connect_db(&path.to_path_buf()).map_err(|e| format!("Failed to create book: {}", e))?;
    db::clear_db(&conn).map_err(|e| format!("Failed to init book: {}", e))?;
    db::stamp_app_id(&conn);
    crate::init::seed_empty_book(&conn, &mnemonic)?;
    Ok(conn)
}

/// Escape a path for embedding in `VACUUM INTO '…'` (single quotes doubled).
fn escape_path_for_sql(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

/// Remove a previous file at the Save As target (`VACUUM INTO` refuses to
/// overwrite) plus any stale WAL sidecars.
fn remove_target(path: &Path) {
    std::fs::remove_file(path).ok();
    std::fs::remove_file(path.with_extension("spendo-wal")).ok();
    // Sidecar names are `<file>-wal`/`<file>-shm`, not extension swaps.
    let name = format!("{}-wal", path.display());
    std::fs::remove_file(name).ok();
    let name = format!("{}-shm", path.display());
    std::fs::remove_file(name).ok();
}

/// Swap the shared connection to `new_conn` and remember `new_path`.
/// The connection swap runs on a blocking thread; the path mutex is only held
/// for a `PathBuf` clone afterward (never the DB mutex in async context).
/// The sidecar write must not fail the switch, so it is logged, not bubbled.
async fn activate_book(
    state: &State<'_, AppState>,
    new_path: PathBuf,
    new_conn: rusqlite::Connection,
) -> Result<HomeDataResult, String> {
    let summary = db::run_swap_blocking(state.db.clone(), move |conn| {
        db::checkpoint(conn);
        *conn = new_conn;
        get_summary(conn)
    })
    .await?;
    // Brief path-mutex hold in async context: plain PathBuf clone, no I/O.
    *state.db_path.lock().map_err(|e| e.to_string())? = Some(new_path.clone());
    if let Err(e) = db::write_last_book(&state.app_data_dir, &new_path) {
        eprintln!("spendo: {e}");
    }
    Ok(HomeDataResult {
        has_data: true,
        summary: Some(summary),
    })
}

/// Active book path, or `null` while on the in-memory placeholder (fresh
/// install before any New/Open). Locks only the path mutex — a PathBuf clone
/// with no DB access — so no blocking-thread hop is needed.
#[tauri::command]
pub async fn get_active_book(state: State<'_, AppState>) -> Result<ActiveBook, String> {
    let guard = state.db_path.lock().map_err(|e| e.to_string())?;
    Ok(ActiveBook {
        path: guard
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
    })
}

#[tauri::command]
pub async fn open_book(state: State<'_, AppState>, path: String) -> Result<HomeDataResult, String> {
    let new_path = PathBuf::from(&path);
    // Validate + connect before touching shared state (file I/O pre-lock).
    let new_conn = open_book_inner(&new_path)?;
    activate_book(&state, new_path, new_conn).await
}

#[tauri::command]
pub async fn create_book_at(
    state: State<'_, AppState>,
    path: String,
    currency: String,
) -> Result<HomeDataResult, String> {
    let new_path = PathBuf::from(&path);
    let new_conn = create_book_at_inner(&new_path, &currency)?;
    activate_book(&state, new_path, new_conn).await
}

#[tauri::command]
pub async fn save_book_as(
    state: State<'_, AppState>,
    path: String,
) -> Result<HomeDataResult, String> {
    let new_path = PathBuf::from(&path);
    // Saving onto the open file is a no-op: just re-report its summary.
    let is_same = state
        .db_path
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|p| p == &new_path);
    if is_same {
        return db::run_blocking(state.db.clone(), move |conn| {
            Ok(HomeDataResult {
                has_data: true,
                summary: Some(get_summary(conn)?),
            })
        })
        .await;
    }
    if let Some(parent) = new_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create folder: {}", e))?;
        }
    }
    remove_target(&new_path);
    // VACUUM INTO must run on the live connection, then the fresh file is
    // opened and swapped in atomically on the blocking thread.
    let escaped = escape_path_for_sql(&new_path);
    let fresh_path = new_path.clone();
    let summary = db::run_swap_blocking(state.db.clone(), move |conn| {
        conn.execute_batch(&format!("VACUUM INTO '{escaped}';"))
            .map_err(|e| format!("Failed to save book: {}", e))?;
        let fresh =
            db::connect_db(&fresh_path).map_err(|e| format!("Failed to open saved book: {}", e))?;
        db::stamp_app_id(&fresh);
        *conn = fresh;
        get_summary(conn)
    })
    .await?;
    *state.db_path.lock().map_err(|e| e.to_string())? = Some(new_path.clone());
    if let Err(e) = db::write_last_book(&state.app_data_dir, &new_path) {
        eprintln!("spendo: {e}");
    }
    Ok(HomeDataResult {
        has_data: true,
        summary: Some(summary),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("spendo-books-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stamped_book(path: &Path) {
        let conn = db::connect_db(&path.to_path_buf()).unwrap();
        db::init_db(&conn).unwrap();
        db::stamp_app_id(&conn);
    }

    #[test]
    fn validate_accepts_stamped_and_legacy_books() {
        let dir = tmp_dir("accept");
        let stamped = dir.join("a.spendo");
        stamped_book(&stamped);
        assert!(validate_spendo_db_inner(&stamped).is_ok());

        // Pre-stamp spendo.db: core tables + import_log, no application_id.
        let legacy = dir.join("legacy.db");
        let conn = db::connect_db(&legacy.to_path_buf()).unwrap();
        db::init_db(&conn).unwrap();
        assert!(validate_spendo_db_inner(&legacy).is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_rejects_gnucash_sources() {
        let dir = tmp_dir("gnucash");
        // GnuCash-shaped: core tables + slots, no import_log.
        let src = dir.join("book.db");
        let conn = db::connect_db(&src.to_path_buf()).unwrap();
        conn.execute_batch(
            "CREATE TABLE commodities (guid TEXT PRIMARY KEY, mnemonic TEXT, namespace TEXT, fullname TEXT);
             CREATE TABLE accounts (guid TEXT PRIMARY KEY, name TEXT, account_type TEXT, parent_guid TEXT, code TEXT, description TEXT, placeholder INTEGER, commodity_guid TEXT);
             CREATE TABLE transactions (guid TEXT PRIMARY KEY, post_date TEXT, description TEXT, num TEXT, notes TEXT);
             CREATE TABLE splits (guid TEXT PRIMARY KEY, tx_guid TEXT, account_guid TEXT, value_num INTEGER, value_denom INTEGER, quantity_num INTEGER, quantity_denom INTEGER, reconciled INTEGER, reconcile_state TEXT, memo TEXT);
             CREATE TABLE slots (obj_guid TEXT, name TEXT, string_val TEXT);",
        )
        .unwrap();
        let err = validate_spendo_db_inner(&src).unwrap_err();
        assert!(err.contains("GnuCash"), "unexpected: {err}");

        // Extension check fires even for real Spendo content.
        let renamed = dir.join("real.gnucash");
        stamped_book(&renamed);
        let err = validate_spendo_db_inner(&renamed).unwrap_err();
        assert!(err.contains("GnuCash"), "unexpected: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_rejects_non_books_and_missing_files() {
        let dir = tmp_dir("reject");
        assert!(validate_spendo_db_inner(&dir.join("nope.spendo"))
            .unwrap_err()
            .contains("File not found"));

        let text = dir.join("notes.spendo");
        std::fs::write(&text, "hello, not sqlite").unwrap();
        assert!(validate_spendo_db_inner(&text)
            .unwrap_err()
            .contains("Not a Spendo database"));

        let other = dir.join("other.db");
        let conn = db::connect_db(&other.to_path_buf()).unwrap();
        conn.execute("CREATE TABLE foo (id INTEGER)", [])
            .unwrap();
        assert!(validate_spendo_db_inner(&other)
            .unwrap_err()
            .contains("Not a Spendo database"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn create_then_open_round_trip() {
        let dir = tmp_dir("roundtrip");
        let path = dir.join("mine.spendo");
        let conn = create_book_at_inner(&path, "eur").unwrap();
        let accounts: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(accounts, 1);
        let id: i32 = conn
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .unwrap();
        assert_eq!(id, db::SPENDO_APP_ID);
        drop(conn);

        assert!(validate_spendo_db_inner(&path).is_ok());
        let conn = open_book_inner(&path).unwrap();
        let summary = get_summary(&conn).unwrap();
        assert_eq!(summary.currency, "EUR");
        assert_eq!(summary.total_accounts, 1);

        // Creating again at the same path replaces cleanly (same-file safe).
        let conn = create_book_at_inner(&path, "USD").unwrap();
        assert_eq!(get_summary(&conn).unwrap().currency, "USD");

        assert!(create_book_at_inner(&path, "EURO").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn vacuum_copies_book_content() {
        let dir = tmp_dir("vacuum");
        let src = dir.join("src.spendo");
        let conn = create_book_at_inner(&src, "USD").unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-x', 'X', 'CURRENCY')",
            [],
        )
        .unwrap();
        let dst = dir.join("dst.spendo");
        let escaped = escape_path_for_sql(&dst);
        conn.execute_batch(&format!("VACUUM INTO '{escaped}';"))
            .unwrap();
        drop(conn);

        let fresh = db::connect_db(&dst).unwrap();
        let count: i64 = fresh
            .query_row(
                "SELECT COUNT(*) FROM commodities WHERE mnemonic = 'X'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert!(validate_spendo_db_inner(&dst).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }
}
