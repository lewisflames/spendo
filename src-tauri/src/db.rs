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
	notes TEXT,
	created_at TEXT NOT NULL,
	UNIQUE (account_guid, year)
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
/// (Budget dormant columns are removed by the v3 yearly migration instead.)
const COLUMN_MIGRATIONS: &[(&str, &str)] = &[
    ("import_log", "file_path"),
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
    convert_yearly_budgets(conn)?;
    migrate_monthly_to_yearly_budgets(conn)?;
    migrate_default_override_to_per_year(conn)?;
    // Created here (not in SCHEMA) so pre-v4 databases — whose `budgets`
    // table has no `year` column yet — don't fail on startup before the
    // migration runs. The v4 migration creates it as well when rebuilding.
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_budgets_account_year ON budgets(account_guid, year);",
    )
}

/// One-time backfill of the old single `budgets.amount` into per-period
/// `budget_amounts` rows (legacy monthly schema). No-op on the yearly schema
/// (fresh installs): just bumps `user_version` so the step never re-runs.
fn seed_budget_amounts(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 1 {
        return Ok(());
    }
    if !column_exists(conn, "budgets", "period_type")?
        || !column_exists(conn, "budget_amounts", "month")?
    {
        conn.execute_batch("PRAGMA user_version = 1;")?;
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

/// One-time conversion of the removed YEARLY budget period type (legacy
/// monthly schema). No-op on the yearly schema: just bumps `user_version`.
fn convert_yearly_budgets(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 2 {
        return Ok(());
    }
    if !column_exists(conn, "budgets", "period_type")?
        || !column_exists(conn, "budget_amounts", "month")?
    {
        conn.execute_batch("PRAGMA user_version = 2;")?;
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

/// v3: monthly budgets → yearly budgets.
///
/// - `budgets.amount` (monthly default) becomes the annual default (`× 12`).
/// - Each year's 12 effective monthly amounts (override wins, else the old
///   monthly default) are summed into one yearly override row — but only when
///   the sum differs from the new annual default, otherwise the year falls
///   back to the default and needs no row.
/// - Tables are rebuilt to the yearly schema: `budgets` loses the dormant
///   `period_type / rollover / rollover_start` columns and `budget_amounts`
///   loses `month` (new PK is `(budget_guid, year)`).
/// - Carryover equivalence is exact: the sum of 12 monthly
///   `(budget − actual)` equals the yearly `(annual − annual actual)`.
/// Guarded by `PRAGMA user_version` so it runs exactly once. No-op when the
/// yearly schema is already in place (fresh installs).
fn migrate_monthly_to_yearly_budgets(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 3 {
        return Ok(());
    }
    // Yearly schema already in place (fresh install): nothing to convert.
    if !column_exists(conn, "budget_amounts", "month")? {
        conn.execute_batch("PRAGMA user_version = 3;")?;
        return Ok(());
    }

    // Read monthly defaults and monthly entries with the legacy schema.
    let budgets: Vec<(String, f64)> = {
        let mut stmt = conn.prepare("SELECT guid, amount FROM budgets")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut monthly: std::collections::HashMap<(String, i32, i32), f64> =
        std::collections::HashMap::new();
    // `month` is NOT NULL in practice post-v2, but tolerate NULL rows.
    {
        let mut stmt =
            conn.prepare("SELECT budget_guid, year, month, amount FROM budget_amounts")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, Option<i32>>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;
        for row in rows {
            let (guid, year, month, amount) = row?;
            if let Some(m) = month {
                monthly.insert((guid, year, m), amount);
            }
        }
    }
    // Years present per budget (only years with at least one monthly row can
    // produce a yearly override; other years fall back to the new default).
    let mut years_per_budget: std::collections::HashMap<String, std::collections::HashSet<i32>> =
        std::collections::HashMap::new();
    for (guid, year, _) in monthly.keys().map(|(g, y, m)| (g.clone(), *y, *m)) {
        years_per_budget.entry(guid).or_default().insert(year);
    }

    // Annual defaults + yearly overrides to write.
    let mut annual_defaults: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let mut yearly_overrides: Vec<(String, i32, f64)> = Vec::new();
    for (guid, monthly_default) in &budgets {
        let annual = monthly_default * 12.0;
        annual_defaults.insert(guid.clone(), annual);
        if let Some(years) = years_per_budget.get(guid) {
            for year in years {
                let mut sum = 0.0;
                for m in 1..=12 {
                    sum += monthly
                        .get(&(guid.clone(), *year, m))
                        .copied()
                        .unwrap_or(*monthly_default);
                }
                if (sum - annual).abs() > 0.005 {
                    yearly_overrides.push((guid.clone(), *year, (sum * 100.0).round() / 100.0));
                }
            }
        }
    }

    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    // Update monthly defaults to annual defaults before rebuilding.
    for (guid, annual) in &annual_defaults {
        conn.execute(
            "UPDATE budgets SET amount = ?1 WHERE guid = ?2",
            rusqlite::params![annual, guid],
        )?;
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS budgets_new (
            guid TEXT PRIMARY KEY,
            account_guid TEXT NOT NULL REFERENCES accounts(guid) ON DELETE CASCADE,
            amount REAL NOT NULL,
            notes TEXT,
            created_at TEXT NOT NULL
        );
        INSERT OR REPLACE INTO budgets_new (guid, account_guid, amount, notes, created_at)
            SELECT guid, account_guid, amount, notes, created_at FROM budgets;
        DROP TABLE budgets;
        ALTER TABLE budgets_new RENAME TO budgets;

        CREATE TABLE IF NOT EXISTS budget_amounts_new (
            budget_guid TEXT NOT NULL REFERENCES budgets(guid) ON DELETE CASCADE,
            year INTEGER NOT NULL,
            amount REAL NOT NULL,
            PRIMARY KEY (budget_guid, year)
        );",
    )?;
    for (guid, year, amount) in &yearly_overrides {
        conn.execute(
            "INSERT OR REPLACE INTO budget_amounts_new (budget_guid, year, amount)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![guid, year, amount],
        )?;
    }
    conn.execute_batch(
        "DROP TABLE budget_amounts;
        ALTER TABLE budget_amounts_new RENAME TO budget_amounts;
        PRAGMA foreign_keys = ON;
        PRAGMA user_version = 3;",
    )?;
    Ok(())
}

/// v4: default+override budgets → per-year-only budgets.
///
/// Only per-year override rows survive (each inherits its parent budget's
/// notes and creation timestamp); annual defaults are dropped, so
/// default-only accounts end up with no budgets at all. Years without a row
/// simply have no budget. Guarded by `PRAGMA user_version` so it runs exactly
/// once. No-op when the per-year schema is already in place (fresh installs).
fn migrate_default_override_to_per_year(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 4 {
        return Ok(());
    }
    // Per-year schema already in place (fresh install): nothing to convert.
    if column_exists(conn, "budgets", "year")? {
        conn.execute_batch("PRAGMA user_version = 4;")?;
        return Ok(());
    }

    // Every override year becomes a per-year row. The new guid is derived
    // from the old `(budget_guid, year)` primary key, which is unique.
    let overrides: Vec<(String, String, i32, f64, Option<String>, String)> = {
        let table: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'budget_amounts'",
            [],
            |row| row.get(0),
        )?;
        if table == 0 {
            Vec::new()
        } else {
            let mut stmt = conn.prepare(
                "SELECT b.guid, b.account_guid, ba.year, ba.amount, b.notes, b.created_at
                 FROM budget_amounts ba
                 JOIN budgets b ON b.guid = ba.budget_guid",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i32>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        }
    };

    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    conn.execute_batch(
        "DROP TABLE IF EXISTS budget_amounts;
         DROP TABLE budgets;
         CREATE TABLE budgets (
             guid TEXT PRIMARY KEY,
             account_guid TEXT NOT NULL REFERENCES accounts(guid) ON DELETE CASCADE,
             year INTEGER NOT NULL,
             amount REAL NOT NULL,
             notes TEXT,
             created_at TEXT NOT NULL,
             UNIQUE (account_guid, year)
         );
         CREATE INDEX IF NOT EXISTS idx_budgets_account_year ON budgets(account_guid, year);",
    )?;
    for (old_guid, account_guid, year, amount, notes, created_at) in &overrides {
        conn.execute(
            "INSERT OR IGNORE INTO budgets (guid, account_guid, year, amount, notes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                format!("{old_guid}:{year}"),
                account_guid,
                year,
                amount,
                notes,
                created_at
            ],
        )?;
    }
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA user_version = 4;")?;
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
    use rusqlite::OptionalExtension;

    fn create_legacy_schema(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE commodities (
                guid TEXT PRIMARY KEY, mnemonic TEXT NOT NULL,
                namespace TEXT NOT NULL, fullname TEXT);
            CREATE TABLE accounts (
                guid TEXT PRIMARY KEY, name TEXT NOT NULL, account_type TEXT NOT NULL,
                parent_guid TEXT, code TEXT, description TEXT,
                placeholder INTEGER DEFAULT 0, commodity_guid TEXT);
            CREATE TABLE transactions (guid TEXT PRIMARY KEY, post_date TEXT NOT NULL,
                description TEXT, num TEXT, notes TEXT);
            CREATE TABLE splits (guid TEXT PRIMARY KEY, tx_guid TEXT NOT NULL,
                account_guid TEXT NOT NULL, value_num INTEGER NOT NULL,
                value_denom INTEGER NOT NULL, quantity_num INTEGER NOT NULL,
                quantity_denom INTEGER NOT NULL, reconcile_state TEXT,
                reconciled INTEGER DEFAULT 0, memo TEXT);
            CREATE TABLE prices (guid TEXT PRIMARY KEY, commodity_guid TEXT NOT NULL,
                currency_guid TEXT NOT NULL, value_num INTEGER NOT NULL,
                value_denom INTEGER NOT NULL, date TEXT NOT NULL);
            CREATE TABLE import_log (id INTEGER PRIMARY KEY AUTOINCREMENT,
                filename TEXT NOT NULL, file_path TEXT, imported_at TEXT NOT NULL,
                sha256 TEXT, accounts_added INTEGER DEFAULT 0,
                transactions_added INTEGER DEFAULT 0, splits_added INTEGER DEFAULT 0,
                commodities_added INTEGER DEFAULT 0, accounts_orphaned INTEGER DEFAULT 0,
                transactions_orphaned INTEGER DEFAULT 0, splits_orphaned INTEGER DEFAULT 0,
                accounts_updated INTEGER DEFAULT 0, transactions_updated INTEGER DEFAULT 0,
                splits_updated INTEGER DEFAULT 0, commodities_orphaned INTEGER DEFAULT 0);
            CREATE TABLE budgets (
                guid TEXT PRIMARY KEY, account_guid TEXT NOT NULL,
                period_type TEXT NOT NULL DEFAULT 'MONTHLY', amount REAL NOT NULL,
                rollover INTEGER NOT NULL DEFAULT 0, rollover_start TEXT,
                notes TEXT, created_at TEXT NOT NULL);
            CREATE TABLE budget_amounts (
                budget_guid TEXT NOT NULL, year INTEGER NOT NULL, month INTEGER,
                amount REAL NOT NULL, PRIMARY KEY (budget_guid, year, month));
            CREATE TABLE refresh_log (id INTEGER PRIMARY KEY AUTOINCREMENT,
                refreshed_at TEXT NOT NULL);",
        )
        .unwrap();
    }

    /// A pre-v3 monthly database must collapse into per-year budgets: the
    /// monthly default × 12 becomes the annual default, each year's effective
    /// months are summed into a yearly row (only when it differs from the
    /// default), and the v4 step then keeps only those yearly rows.
    #[test]
    fn monthly_budgets_collapse_into_yearly() {
        let conn = Connection::open_in_memory().unwrap();
        create_legacy_schema(&conn);
        conn.execute_batch("PRAGMA user_version = 2").unwrap();
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
        // Monthly default 100 with two overridden months in 2024.
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, period_type, amount, rollover, notes, created_at)
             VALUES ('b-food', 'food', 'MONTHLY', 100, 0, NULL, '2024-01-01T00:00:00Z'),
                    ('b-groc', 'groceries', 'MONTHLY', 50, 0, NULL, '2024-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO budget_amounts (budget_guid, year, month, amount)
             VALUES ('b-food', 2024, 1, 50),
                    ('b-food', 2024, 2, 120),
                    ('b-groc', 2024, 1, 50),
                    ('b-groc', 2024, 2, 50)",
            [],
        )
        .unwrap();

        // v3 step in isolation: monthly default × 12 becomes the annual
        // default and differing years become override rows.
        migrate_monthly_to_yearly_budgets(&conn).unwrap();

        // Monthly default × 12 becomes the annual default.
        let annual: f64 = conn
            .query_row("SELECT amount FROM budgets WHERE guid = 'b-food'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!((annual - 1200.0).abs() < 0.001, "annual {annual}");

        // 2024: 50 + 120 + 10×100 = 1170 differs from 1200 → yearly override.
        let ov: Option<f64> = conn
            .query_row(
                "SELECT amount FROM budget_amounts WHERE budget_guid = 'b-food' AND year = 2024",
                [],
                |r| r.get(0),
            )
            .optional()
            .unwrap();
        assert!(ov.is_some());
        assert!((ov.unwrap() - 1170.0).abs() < 0.01);

        // Groceries 2024 sums to the default (12×50) → no override row needed.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM budget_amounts WHERE budget_guid = 'b-groc'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);

        // Dormant columns and the month column are gone.
        assert!(!column_exists(&conn, "budgets", "period_type").unwrap());
        assert!(!column_exists(&conn, "budgets", "rollover").unwrap());
        assert!(!column_exists(&conn, "budgets", "rollover_start").unwrap());
        assert!(!column_exists(&conn, "budget_amounts", "month").unwrap());

        // v4 step: only override years survive as per-year rows.
        migrate_default_override_to_per_year(&conn).unwrap();

        let rows: Vec<(String, i32, f64)> = {
            let mut stmt = conn
                .prepare("SELECT account_guid, year, amount FROM budgets ORDER BY account_guid")
                .unwrap();
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i32>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "food");
        assert_eq!(rows[0].1, 2024);
        assert!((rows[0].2 - 1170.0).abs() < 0.01);

        // The overrides table is gone.
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'budget_amounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 4);

        // Second run is a no-op.
        migrate(&conn).unwrap();
        let count2: i64 = conn
            .query_row("SELECT COUNT(*) FROM budgets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count2, 1);
    }

    /// A v3 default+override database keeps only its override years: each
    /// becomes a per-year row inheriting the parent notes, while default-only
    /// accounts end up with no budgets.
    #[test]
    fn default_override_migrates_to_per_year() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE commodities (
                guid TEXT PRIMARY KEY, mnemonic TEXT NOT NULL,
                namespace TEXT NOT NULL, fullname TEXT);
            CREATE TABLE accounts (
                guid TEXT PRIMARY KEY, name TEXT NOT NULL, account_type TEXT NOT NULL,
                parent_guid TEXT, code TEXT, description TEXT,
                placeholder INTEGER DEFAULT 0, commodity_guid TEXT);
            CREATE TABLE transactions (guid TEXT PRIMARY KEY, post_date TEXT NOT NULL,
                description TEXT, num TEXT, notes TEXT);
            CREATE TABLE splits (guid TEXT PRIMARY KEY, tx_guid TEXT NOT NULL,
                account_guid TEXT NOT NULL, value_num INTEGER NOT NULL,
                value_denom INTEGER NOT NULL, quantity_num INTEGER NOT NULL,
                quantity_denom INTEGER NOT NULL, reconcile_state TEXT,
                reconciled INTEGER DEFAULT 0, memo TEXT);
            CREATE TABLE prices (guid TEXT PRIMARY KEY, commodity_guid TEXT NOT NULL,
                currency_guid TEXT NOT NULL, value_num INTEGER NOT NULL,
                value_denom INTEGER NOT NULL, date TEXT NOT NULL);
            CREATE TABLE import_log (id INTEGER PRIMARY KEY AUTOINCREMENT,
                filename TEXT NOT NULL, file_path TEXT, imported_at TEXT NOT NULL,
                sha256 TEXT, accounts_added INTEGER DEFAULT 0,
                transactions_added INTEGER DEFAULT 0, splits_added INTEGER DEFAULT 0,
                commodities_added INTEGER DEFAULT 0, accounts_orphaned INTEGER DEFAULT 0,
                transactions_orphaned INTEGER DEFAULT 0, splits_orphaned INTEGER DEFAULT 0,
                accounts_updated INTEGER DEFAULT 0, transactions_updated INTEGER DEFAULT 0,
                splits_updated INTEGER DEFAULT 0, commodities_orphaned INTEGER DEFAULT 0);
            CREATE TABLE budgets (
                guid TEXT PRIMARY KEY, account_guid TEXT NOT NULL,
                amount REAL NOT NULL, notes TEXT, created_at TEXT NOT NULL);
            CREATE TABLE budget_amounts (
                budget_guid TEXT NOT NULL, year INTEGER NOT NULL,
                amount REAL NOT NULL, PRIMARY KEY (budget_guid, year));
            CREATE TABLE refresh_log (id INTEGER PRIMARY KEY AUTOINCREMENT,
                refreshed_at TEXT NOT NULL);
            PRAGMA user_version = 3;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder)
             VALUES ('root', 'Root', 'ROOT', NULL, 0),
                    ('food', 'Food', 'EXPENSE', 'root', 0),
                    ('groceries', 'Groceries', 'EXPENSE', 'root', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, amount, notes, created_at)
             VALUES ('b-food', 'food', 1200, 'weekly shop', '2024-01-01T00:00:00Z'),
                    ('b-groc', 'groceries', 960, NULL, '2024-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO budget_amounts (budget_guid, year, amount)
             VALUES ('b-food', 2024, 600), ('b-food', 2025, 900)",
            [],
        )
        .unwrap();

        // init_db (not just migrate): opening an existing v3 database must
        // not fail on SCHEMA statements before the migration runs.
        init_db(&conn).unwrap();

        let rows: Vec<(String, i32, f64, Option<String>)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT account_guid, year, amount, notes FROM budgets
                     ORDER BY account_guid, year",
                )
                .unwrap();
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i32>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        };
        // Only the override years survive; the default-only account is gone.
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "food");
        assert_eq!(rows[0].1, 2024);
        assert!((rows[0].2 - 600.0).abs() < 0.001);
        assert_eq!(rows[0].3.as_deref(), Some("weekly shop"));
        assert_eq!(rows[1].0, "food");
        assert_eq!(rows[1].1, 2025);
        assert!((rows[1].2 - 900.0).abs() < 0.001);

        assert!(column_exists(&conn, "budgets", "year").unwrap());
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'budget_amounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 4);
    }

    /// Fresh installs get the per-year schema directly with `user_version = 4`.
    #[test]
    fn fresh_install_uses_yearly_schema() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        assert!(column_exists(&conn, "budgets", "year").unwrap());
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'budget_amounts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
        let indexes: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_budgets_account_year'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(indexes, 1);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 4);
    }
}
