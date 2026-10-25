use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    /// Bumped on every connection swap (open/create/save-as) while the DB lock
    /// is held. Long-running callers that await the network (price refresh,
    /// import) capture it before their fetch and discard the write if it
    /// changed, so results can never land in a different book.
    pub book_generation: Arc<AtomicU64>,
    /// Absolute path of the active book file. `None` while on the in-memory
    /// placeholder (no book opened or created yet) — see `books::fetch_active_book`.
    pub db_path: Mutex<Option<PathBuf>>,
    /// Tauri app-data dir: home of the `last_book.json` sidecar.
    pub app_data_dir: PathBuf,
}

/// Magic stamped via `PRAGMA application_id` on every Spendo book so
/// `books::validate_spendo_db` can tell our files apart from GnuCash source
/// books (which share table names like `accounts`/`splits`) and random SQLite.
pub const SPENDO_APP_ID: i32 = 0x5350_4e44; // 'SPND'

impl AppState {
    /// Reject an export/attachment target that resolves to the active book.
    /// Those writes use `std::fs::write`, which truncates — exporting onto the
    /// live `.spendo` file would destroy the book.
    pub fn ensure_not_active_book(&self, target: &Path) -> Result<(), AppError> {
        let active = self.db_path.lock()?.clone();
        if let Some(active) = active {
            if crate::utils::same_path(&active, target) {
                return Err(AppError::Validation(
                    "Choose a file other than the open book".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// User-data tables whose names may be interpolated into SQL by the import
/// orphan sweep. Interpolating table names into SQL is only safe through this
/// allowlist — never inline a table name without `validate_data_table`.
pub(crate) const DATA_TABLES: &[&str] = &[
    "commodities",
    "accounts",
    "transactions",
    "splits",
    "recurring_schedules",
];

pub(crate) fn validate_data_table(table: &str) -> Result<(), AppError> {
    if DATA_TABLES.contains(&table) {
        Ok(())
    } else {
        Err(AppError::Validation(format!("Invalid table name: {table}")))
    }
}

/// Run a closure with the DB connection on a blocking thread. Every DB-touching
/// command must go through here so the tokio runtime threads are never blocked
/// while the global mutex is held.
pub async fn run_blocking<T, F>(db: Arc<Mutex<Connection>>, f: F) -> Result<T, AppError>
where
    F: FnOnce(&Connection) -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(&*db.lock()?))
        .await
        .map_err(task_failed)?
}

fn task_failed(e: tauri::Error) -> AppError {
    AppError::Internal(format!("Background task failed: {}", e))
}

/// Run a synchronous, non-DB closure (file hashing, reading, writing) on a
/// blocking thread so async commands never stall a tokio worker on disk I/O.
/// Mirrors [`run_blocking`] without the connection lock.
pub async fn run_io<T, F>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(task_failed)?
}

/// Run `f` inside a SQLite transaction on a shared `&Connection`.
/// Uses `unchecked_transaction()` (shared-borrow) because every command holds
/// only `&Connection` via `run_blocking`. Drop rolls back on error; commit
/// only on `Ok`. Replaces manual `BEGIN`/`COMMIT`/`ROLLBACK` blocks.
pub fn with_tx_err<T, E>(conn: &Connection, f: impl FnOnce() -> Result<T, E>) -> Result<T, E>
where
    E: From<rusqlite::Error>,
{
    let tx = conn.unchecked_transaction().map_err(E::from)?;
    let out = f()?;
    tx.commit().map_err(E::from)?;
    Ok(out)
}

/// Abort a background operation whose active book changed while it was
/// preparing (file hashing, CSV parsing, network fetch). Call this **inside**
/// the `run_blocking` closure that performs the write: connection swaps also
/// take the DB lock, so the check is authoritative once the lock is held.
pub(crate) fn ensure_same_book(book_generation: &AtomicU64, expected: u64) -> Result<(), AppError> {
    if book_generation.load(std::sync::atomic::Ordering::SeqCst) != expected {
        return Err(AppError::Validation(
            "Book changed while the operation was preparing. Please try again".to_string(),
        ));
    }
    Ok(())
}

/// Typed backend error returned by every `*_inner` helper and every
/// `#[tauri::command]`. It crosses IPC as `{ kind, message }` (see the
/// `Serialize` impl and `BackendError` in `$lib/tauri.ts`), so the frontend
/// branches on `kind` instead of matching message text.
#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Validation(String),
    /// Filesystem or network failure outside the database.
    Io(String),
    Db(DbError),
    /// Poisoned lock or failed background task: a bug, not user input.
    Internal(String),
}

impl AppError {
    /// Wire name of the variant (`BackendErrorKind` on the frontend).
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "notFound",
            AppError::Validation(_) => "validation",
            AppError::Io(_) => "io",
            AppError::Db(_) => "db",
            AppError::Internal(_) => "internal",
        }
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut out = serializer.serialize_struct("AppError", 2)?;
        out.serialize_field("kind", self.kind())?;
        out.serialize_field("message", &self.to_string())?;
        out.end()
    }
}

