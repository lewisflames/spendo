//! Schema versions and the migrations between them.
//!
//! A book records its schema version in `PRAGMA user_version`. `db::SCHEMA`
//! always creates the current version (`CURRENT_VERSION`) for new books;
//! existing books are brought up to it on open by [`migrate`], one ordered
//! step per version. Rules:
//!
//! - Never edit a released migration: add a new one and bump `CURRENT_VERSION`.
//! - Change `db::SCHEMA` in the same commit so a fresh book matches a migrated
//!   one (`migrated_books_match_a_fresh_one` checks it), and freeze a fixture of
//!   the old version in `migrations/fixtures/` for the tests.
//! - Migrations run in one transaction with foreign keys off (so a table can be
//!   rebuilt without cascading), after a `beforeMigrate` safety snapshot. Any
//!   failure, including a new foreign-key violation, rolls the whole upgrade
//!   back and leaves the file as it was.
//! - A book from a newer Spendo (`user_version > CURRENT_VERSION`) is never
//!   migrated: it opens read-only (`books::connect_book`).

use rusqlite::{Connection, Transaction};
use std::path::Path;

use crate::backups::{self, BackupReason};
use crate::db::AppError;

/// Schema version `db::SCHEMA` creates and every opened book is migrated to.
pub const CURRENT_VERSION: i32 = 3;

/// One upgrade step, run inside the migration transaction.
type Migration = fn(&Transaction) -> rusqlite::Result<()>;

/// `MIGRATIONS[i]` upgrades a book from version `i + 1` to `i + 2`.
const MIGRATIONS: &[Migration] = &[drop_legacy_tables, strict_core_tables];

const _: () = assert!(MIGRATIONS.len() as i32 == CURRENT_VERSION - 1);

/// Shown (banner and refused writes) for a book saved by a newer Spendo.
pub const NEWER_BOOK_MESSAGE: &str = "This book was saved by a newer Spendo. Update to edit it";

/// Tables every version 1 book has. Unversioned books (saved before
/// versioning) are adopted as version 1 only when all of them exist; each
/// build since the last of them was added created them on open.
const V1_TABLES: &[&str] = &[
    "accounts",
    "allocation_targets",
    "app_settings",
    "attachments",
    "budgets",
    "commodities",
    "csv_import_presets",
    "dividend_links",
    "import_rules",
    "payees",
    "prices",
    "reconcile_batch_splits",
    "reconcile_batches",
    "recurring_schedules",
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
];

/// v1 → v2: audit-log and manual-price tables early builds created and no
/// build reads anymore. Snapshots reference log entries, so they go first.
fn drop_legacy_tables(tx: &Transaction) -> rusqlite::Result<()> {
    tx.execute_batch(
        "DROP TABLE IF EXISTS change_snapshots;
         DROP TABLE IF EXISTS change_log;
         DROP TABLE IF EXISTS manual_prices;",
    )
}

/// v3 tables, frozen as `db::SCHEMA` first created them: the CREATE text must
/// match it exactly (`migrated_books_match_a_fresh_one`).
const V3_TABLES: &str = "
CREATE TABLE commodities (
	guid TEXT PRIMARY KEY,
	mnemonic TEXT NOT NULL,
	namespace TEXT NOT NULL,
	fullname TEXT
) STRICT;

CREATE TABLE accounts (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL,
	account_type TEXT NOT NULL CHECK (account_type IN ('ROOT', 'ASSET', 'BANK', 'CASH', 'STOCK',
		'MUTUAL', 'FUND', 'CURRENCY', 'RECEIVABLE', 'LIABILITY', 'CREDIT', 'PAYABLE', 'INCOME',
		'EXPENSE', 'EQUITY', 'TRADING')),
	parent_guid TEXT REFERENCES accounts(guid),
	code TEXT,
	description TEXT,
	placeholder INTEGER DEFAULT 0,
	commodity_guid TEXT REFERENCES commodities(guid)
) STRICT;

CREATE TABLE transactions (
	guid TEXT PRIMARY KEY,
	post_date TEXT NOT NULL CHECK (date(substr(post_date, 1, 10)) IS substr(post_date, 1, 10)),
	description TEXT,
	num TEXT,
	notes TEXT,
	payee_guid TEXT REFERENCES payees(guid) ON DELETE SET NULL
) STRICT;

