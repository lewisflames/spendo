use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    /// Absolute path of the active book file. `None` while on the in-memory
    /// placeholder (no book opened or created yet) — see `books::fetch_active_book`.
    pub db_path: Mutex<Option<PathBuf>>,
    /// Tauri app-data dir: legacy `spendo.db` location + `last_book.json` sidecar.
    pub app_data_dir: PathBuf,
}

/// Magic stamped via `PRAGMA application_id` on every Spendo book so
/// `books::validate_spendo_db` can tell our files apart from GnuCash source
/// books (which share table names like `accounts`/`splits`) and random SQLite.
pub const SPENDO_APP_ID: i32 = 0x5350_4e44; // 'SPND'

/// User-data tables shared by the history snapshots and the import orphan
/// sweep. Interpolating table names into SQL is only safe through this
/// allowlist — never inline a table name without `validate_data_table`.
pub(crate) const DATA_TABLES: &[&str] = &["commodities", "accounts", "transactions", "splits"];

pub(crate) fn validate_data_table(table: &str) -> Result<(), String> {
    if DATA_TABLES.contains(&table) {
        Ok(())
    } else {
        Err(format!("Invalid table name: {table}"))
    }
}

/// Run a closure with the DB connection on a blocking thread. Every DB-touching
/// command must go through here so the tokio runtime threads are never blocked
/// while the global mutex is held.
pub async fn run_blocking<T, F>(db: Arc<Mutex<Connection>>, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db.lock().map_err(|e| e.to_string())?;
        f(&conn)
    })
    .await
    .map_err(|e| format!("Background task failed: {}", e))?
}

/// Run a mutable closure with the DB connection on a blocking thread. This is
/// the swap-path counterpart of [`run_blocking`]: book switching must replace
/// the `Connection` itself (`*conn = new_conn`), which needs `&mut`. Same
/// rule applies — callers must do file I/O (validate, connect new file) before
/// entering, and never lock directly inside an `async fn`.
pub async fn run_swap_blocking<T, F>(db: Arc<Mutex<Connection>>, f: F) -> Result<T, String>
where
    F: FnOnce(&mut Connection) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = db.lock().map_err(|e| e.to_string())?;
        f(&mut conn)
    })
    .await
    .map_err(|e| format!("Background task failed: {}", e))?
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

CREATE TABLE IF NOT EXISTS change_log (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	occurred_at TEXT NOT NULL,
	kind TEXT NOT NULL,
	label TEXT NOT NULL,
	source TEXT NOT NULL,
	transactions_affected INTEGER DEFAULT 0,
	accounts_affected INTEGER DEFAULT 0,
	detail_json TEXT,
	reverts_id INTEGER REFERENCES change_log(id),
	reverted_by INTEGER REFERENCES change_log(id)
);

CREATE TABLE IF NOT EXISTS change_snapshots (
	entry_id INTEGER NOT NULL REFERENCES change_log(id) ON DELETE CASCADE,
	table_name TEXT NOT NULL,
	row_guid TEXT NOT NULL,
	before_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_change_log_occurred ON change_log(occurred_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_snapshots_entry ON change_snapshots(entry_id);

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

CREATE TABLE IF NOT EXISTS app_settings (
	key TEXT PRIMARY KEY,
	value TEXT NOT NULL
);
";

pub fn get_db_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("spendo.db")
}

/// No migrations by design (pre-release): the schema is the single source of
/// truth in `SCHEMA`. Schema changes require a database reset via
/// `clear_db` (Settings → Clear all data) or by deleting `spendo.db`.
pub fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)
}

pub fn connect_db(db_path: &PathBuf) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
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
/// an error — startup falls back to the legacy `spendo.db` or the welcome
/// screen instead of crashing.
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
pub fn write_last_book(app_data_dir: &Path, path: &Path) -> Result<(), String> {
    let body = serde_json::json!({ "path": path.to_string_lossy() });
    std::fs::write(last_book_file(app_data_dir), body.to_string())
        .map_err(|e| format!("Failed to remember open book: {}", e))
}

/// Forget the active book (e.g. its file vanished). Never errors.
pub fn clear_last_book(app_data_dir: &Path) {
    std::fs::remove_file(last_book_file(app_data_dir)).ok();
}

/// Full database reset: drops every app table (including the legacy
/// `budget_amounts` table from pre-release builds and the pre-history
/// `import_log` table) and recreates the current `SCHEMA`. This is the
/// supported upgrade path while there are no migrations — Settings →
/// Clear all data, then re-import.
pub fn clear_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(
         "PRAGMA foreign_keys = OFF;
          DROP TABLE IF EXISTS budget_amounts;
          DROP TABLE IF EXISTS budgets;
          DROP TABLE IF EXISTS prices;
          DROP TABLE IF EXISTS splits;
          DROP TABLE IF EXISTS transactions;
          DROP TABLE IF EXISTS accounts;
          DROP TABLE IF EXISTS commodities;
          DROP TABLE IF EXISTS change_snapshots;
          DROP TABLE IF EXISTS change_log;
          DROP TABLE IF EXISTS import_log;
          DROP TABLE IF EXISTS refresh_log;
          DROP TABLE IF EXISTS symbol_overrides;
          DROP TABLE IF EXISTS app_settings;
          PRAGMA foreign_keys = ON;",
    )?;
    init_db(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn init_creates_current_schema() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        // Second run is idempotent.
        init_db(&conn).unwrap();

        let tables = table_names(&conn);
        for expected in [
            "accounts",
            "app_settings",
            "budgets",
            "change_log",
            "change_snapshots",
            "commodities",
            "prices",
            "refresh_log",
            "splits",
            "symbol_overrides",
            "transactions",
        ] {
            assert!(tables.contains(&expected.to_string()), "missing {expected}");
        }
        // No legacy tables linger on fresh installs.
        assert!(!tables.contains(&"budget_amounts".to_string()));
        assert!(!tables.contains(&"import_log".to_string()));

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
    fn clear_resets_data_and_legacy_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        // Legacy tables from pre-release builds must not survive a reset.
        conn.execute_batch(
            "CREATE TABLE budget_amounts (budget_guid TEXT NOT NULL, year INTEGER NOT NULL);
             CREATE TABLE import_log (id INTEGER PRIMARY KEY AUTOINCREMENT, filename TEXT NOT NULL);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();

        clear_db(&conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM commodities", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(!table_names(&conn).contains(&"budget_amounts".to_string()));
        assert!(!table_names(&conn).contains(&"import_log".to_string()));
        // Schema is usable again right after the reset.
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-usd', 'USD', 'CURRENCY')",
            [],
        )
        .unwrap();
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
}
