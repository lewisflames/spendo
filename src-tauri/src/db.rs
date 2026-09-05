use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    /// Absolute path of the app database file (for display / reveal-in-folder).
    pub db_path: PathBuf,
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

CREATE TABLE IF NOT EXISTS import_log (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	filename TEXT NOT NULL,
	file_path TEXT,
	imported_at TEXT NOT NULL,
	sha256 TEXT,
	accounts_added INTEGER DEFAULT 0,
	transactions_added INTEGER DEFAULT 0,
	splits_added INTEGER DEFAULT 0,
	commodities_added INTEGER DEFAULT 0,
	accounts_orphaned INTEGER DEFAULT 0,
	transactions_orphaned INTEGER DEFAULT 0,
	splits_orphaned INTEGER DEFAULT 0,
	accounts_updated INTEGER DEFAULT 0,
	transactions_updated INTEGER DEFAULT 0,
	splits_updated INTEGER DEFAULT 0,
	commodities_orphaned INTEGER DEFAULT 0
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

/// Full database reset: drops every app table (including the legacy
/// `budget_amounts` table from pre-release builds) and recreates the current
/// `SCHEMA`. This is the supported upgrade path while there are no
/// migrations — Settings → Clear all data, then re-import.
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
         DROP TABLE IF EXISTS import_log;
         DROP TABLE IF EXISTS refresh_log;
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
            "budgets",
            "commodities",
            "import_log",
            "prices",
            "refresh_log",
            "splits",
            "transactions",
        ] {
            assert!(tables.contains(&expected.to_string()), "missing {expected}");
        }
        // No legacy tables linger on fresh installs.
        assert!(!tables.contains(&"budget_amounts".to_string()));

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
        // Legacy table from pre-release builds must not survive a reset.
        conn.execute_batch(
            "CREATE TABLE budget_amounts (budget_guid TEXT NOT NULL, year INTEGER NOT NULL);",
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
        // Schema is usable again right after the reset.
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-usd', 'USD', 'CURRENCY')",
            [],
        )
        .unwrap();
    }
}