CREATE TABLE splits (
	guid TEXT PRIMARY KEY,
	tx_guid TEXT NOT NULL REFERENCES transactions(guid) ON DELETE CASCADE,
	account_guid TEXT NOT NULL REFERENCES accounts(guid),
	value_num INTEGER NOT NULL,
	value_denom INTEGER NOT NULL CHECK (value_denom > 0),
	quantity_num INTEGER NOT NULL,
	quantity_denom INTEGER NOT NULL CHECK (quantity_denom > 0),
	reconcile_state TEXT,
	reconciled INTEGER DEFAULT 0,
	memo TEXT
) STRICT;

CREATE TABLE prices (
	guid TEXT PRIMARY KEY,
	commodity_guid TEXT NOT NULL REFERENCES commodities(guid) ON DELETE CASCADE,
	currency_guid TEXT NOT NULL REFERENCES commodities(guid) ON DELETE CASCADE,
	value_num INTEGER NOT NULL,
	value_denom INTEGER NOT NULL CHECK (value_denom > 0),
	date TEXT NOT NULL
) STRICT;

CREATE TABLE budgets (
	guid TEXT PRIMARY KEY,
	account_guid TEXT NOT NULL REFERENCES accounts(guid) ON DELETE CASCADE,
	year INTEGER NOT NULL,
	amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
	created_at TEXT NOT NULL,
	UNIQUE (account_guid, year)
) STRICT;

CREATE TABLE recurring_schedules (
	guid TEXT PRIMARY KEY,
	name TEXT NOT NULL,
	frequency TEXT NOT NULL,
	interval_mult INTEGER NOT NULL DEFAULT 1,
	start_date TEXT NOT NULL,
	end_date TEXT,
	max_occurrences INTEGER,
	occurrences_posted INTEGER NOT NULL DEFAULT 0,
	skipped_count INTEGER NOT NULL DEFAULT 0,
	next_due_date TEXT,
	active INTEGER NOT NULL DEFAULT 1,
	auto_post INTEGER NOT NULL DEFAULT 0,
	spread_total INTEGER NOT NULL DEFAULT 0,
	template_json TEXT NOT NULL,
	created_at TEXT NOT NULL
) STRICT;
";

/// Indexes of the rebuilt tables; their v2 copies go with the old tables.
const V3_INDEXES: &str = "
CREATE INDEX idx_transactions_post_date ON transactions(post_date);
CREATE INDEX idx_transactions_post_date_guid ON transactions(post_date, guid);
CREATE INDEX idx_splits_tx_guid ON splits(tx_guid);
CREATE INDEX idx_splits_account_guid ON splits(account_guid);
CREATE INDEX idx_accounts_account_type ON accounts(account_type);
CREATE INDEX idx_accounts_parent_guid ON accounts(parent_guid);
CREATE INDEX idx_prices_commodity_guid_date ON prices(commodity_guid, date);
CREATE INDEX idx_prices_currency_guid_date ON prices(currency_guid, date);
CREATE INDEX idx_commodities_mnemonic ON commodities(mnemonic);
CREATE INDEX idx_transactions_payee ON transactions(payee_guid);
CREATE INDEX idx_budgets_account_year ON budgets(account_guid, year);
CREATE INDEX idx_schedules_active_due ON recurring_schedules(active, next_due_date);
";

/// Tables v3 rebuilds. Each is renamed to `<name>_v2`, recreated from
/// `V3_TABLES` and filled from the old copy.
const V3_REBUILT: &[&str] = &[
    "commodities",
    "accounts",
    "transactions",
    "splits",
    "prices",
    "budgets",
    "recurring_schedules",
];

/// v2 → v3: fold the side tables into their rows (`schedule_skips` and
/// `schedule_options` into `recurring_schedules`, `transaction_payees` into
/// `transactions.payee_guid`), rebuild the core tables STRICT with foreign keys
/// and `CHECK`s, and store budgets in integer cents. Orphan rows are fixed or
/// removed first ([`clean_orphans`]) so the new keys hold; what changed is
/// logged, and the `beforeMigrate` snapshot keeps the original.
fn strict_core_tables(tx: &Transaction) -> rusqlite::Result<()> {
    for (what, n) in upgrade_to_v3(tx)? {
        eprintln!("spendo: schema upgrade removed or fixed {n} {what}");
    }
    Ok(())
}

