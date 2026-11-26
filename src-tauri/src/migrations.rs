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
pub const CURRENT_VERSION: i32 = 2;

/// One upgrade step, run inside the migration transaction.
type Migration = fn(&Transaction) -> rusqlite::Result<()>;

/// `MIGRATIONS[i]` upgrades a book from version `i + 1` to `i + 2`.
const MIGRATIONS: &[Migration] = &[drop_legacy_tables];

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
    use std::path::PathBuf;

    /// Today's books before versioning: the v1 schema with sample rows,
    /// including the legacy tables early builds left behind.
    const FIXTURE_V1: &str = include_str!("migrations/fixtures/v1.sql");

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

    #[test]
    fn fixtures_of_every_version_migrate_to_current() {
        // Unversioned books (0) are adopted as version 1, then upgraded.
        for version in [0, 1] {
            let dir = tmp_dir(&format!("v{version}"));
            let app = dir.join("app");
            let path = fixture_book(&dir, FIXTURE_V1, version);
            let conn = db::connect_db(&path).unwrap();
            migrate(&conn, Some(&app)).unwrap();

            assert_eq!(schema_version(&conn).unwrap(), CURRENT_VERSION);
            assert_healthy(&conn);
            let tables = table_names(&conn);
            for legacy in ["change_log", "change_snapshots", "manual_prices"] {
                assert!(!tables.contains(&legacy.to_string()), "{legacy} survived");
            }
            // Data and foreign-key enforcement survive.
            assert_eq!(count(&conn, "transactions"), 2);
            assert_eq!(count(&conn, "splits"), 4);
            assert_eq!(count(&conn, "transaction_payees"), 1);
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

    #[test]
    fn migrated_books_match_a_fresh_one() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(FIXTURE_V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        migrate(&conn, None).unwrap();
        let fresh = db::open_placeholder().unwrap();
        assert_eq!(schema_of(&conn), schema_of(&fresh));
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
