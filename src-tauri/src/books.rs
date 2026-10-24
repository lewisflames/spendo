use crate::db::AppError;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
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

/// Check that `path` is an openable Spendo book: a Spendo `application_id`
/// stamp. GnuCash sources and random SQLite/text files are rejected with
/// actionable errors.
pub(crate) fn validate_spendo_db(path: &Path) -> Result<(), AppError> {
    validate_spendo_db_inner(path)
}

fn validate_spendo_db_inner(path: &Path) -> Result<(), AppError> {
    if !path.is_file() {
        return Err(AppError::Validation(format!(
            "File not found. We couldn't find it: {}",
            path.display()
        )));
    }
    // Extension hint first: a valid Spendo book renamed to .gnucash is still
    // opened via Import, never via Open.
    if path.extension().and_then(|e| e.to_str()) == Some("gnucash") {
        return Err(AppError::Validation(gnucash_guidance()));
    }
    let probe = db::connect_source_db(&path.to_path_buf())
        .map_err(|e| AppError::Io(format!("We couldn't open that file: {}", e)))?;

    let app_id: Result<i32, _> = probe.query_row("PRAGMA application_id", [], |row| row.get(0));
    match app_id {
        Ok(id) if id == db::SPENDO_APP_ID => return Ok(()),
        // Not a SQLite database at all.
        Err(_) => {
            return Err(AppError::Validation(format!(
                "Not a Spendo database. That isn't a Spendo book: {}",
                path.display()
            )))
        }
        Ok(_) => {}
    }

    // GnuCash sources share our core table names; detect their `slots` table
    // so the error can steer toward Import instead of a generic rejection.
    let has_slots: bool = probe
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'slots'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if has_slots {
        return Err(AppError::Validation(gnucash_guidance()));
    }
    Err(AppError::Validation(format!(
        "Not a Spendo database: {}",
        path.display()
    )))
}

/// Open + ensure schema (idempotent `init_db`) + stamp + seed any missing
/// canonical root accounts (heals pre-existing books on open).
/// File I/O happens here, before the shared lock is touched.
fn open_book_inner(path: &Path) -> Result<rusqlite::Connection, AppError> {
    validate_spendo_db_inner(path)?;
    let conn = db::connect_db(&path.to_path_buf())
        .map_err(|e| AppError::Io(format!("Failed to open book: {}", e)))?;
    db::init_db(&conn).map_err(|e| AppError::Io(format!("Failed to init book: {}", e)))?;
    db::stamp_app_id(&conn);
    crate::init::ensure_default_roots(&conn)?;
    Ok(conn)
}

/// Create (or replace) a `.spendo` file and seed it as an empty book.
/// `clear_db` on the fresh connection keeps this correct when the target is
/// the currently open file too (deleting an open SQLite file fails on
/// Windows, dropping tables does not).
fn create_book_at_inner(path: &Path, currency: &str) -> Result<rusqlite::Connection, AppError> {
    let mnemonic = crate::init::normalize_mnemonic(currency)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Io(format!("Failed to create folder: {}", e)))?;
        }
    }
    let conn = db::connect_db(&path.to_path_buf())
        .map_err(|e| AppError::Io(format!("Failed to create book: {}", e)))?;
    db::clear_db(&conn).map_err(|e| AppError::Io(format!("Failed to init book: {}", e)))?;
    db::stamp_app_id(&conn);
    crate::init::seed_empty_book(&conn, &mnemonic)?;
    Ok(conn)
}

