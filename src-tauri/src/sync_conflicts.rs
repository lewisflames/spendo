//! Sync conflict copies next to the open book.
//!
//! When two devices change a book before syncing, Syncthing keeps one version
//! and renames the other to `<stem>.sync-conflict-<YYYYMMDD>-<HHMMSS>-<device>.<ext>`
//! in the same folder. This module finds those copies and compares their
//! transactions with the open book by GUID. Copies are only ever opened with
//! `SQLITE_OPEN_READ_ONLY`: nothing here writes, migrates, merges or deletes.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::{self, AppError, AppState};

const MARKER: &str = ".sync-conflict-";

#[derive(Debug, Clone, Serialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflictCopy {
    pub path: String,
    pub file_name: String,
    /// Local time Syncthing stamped into the name (`YYYY-MM-DDTHH:MM:SS`).
    pub saved_at: Option<String>,
    /// Short Syncthing device ID of the device whose version this is.
    pub device: Option<String>,
}

/// One transaction as one side has it.
#[derive(Debug, Clone, Serialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConflictTx {
    pub guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub payee: Option<String>,
    /// Sum of the positive split values: the size of the transaction.
    pub amount: f64,
    /// Names of the accounts its splits touch, sorted, without repeats.
    pub accounts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConflictChange {
    pub book: ConflictTx,
    pub copy: ConflictTx,
    pub fields: Vec<ConflictField>,
}

/// What differs between the book's and the copy's version of a transaction.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ConflictField {
    Date,
    Description,
    Payee,
    Number,
    Notes,
    Tags,
    /// Accounts, amounts or memos.
    Splits,
    /// Reconcile states only.
    Status,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflictReview {
    pub copy: SyncConflictCopy,
    /// Transactions the copy has and the open book doesn't.
    pub only_in_copy: Vec<ConflictTx>,
    /// Transactions the open book has and the copy doesn't.
    pub only_in_book: Vec<ConflictTx>,
    pub changed: Vec<ConflictChange>,
    pub unchanged: usize,
}