impl<T> From<std::sync::PoisonError<T>> for AppError {
    fn from(e: std::sync::PoisonError<T>) -> AppError {
        AppError::Internal(e.to_string())
    }
}

/// A database failure with its message plus the extended SQLite result code,
/// so callers can match failure kinds (e.g. foreign-key) without substrings.
#[derive(Debug)]
pub struct DbError {
    pub message: String,
    pub extended_code: Option<i32>,
}

impl DbError {
    // No production caller matches FK failures yet; kept as the typed matcher.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_foreign_key(&self) -> bool {
        self.extended_code == Some(rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY)
    }

    pub fn is_unique(&self) -> bool {
        self.extended_code == Some(rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(m)
            | AppError::Validation(m)
            | AppError::Io(m)
            | AppError::Internal(m) => write!(f, "{m}"),
            AppError::Db(e) => write!(f, "{}", e.message),
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> AppError {
        let extended_code = match &e {
            rusqlite::Error::SqliteFailure(err, _) => Some(err.extended_code),
            _ => None,
        };
        AppError::Db(DbError {
            message: e.to_string(),
            extended_code,
        })
    }
}

/// Run a mutable closure with the DB connection on a blocking thread. This is
/// the swap-path counterpart of [`run_blocking`]: book switching must replace
/// the `Connection` itself (`*conn = new_conn`), which needs `&mut`. Same
/// rule applies — callers must do file I/O (validate, connect new file) before
/// entering, and never lock directly inside an `async fn`.
pub async fn run_swap_blocking<T, F>(db: Arc<Mutex<Connection>>, f: F) -> Result<T, AppError>
where
    F: FnOnce(&mut Connection) -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(&mut *db.lock()?))
        .await
        .map_err(task_failed)?
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS commodities (
	guid TEXT PRIMARY KEY,
	mnemonic TEXT NOT NULL,
	namespace TEXT NOT NULL,
	fullname TEXT
);

CREATE TABLE IF NOT EXISTS accounts (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL,
	account_type TEXT NOT NULL,
	parent_guid TEXT,
	code TEXT,
	description TEXT,
	placeholder INTEGER DEFAULT 0,
	commodity_guid TEXT
);

CREATE TABLE IF NOT EXISTS transactions (
	guid TEXT PRIMARY KEY,
	post_date TEXT NOT NULL,
	description TEXT,
	num TEXT,
	notes TEXT
);

CREATE TABLE IF NOT EXISTS splits (
	guid TEXT PRIMARY KEY,
	tx_guid TEXT NOT NULL,
	account_guid TEXT NOT NULL,
	value_num INTEGER NOT NULL,
	value_denom INTEGER NOT NULL,
	quantity_num INTEGER NOT NULL,
	quantity_denom INTEGER NOT NULL,
	reconcile_state TEXT,
	reconciled INTEGER DEFAULT 0,
	memo TEXT
);

