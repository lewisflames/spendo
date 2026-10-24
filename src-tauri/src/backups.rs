//! Automatic book snapshots. Each book gets its own folder under
//! `<app data>/backups/`, filled by `VACUUM INTO` copies of the live
//! connection: one `daily` snapshot per calendar day (on open plus an hourly
//! check), a safety snapshot right before every bulk change (GnuCash/CSV
//! import, Clear all data, restore), and `manual` ones from Settings.
//!
//! The book a snapshot belongs to is always read from the locked connection
//! (`Connection::path`), never from `AppState::db_path`: that path is updated
//! only after a swap releases the DB lock, so it can briefly name the old book.

use chrono::{DateTime, Local, NaiveDateTime, Utc};
use rusqlite::backup::{Backup, StepResult};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::State;

use crate::db::{self, AppError, AppState};
use crate::home::{get_summary, HomeDataResult};

/// Daily snapshots kept per book (one per day, so about a week back).
const KEEP_DAILY: usize = 7;
/// Safety snapshots (`before*` reasons) kept per book, newest first.
const KEEP_SAFETY: usize = 10;
/// How often the background thread checks for a missing daily snapshot.
const DAILY_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60 * 60);
/// Delay before the first background check after launch.
const STARTUP_DELAY: std::time::Duration = std::time::Duration::from_secs(15);

const EXTENSION: &str = "spendo";
/// UTC timestamp at the start of every snapshot file name.
const STAMP_FORMAT: &str = "%Y%m%dT%H%M%S%.3f";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum BackupReason {
    Daily,
    Manual,
    BeforeImport,
    BeforeClear,
    BeforeRestore,
}

impl BackupReason {
    const ALL: [BackupReason; 5] = [
        BackupReason::Daily,
        BackupReason::Manual,
        BackupReason::BeforeImport,
        BackupReason::BeforeClear,
        BackupReason::BeforeRestore,
    ];

    /// File-name token (kebab-case so names stay readable in a file manager).
    fn token(self) -> &'static str {
        match self {
            BackupReason::Daily => "daily",
            BackupReason::Manual => "manual",
            BackupReason::BeforeImport => "before-import",
            BackupReason::BeforeClear => "before-clear",
            BackupReason::BeforeRestore => "before-restore",
        }
    }

    fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.token() == token)
    }

    fn is_safety(self) -> bool {
        matches!(
            self,
            BackupReason::BeforeImport | BackupReason::BeforeClear | BackupReason::BeforeRestore
        )
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub file_name: String,
    pub created_at: DateTime<Utc>,
    pub reason: BackupReason,
    pub byte_size: u64,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BackupList {
    /// Snapshot folder of the active book; `None` while book-less.
    pub folder: Option<String>,
    pub backups: Vec<BackupEntry>,
}

/// Snapshot folder for `book`: `<stem>-<hash>`, where the hash of the
/// canonical path keeps two `book.spendo` files in different folders apart.
fn backup_dir(app_data_dir: &Path, book: &Path) -> PathBuf {
    let canonical = book.canonicalize().unwrap_or_else(|_| book.to_path_buf());
    let digest = crate::utils::hex_digest(Sha256::digest(canonical.to_string_lossy().as_bytes()));
    let stem = book
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let safe: String = stem
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(40)
        .collect();
    let hash = &digest[..8];
    let name = if safe.is_empty() {
        hash.to_string()
    } else {
        format!("{safe}-{hash}")
    };
    app_data_dir.join("backups").join(name)
}

/// Folder for the book behind `conn`, or `None` for the in-memory placeholder.
fn dir_for_conn(app_data_dir: &Path, conn: &Connection) -> Option<PathBuf> {
    conn.path()
        .filter(|p| !p.is_empty())
        .map(|p| backup_dir(app_data_dir, Path::new(p)))
}

fn file_name_for(created: DateTime<Utc>, reason: BackupReason) -> String {
    format!(
        "{}Z_{}.{EXTENSION}",
        created.format(STAMP_FORMAT),
        reason.token()
    )
}

/// Parse a snapshot file name. Anything else in the folder (partials, files
/// the user dropped there, path-like input from IPC) yields `None`.
fn parse_file_name(name: &str) -> Option<(DateTime<Utc>, BackupReason)> {
    if Path::new(name).file_name().and_then(|n| n.to_str()) != Some(name) {
        return None;
    }
    let stem = name.strip_suffix(&format!(".{EXTENSION}"))?;
    let (stamp, token) = stem.split_once("Z_")?;
    let created = NaiveDateTime::parse_from_str(stamp, STAMP_FORMAT).ok()?;
    Some((created.and_utc(), BackupReason::from_token(token)?))
}

/// Snapshots in `dir`, newest first. A missing folder is an empty list.
fn list_entries(dir: &Path) -> Result<Vec<BackupEntry>, AppError> {
    let read = match std::fs::read_dir(dir) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(AppError::Io(format!(
                "We couldn't read the backups folder: {e}"
            )))
        }
    };
    let mut entries = Vec::new();
    for item in read {
        let item =
            item.map_err(|e| AppError::Io(format!("We couldn't read the backups folder: {e}")))?;
        let Some(file_name) = item.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some((created_at, reason)) = parse_file_name(&file_name) else {
            continue;
        };
        let meta = item
            .metadata()
            .map_err(|e| AppError::Io(format!("We couldn't read a backup: {e}")))?;
        if !meta.is_file() {
            continue;
        }
        entries.push(BackupEntry {
            file_name,
            created_at,
            reason,
            byte_size: meta.len(),
        });
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.created_at));
    Ok(entries)
}

