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
	period_type TEXT NOT NULL DEFAULT 'MONTHLY',
	amount REAL NOT NULL,
	rollover INTEGER NOT NULL DEFAULT 0,
	rollover_start TEXT,
	notes TEXT,
	created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS budget_amounts (
	budget_guid TEXT NOT NULL REFERENCES budgets(guid) ON DELETE CASCADE,
	year INTEGER NOT NULL,
	month INTEGER,
	amount REAL NOT NULL,
	PRIMARY KEY (budget_guid, year, month)
);

CREATE TABLE IF NOT EXISTS refresh_log (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	refreshed_at TEXT NOT NULL
);


";

pub fn get_db_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("spendo.db")
}

/// Columns added after the initial release. `SCHEMA` already includes them for
/// fresh installs; these migrations only apply to pre-existing databases.
const COLUMN_MIGRATIONS: &[(&str, &str)] = &[
    ("import_log", "file_path"),
    ("budgets", "rollover_start"),
    ("transactions", "num"),
    ("splits", "memo"),
    ("transactions", "notes"),
];

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for col in cols {
        if col? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate(conn: &Connection) -> Result<()> {
    for (table, column) in COLUMN_MIGRATIONS {
        if !column_exists(conn, table, column)? {
            conn.execute_batch(&format!("ALTER TABLE {} ADD COLUMN {}", table, column))?;
        }
    }
    seed_budget_amounts(conn)?;
    convert_yearly_budgets(conn)
}

/// One-time backfill of the old single `budgets.amount` into per-period
/// `budget_amounts` rows (every month/year from creation until now), so
/// pre-existing budgets keep applying and the carryover window has meaningful
/// entries. Guarded by `PRAGMA user_version` so it runs exactly once even
/// though `budgets.amount` stays in the schema (SQLite cannot drop columns
/// without a table rebuild).
fn seed_budget_amounts(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 1 {
        return Ok(());
    }
    conn.execute_batch(
        "INSERT OR IGNORE INTO budget_amounts (budget_guid, year, month, amount)
         WITH RECURSIVE months(n, budget_guid, amount, end_n) AS (
             SELECT CAST(substr(created_at, 1, 4) AS INTEGER) * 12
                        + CAST(substr(created_at, 6, 2) AS INTEGER) - 1,
                    guid, amount,
                    CAST(strftime('%Y', 'now') AS INTEGER) * 12
                        + CAST(strftime('%m', 'now') AS INTEGER) - 1
             FROM budgets
             WHERE period_type = 'MONTHLY'
             UNION ALL
             SELECT n + 1, budget_guid, amount, end_n FROM months WHERE n < end_n
         )
         SELECT budget_guid, n / 12, (n % 12) + 1, amount FROM months;

         INSERT OR IGNORE INTO budget_amounts (budget_guid, year, month, amount)
         WITH RECURSIVE years(y, budget_guid, amount, end_y) AS (
             SELECT CAST(substr(created_at, 1, 4) AS INTEGER), guid, amount,
                    CAST(strftime('%Y', 'now') AS INTEGER)
             FROM budgets
             WHERE period_type = 'YEARLY'
             UNION ALL
             SELECT y + 1, budget_guid, amount, end_y FROM years WHERE y < end_y
         )
         SELECT budget_guid, y, NULL, amount FROM years;

         PRAGMA user_version = 1;",
    )?;
    Ok(())
}

/// One-time conversion of the removed YEARLY budget period type: every yearly
/// budget becomes MONTHLY and each whole-year entry (month NULL) is replaced
/// by 12 monthly entries of `amount / 12`. Guarded by `PRAGMA user_version` so
/// it runs exactly once.
fn convert_yearly_budgets(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 2 {
        return Ok(());
    }
    conn.execute_batch(
        "UPDATE budgets SET period_type = 'MONTHLY' WHERE period_type = 'YEARLY';

         INSERT OR IGNORE INTO budget_amounts (budget_guid, year, month, amount)
         SELECT ba.budget_guid, ba.year, m.m, ROUND(ba.amount / 12.0, 2)
         FROM budget_amounts ba
         CROSS JOIN (SELECT 1 m UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4
                     UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8
                     UNION ALL SELECT 9 UNION ALL SELECT 10 UNION ALL SELECT 11 UNION ALL SELECT 12) m
         WHERE ba.month IS NULL;

         DELETE FROM budget_amounts WHERE month IS NULL;

         PRAGMA user_version = 2;",
    )?;
    Ok(())
}

pub fn init_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    migrate(conn)
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

pub fn clear_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "BEGIN;
         DELETE FROM budget_amounts;
         DELETE FROM budgets;
         DELETE FROM prices;
         DELETE FROM splits;
         DELETE FROM transactions;
         DELETE FROM accounts;
         DELETE FROM commodities;
         DELETE FROM import_log;
         DELETE FROM refresh_log;
         COMMIT;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pre-v2 database with a YEARLY budget must be converted to monthly
    /// entries (amount ÷ 12 per month) on migrate.
    #[test]
    fn convert_yearly_budgets_to_monthly_entries() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute_batch("PRAGMA user_version = 1").unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
                    ('food', 'Food', 'EXPENSE', 'root', 0, 'c-eur'),
                    ('groceries', 'Groceries', 'EXPENSE', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        // One YEARLY budget (whole-year entry) and one already-MONTHLY budget.
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, period_type, amount, rollover, notes, created_at)
             VALUES ('b-year', 'food', 'YEARLY', 1200, 0, NULL, '2024-01-01T00:00:00Z'),
                    ('b-month', 'groceries', 'MONTHLY', 0, 0, NULL, '2024-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO budget_amounts (budget_guid, year, month, amount)
             VALUES ('b-year', 2024, NULL, 1200),
                    ('b-month', 2024, 1, 60),
                    ('b-month', 2024, 2, 70)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let pt: String = conn
            .query_row("SELECT period_type FROM budgets WHERE guid = 'b-year'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(pt, "MONTHLY");

        let year: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM budget_amounts WHERE budget_guid = 'b-year'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(year, 12);
        let total: f64 = conn
            .query_row(
                "SELECT SUM(amount) FROM budget_amounts WHERE budget_guid = 'b-year'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!((total - 1200.0).abs() < 0.001, "total {total}");
        // No whole-year rows may survive, and monthly budgets are untouched.
        let nulls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM budget_amounts WHERE month IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(nulls, 0);
        let monthly: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM budget_amounts WHERE budget_guid = 'b-month'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(monthly, 2);

        // Second run is a no-op.
        migrate(&conn).unwrap();
        let year: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM budget_amounts WHERE budget_guid = 'b-year'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(year, 12);
    }
}