/// Parse a conflict copy's file name against the book's. `None` when it isn't
/// one of this book's copies.
fn parse_conflict_name(book: &Path, file_name: &str) -> Option<SyncConflictCopy> {
    let stem = book.file_stem()?.to_str()?;
    let rest = file_name.strip_prefix(stem)?.strip_prefix(MARKER)?;
    let rest = match book.extension() {
        Some(ext) => rest.strip_suffix(ext.to_str()?)?.strip_suffix('.')?,
        None => rest,
    };
    if rest.is_empty() {
        return None;
    }
    // `<YYYYMMDD>-<HHMMSS>-<device>`; nested conflicts append another marker,
    // so only the first stamp is read.
    let mut parts = rest.splitn(3, '-');
    let (date, time) = (parts.next()?, parts.next().unwrap_or(""));
    let device = parts
        .next()
        .map(|d| d.split('.').next().unwrap_or(d))
        .filter(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(str::to_string);
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    let saved_at = (digits(date, 8) && digits(time, 6)).then(|| {
        format!(
            "{}-{}-{}T{}:{}:{}",
            &date[..4],
            &date[4..6],
            &date[6..],
            &time[..2],
            &time[2..4],
            &time[4..]
        )
    });
    Some(SyncConflictCopy {
        path: book.with_file_name(file_name).to_string_lossy().to_string(),
        file_name: file_name.to_string(),
        saved_at,
        device,
    })
}

/// Conflict copies of `book` in its folder, newest first. A missing or
/// unreadable folder lists nothing: the banner is a hint, not a check.
fn find_conflicts(book: &Path) -> Vec<SyncConflictCopy> {
    let Some(dir) = book.parent() else {
        return Vec::new();
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<SyncConflictCopy> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| parse_conflict_name(book, e.file_name().to_str()?))
        .collect();
    found.sort_by(|a, b| {
        b.saved_at
            .cmp(&a.saved_at)
            .then_with(|| a.file_name.cmp(&b.file_name))
    });
    found
}

/// Resolve a conflict copy by file name next to `book`, refusing anything
/// else (other folders, other books' copies, the book itself).
fn resolve_conflict(book: &Path, file_name: &str) -> Result<SyncConflictCopy, AppError> {
    let not_found = || AppError::NotFound("That conflicting copy is gone".to_string());
    if file_name.contains(['/', '\\']) {
        return Err(not_found());
    }
    let copy = parse_conflict_name(book, file_name).ok_or_else(not_found)?;
    if !Path::new(&copy.path).is_file() {
        return Err(not_found());
    }
    Ok(copy)
}

/// A split's comparable content: account, reduced value, reduced quantity, memo.
type SplitKey = (String, (i64, i64), (i64, i64), String);

/// Everything compared for one transaction.
#[derive(Debug, Default, PartialEq)]
struct TxSnap {
    post_date: String,
    description: Option<String>,
    num: Option<String>,
    notes: Option<String>,
    payee: Option<String>,
    tags: Vec<String>,
    /// (account, reduced value, reduced quantity, memo), sorted.
    splits: Vec<SplitKey>,
    /// Reconcile states in the order of `splits`.
    states: Vec<String>,
    amount: f64,
    accounts: Vec<String>,
}

fn reduce(num: i64, denom: i64) -> (i64, i64) {
    fn gcd(a: i64, b: i64) -> i64 {
        let (mut a, mut b) = (a.unsigned_abs(), b.unsigned_abs());
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a as i64
    }
    let g = gcd(num, denom).max(1);
    let (n, d) = (num / g, denom / g);
    if d < 0 {
        (-n, -d)
    } else {
        (n, d)
    }
}

/// Read every transaction of a book (future-dated ones too: this compares
/// files, it isn't a "current state" view). Only what every version has is
/// read (payees from wherever the copy's version keeps them), so an older copy
/// compares without being migrated.
fn read_snapshot(conn: &Connection) -> Result<HashMap<String, TxSnap>, AppError> {
    let mut txs: HashMap<String, TxSnap> = HashMap::new();
    let mut stmt =
        conn.prepare("SELECT guid, post_date, description, num, notes FROM transactions")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            TxSnap {
                post_date: r.get(1)?,
                description: r.get(2)?,
                num: r.get(3)?,
                notes: r.get(4)?,
                ..TxSnap::default()
            },
        ))
    })?;
    for row in rows {
        let (guid, snap) = row?;
        txs.insert(guid, snap);
    }

    let mut account_names: HashMap<String, String> = HashMap::new();
    let mut stmt = conn.prepare("SELECT guid, name FROM accounts")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (guid, name) = row?;
        account_names.insert(guid, name);
    }

    type SplitRow = (
        String,
        String,
        i64,
        i64,
        i64,
        i64,
        Option<String>,
        Option<String>,
    );
    let mut per_tx: HashMap<String, Vec<(SplitKey, String)>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom,
                reconcile_state, memo
         FROM splits",
    )?;
    let rows = stmt.query_map([], |r| -> rusqlite::Result<SplitRow> {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
            r.get(6)?,
            r.get(7)?,
        ))
    })?;
    for row in rows {
        let (tx, account, vn, vd, qn, qd, state, memo) = row?;
        per_tx.entry(tx).or_default().push((
            (
                account,
                reduce(vn, vd),
                reduce(qn, qd),
                memo.unwrap_or_default(),
            ),
            state.unwrap_or_else(|| "n".to_string()),
        ));
    }
    for (tx, mut splits) in per_tx {
        let Some(snap) = txs.get_mut(&tx) else {
            continue;
        };
        splits.sort();
        snap.amount = splits
            .iter()
            .map(|((_, (n, d), ..), _)| if *d == 0 { 0.0 } else { *n as f64 / *d as f64 })
            .filter(|v| *v > 0.0)
            .sum();
        let mut accounts: Vec<String> = splits
            .iter()
            .map(|((a, ..), _)| account_names.get(a).cloned().unwrap_or_else(|| a.clone()))
            .collect();
        accounts.sort();
        accounts.dedup();
        snap.accounts = accounts;
        (snap.splits, snap.states) = splits.into_iter().unzip();
    }

    // Version 3 keeps the payee on the transaction; older copies in a side table.
    let payee_column = conn
        .prepare("SELECT 1 FROM pragma_table_info('transactions') WHERE name = 'payee_guid'")?
        .exists([])?;
    let mut stmt = conn.prepare(if payee_column {
        "SELECT t.guid, p.name FROM transactions t JOIN payees p ON p.guid = t.payee_guid"
    } else {
        "SELECT tp.tx_guid, p.name FROM transaction_payees tp JOIN payees p ON p.guid = tp.payee_guid"
    })?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (tx, name) = row?;
        if let Some(snap) = txs.get_mut(&tx) {
            snap.payee = Some(name);
        }
    }

    let mut stmt = conn.prepare(
        "SELECT tt.tx_guid, t.name FROM transaction_tags tt JOIN tags t ON t.guid = tt.tag_guid",
    )?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (tx, name) = row?;
        if let Some(snap) = txs.get_mut(&tx) {
            snap.tags.push(name);
        }
    }
    for snap in txs.values_mut() {
        snap.tags.sort_by_key(|t| t.to_lowercase());
    }
    Ok(txs)
}