/// Drop daily and safety snapshots beyond their retention; manual ones stay
/// until the user deletes them. `protect` is never removed (the file a restore
/// is about to read). Removal failures only log: retention is housekeeping and
/// must never fail the snapshot that triggered it.
fn prune(dir: &Path, protect: Option<&str>) -> Result<(), AppError> {
    let entries = list_entries(dir)?;
    let mut daily = 0;
    let mut safety = 0;
    for entry in entries {
        let count = if entry.reason == BackupReason::Daily {
            &mut daily
        } else if entry.reason.is_safety() {
            &mut safety
        } else {
            continue;
        };
        *count += 1;
        let keep = if entry.reason == BackupReason::Daily {
            KEEP_DAILY
        } else {
            KEEP_SAFETY
        };
        if *count > keep && protect != Some(entry.file_name.as_str()) {
            if let Err(e) = std::fs::remove_file(dir.join(&entry.file_name)) {
                eprintln!("spendo: could not prune backup {}: {e}", entry.file_name);
            }
        }
    }
    Ok(())
}

/// Copy the live book into `dir`. `VACUUM INTO` writes a `.partial` file that
/// is renamed into place, so a crash mid-copy never leaves a listed snapshot
/// that can't be restored. Runs with the DB lock held, like Save As: the copy
/// has to come from the live connection.
fn snapshot_into(
    conn: &Connection,
    dir: &Path,
    reason: BackupReason,
    protect: Option<&str>,
) -> Result<BackupEntry, AppError> {
    std::fs::create_dir_all(dir)
        .map_err(|e| AppError::Io(format!("We couldn't create the backups folder: {e}")))?;
    let created_at = Utc::now();
    let file_name = file_name_for(created_at, reason);
    let target = dir.join(&file_name);
    let partial = dir.join(format!(".{file_name}.partial"));
    std::fs::remove_file(&partial).ok();
    let escaped = partial.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{escaped}';"))
        .map_err(|e| AppError::Io(format!("We couldn't back up the book: {e}")))?;
    std::fs::rename(&partial, &target).map_err(|e| {
        std::fs::remove_file(&partial).ok();
        AppError::Io(format!("We couldn't save the backup: {e}"))
    })?;
    let byte_size = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    prune(dir, protect)?;
    Ok(BackupEntry {
        file_name,
        created_at,
        reason,
        byte_size,
    })
}