/// [`strict_core_tables`], returning what the cleanup changed.
fn upgrade_to_v3(tx: &Transaction) -> rusqlite::Result<Vec<(String, usize)>> {
    let report = clean_orphans(tx)?;
    // Legacy rename leaves other tables' `REFERENCES accounts(guid)` pointing
    // at the name (not at the renamed `accounts_v2`), so they bind to the new
    // table once it exists.
    tx.pragma_update(None, "legacy_alter_table", true)?;
    let result = rebuild_core_tables(tx);
    let restore = tx.pragma_update(None, "legacy_alter_table", false);
    result?;
    restore?;
    Ok([report, drop_dangling_rows(tx)?].concat())
}

fn rebuild_core_tables(tx: &Transaction) -> rusqlite::Result<()> {
    for table in V3_REBUILT {
        tx.execute_batch(&format!("ALTER TABLE {table} RENAME TO {table}_v2;"))?;
    }
    tx.execute_batch(V3_TABLES)?;
    tx.execute_batch(
        "INSERT INTO commodities (guid, mnemonic, namespace, fullname)
         SELECT guid, mnemonic, namespace, fullname FROM commodities_v2;

         INSERT INTO accounts (guid, name, account_type, parent_guid, code, description,
                               placeholder, commodity_guid)
         SELECT guid, name, account_type, parent_guid, code, description, placeholder,
                commodity_guid
         FROM accounts_v2;

         INSERT INTO transactions (guid, post_date, description, num, notes, payee_guid)
         SELECT t.guid, t.post_date, t.description, t.num, t.notes, p.guid
         FROM transactions_v2 t
         LEFT JOIN transaction_payees tp ON tp.tx_guid = t.guid
         LEFT JOIN payees p ON p.guid = tp.payee_guid;

         INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num,
                             quantity_denom, reconcile_state, reconciled, memo)
         SELECT guid, tx_guid, account_guid, value_num, value_denom, quantity_num,
                quantity_denom, reconcile_state, reconciled, memo
         FROM splits_v2;

         INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
         SELECT guid, commodity_guid, currency_guid, value_num, value_denom, date FROM prices_v2;

         INSERT INTO budgets (guid, account_guid, year, amount_cents, created_at)
         SELECT guid, account_guid, year, CAST(ROUND(amount * 100) AS INTEGER), created_at
         FROM budgets_v2;

         INSERT INTO recurring_schedules (guid, name, frequency, interval_mult, start_date,
             end_date, max_occurrences, occurrences_posted, skipped_count, next_due_date, active,
             auto_post, spread_total, template_json, created_at)
         SELECT r.guid, r.name, r.frequency, r.interval_mult, r.start_date, r.end_date,
                r.max_occurrences, r.occurrences_posted, COALESCE(k.skipped_count, 0),
                r.next_due_date, r.active, COALESCE(o.auto_post, 0), COALESCE(o.spread_total, 0),
                r.template_json, r.created_at
         FROM recurring_schedules_v2 r
         LEFT JOIN schedule_skips k ON k.schedule_guid = r.guid
         LEFT JOIN schedule_options o ON o.schedule_guid = r.guid;

         DROP TABLE schedule_skips;
         DROP TABLE schedule_options;
         DROP TABLE transaction_payees;",
    )?;
    for table in V3_REBUILT {
        tx.execute_batch(&format!("DROP TABLE {table}_v2;"))?;
    }
    tx.execute_batch(V3_INDEXES)
}

/// Rows the v3 keys and checks would reject, fixed where the data survives
/// (an account under a missing parent moves under the root) and removed where
/// it can't (a split of a missing transaction). Returns `(what, count)` for
/// each non-empty fix.
fn clean_orphans(tx: &Transaction) -> rusqlite::Result<Vec<(String, usize)>> {
    let root: Option<String> = {
        let mut stmt = tx.prepare("SELECT guid FROM accounts WHERE account_type = 'ROOT'")?;
        let roots: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        (roots.len() == 1).then(|| roots[0].clone())
    };
    let fixes: [(&str, &str); 7] = [
        (
            "accounts under a missing parent",
            "UPDATE accounts
             SET parent_guid = CASE WHEN account_type = 'ROOT' THEN NULL ELSE ?1 END
             WHERE parent_guid IS NOT NULL
               AND parent_guid NOT IN (SELECT guid FROM accounts)",
        ),
        (
            "accounts with a missing currency",
            "UPDATE accounts SET commodity_guid = NULL
             WHERE commodity_guid IS NOT NULL
               AND commodity_guid NOT IN (SELECT guid FROM commodities)",
        ),
        (
            "splits of a missing transaction",
            "DELETE FROM splits WHERE tx_guid NOT IN (SELECT guid FROM transactions)",
        ),
        (
            "splits in a missing account",
            "DELETE FROM splits WHERE account_guid NOT IN (SELECT guid FROM accounts)",
        ),
        (
            "prices of a missing commodity",
            "DELETE FROM prices
             WHERE commodity_guid NOT IN (SELECT guid FROM commodities)
                OR currency_guid NOT IN (SELECT guid FROM commodities)",
        ),
        (
            "payee links of a missing transaction or payee",
            "DELETE FROM transaction_payees
             WHERE tx_guid NOT IN (SELECT guid FROM transactions)
                OR payee_guid NOT IN (SELECT guid FROM payees)",
        ),
        (
            "budgets of zero or less",
            "DELETE FROM budgets WHERE CAST(ROUND(amount * 100) AS INTEGER) <= 0",
        ),
    ];
    let mut report = Vec::new();
    for (what, sql) in fixes {
        let n = if sql.contains("?1") {
            tx.execute(sql, [&root])?
        } else {
            tx.execute(sql, [])?
        };
        if n > 0 {
            report.push((what.to_string(), n));
        }
    }
    Ok(report)
}