fn summary(guid: &str, snap: &TxSnap) -> ConflictTx {
    ConflictTx {
        guid: guid.to_string(),
        post_date: snap.post_date.clone(),
        description: snap.description.clone(),
        payee: snap.payee.clone(),
        amount: snap.amount,
        accounts: snap.accounts.clone(),
    }
}

fn changed_fields(book: &TxSnap, copy: &TxSnap) -> Vec<ConflictField> {
    let blank = |s: &Option<String>| s.as_deref().unwrap_or("").to_string();
    let mut fields = Vec::new();
    if book.post_date != copy.post_date {
        fields.push(ConflictField::Date);
    }
    if blank(&book.description) != blank(&copy.description) {
        fields.push(ConflictField::Description);
    }
    if book.payee != copy.payee {
        fields.push(ConflictField::Payee);
    }
    if blank(&book.num) != blank(&copy.num) {
        fields.push(ConflictField::Number);
    }
    if blank(&book.notes) != blank(&copy.notes) {
        fields.push(ConflictField::Notes);
    }
    if book.tags != copy.tags {
        fields.push(ConflictField::Tags);
    }
    if book.splits != copy.splits {
        fields.push(ConflictField::Splits);
    } else if book.states != copy.states {
        fields.push(ConflictField::Status);
    }
    fields
}

/// Newest first, then by GUID so the order is stable.
fn by_date_desc(a: &ConflictTx, b: &ConflictTx) -> std::cmp::Ordering {
    b.post_date
        .cmp(&a.post_date)
        .then_with(|| a.guid.cmp(&b.guid))
}

fn compare(
    copy: SyncConflictCopy,
    book: &HashMap<String, TxSnap>,
    other: &HashMap<String, TxSnap>,
) -> SyncConflictReview {
    let mut only_in_copy: Vec<ConflictTx> = other
        .iter()
        .filter(|(guid, _)| !book.contains_key(*guid))
        .map(|(guid, snap)| summary(guid, snap))
        .collect();
    let mut only_in_book: Vec<ConflictTx> = book
        .iter()
        .filter(|(guid, _)| !other.contains_key(*guid))
        .map(|(guid, snap)| summary(guid, snap))
        .collect();
    let mut changed = Vec::new();
    let mut unchanged = 0;
    // BTreeMap only for a deterministic walk; the result is sorted anyway.
    let shared: BTreeMap<&String, (&TxSnap, &TxSnap)> = book
        .iter()
        .filter_map(|(guid, mine)| other.get(guid).map(|theirs| (guid, (mine, theirs))))
        .collect();
    for (guid, (mine, theirs)) in shared {
        let fields = changed_fields(mine, theirs);
        if fields.is_empty() {
            unchanged += 1;
        } else {
            changed.push(ConflictChange {
                book: summary(guid, mine),
                copy: summary(guid, theirs),
                fields,
            });
        }
    }
    only_in_copy.sort_by(by_date_desc);
    only_in_book.sort_by(by_date_desc);
    changed.sort_by(|a, b| by_date_desc(&a.book, &b.book));
    SyncConflictReview {
        copy,
        only_in_copy,
        only_in_book,
        changed,
        unchanged,
    }
}