CREATE TABLE IF NOT EXISTS prices (
	guid TEXT PRIMARY KEY,
	commodity_guid TEXT NOT NULL,
	currency_guid TEXT NOT NULL,
	value_num INTEGER NOT NULL,
	value_denom INTEGER NOT NULL,
	date TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_transactions_post_date ON transactions(post_date);
CREATE INDEX IF NOT EXISTS idx_transactions_post_date_guid ON transactions(post_date, guid);
CREATE INDEX IF NOT EXISTS idx_splits_tx_guid ON splits(tx_guid);
CREATE INDEX IF NOT EXISTS idx_splits_account_guid ON splits(account_guid);
CREATE INDEX IF NOT EXISTS idx_accounts_account_type ON accounts(account_type);
CREATE INDEX IF NOT EXISTS idx_accounts_parent_guid ON accounts(parent_guid);
CREATE INDEX IF NOT EXISTS idx_prices_commodity_guid_date ON prices(commodity_guid, date);
CREATE INDEX IF NOT EXISTS idx_prices_currency_guid_date ON prices(currency_guid, date);
CREATE INDEX IF NOT EXISTS idx_commodities_mnemonic ON commodities(mnemonic);

CREATE TABLE IF NOT EXISTS budgets (
	guid TEXT PRIMARY KEY,
	account_guid TEXT NOT NULL REFERENCES accounts(guid) ON DELETE CASCADE,
	year INTEGER NOT NULL,
	amount REAL NOT NULL,
	created_at TEXT NOT NULL,
	UNIQUE (account_guid, year)
);

CREATE TABLE IF NOT EXISTS refresh_log (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	refreshed_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_budgets_account_year ON budgets(account_guid, year);

CREATE TABLE IF NOT EXISTS symbol_overrides (
	commodity_guid TEXT PRIMARY KEY,
	symbol TEXT NOT NULL
);

-- Files attached to a transaction. Bytes live in the book itself
-- so the `.spendo` file stays portable (Save As copies them via VACUUM INTO).
-- Up to one `MAX_ATTACHMENTS_PER_TX` rows per transaction (enforced in
-- `attachments.rs`, not by schema); adding files accumulates. Deleting a
-- transaction (manual delete or the GnuCash orphan
-- sweep) removes its attachments through the FK cascade.
CREATE TABLE IF NOT EXISTS attachments (
	guid TEXT PRIMARY KEY,
	tx_guid TEXT NOT NULL REFERENCES transactions(guid) ON DELETE CASCADE,
	filename TEXT NOT NULL,
	mime_type TEXT NOT NULL,
	byte_size INTEGER NOT NULL,
	data BLOB NOT NULL,
	created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_attachments_tx_guid ON attachments(tx_guid);

-- Recurring transaction schedules (Scheduled feature). Future intent lives
-- here, never in `transactions`: rows there must stay within the visibility
-- window (`VISIBLE_TX_FILTER`) and the manual future-date block. A schedule
-- holds a serialized `TxInput`-shaped template; posting materializes one real
-- transaction per due occurrence and advances `next_due_date`. `NULL`
-- `next_due_date` means completed (end date / occurrence cap reached).
-- The GnuCash orphan sweep never deletes schedules (nor accounts they
-- reference).
CREATE TABLE IF NOT EXISTS recurring_schedules (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL,
	frequency TEXT NOT NULL,
	interval_mult INTEGER NOT NULL DEFAULT 1,
	start_date TEXT NOT NULL,
	end_date TEXT,
	max_occurrences INTEGER,
	occurrences_posted INTEGER NOT NULL DEFAULT 0,
	next_due_date TEXT,
	active INTEGER NOT NULL DEFAULT 1,
	template_json TEXT NOT NULL,
	created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_schedules_active_due ON recurring_schedules(active, next_due_date);

-- Skip counter for recurring schedules, in a side table so the core row keeps
-- its GnuCash-shaped columns and older books pick this up on open. A missing
-- row means nothing has been skipped; rows cascade with their schedule.
CREATE TABLE IF NOT EXISTS schedule_skips (
	schedule_guid TEXT PRIMARY KEY REFERENCES recurring_schedules(guid) ON DELETE CASCADE,
	skipped_count INTEGER NOT NULL DEFAULT 0
);

-- Per-schedule posting options, kept beside `recurring_schedules` for the same
-- reason as `schedule_skips`. `auto_post` posts due occurrences when the book
-- opens; `spread_total` treats the template amounts as a total divided across
-- `max_occurrences` (e.g. depreciating a device). A missing row means both off.
CREATE TABLE IF NOT EXISTS schedule_options (
	schedule_guid TEXT PRIMARY KEY REFERENCES recurring_schedules(guid) ON DELETE CASCADE,
	auto_post INTEGER NOT NULL DEFAULT 0,
	spread_total INTEGER NOT NULL DEFAULT 0
);

-- Finished reconciliation batches. Persisting the membership (not inferring it
-- from transaction dates) is what lets Undo last statement reopen exactly the
-- splits a Finish promoted, even when one was dated before the previous
-- batch's last transaction. Splits reconciled outside Spendo (imported books)
-- have no batch and can't be reopened. Rows are deleted by reopen; ON DELETE
-- CASCADE clears mappings.
CREATE TABLE IF NOT EXISTS reconcile_batches (
	guid TEXT PRIMARY KEY,
	account_guid TEXT NOT NULL REFERENCES accounts(guid) ON DELETE CASCADE,
	finished_at TEXT NOT NULL,
	through_date TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reconcile_batches_account ON reconcile_batches(account_guid, finished_at);

CREATE TABLE IF NOT EXISTS reconcile_batch_splits (
	batch_guid TEXT NOT NULL REFERENCES reconcile_batches(guid) ON DELETE CASCADE,
	split_guid TEXT NOT NULL REFERENCES splits(guid) ON DELETE CASCADE,
	PRIMARY KEY (batch_guid, split_guid)
);

-- Saved CSV import mapping presets, keyed by a hash of the normalized header
-- row. Book-scoped convenience config, dropped by `clear_db` like any other
-- table.
CREATE TABLE IF NOT EXISTS csv_import_presets (
	signature TEXT PRIMARY KEY,
	name TEXT NOT NULL,
	mapping_json TEXT NOT NULL,
	updated_at TEXT NOT NULL
);

-- Description-match rules that pick the category (offset account) for a CSV
-- import row. User config, so account deletion is guarded instead of
-- cascading silently.
CREATE TABLE IF NOT EXISTS import_rules (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	priority INTEGER NOT NULL,
	match_type TEXT NOT NULL,
	pattern TEXT NOT NULL,
	account_guid TEXT NOT NULL REFERENCES accounts(guid) ON DELETE CASCADE,
	created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_import_rules_priority ON import_rules(priority, id);

-- Target allocation per commodity for the portfolio rebalance calculator.
-- Book-scoped user config like `import_rules`. A target dies with its
-- commodity through the FK cascade, e.g. when the GnuCash orphan sweep drops
-- one.
CREATE TABLE IF NOT EXISTS allocation_targets (
	commodity_guid TEXT PRIMARY KEY REFERENCES commodities(guid) ON DELETE CASCADE,
	target_pct REAL NOT NULL
);

-- Income accounts whose postings are dividends of one holding, for books
-- that don't record the GnuCash zero-amount split on the stock account.
-- User config like `import_rules`: account deletion and the GnuCash orphan
-- sweep are guarded instead of cascading silently.
CREATE TABLE IF NOT EXISTS dividend_links (
	account_guid TEXT PRIMARY KEY REFERENCES accounts(guid) ON DELETE CASCADE,
	commodity_guid TEXT NOT NULL REFERENCES commodities(guid) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_dividend_links_commodity ON dividend_links(commodity_guid);

-- Free-form labels on whole transactions (a trip, a project, deductible).
-- Names are unique ignoring case. A tag outlives its last transaction until
-- it is deleted from Settings; links die with either side (FK cascade, so the
-- GnuCash orphan sweep and transaction deletes clean them up).
CREATE TABLE IF NOT EXISTS tags (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL UNIQUE COLLATE NOCASE,
	created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS transaction_tags (
	tx_guid TEXT NOT NULL REFERENCES transactions(guid) ON DELETE CASCADE,
	tag_guid TEXT NOT NULL REFERENCES tags(guid) ON DELETE CASCADE,
	PRIMARY KEY (tx_guid, tag_guid)
);

CREATE INDEX IF NOT EXISTS idx_transaction_tags_tag ON transaction_tags(tag_guid);

-- Who a transaction was with (a shop, a person, an employer); at most one per
-- transaction. Kept beside `transactions` like `schedule_skips` so the core
-- row keeps its GnuCash-shaped columns and older books pick this up on open.
-- Names are unique ignoring case; links die with either side (FK cascade, so
-- deleting a payee only clears it from its transactions).
CREATE TABLE IF NOT EXISTS payees (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL UNIQUE COLLATE NOCASE,
	created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS transaction_payees (
	tx_guid TEXT PRIMARY KEY REFERENCES transactions(guid) ON DELETE CASCADE,
	payee_guid TEXT NOT NULL REFERENCES payees(guid) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_transaction_payees_payee ON transaction_payees(payee_guid);

-- Tax role per account for the tax summary; descendants inherit the nearest
-- ancestor's role. User config like `import_rules`: account deletion and the
-- GnuCash orphan sweep are guarded instead of cascading silently.
CREATE TABLE IF NOT EXISTS tax_roles (
	account_guid TEXT PRIMARY KEY REFERENCES accounts(guid) ON DELETE CASCADE,
	role TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS app_settings (
	key TEXT PRIMARY KEY,
	value TEXT NOT NULL
);
";

/// No migrations by design (pre-release): the schema is the single source of
/// truth in `SCHEMA`. Schema changes require a database reset via `clear_db`
/// (Settings → Clear all data) or by deleting the book file.
pub fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)
}

pub fn connect_db(db_path: &PathBuf) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    configure_connection(&conn, true)?;
    Ok(conn)
}

/// Shared connection configuration: FK enforcement on every connection, WAL
/// only for file-backed books (in-memory DBs have no sidecars), and a busy
/// timeout so a second process or tool touching the file waits instead of
/// failing instantly with `database is locked`.
fn configure_connection(conn: &Connection, file_backed: bool) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    if file_backed {
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    }
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

/// Fresh in-memory placeholder book (schema, no rows) for the book-less state:
/// startup before any New/Open, and `books::close_book`.
pub fn open_placeholder() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure_connection(&conn, false)?;
    init_db(&conn)?;
    Ok(conn)
}

/// Open a GnuCash source file strictly read-only: never enables WAL or writes
/// anything next to the user's original book.
pub fn connect_source_db(db_path: &PathBuf) -> Result<Connection> {
    Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
}

/// Stamp a freshly created or adopted book with our `application_id` magic so
/// `books::validate_spendo_db` recognizes it on later opens. Cheap and
/// idempotent; never fails an open if the PRAGMA itself errors.
pub fn stamp_app_id(conn: &Connection) {
    conn.execute_batch(&format!("PRAGMA application_id = {SPENDO_APP_ID};"))
        .ok();
}

/// Best-effort WAL checkpoint before a book switch or `VACUUM INTO`: flushes
/// `-wal`/`-shm` sidecars back into the main file so no data lingers next to
/// the old book. Never fails the caller (e.g. in-memory placeholder has no WAL).
pub fn checkpoint(conn: &Connection) {
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").ok();
}

/// Path of the sidecar remembering the last active book across restarts.
pub fn last_book_file(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("last_book.json")
}

/// Read the sidecar; missing/corrupt content means "no remembered book", never
/// an error — startup falls back to the welcome screen instead of crashing.
pub fn read_last_book(app_data_dir: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(last_book_file(app_data_dir)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let path = value.get("path")?.as_str()?;
    if path.is_empty() {
        return None;
    }
    Some(PathBuf::from(path))
}

/// Remember the active book. Called after every successful open/create/save-as;
/// a failure here must not fail the switch, so callers log and continue.
pub fn write_last_book(app_data_dir: &Path, path: &Path) -> Result<(), AppError> {
    let body = serde_json::json!({ "path": path.to_string_lossy() });
    std::fs::write(last_book_file(app_data_dir), body.to_string())
        .map_err(|e| AppError::Io(format!("Failed to remember open book: {}", e)))
}

/// Forget the active book (e.g. its file vanished). Never errors.
pub fn clear_last_book(app_data_dir: &Path) {
    std::fs::remove_file(last_book_file(app_data_dir)).ok();
}

/// Full database reset: drops every app table and recreates the current
/// `SCHEMA`. This is the supported upgrade path while there are no
/// migrations — Settings → Clear all data, then re-import. The `change_log` /
/// `change_snapshots` drops purge the audit log that pre-removal books still
/// carry; `manual_prices` is the same kind of leftover from early builds.
pub fn clear_db(conn: &Connection) -> Result<()> {
    let drops = conn.execute_batch(
        "PRAGMA foreign_keys = OFF;
          DROP TABLE IF EXISTS attachments;
          DROP TABLE IF EXISTS allocation_targets;
          DROP TABLE IF EXISTS dividend_links;
          DROP TABLE IF EXISTS tax_roles;
          DROP TABLE IF EXISTS transaction_tags;
          DROP TABLE IF EXISTS tags;
          DROP TABLE IF EXISTS transaction_payees;
          DROP TABLE IF EXISTS payees;
          DROP TABLE IF EXISTS budgets;
          DROP TABLE IF EXISTS csv_import_presets;
          DROP TABLE IF EXISTS import_rules;
          DROP TABLE IF EXISTS reconcile_batch_splits;
          DROP TABLE IF EXISTS reconcile_batches;
          DROP TABLE IF EXISTS prices;
          DROP TABLE IF EXISTS splits;
          DROP TABLE IF EXISTS transactions;
          DROP TABLE IF EXISTS accounts;
          DROP TABLE IF EXISTS commodities;
          DROP TABLE IF EXISTS change_snapshots;
          DROP TABLE IF EXISTS change_log;
          DROP TABLE IF EXISTS manual_prices;
           DROP TABLE IF EXISTS refresh_log;
           DROP TABLE IF EXISTS symbol_overrides;
           DROP TABLE IF EXISTS schedule_skips;
           DROP TABLE IF EXISTS schedule_options;
           DROP TABLE IF EXISTS recurring_schedules;
           DROP TABLE IF EXISTS app_settings;",
    );
    // Restore foreign keys on every path: a failed drop must not leave the
    // live connection with FK enforcement off (orphan handling depends on real
    // FK errors).
    let restore = conn.execute_batch("PRAGMA foreign_keys = ON;");
    drops?;
    restore?;
    init_db(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_serializes_kind_and_message() {
        let cases = [
            (AppError::NotFound("gone".into()), "notFound"),
            (AppError::Validation("bad".into()), "validation"),
            (AppError::Io("disk".into()), "io"),
            (AppError::Internal("bug".into()), "internal"),
            (AppError::from(rusqlite::Error::QueryReturnedNoRows), "db"),
        ];
        for (err, kind) in cases {
            let message = err.to_string();
            assert_eq!(
                serde_json::to_value(&err).unwrap(),
                serde_json::json!({ "kind": kind, "message": message })
            );
        }
    }

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn unique_error_matches_extended_code() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE u (x TEXT UNIQUE); INSERT INTO u VALUES ('a');")
            .unwrap();
        let err = AppError::from(conn.execute("INSERT INTO u VALUES ('a')", []).unwrap_err());
        assert!(matches!(err, AppError::Db(ref e) if e.is_unique() && !e.is_foreign_key()));
    }

    #[test]
    fn fk_error_matches_extended_code_not_message() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE fk_parent (id INTEGER PRIMARY KEY);
             CREATE TABLE fk_child (id INTEGER REFERENCES fk_parent(id));
             INSERT INTO fk_parent VALUES (1);
             INSERT INTO fk_child VALUES (1);",
        )
        .unwrap();
        // Real foreign-key violation: deleting the referenced parent.
        let fk_err = conn
            .execute("DELETE FROM fk_parent WHERE id = 1", [])
            .unwrap_err();
        // Other constraint failures must NOT classify as FK violations
        // (an old substring match misclassified them as FK errors).
        let unique_err = conn
            .execute("INSERT INTO fk_parent VALUES (1)", [])
            .unwrap_err();

        // AppError preserves the code through conversion for typed matching.
        let app_err = AppError::from(fk_err);
        assert!(matches!(app_err, AppError::Db(ref e) if e.is_foreign_key()));
        let app_unique = AppError::from(unique_err);
        assert!(matches!(app_unique, AppError::Db(ref e) if !e.is_foreign_key()));
        assert_eq!(
            app_unique.to_string(),
            "UNIQUE constraint failed: fk_parent.id"
        );
    }

    #[test]
    fn with_tx_commits_and_rolls_back() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        with_tx_err(&conn, || -> Result<(), AppError> {
            conn.execute(
                "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c1', 'EUR', 'CURRENCY')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM commodities", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let err = with_tx_err(&conn, || -> Result<(), AppError> {
            conn.execute(
                "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c2', 'USD', 'CURRENCY')",
                [],
            )?;
            Err(AppError::Validation("boom".to_string()))
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "boom");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM commodities WHERE guid = 'c2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn init_creates_current_schema() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        // Second run is idempotent.
        init_db(&conn).unwrap();

        let tables = table_names(&conn);
        for expected in [
            "accounts",
            "allocation_targets",
            "app_settings",
            "attachments",
            "budgets",
            "commodities",
            "payees",
            "prices",
            "recurring_schedules",
            "reconcile_batch_splits",
            "reconcile_batches",
            "refresh_log",
            "schedule_options",
            "schedule_skips",
            "splits",
            "symbol_overrides",
            "tags",
            "tax_roles",
            "transaction_payees",
            "transaction_tags",
            "transactions",
        ] {
            assert!(tables.contains(&expected.to_string()), "missing {expected}");
        }

        let indexes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_budgets_account_year'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(indexes, 1);
    }

    #[test]
    fn clear_resets_data_and_schema() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();
        // Attachment bytes must not survive a reset: their parent transaction
        // is gone and the drop list must include the new table.
        conn.execute_batch(
            "INSERT INTO transactions (guid, post_date) VALUES ('t-reset', '2024-01-01');
             INSERT INTO attachments (guid, tx_guid, filename, mime_type, byte_size, data, created_at)
             VALUES ('a-reset', 't-reset', 'receipt.pdf', 'application/pdf', 1, X'00', '2024-01-01T00:00:00Z');",
        )
        .unwrap();
        // Book-scoped config (allocation targets) must not survive a reset
        // either, or a later import could resurrect the old GUID and apply a
        // target the user already cleared.
        conn.execute(
            "INSERT INTO allocation_targets (commodity_guid, target_pct) VALUES ('c-eur', 50)",
            [],
        )
        .unwrap();

        // Tags and tax roles are book config too.
        conn.execute_batch(
            "INSERT INTO tags (guid, name, created_at) VALUES ('g-reset', 'trip', '2024-01-01T00:00:00Z');
             INSERT INTO transaction_tags (tx_guid, tag_guid) VALUES ('t-reset', 'g-reset');
             INSERT INTO payees (guid, name, created_at) VALUES ('p-reset', 'Renfe', '2024-01-01T00:00:00Z');
             INSERT INTO transaction_payees (tx_guid, payee_guid) VALUES ('t-reset', 'p-reset');",
        )
        .unwrap();

        clear_db(&conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM commodities", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let tags: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tags, 0);
        let payees: i64 = conn
            .query_row("SELECT COUNT(*) FROM payees", [], |r| r.get(0))
            .unwrap();
        assert_eq!(payees, 0);
        let attachments: i64 = conn
            .query_row("SELECT COUNT(*) FROM attachments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(attachments, 0);
        let targets: i64 = conn
            .query_row("SELECT COUNT(*) FROM allocation_targets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(targets, 0);
        // Foreign-key enforcement must survive the reset on every path.
        let fk_on: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk_on, 1);
        // Schema is usable again right after the reset.
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-usd', 'USD', 'CURRENCY')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn clear_purges_leftover_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        let fresh = table_names(&conn);
        // Tables older builds created and the current SCHEMA no longer has.
        conn.execute_batch(
            "CREATE TABLE change_log (id INTEGER PRIMARY KEY);
             CREATE TABLE change_snapshots (entry_id INTEGER);
             CREATE TABLE manual_prices (commodity_guid TEXT, date TEXT);",
        )
        .unwrap();

        clear_db(&conn).unwrap();

        assert_eq!(table_names(&conn), fresh);
    }

    #[test]
    fn stamp_sets_application_id() {
        let conn = Connection::open_in_memory().unwrap();
        stamp_app_id(&conn);
        let id: i32 = conn
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .unwrap();
        assert_eq!(id, SPENDO_APP_ID);
    }

    #[test]
    fn placeholder_has_schema_and_no_rows() {
        let conn = open_placeholder().unwrap();
        let tables = table_names(&conn);
        assert!(tables.contains(&"accounts".to_string()));
        assert!(tables.contains(&"transactions".to_string()));
        let accounts: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(accounts, 0);
    }

    #[test]
    fn last_book_sidecar_round_trips() {
        let dir = std::env::temp_dir().join(format!("spendo-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // Missing sidecar means "no remembered book", not an error.
        assert_eq!(read_last_book(&dir), None);

        let book = dir.join("my.spendo");
        write_last_book(&dir, &book).unwrap();
        assert_eq!(read_last_book(&dir), Some(book.clone()));

        clear_last_book(&dir);
        assert_eq!(read_last_book(&dir), None);

        // Corrupt content also falls back to None.
        std::fs::write(last_book_file(&dir), "not json{").unwrap();
        assert_eq!(read_last_book(&dir), None);
        std::fs::write(last_book_file(&dir), r#"{"path":""}"#).unwrap();
        assert_eq!(read_last_book(&dir), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ensure_same_book_rejects_stale_generation() {
        let generation = AtomicU64::new(7);
        assert!(ensure_same_book(&generation, 7).is_ok());
        generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(ensure_same_book(&generation, 7).is_err());
    }
}