/// Delete whatever still breaks a foreign key once the tables are rebuilt
/// (e.g. an attachment of a missing transaction), repeating because one
/// delete can orphan another row. Returns `(rows in <table>, count)`.
fn drop_dangling_rows(tx: &Transaction) -> rusqlite::Result<Vec<(String, usize)>> {
    let mut removed: std::collections::BTreeMap<String, usize> = Default::default();
    loop {
        let rows: Vec<(String, i64)> = {
            let mut stmt = tx.prepare(
                "SELECT DISTINCT \"table\", rowid FROM pragma_foreign_key_check
                 WHERE rowid IS NOT NULL",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            rows
        };
        let mut deleted = 0;
        for (table, rowid) in rows {
            let sql = format!(
                "DELETE FROM \"{}\" WHERE rowid = ?1",
                table.replace('"', "\"\"")
            );
            let n = tx.execute(&sql, [rowid])?;
            deleted += n;
            *removed.entry(table).or_default() += n;
        }
        // Nothing left to delete (or rows `apply` will refuse): stop.
        if deleted == 0 {
            break;
        }
    }
    Ok(removed
        .into_iter()
        .map(|(table, n)| (format!("rows in {table} linked to missing records"), n))
        .collect())
}

/// Schema version stored in the book (`0` for books saved before versioning).
pub fn schema_version(conn: &Connection) -> rusqlite::Result<i32> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
}

/// Record `CURRENT_VERSION` in a book whose schema `db::SCHEMA` just created.
pub(crate) fn stamp_current(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "user_version", CURRENT_VERSION)
}

/// Bring an open, writable book to `CURRENT_VERSION`. With `app_data_dir`, a
/// `beforeMigrate` snapshot is taken first and its failure aborts; without
/// one (a restore's staged copy, which is itself a backup) none is taken.
pub(crate) fn migrate(conn: &Connection, app_data_dir: Option<&Path>) -> Result<(), AppError> {
    run_migrations(conn, MIGRATIONS, app_data_dir)
}

fn run_migrations(
    conn: &Connection,
    migrations: &[Migration],
    app_data_dir: Option<&Path>,
) -> Result<(), AppError> {
    let target = migrations.len() as i32 + 1;
    let version = schema_version(conn)?;
    if version > target {
        return Err(AppError::ReadOnly(NEWER_BOOK_MESSAGE.to_string()));
    }
    if version == target {
        return Ok(());
    }
    if version < 0 {
        return Err(AppError::Validation(format!(
            "This book has an unknown schema version ({version})"
        )));
    }
    if version == 0 {
        ensure_v1_tables(conn)?;
    }
    if let Some(dir) = app_data_dir {
        backups::before_change(conn, dir, BackupReason::BeforeMigrate)?;
    }
    // `foreign_keys` can't change inside a transaction, so it is switched off
    // around it and restored on every path.
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let result = apply(conn, &migrations[(version.max(1) - 1) as usize..], target);
    let restore = conn.execute_batch("PRAGMA foreign_keys = ON;");
    result.map_err(|e| {
        AppError::Internal(format!(
            "We couldn't upgrade this book, so it was left as it was. {e}"
        ))
    })?;
    restore?;
    Ok(())
}