/// Safety snapshot before a bulk change. A no-op while book-less; a failure
/// aborts the caller so nothing is changed without a way back.
pub(crate) fn before_change(
    conn: &Connection,
    app_data_dir: &Path,
    reason: BackupReason,
) -> Result<(), AppError> {
    before_change_protecting(conn, app_data_dir, reason, None)
}

fn before_change_protecting(
    conn: &Connection,
    app_data_dir: &Path,
    reason: BackupReason,
    protect: Option<&str>,
) -> Result<(), AppError> {
    let Some(dir) = dir_for_conn(app_data_dir, conn) else {
        return Ok(());
    };
    snapshot_into(conn, &dir, reason, protect).map_err(|e| {
        AppError::Io(format!(
            "We couldn't take a safety backup, so nothing was changed. {e}"
        ))
    })?;
    Ok(())
}

/// Take today's daily snapshot unless the book has one already, the setting
/// is off, or the book holds no transactions yet.
fn daily_snapshot_inner(
    conn: &Connection,
    app_data_dir: &Path,
) -> Result<Option<BackupEntry>, AppError> {
    let Some(dir) = dir_for_conn(app_data_dir, conn) else {
        return Ok(None);
    };
    if !crate::settings::auto_backup_enabled(conn)? {
        return Ok(None);
    }
    let has_transactions: bool =
        conn.query_row("SELECT EXISTS (SELECT 1 FROM transactions)", [], |r| {
            r.get(0)
        })?;
    if !has_transactions {
        return Ok(None);
    }
    let today = Local::now().date_naive();
    let taken_today = list_entries(&dir)?.iter().any(|e| {
        e.reason == BackupReason::Daily && e.created_at.with_timezone(&Local).date_naive() == today
    });
    if taken_today {
        return Ok(None);
    }
    snapshot_into(conn, &dir, BackupReason::Daily, None).map(Some)
}

fn run_daily_check(db: &Mutex<Connection>, app_data_dir: &Path) {
    let result = match db.lock() {
        Ok(conn) => daily_snapshot_inner(&conn, app_data_dir),
        Err(e) => Err(AppError::from(e)),
    };
    if let Err(e) = result {
        eprintln!("spendo: daily backup failed: {e}");
    }
}

/// Daily check for a book that was just opened, off the command's thread.
pub(crate) fn spawn_daily_check(db: Arc<Mutex<Connection>>, app_data_dir: PathBuf) {
    tauri::async_runtime::spawn_blocking(move || run_daily_check(&db, &app_data_dir));
}

/// Background thread: check shortly after startup (so the copy doesn't hold
/// the DB lock while the first pages load), then hourly so a session left open
/// across midnight still gets its next daily snapshot.
pub(crate) fn start_daily_thread(db: Arc<Mutex<Connection>>, app_data_dir: PathBuf) {
    std::thread::spawn(move || {
        std::thread::sleep(STARTUP_DELAY);
        loop {
            run_daily_check(&db, &app_data_dir);
            std::thread::sleep(DAILY_CHECK_INTERVAL);
        }
    });
}

fn require_dir(app_data_dir: &Path, conn: &Connection) -> Result<PathBuf, AppError> {
    dir_for_conn(app_data_dir, conn).ok_or_else(|| AppError::Validation("No book open".to_string()))
}

/// Resolve a snapshot file name from IPC inside `dir`.
fn resolve_backup(dir: &Path, file_name: &str) -> Result<PathBuf, AppError> {
    if parse_file_name(file_name).is_none() {
        return Err(AppError::Validation(format!("Not a backup: {file_name}")));
    }
    let path = dir.join(file_name);
    if !path.is_file() {
        return Err(AppError::NotFound(
            "That backup is gone. It may have been deleted".to_string(),
        ));
    }
    Ok(path)
}