/// Escape a path for embedding in `VACUUM INTO '…'` (single quotes doubled).
fn escape_path_for_sql(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

/// Remove a previous file at the Save As target (`VACUUM INTO` refuses to
/// overwrite) plus any stale WAL sidecars (`<file>-wal`/`<file>-shm`).
fn remove_target(path: &Path) {
    std::fs::remove_file(path).ok();
    for suffix in ["-wal", "-shm"] {
        // Append to the OsString so non-UTF-8 paths keep their exact bytes.
        let mut name = path.as_os_str().to_os_string();
        name.push(suffix);
        std::fs::remove_file(PathBuf::from(name)).ok();
    }
}

/// Copy the live book into a fresh stamped file at `new_path`. The caller
/// must have prepared the destination (parent dir + stale-target sweep).
fn save_book_as_inner(
    conn: &rusqlite::Connection,
    new_path: &Path,
) -> Result<rusqlite::Connection, AppError> {
    let escaped = escape_path_for_sql(new_path);
    conn.execute_batch(&format!("VACUUM INTO '{escaped}';"))
        .map_err(|e| AppError::Io(format!("Failed to save book: {}", e)))?;
    let fresh = db::connect_db(&new_path.to_path_buf())
        .map_err(|e| AppError::Io(format!("Failed to open saved book: {}", e)))?;
    db::stamp_app_id(&fresh);
    Ok(fresh)
}

/// Swap the shared connection to `new_conn` and remember `new_path`.
/// Summarize the incoming book *before* swapping it in: if the summary query
/// fails, the live connection (and the recorded path) must stay on the old
/// book instead of silently adopting one the session doesn't know about.
fn swap_in_book(
    conn: &mut rusqlite::Connection,
    new_conn: rusqlite::Connection,
    generation: &std::sync::atomic::AtomicU64,
) -> Result<crate::home::DataSummary, AppError> {
    let summary = get_summary(&new_conn)?;
    *conn = new_conn;
    generation.fetch_add(1, Ordering::SeqCst);
    Ok(summary)
}

/// Swap the shared connection to `new_conn` and remember `new_path`.
/// The connection swap runs on a blocking thread; the path mutex is only held
/// for a `PathBuf` clone afterward (never the DB mutex in async context).
/// The sidecar write must not fail the switch, so it is logged, not bubbled.
async fn activate_book(
    state: &State<'_, AppState>,
    new_path: PathBuf,
    new_conn: rusqlite::Connection,
) -> Result<HomeDataResult, AppError> {
    let generation = state.book_generation.clone();
    let summary = db::run_swap_blocking(state.db.clone(), move |conn| {
        db::checkpoint(conn);
        swap_in_book(conn, new_conn, &generation)
    })
    .await?;
    // Brief path-mutex hold in async context: plain PathBuf clone, no I/O.
    *state.db_path.lock()? = Some(new_path.clone());
    if let Err(e) = db::write_last_book(&state.app_data_dir, &new_path) {
        eprintln!("spendo: {e}");
    }
    Ok(HomeDataResult {
        has_data: true,
        summary: Some(summary),
    })
}

/// Close the active book and return to the book-less placeholder. Writes are
/// persisted live, so there is nothing to save; the checkpoint flushes WAL
/// sidecars next to the closed file. The sidecar clear must not fail the
/// close, so it is logged, not bubbled.
#[tauri::command]
pub async fn close_book(state: State<'_, AppState>) -> Result<HomeDataResult, AppError> {
    let generation = state.book_generation.clone();
    db::run_swap_blocking(state.db.clone(), move |conn| {
        db::checkpoint(conn);
        let placeholder = db::open_placeholder()
            .map_err(|e| AppError::Io(format!("Failed to close book: {e}")))?;
        *conn = placeholder;
        generation.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .await?;
    *state.db_path.lock()? = None;
    db::clear_last_book(&state.app_data_dir);
    Ok(HomeDataResult {
        has_data: false,
        summary: None,
    })
}

/// Active book path, or `null` while on the in-memory placeholder (fresh
/// install before any New/Open). Locks only the path mutex — a PathBuf clone
/// with no DB access — so no blocking-thread hop is needed.
#[tauri::command]
pub async fn fetch_active_book(state: State<'_, AppState>) -> Result<ActiveBook, AppError> {
    let guard = state.db_path.lock()?;
    Ok(ActiveBook {
        path: guard.as_ref().map(|p| p.to_string_lossy().to_string()),
    })
}

#[tauri::command]
pub async fn open_book(
    state: State<'_, AppState>,
    path: String,
) -> Result<HomeDataResult, AppError> {
    let new_path = PathBuf::from(&path);
    // Validate + connect on a blocking thread before touching shared state.
    let open_path = new_path.clone();
    let new_conn = db::run_io(move || open_book_inner(&open_path)).await?;
    let result = activate_book(&state, new_path, new_conn).await?;
    crate::backups::spawn_daily_check(state.db.clone(), state.app_data_dir.clone());
    Ok(result)
}

#[tauri::command]
pub async fn create_book_at(
    state: State<'_, AppState>,
    path: String,
    currency: String,
) -> Result<HomeDataResult, AppError> {
    let new_path = PathBuf::from(&path);
    let create_path = new_path.clone();
    let new_conn = db::run_io(move || create_book_at_inner(&create_path, &currency)).await?;
    activate_book(&state, new_path, new_conn).await
}

#[tauri::command]
pub async fn save_book_as(
    state: State<'_, AppState>,
    path: String,
) -> Result<HomeDataResult, AppError> {
    let new_path = PathBuf::from(&path);
    // Saving onto the open file is a no-op: just re-report its summary. Compare
    // canonical forms too: `sub/../book.spendo` aliases the live file, and the
    // stale-target sweep below would otherwise delete it before copying.
    let is_same = state
        .db_path
        .lock()?
        .as_ref()
        .is_some_and(|p| crate::utils::same_path(p, &new_path));
    if is_same {
        return db::run_blocking(state.db.clone(), move |conn| {
            Ok(HomeDataResult {
                has_data: true,
                summary: Some(get_summary(conn)?),
            })
        })
        .await;
    }
    // Prepare the destination on a blocking thread (mkdir + stale-file sweep).
    let prepare_path = new_path.clone();
    db::run_io(move || {
        if let Some(parent) = prepare_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AppError::Io(format!("Failed to create folder: {}", e)))?;
            }
        }
        remove_target(&prepare_path);
        Ok(())
    })
    .await?;
    // SAFETY: `VACUUM INTO` must run on the live connection, so this is the
    // one intentional exception to "file I/O before the DB lock". It runs on
    // the blocking thread via `run_swap_blocking` (never blocking a tokio
    // worker); the UI may briefly stall on very large books while SQLite
    // copies the file, then the fresh file is opened and swapped atomically.
    let fresh_path = new_path.clone();
    let generation = state.book_generation.clone();
    let summary = db::run_swap_blocking(state.db.clone(), move |conn| {
        let fresh = save_book_as_inner(conn, &fresh_path)?;
        *conn = fresh;
        generation.fetch_add(1, Ordering::SeqCst);
        get_summary(conn)
    })
    .await?;
    *state.db_path.lock()? = Some(new_path.clone());
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

    /// Regression: `activate_book` used to swap the live connection first and
    /// only then summarize, so a failing summary left the session on a book
    /// whose path was never recorded.
    #[test]
    fn failed_summary_leaves_the_old_book_active() {
        let mut live = db::open_placeholder().unwrap();
        live.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();
        let generation = std::sync::atomic::AtomicU64::new(0);

        // Missing schema: `get_summary` fails with "no such table".
        let broken = rusqlite::Connection::open_in_memory().unwrap();
        let err = swap_in_book(&mut live, broken, &generation).unwrap_err();
        assert!(
            err.to_string().contains("no such table"),
            "unexpected: {err}"
        );
        assert_eq!(generation.load(Ordering::SeqCst), 0);

        // The live connection is untouched and still usable.
        let commodities: i64 = live
            .query_row("SELECT COUNT(*) FROM commodities", [], |r| r.get(0))
            .unwrap();
        assert_eq!(commodities, 1);
    }

    #[test]
    fn validate_accepts_stamped_books() {
        let dir = tmp_dir("accept");
        let stamped = dir.join("a.spendo");
        stamped_book(&stamped);
        assert!(validate_spendo_db_inner(&stamped).is_ok());

        // Unstamped Spendo-shaped files (pre-stamp builds) are rejected.
        let legacy = dir.join("legacy.db");
        {
            let conn = db::connect_db(&legacy.to_path_buf()).unwrap();
            db::init_db(&conn).unwrap();
        }
        assert!(validate_spendo_db_inner(&legacy).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_rejects_gnucash_sources() {
        let dir = tmp_dir("gnucash");
        // GnuCash-shaped: core tables + slots, no application_id.
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
        assert!(err.to_string().contains("GnuCash"), "unexpected: {err}");

        // Extension check fires even for real Spendo content.
        let renamed = dir.join("real.gnucash");
        stamped_book(&renamed);
        let err = validate_spendo_db_inner(&renamed).unwrap_err();
        assert!(err.to_string().contains("GnuCash"), "unexpected: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_rejects_non_books_and_missing_files() {
        let dir = tmp_dir("reject");
        assert!(validate_spendo_db_inner(&dir.join("nope.spendo"))
            .unwrap_err()
            .to_string()
            .contains("File not found"));

        let text = dir.join("notes.spendo");
        std::fs::write(&text, "hello, not sqlite").unwrap();
        assert!(validate_spendo_db_inner(&text)
            .unwrap_err()
            .to_string()
            .contains("Not a Spendo database"));

        let other = dir.join("other.db");
        let conn = db::connect_db(&other.to_path_buf()).unwrap();
        conn.execute("CREATE TABLE foo (id INTEGER)", []).unwrap();
        assert!(validate_spendo_db_inner(&other)
            .unwrap_err()
            .to_string()
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
        // ROOT + the five canonical category roots seeded by `seed_empty_book`.
        assert_eq!(accounts, 6);
        let id: i32 = conn
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .unwrap();
        assert_eq!(id, db::SPENDO_APP_ID);
        drop(conn);

        assert!(validate_spendo_db_inner(&path).is_ok());
        let conn = open_book_inner(&path).unwrap();
        let summary = get_summary(&conn).unwrap();
        assert_eq!(summary.currency, "EUR");
        assert_eq!(summary.total_accounts, 6);

        // Creating again at the same path replaces cleanly (same-file safe).
        let conn = create_book_at_inner(&path, "USD").unwrap();
        assert_eq!(get_summary(&conn).unwrap().currency, "USD");

        assert!(create_book_at_inner(&path, "EURO").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_as_copies_content_into_a_fresh_stamped_file() {
        let dir = tmp_dir("save-as");
        let src = dir.join("src.spendo");
        let conn = create_book_at_inner(&src, "USD").unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-x', 'X', 'CURRENCY')",
            [],
        )
        .unwrap();

        let dst = dir.join("nested").join("dst.spendo");
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        std::fs::write(&dst, "stale, not a database").unwrap();
        let wal = format!("{}-wal", dst.display());
        std::fs::write(&wal, "stale").unwrap();
        remove_target(&dst);
        assert!(!dst.exists());
        assert!(!Path::new(&wal).exists());

        let fresh = save_book_as_inner(&conn, &dst).unwrap();
        let count: i64 = fresh
            .query_row(
                "SELECT COUNT(*) FROM commodities WHERE mnemonic = 'X'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let id: i32 = fresh
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .unwrap();
        assert_eq!(id, db::SPENDO_APP_ID);
        drop(fresh);
        drop(conn);

        assert!(validate_spendo_db_inner(&dst).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }
}