/// Run `steps` and set `target` in one transaction; dropping it on any error
/// rolls everything back.
fn apply(conn: &Connection, steps: &[Migration], target: i32) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    let violations_before = foreign_key_violations(&tx)?;
    for step in steps {
        step(&tx)?;
    }
    // Books may carry old violations; only new ones are the migration's fault.
    if foreign_key_violations(&tx)? > violations_before {
        return Err(AppError::Validation(
            "The upgrade would break links between records".to_string(),
        ));
    }
    tx.pragma_update(None, "user_version", target)?;
    tx.commit()?;
    Ok(())
}

fn foreign_key_violations(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
        r.get(0)
    })
}

fn ensure_v1_tables(conn: &Connection) -> Result<(), AppError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    let mut missing = Vec::new();
    for table in V1_TABLES {
        if !stmt.exists([table])? {
            missing.push(*table);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "This book is from an early Spendo build and is missing tables ({}). \
         Open it once with Spendo 0.7 to complete it",
        missing.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::OptionalExtension;
    use std::path::PathBuf;

    /// Today's books before versioning: the v1 schema with sample rows,
    /// including the legacy tables early builds left behind.
    const FIXTURE_V1: &str = include_str!("migrations/fixtures/v1.sql");
    /// A real-shaped version 2 household (see the file header).
    const FIXTURE_V2: &str = include_str!("migrations/fixtures/v2.sql");

    /// Tables a version 3 book no longer has.
    const GONE_IN_V3: &[&str] = &[
        "change_log",
        "change_snapshots",
        "manual_prices",
        "schedule_skips",
        "schedule_options",
        "transaction_payees",
    ];

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("spendo-migrate-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A stamped book file built from `sql` at `user_version`.
    fn fixture_book(dir: &Path, sql: &str, version: i32) -> PathBuf {
        let path = dir.join("book.spendo");
        let conn = db::connect_db(&path).unwrap();
        conn.execute_batch(sql).unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
        db::stamp_app_id(&conn);
        path
    }

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// `(type, name, sql)` of every schema object, for comparing books.
    fn schema_of(conn: &Connection) -> Vec<(String, String, Option<String>)> {
        let mut stmt = conn
            .prepare("SELECT type, name, sql FROM sqlite_master ORDER BY type, name")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn assert_healthy(conn: &Connection) {
        let integrity: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
        assert_eq!(foreign_key_violations(conn).unwrap(), 0);
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn backup_names(app: &Path) -> Vec<String> {
        let root = app.join("backups");
        let Ok(dirs) = std::fs::read_dir(&root) else {
            return Vec::new();
        };
        dirs.flat_map(|d| std::fs::read_dir(d.unwrap().path()).unwrap())
            .map(|f| f.unwrap().file_name().to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn fresh_books_are_current() {
        let conn = db::open_placeholder().unwrap();
        assert_eq!(schema_version(&conn).unwrap(), CURRENT_VERSION);
        assert!(run_migrations(&conn, MIGRATIONS, None).is_ok());
    }

    fn payee_of(conn: &Connection, tx: &str) -> Option<String> {
        conn.query_row(
            "SELECT p.name FROM transactions t JOIN payees p ON p.guid = t.payee_guid
             WHERE t.guid = ?1",
            [tx],
            |r| r.get(0),
        )
        .optional()
        .unwrap()
    }

    #[test]
    fn fixtures_of_every_version_migrate_to_current() {
        // Unversioned books (0) are adopted as version 1, then upgraded.
        for (version, sql, txns, splits, payee) in [
            (0, FIXTURE_V1, 2, 4, ("t-1", "Mercado")),
            (1, FIXTURE_V1, 2, 4, ("t-1", "Mercado")),
            (2, FIXTURE_V2, 14, 32, ("t-rent1", "Landlord")),
        ] {
            let dir = tmp_dir(&format!("v{version}"));
            let app = dir.join("app");
            let path = fixture_book(&dir, sql, version);
            let conn = db::connect_db(&path).unwrap();
            migrate(&conn, Some(&app)).unwrap();

            assert_eq!(schema_version(&conn).unwrap(), CURRENT_VERSION);
            assert_healthy(&conn);
            let tables = table_names(&conn);
            for gone in GONE_IN_V3 {
                assert!(!tables.contains(&gone.to_string()), "{gone} survived");
            }
            // Data and foreign-key enforcement survive.
            assert_eq!(count(&conn, "transactions"), txns);
            assert_eq!(count(&conn, "splits"), splits);
            assert_eq!(payee_of(&conn, payee.0).as_deref(), Some(payee.1));
            let fk_on: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .unwrap();
            assert_eq!(fk_on, 1);
            // A safety snapshot of the old version was taken first.
            let backups = backup_names(&app);
            assert_eq!(backups.len(), 1, "{backups:?}");
            assert!(
                backups[0].ends_with("Z_before-migrate.spendo"),
                "{backups:?}"
            );
            drop(conn);

            // Reopening is a no-op: no second snapshot.
            let conn = db::connect_db(&path).unwrap();
            migrate(&conn, Some(&app)).unwrap();
            assert_eq!(backup_names(&app).len(), 1);
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// An in-memory book from `sql` at `version`, foreign keys on like a
    /// real connection.
    fn memory_book(sql: &str, version: i32) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(sql).unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn
    }

    /// Every figure the reports show, from the same functions the pages call.
    fn figures(conn: &Connection) -> serde_json::Value {
        let sheet = |as_of: &str| {
            let input = serde_json::from_value(serde_json::json!({ "asOf": as_of })).unwrap();
            crate::accounts::fetch_balance_sheet_inner(conn, input).unwrap()
        };
        serde_json::json!({
            "accounts": crate::accounts::fetch_accounts_inner(conn).unwrap(),
            "sheet2024": sheet("2024-06-30"),
            "sheet2025": sheet("2025-01-31"),
            "pnl": crate::reports::fetch_pnl_report_inner(
                conn, "2024-01-01", "2025-12-31", None, None
            ).unwrap(),
            "portfolio": crate::portfolio::fetch_portfolio_data_inner(conn).unwrap(),
            "gains": crate::portfolio::realized_gains_inner(conn, 2024).unwrap(),
        })
    }

    /// `a == b`, with numbers equal to 1e-9 relative: some report sums run in
    /// hash-map order, so repeated calls on the same book differ in the last
    /// digit.
    fn assert_same_figures(a: &serde_json::Value, b: &serde_json::Value, path: &str) {
        use serde_json::Value;
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => {
                let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
                let tolerance = 1e-9 * x.abs().max(y.abs()).max(1.0);
                assert!((x - y).abs() <= tolerance, "{path}: {x} != {y}");
            }
            (Value::Array(x), Value::Array(y)) => {
                assert_eq!(x.len(), y.len(), "{path}: length");
                for (i, (x, y)) in x.iter().zip(y).enumerate() {
                    assert_same_figures(x, y, &format!("{path}[{i}]"));
                }
            }
            (Value::Object(x), Value::Object(y)) => {
                assert_eq!(
                    x.keys().collect::<Vec<_>>(),
                    y.keys().collect::<Vec<_>>(),
                    "{path}: keys"
                );
                for (k, x) in x {
                    assert_same_figures(x, &y[k], &format!("{path}.{k}"));
                }
            }
            _ => assert_eq!(a, b, "{path}"),
        }
    }

    #[test]
    fn v3_keeps_every_figure_and_folds_the_side_tables() {
        let conn = memory_book(FIXTURE_V2, 2);
        let before = figures(&conn);
        migrate(&conn, None).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), CURRENT_VERSION);
        assert_healthy(&conn);
        assert_same_figures(&before, &figures(&conn), "figures");

        // Side tables became columns.
        let mut stmt = conn
            .prepare(
                "SELECT guid, skipped_count, auto_post, spread_total
                 FROM recurring_schedules ORDER BY guid",
            )
            .unwrap();
        let schedules: Vec<(String, i64, i64, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            schedules,
            [
                ("r-gym".to_string(), 0, 0, 0),
                ("r-laptop".to_string(), 0, 0, 1),
                ("r-rent".to_string(), 2, 1, 0),
            ]
        );
        assert_eq!(payee_of(&conn, "t-groc2").as_deref(), Some("Mercado"));
        assert_eq!(payee_of(&conn, "t-sal2").as_deref(), Some("Employer"));
        assert_eq!(payee_of(&conn, "t-open"), None);
        let linked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE payee_guid IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(linked, 6);

        // Budgets keep their cents.
        let mut stmt = conn
            .prepare("SELECT guid, amount_cents FROM budgets ORDER BY guid")
            .unwrap();
        let budgets: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            budgets,
            [
                ("b-food-24".to_string(), 300050),
                ("b-food-25".to_string(), 123456),
                ("b-rent-24".to_string(), 1080000),
            ]
        );
        let rows = crate::budgets::fetch_budget_vs_actual_inner(&conn, 2025).unwrap();
        let food = serde_json::to_value(&rows).unwrap();
        assert_eq!(food[0]["budgetAmount"], 1234.56, "{food}");

        // Nothing else was lost.
        for (table, n) in [
            ("reconcile_batch_splits", 3),
            ("attachments", 1),
            ("transaction_tags", 2),
            ("prices", 7),
        ] {
            assert_eq!(count(&conn, table), n, "{table}");
        }
    }

    #[test]
    fn v3_enforces_keys_and_checks() {
        let conn = memory_book(FIXTURE_V2, 2);
        migrate(&conn, None).unwrap();
        for bad in [
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom,
                 quantity_num, quantity_denom) VALUES ('x', 'no-tx', 'a-bank', 1, 100, 1, 100)",
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom,
                 quantity_num, quantity_denom) VALUES ('x', 't-open', 'a-bank', 1, 0, 1, 100)",
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom,
                 quantity_num, quantity_denom) VALUES ('x', 't-open', 'a-bank', 1.5, 100, 1, 100)",
            "INSERT INTO accounts (guid, name, account_type, parent_guid)
                 VALUES ('x', 'X', 'BANK', 'no-parent')",
            "INSERT INTO accounts (guid, name, account_type, parent_guid)
                 VALUES ('x', 'X', 'bank', 'a-root')",
            "INSERT INTO transactions (guid, post_date) VALUES ('x', '05/01/2024')",
            "INSERT INTO transactions (guid, post_date) VALUES ('x', '2024-02-30')",
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
                 VALUES ('x', 'no-commodity', 'c-eur', 1, 1, '2024-01-01')",
            "INSERT INTO budgets (guid, account_guid, year, amount_cents, created_at)
                 VALUES ('x', 'a-rent', 2025, 0, '2025-01-01')",
            "DELETE FROM accounts WHERE guid = 'a-food'",
        ] {
            assert!(conn.execute(bad, []).is_err(), "accepted: {bad}");
        }
        // The account types the check allows are exactly the app's.
        for account_type in crate::accounts_mut::ACCOUNT_TYPES.iter().chain(&["ROOT"]) {
            conn.execute(
                "INSERT INTO accounts (guid, name, account_type, parent_guid)
                 VALUES ('t', 'T', ?1, 'a-root')",
                [account_type],
            )
            .unwrap();
            conn.execute("DELETE FROM accounts WHERE guid = 't'", [])
                .unwrap();
        }
        // Deleting a transaction takes its splits; deleting a payee clears it.
        conn.execute("DELETE FROM transactions WHERE guid = 't-groc3'", [])
            .unwrap();
        let left: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM splits WHERE tx_guid = 't-groc3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0);
        conn.execute("DELETE FROM payees WHERE guid = 'p-mercado'", [])
            .unwrap();
        assert_eq!(payee_of(&conn, "t-groc1"), None);
        assert_eq!(count(&conn, "transactions"), 13);
    }

    #[test]
    fn v3_cleans_up_orphans_and_reports_them() {
        let conn = memory_book(FIXTURE_V2, 2);
        conn.execute_batch(
            "PRAGMA foreign_keys = OFF;
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom,
                 quantity_num, quantity_denom) VALUES
                 ('s-no-tx', 'gone-tx', 'a-bank', 100, 100, 100, 100),
                 ('s-no-acct', 't-open', 'gone-acct', 100, 100, 100, 100);
             INSERT INTO reconcile_batch_splits (batch_guid, split_guid)
                 VALUES ('rb-1', 's-no-tx');
             INSERT INTO accounts (guid, name, account_type, parent_guid, commodity_guid) VALUES
                 ('a-lost', 'Lost', 'BANK', 'gone-parent', 'c-eur'),
                 ('a-odd', 'Odd', 'BANK', 'a-assets', 'gone-commodity');
             INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
                 VALUES ('p-gone', 'gone-commodity', 'c-eur', 1, 1, '2024-01-01');
             INSERT INTO transaction_payees (tx_guid, payee_guid) VALUES ('t-cash', 'gone-payee');
             INSERT INTO attachments (guid, tx_guid, filename, mime_type, byte_size, data, created_at)
                 VALUES ('at-gone', 'gone-tx', 'x.pdf', 'application/pdf', 1, X'00', '2024-01-01');
             INSERT INTO budgets (guid, account_guid, year, amount, created_at)
                 VALUES ('b-tiny', 'a-fees', 2024, 0.004, '2024-01-01');",
        )
        .unwrap();
        // Run the step itself to see its report.
        let tx = conn.unchecked_transaction().unwrap();
        drop_legacy_tables(&tx).unwrap();
        let mut report = upgrade_to_v3(&tx).unwrap();
        tx.commit().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        report.sort();
        let report: Vec<(&str, usize)> = report.iter().map(|(w, n)| (w.as_str(), *n)).collect();
        assert_eq!(
            report,
            [
                ("accounts under a missing parent", 1),
                ("accounts with a missing currency", 1),
                ("budgets of zero or less", 1),
                ("payee links of a missing transaction or payee", 1),
                ("prices of a missing commodity", 1),
                ("rows in attachments linked to missing records", 1),
                (
                    "rows in reconcile_batch_splits linked to missing records",
                    1
                ),
                ("splits in a missing account", 1),
                ("splits of a missing transaction", 1),
            ]
        );
        assert_healthy(&conn);
        let (parent, commodity): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT (SELECT parent_guid FROM accounts WHERE guid = 'a-lost'),
                        (SELECT commodity_guid FROM accounts WHERE guid = 'a-odd')",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(parent.as_deref(), Some("a-root"));
        assert_eq!(commodity, None);
        assert_eq!(count(&conn, "splits"), 32);
        assert_eq!(count(&conn, "reconcile_batch_splits"), 3);
        assert_eq!(payee_of(&conn, "t-cash"), None);
    }

    #[test]
    fn migrated_books_match_a_fresh_one() {
        let fresh = db::open_placeholder().unwrap();
        for (sql, version) in [(FIXTURE_V1, 1), (FIXTURE_V2, 2)] {
            let conn = memory_book(sql, version);
            migrate(&conn, None).unwrap();
            assert_eq!(
                schema_of(&conn),
                schema_of(&fresh),
                "from version {version}"
            );
        }
    }

    #[test]
    fn unversioned_books_missing_tables_are_refused() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE accounts (guid TEXT PRIMARY KEY);
             CREATE TABLE transactions (guid TEXT PRIMARY KEY);",
        )
        .unwrap();
        let err = migrate(&conn, None).unwrap_err();
        assert_eq!(err.kind(), "validation");
        assert!(err.to_string().contains("payees"), "{err}");
        assert_eq!(schema_version(&conn).unwrap(), 0);
    }

    fn failing_step(tx: &Transaction) -> rusqlite::Result<()> {
        tx.execute_batch("CREATE TABLE half_done (x INTEGER); SELECT * FROM no_such_table;")
    }

    #[test]
    fn failed_migration_leaves_the_file_untouched_with_its_backup() {
        let dir = tmp_dir("fail");
        let app = dir.join("app");
        let path = fixture_book(&dir, FIXTURE_V1, 1);
        let before = std::fs::read(&path).unwrap();

        let conn = db::connect_db(&path).unwrap();
        // The real step runs first, then one that fails half-way.
        let steps: &[Migration] = &[drop_legacy_tables, failing_step];
        let err = run_migrations(&conn, steps, Some(&app)).unwrap_err();
        assert!(err.to_string().contains("left as it was"), "{err}");

        // Rolled back: old version, legacy tables, no partial table, FKs on.
        assert_eq!(schema_version(&conn).unwrap(), 1);
        let tables = table_names(&conn);
        assert!(tables.contains(&"change_log".to_string()));
        assert!(!tables.contains(&"half_done".to_string()));
        let fk_on: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk_on, 1);
        drop(conn);
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let backups = backup_names(&app);
        assert_eq!(backups.len(), 1, "{backups:?}");
        assert!(backups[0].ends_with("Z_before-migrate.spendo"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migrations_that_break_foreign_keys_roll_back() {
        fn orphan_budget(tx: &Transaction) -> rusqlite::Result<()> {
            tx.execute_batch("DELETE FROM accounts WHERE guid = 'a-food'")
        }
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(FIXTURE_V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        let steps: &[Migration] = &[orphan_budget];
        assert!(run_migrations(&conn, steps, None).is_err());
        assert_eq!(count(&conn, "budgets"), 1);
        assert_eq!(schema_version(&conn).unwrap(), 1);
    }

    #[test]
    fn newer_books_are_never_migrated() {
        let conn = db::open_placeholder().unwrap();
        conn.pragma_update(None, "user_version", CURRENT_VERSION + 1)
            .unwrap();
        let err = migrate(&conn, None).unwrap_err();
        assert_eq!(err.kind(), "readOnly");
        assert_eq!(schema_version(&conn).unwrap(), CURRENT_VERSION + 1);
    }
}