/// Replace the live book's content with a snapshot. A `beforeRestore`
/// snapshot of the current state comes first, so a restore can be undone by
/// restoring that one. The SQLite backup API copies every page in one step
/// inside the destination's write transaction, so a failed restore leaves the
/// book as it was.
fn restore_backup_inner(
    conn: &mut Connection,
    app_data_dir: &Path,
    file_name: &str,
) -> Result<crate::home::DataSummary, AppError> {
    let dir = require_dir(app_data_dir, conn)?;
    let source_path = resolve_backup(&dir, file_name)?;
    crate::books::validate_spendo_db(&source_path)?;
    before_change_protecting(
        conn,
        app_data_dir,
        BackupReason::BeforeRestore,
        Some(file_name),
    )?;
    let source = db::connect_source_db(&source_path)
        .map_err(|e| AppError::Io(format!("We couldn't open that backup: {e}")))?;
    {
        let backup = Backup::new(&source, conn)?;
        match backup.step(-1)? {
            StepResult::Done => {}
            _ => {
                return Err(AppError::Io(
                    "The book is busy. Please try the restore again".to_string(),
                ))
            }
        }
    }
    conn.flush_prepared_statement_cache();
    // Heal the restored content the same way an open does.
    db::init_db(conn)?;
    db::stamp_app_id(conn);
    crate::init::ensure_default_roots(conn)?;
    get_summary(conn)
}

#[tauri::command]
pub async fn fetch_backups(state: State<'_, AppState>) -> Result<BackupList, AppError> {
    // Listing is plain disk I/O, so it reads the recorded path instead of
    // taking the DB lock; a swap racing this only shows a stale list.
    let book = state.db_path.lock()?.clone();
    let Some(book) = book else {
        return Ok(BackupList {
            folder: None,
            backups: Vec::new(),
        });
    };
    let dir = backup_dir(&state.app_data_dir, &book);
    db::run_io(move || {
        let backups = list_entries(&dir)?;
        Ok(BackupList {
            folder: Some(dir.to_string_lossy().to_string()),
            backups,
        })
    })
    .await
}

#[tauri::command]
pub async fn create_backup(state: State<'_, AppState>) -> Result<BackupEntry, AppError> {
    let app_data_dir = state.app_data_dir.clone();
    db::run_blocking(state.db.clone(), move |conn| {
        let dir = require_dir(&app_data_dir, conn)?;
        snapshot_into(conn, &dir, BackupReason::Manual, None)
    })
    .await
}

#[tauri::command]
pub async fn delete_backup(state: State<'_, AppState>, file_name: String) -> Result<(), AppError> {
    let book = state
        .db_path
        .lock()?
        .clone()
        .ok_or_else(|| AppError::Validation("No book open".to_string()))?;
    let dir = backup_dir(&state.app_data_dir, &book);
    db::run_io(move || {
        let path = resolve_backup(&dir, &file_name)?;
        std::fs::remove_file(path)
            .map_err(|e| AppError::Io(format!("We couldn't delete that backup: {e}")))
    })
    .await
}