/// Read a conflict copy without writing to it: a Spendo stamp check, then a
/// `SQLITE_OPEN_READ_ONLY` connection (no migration, no journal change).
fn read_copy(path: &Path) -> Result<HashMap<String, TxSnap>, AppError> {
    crate::books::validate_spendo_db(path)?;
    let conn = db::connect_read_only(path)
        .map_err(|e| AppError::Io(format!("We couldn't open that copy: {e}")))?;
    read_snapshot(&conn)
}

fn active_path(state: &State<'_, AppState>) -> Option<PathBuf> {
    db::lock(&state.db_path).clone()
}

/// Conflict copies next to the open book, newest first; empty without one.
#[tauri::command]
#[specta::specta]
pub async fn list_sync_conflicts(
    state: State<'_, AppState>,
) -> Result<Vec<SyncConflictCopy>, AppError> {
    let Some(book) = active_path(&state) else {
        return Ok(Vec::new());
    };
    db::run_io(move || Ok(find_conflicts(&book))).await
}

/// Compare a conflict copy (by file name, next to the open book) with the open
/// book. The copy is read first, outside the DB lock; the book after, under it.
#[tauri::command]
#[specta::specta]
pub async fn review_sync_conflict(
    state: State<'_, AppState>,
    file_name: String,
) -> Result<SyncConflictReview, AppError> {
    let Some(book) = active_path(&state) else {
        return Err(AppError::Validation("No book open".to_string()));
    };
    let (copy, theirs) = db::run_io(move || {
        let copy = resolve_conflict(&book, &file_name)?;
        let theirs = read_copy(Path::new(&copy.path))?;
        Ok((copy, theirs))
    })
    .await?;
    let mine = db::run_blocking(state.db.clone(), read_snapshot).await?;
    Ok(compare(copy, &mine, &theirs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("spendo-conflicts-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn book_at(path: &Path) -> Connection {
        let conn = db::connect_db(&path.to_path_buf()).unwrap();
        db::init_db(&conn).unwrap();
        db::stamp_app_id(&conn);
        conn.execute_batch(
            "INSERT INTO accounts (guid, name, account_type) VALUES
                ('a-bank', 'Bank', 'BANK'), ('a-food', 'Food', 'EXPENSE'), ('a-rent', 'Rent', 'EXPENSE');
             INSERT INTO transactions (guid, post_date, description) VALUES
                ('t-same', '2026-01-01', 'Coffee'),
                ('t-edit', '2026-01-02', 'Groceries'),
                ('t-gone', '2026-01-03', 'Cinema');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state) VALUES
                ('s1', 't-same', 'a-food', 350, 100, 350, 100, 'n'),
                ('s2', 't-same', 'a-bank', -350, 100, -350, 100, 'n'),
                ('s3', 't-edit', 'a-food', 4000, 100, 4000, 100, 'n'),
                ('s4', 't-edit', 'a-bank', -4000, 100, -4000, 100, 'n'),
                ('s5', 't-gone', 'a-food', 1200, 100, 1200, 100, 'n'),
                ('s6', 't-gone', 'a-bank', -1200, 100, -1200, 100, 'n');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn parses_syncthing_names_for_this_book_only() {
        let book = Path::new("/sync/home.spendo");
        let copy =
            parse_conflict_name(book, "home.sync-conflict-20261125-143022-ABCDEF7.spendo").unwrap();
        assert_eq!(copy.saved_at.as_deref(), Some("2026-11-25T14:30:22"));
        assert_eq!(copy.device.as_deref(), Some("ABCDEF7"));
        assert_eq!(
            copy.path,
            "/sync/home.sync-conflict-20261125-143022-ABCDEF7.spendo"
        );

        // A nested conflict keeps its first stamp.
        let nested = parse_conflict_name(
            book,
            "home.sync-conflict-20261125-143022-ABCDEF7.sync-conflict-20261126-090000-XYZ1234.spendo",
        )
        .unwrap();
        assert_eq!(nested.saved_at.as_deref(), Some("2026-11-25T14:30:22"));
        assert_eq!(nested.device.as_deref(), Some("ABCDEF7"));

        // An unusual stamp still counts, just without a date.
        let odd = parse_conflict_name(book, "home.sync-conflict-later.spendo").unwrap();
        assert_eq!(odd.saved_at, None);

        for other in [
            "home.spendo",
            "home.sync-conflict-20261125-143022-ABCDEF7.spendo-journal",
            "home.sync-conflict-20261125-143022-ABCDEF7.db",
            "work.sync-conflict-20261125-143022-ABCDEF7.spendo",
            "homework.sync-conflict-20261125-143022-ABCDEF7.spendo",
            "home.sync-conflict-.spendo",
        ] {
            assert!(parse_conflict_name(book, other).is_none(), "{other}");
        }
    }

    #[test]
    fn finds_copies_newest_first_and_resolves_only_them() {
        let dir = tmp_dir("find");
        let book = dir.join("home.spendo");
        std::fs::write(&book, b"").unwrap();
        for name in [
            "home.sync-conflict-20261101-080000-AAAAAAA.spendo",
            "home.sync-conflict-20261125-143022-BBBBBBB.spendo",
            "other.sync-conflict-20261125-143022-BBBBBBB.spendo",
        ] {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        std::fs::create_dir(dir.join("home.sync-conflict-20261126-000000-CCCCCCC.spendo")).unwrap();

        let found: Vec<String> = find_conflicts(&book)
            .into_iter()
            .map(|c| c.file_name)
            .collect();
        assert_eq!(
            found,
            [
                "home.sync-conflict-20261125-143022-BBBBBBB.spendo",
                "home.sync-conflict-20261101-080000-AAAAAAA.spendo"
            ]
        );

        assert!(
            resolve_conflict(&book, "home.sync-conflict-20261101-080000-AAAAAAA.spendo").is_ok()
        );
        for bad in [
            "other.sync-conflict-20261125-143022-BBBBBBB.spendo",
            "../home.sync-conflict-20261101-080000-AAAAAAA.spendo",
            "home.sync-conflict-20261231-000000-DDDDDDD.spendo",
            "home.spendo",
        ] {
            assert_eq!(
                resolve_conflict(&book, bad).unwrap_err().kind(),
                "notFound",
                "{bad}"
            );
        }
        assert!(find_conflicts(&dir.join("missing").join("home.spendo")).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn review_lists_added_removed_and_changed_without_touching_the_copy() {
        let dir = tmp_dir("review");
        let book_path = dir.join("home.spendo");
        let copy_path = dir.join("home.sync-conflict-20261125-143022-ABCDEF7.spendo");
        let book = book_at(&book_path);
        let copy = book_at(&copy_path);

        // The copy: one new transaction, one deleted, one edited (amount,
        // description, tag), and a reconcile-only change on another.
        copy.execute_batch(
            "INSERT INTO transactions (guid, post_date, description) VALUES ('t-new', '2026-01-05', 'Rent');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom) VALUES
                ('s7', 't-new', 'a-rent', 90000, 100, 90000, 100),
                ('s8', 't-new', 'a-bank', -90000, 100, -90000, 100);
             DELETE FROM splits WHERE tx_guid = 't-gone';
             DELETE FROM transactions WHERE guid = 't-gone';
             UPDATE transactions SET description = 'Big groceries' WHERE guid = 't-edit';
             UPDATE splits SET value_num = 45, value_denom = 1, quantity_num = 4500 WHERE guid = 's3';
             UPDATE splits SET value_num = -45, value_denom = 1, quantity_num = -4500 WHERE guid = 's4';
             INSERT INTO tags (guid, name, created_at) VALUES ('g1', 'Holiday', '2026-01-01');
             INSERT INTO transaction_tags (tx_guid, tag_guid) VALUES ('t-edit', 'g1');
             UPDATE splits SET reconcile_state = 'c' WHERE guid = 's2';",
        )
        .unwrap();
        // Equal fractions written differently are no change.
        book.execute_batch("UPDATE splits SET value_num = 35, value_denom = 10 WHERE guid = 's1';")
            .unwrap();
        drop(copy);
        let before = std::fs::read(&copy_path).unwrap();
        let modified = std::fs::metadata(&copy_path).unwrap().modified().unwrap();

        let copy_meta = resolve_conflict(
            &book_path,
            "home.sync-conflict-20261125-143022-ABCDEF7.spendo",
        )
        .unwrap();
        let theirs = read_copy(&copy_path).unwrap();
        let mine = read_snapshot(&book).unwrap();
        let review = compare(copy_meta, &mine, &theirs);

        assert_eq!(review.only_in_copy.len(), 1);
        let added = &review.only_in_copy[0];
        assert_eq!(added.guid, "t-new");
        assert!((added.amount - 900.0).abs() < 1e-9);
        assert_eq!(added.accounts, ["Bank", "Rent"]);

        assert_eq!(
            review
                .only_in_book
                .iter()
                .map(|t| t.guid.as_str())
                .collect::<Vec<_>>(),
            ["t-gone"]
        );

        let changes: Vec<(&str, &[ConflictField])> = review
            .changed
            .iter()
            .map(|c| (c.book.guid.as_str(), c.fields.as_slice()))
            .collect();
        assert_eq!(
            changes,
            [
                (
                    "t-edit",
                    &[
                        ConflictField::Description,
                        ConflictField::Tags,
                        ConflictField::Splits
                    ][..]
                ),
                ("t-same", &[ConflictField::Status][..])
            ]
        );
        let edit = &review.changed[0];
        assert!((edit.book.amount - 40.0).abs() < 1e-9);
        assert!((edit.copy.amount - 45.0).abs() < 1e-9);
        assert_eq!(review.unchanged, 0);

        // Not a byte or a timestamp changed, and no sidecar appeared.
        assert_eq!(std::fs::read(&copy_path).unwrap(), before);
        assert_eq!(
            std::fs::metadata(&copy_path).unwrap().modified().unwrap(),
            modified
        );
        let sidecars = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("-journal"))
            .count();
        assert_eq!(sidecars, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_copy_rejects_files_that_are_not_books() {
        let dir = tmp_dir("not-book");
        let path = dir.join("home.sync-conflict-20261125-143022-ABCDEF7.spendo");
        std::fs::write(&path, "not sqlite").unwrap();
        assert!(read_copy(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn copies_from_before_version_3_read_their_payees() {
        // Version 2 kept payees in `transaction_payees`; a copy saved then
        // must compare the same as the book after its upgrade.
        let old = Connection::open_in_memory().unwrap();
        old.execute_batch(include_str!("migrations/fixtures/v2.sql"))
            .unwrap();
        old.pragma_update(None, "user_version", 2).unwrap();
        let before = read_snapshot(&old).unwrap();
        assert_eq!(before["t-rent1"].payee.as_deref(), Some("Landlord"));

        crate::migrations::migrate(&old, None).unwrap();
        assert_eq!(read_snapshot(&old).unwrap(), before);
    }
}