/// Restore a snapshot into the open book in place (same file, same path).
/// Content changes wholesale, so this bumps `book_generation` like a swap.
#[tauri::command]
pub async fn restore_backup(
    state: State<'_, AppState>,
    file_name: String,
) -> Result<HomeDataResult, AppError> {
    let app_data_dir = state.app_data_dir.clone();
    let generation = state.book_generation.clone();
    // SAFETY: like Save As, the restore must run against the live connection,
    // so its file I/O (safety snapshot, page copy) happens under the DB lock.
    let summary = db::run_swap_blocking(state.db.clone(), move |conn| {
        let summary = restore_backup_inner(conn, &app_data_dir, &file_name)?;
        generation.fetch_add(1, Ordering::SeqCst);
        Ok(summary)
    })
    .await?;
    Ok(HomeDataResult {
        has_data: true,
        summary: Some(summary),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("spendo-backups-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A stamped file-backed book with one transaction.
    fn book_with_tx(path: &Path) -> Connection {
        let conn = db::connect_db(&path.to_path_buf()).unwrap();
        db::init_db(&conn).unwrap();
        db::stamp_app_id(&conn);
        crate::init::ensure_default_roots(&conn).unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description) VALUES ('t1', '2026-01-02', 'Coffee')",
            [],
        )
        .unwrap();
        conn
    }

    fn tx_count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn file_names_round_trip_and_reject_foreign_input() {
        let created = DateTime::parse_from_rfc3339("2026-10-24T17:56:17.353Z")
            .unwrap()
            .with_timezone(&Utc);
        for reason in BackupReason::ALL {
            let name = file_name_for(created, reason);
            assert_eq!(parse_file_name(&name), Some((created, reason)), "{name}");
        }
        assert_eq!(
            file_name_for(created, BackupReason::BeforeImport),
            "20261024T175617.353Z_before-import.spendo"
        );
        for bad in [
            "book.spendo",
            "20261024T175617.353Z_weekly.spendo",
            "20261024T175617.353Z_daily.db",
            ".20261024T175617.353Z_daily.spendo.partial",
            "../20261024T175617.353Z_daily.spendo",
            "x/20261024T175617.353Z_daily.spendo",
        ] {
            assert_eq!(parse_file_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn backup_dirs_differ_per_book_path() {
        let app = Path::new("/data");
        let a = backup_dir(app, Path::new("/home/a/book.spendo"));
        let b = backup_dir(app, Path::new("/home/b/book.spendo"));
        assert_ne!(a, b);
        assert!(a.starts_with("/data/backups"));
        let name = a.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("book-"), "{name}");
        // Odd characters are dropped, never turned into path segments.
        let odd = backup_dir(app, Path::new("/x/../ü b.spendo"));
        assert_eq!(odd.parent().unwrap(), Path::new("/data/backups"));
    }

    #[test]
    fn placeholder_books_are_never_snapshotted() {
        let app = tmp_dir("placeholder");
        let conn = db::open_placeholder().unwrap();
        before_change(&conn, &app, BackupReason::BeforeClear).unwrap();
        assert_eq!(daily_snapshot_inner(&conn, &app).unwrap(), None);
        assert!(!app.join("backups").exists());
        std::fs::remove_dir_all(&app).ok();
    }

    #[test]
    fn daily_snapshot_is_taken_once_per_day() {
        let app = tmp_dir("daily");
        let conn = book_with_tx(&app.join("book.spendo"));
        let first = daily_snapshot_inner(&conn, &app).unwrap();
        assert!(first.is_some());
        assert_eq!(daily_snapshot_inner(&conn, &app).unwrap(), None);
        let dir = dir_for_conn(&app, &conn).unwrap();
        let entries = list_entries(&dir).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].reason, BackupReason::Daily);
        assert!(entries[0].byte_size > 0);
        // Snapshots are stamped books that open like any other.
        crate::books::validate_spendo_db(&dir.join(&entries[0].file_name)).unwrap();
        std::fs::remove_dir_all(&app).ok();
    }

    #[test]
    fn daily_snapshot_skips_empty_books_and_the_off_setting() {
        let app = tmp_dir("daily-skip");
        let path = app.join("book.spendo");
        let conn = db::connect_db(&path).unwrap();
        db::init_db(&conn).unwrap();
        assert_eq!(daily_snapshot_inner(&conn, &app).unwrap(), None);

        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t1', '2026-01-02')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES ('auto_backup', 'off')",
            [],
        )
        .unwrap();
        assert_eq!(daily_snapshot_inner(&conn, &app).unwrap(), None);
        std::fs::remove_dir_all(&app).ok();
    }

    #[test]
    fn retention_keeps_manual_and_trims_the_rest() {
        let app = tmp_dir("prune");
        let dir = app.join("snapshots");
        std::fs::create_dir_all(&dir).unwrap();
        let base = Utc::now();
        let write = |i: i64, reason| {
            let name = file_name_for(base - chrono::Duration::hours(i), reason);
            std::fs::write(dir.join(name), "x").unwrap();
        };
        for i in 0..10 {
            write(i, BackupReason::Daily);
        }
        for i in 0..12 {
            write(100 + i, BackupReason::BeforeImport);
            write(200 + i, BackupReason::Manual);
        }
        write(300, BackupReason::BeforeClear);
        let oldest_safety = file_name_for(
            base - chrono::Duration::hours(300),
            BackupReason::BeforeClear,
        );

        prune(&dir, Some(&oldest_safety)).unwrap();
        let entries = list_entries(&dir).unwrap();
        let count = |r| entries.iter().filter(|e| e.reason == r).count();
        assert_eq!(count(BackupReason::Daily), KEEP_DAILY);
        assert_eq!(count(BackupReason::Manual), 12);
        // Ten safety snapshots, plus the protected one past the limit.
        assert_eq!(count(BackupReason::BeforeImport), KEEP_SAFETY);
        assert!(entries.iter().any(|e| e.file_name == oldest_safety));

        prune(&dir, None).unwrap();
        assert!(!dir.join(&oldest_safety).exists());
        std::fs::remove_dir_all(&app).ok();
    }

    #[test]
    fn restore_replaces_content_and_keeps_a_way_back() {
        let app = tmp_dir("restore");
        let mut conn = book_with_tx(&app.join("book.spendo"));
        let dir = dir_for_conn(&app, &conn).unwrap();
        let snap = snapshot_into(&conn, &dir, BackupReason::Manual, None).unwrap();

        conn.execute("DELETE FROM transactions", []).unwrap();
        assert_eq!(tx_count(&conn), 0);

        let summary = restore_backup_inner(&mut conn, &app, &snap.file_name).unwrap();
        assert_eq!(summary.total_transactions, 1);
        assert_eq!(tx_count(&conn), 1);
        let id: i32 = conn
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .unwrap();
        assert_eq!(id, db::SPENDO_APP_ID);

        // The pre-restore state (no transactions) was saved first.
        let entries = list_entries(&dir).unwrap();
        let undo = entries
            .iter()
            .find(|e| e.reason == BackupReason::BeforeRestore)
            .expect("before-restore snapshot");
        restore_backup_inner(&mut conn, &app, &undo.file_name).unwrap();
        assert_eq!(tx_count(&conn), 0);
        std::fs::remove_dir_all(&app).ok();
    }

    #[test]
    fn restore_rejects_unknown_and_foreign_files() {
        let app = tmp_dir("restore-reject");
        let mut conn = book_with_tx(&app.join("book.spendo"));
        let err = restore_backup_inner(&mut conn, &app, "../book.spendo").unwrap_err();
        assert_eq!(err.kind(), "validation");
        let missing = file_name_for(Utc::now(), BackupReason::Manual);
        let err = restore_backup_inner(&mut conn, &app, &missing).unwrap_err();
        assert_eq!(err.kind(), "notFound");

        // A non-book under a valid name fails validation and changes nothing.
        let dir = dir_for_conn(&app, &conn).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(&missing), "not sqlite").unwrap();
        assert!(restore_backup_inner(&mut conn, &app, &missing).is_err());
        assert_eq!(tx_count(&conn), 1);
        assert!(list_entries(&dir)
            .unwrap()
            .iter()
            .all(|e| e.reason != BackupReason::BeforeRestore));
        std::fs::remove_dir_all(&app).ok();
    }
}
