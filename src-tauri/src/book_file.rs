//! Detects a book file replaced or changed behind the open connection.
//!
//! Sync tools (Syncthing & co.) update a file by writing a temp copy and
//! renaming it over the original. A connection opened before that keeps
//! pointing at the old, now unlinked inode: every later write lands there and
//! is lost when the app quits. So the identity of the file each connection
//! opened is recorded, checked before every checked DB command
//! (`db::with_conn`) and refreshed after it, so our own writes never count as
//! a change. A mismatch fails the command with `AppError::BookChanged` and the
//! frontend reopens the book (`books::reload_changed_book`).
//!
//! Records are keyed by the connection's own file name (`Connection::path`),
//! so in-memory connections (placeholder, tests) are never watched and a
//! process-wide registry stays correct with one active book.

use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crate::db::{self, AppError};

/// What a file looked like when last seen by this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    /// `(device, inode)` on Unix. Windows has no stable equivalent in std, but
    /// there SQLite holds the file open without delete sharing, so it can't be
    /// renamed over while the book is open; size and mtime still catch edits.
    file_id: Option<(u64, u64)>,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FileChange {
    Unchanged,
    /// Same file, different size or mtime: written by someone else.
    Modified,
    /// Another file now sits at the path (rename-over).
    Replaced,
    /// Nothing (readable) at the path anymore.
    Missing,
}

impl FileIdentity {
    pub(crate) fn read(path: &Path) -> Option<FileIdentity> {
        let meta = std::fs::metadata(path).ok()?;
        Some(FileIdentity {
            file_id: file_id(&meta),
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }

    fn same_file(&self, other: &FileIdentity) -> bool {
        self.file_id == other.file_id
    }
}

#[cfg(unix)]
fn file_id(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn file_id(_meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

/// Compare the recorded identity with what is on disk now.
pub(crate) fn compare(recorded: &FileIdentity, current: Option<&FileIdentity>) -> FileChange {
    match current {
        None => FileChange::Missing,
        Some(now) if !recorded.same_file(now) => FileChange::Replaced,
        Some(now) if now != recorded => FileChange::Modified,
        Some(_) => FileChange::Unchanged,
    }
}

fn registry() -> &'static Mutex<HashMap<String, FileIdentity>> {
    static WATCHED: OnceLock<Mutex<HashMap<String, FileIdentity>>> = OnceLock::new();
    WATCHED.get_or_init(Default::default)
}

/// File name of a file-backed connection; `None` for in-memory ones.
fn book_file(conn: &Connection) -> Option<String> {
    conn.path().filter(|p| !p.is_empty()).map(str::to_string)
}

/// Start watching the file `conn` has open, replacing any earlier record for
/// that path. Call right after opening a book (startup, open, create, Save As,
/// reload) — never on a connection that may already be stale.
pub(crate) fn watch(conn: &Connection) {
    let Some(path) = book_file(conn) else { return };
    let mut watched = db::lock(registry());
    match FileIdentity::read(Path::new(&path)) {
        Some(identity) => {
            watched.insert(path, identity);
        }
        None => {
            watched.remove(&path);
        }
    }
}

/// Fail with `BookChanged` when the file `conn` opened is no longer the one
/// on disk. Unwatched connections always pass.
pub(crate) fn ensure_unchanged(conn: &Connection) -> Result<(), AppError> {
    let Some(path) = book_file(conn) else {
        return Ok(());
    };
    let watched = db::lock(registry());
    let Some(recorded) = watched.get(&path) else {
        return Ok(());
    };
    match compare(recorded, FileIdentity::read(Path::new(&path)).as_ref()) {
        FileChange::Unchanged => Ok(()),
        FileChange::Missing => Err(AppError::BookChanged(
            "The book file was moved or deleted, so nothing was saved".to_string(),
        )),
        FileChange::Modified | FileChange::Replaced => Err(AppError::BookChanged(
            "This book was updated on another device, so nothing was saved".to_string(),
        )),
    }
}

/// Adopt our own writes after a checked command: record the new size and
/// mtime, but only while the path still holds the same file. A rename-over
/// that happened mid-command stays detected for the next one.
pub(crate) fn refresh(conn: &Connection) {
    let Some(path) = book_file(conn) else { return };
    let mut watched = db::lock(registry());
    let Some(recorded) = watched.get_mut(&path) else {
        return;
    };
    if let Some(now) = FileIdentity::read(Path::new(&path)) {
        if recorded.same_file(&now) {
            *recorded = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    fn identity(file_id: (u64, u64), len: u64, secs: u64) -> FileIdentity {
        FileIdentity {
            file_id: Some(file_id),
            len,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)),
        }
    }

    #[test]
    fn compare_tells_unchanged_modified_replaced_and_missing_apart() {
        let recorded = identity((1, 10), 4096, 100);
        assert_eq!(
            compare(&recorded, Some(&recorded.clone())),
            FileChange::Unchanged
        );
        assert_eq!(
            compare(&recorded, Some(&identity((1, 10), 8192, 100))),
            FileChange::Modified
        );
        assert_eq!(
            compare(&recorded, Some(&identity((1, 10), 4096, 200))),
            FileChange::Modified
        );
        // Same size and mtime but another inode: a rename-over.
        assert_eq!(
            compare(&recorded, Some(&identity((1, 11), 4096, 100))),
            FileChange::Replaced
        );
        assert_eq!(compare(&recorded, None), FileChange::Missing);
    }

    fn tmp_book(tag: &str) -> (PathBuf, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("spendo-book-file-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.spendo");
        (dir, path)
    }

    fn open(path: &Path) -> Connection {
        let conn = db::connect_db(&path.to_path_buf()).unwrap();
        db::init_db(&conn).unwrap();
        conn
    }

    fn insert_commodity(conn: &Connection, guid: &str) {
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES (?1, 'X', 'CURRENCY')",
            [guid],
        )
        .unwrap();
    }

    #[test]
    fn own_writes_are_adopted_after_refresh() {
        let (dir, path) = tmp_book("own");
        let conn = open(&path);
        watch(&conn);
        assert!(ensure_unchanged(&conn).is_ok());
        insert_commodity(&conn, "c-1");
        refresh(&conn);
        assert!(ensure_unchanged(&conn).is_ok());
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_renamed_over_file_is_detected() {
        let (dir, path) = tmp_book("replaced");
        let conn = open(&path);
        watch(&conn);

        // What a sync tool does: write a temp copy, rename it over the book.
        let incoming = dir.join(".syncthing.book.spendo.tmp");
        {
            let other = open(&incoming);
            insert_commodity(&other, "c-remote");
        }
        std::fs::rename(&incoming, &path).unwrap();

        let err = ensure_unchanged(&conn).unwrap_err();
        assert_eq!(err.kind(), "bookChanged");
        // Refreshing must not adopt the new file as ours.
        refresh(&conn);
        assert!(ensure_unchanged(&conn).is_err());

        // Reopening (the reload path) watches the new file.
        let reopened = open(&path);
        watch(&reopened);
        assert!(ensure_unchanged(&reopened).is_ok());
        let remote: i64 = reopened
            .query_row(
                "SELECT COUNT(*) FROM commodities WHERE guid = 'c-remote'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remote, 1);
        drop((conn, reopened));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Done-when of the issue: a write after a rename-over is refused, not
    /// silently written into the orphaned file.
    #[test]
    fn checked_commands_refuse_to_write_after_a_rename_over() {
        let (dir, path) = tmp_book("refused");
        let conn = open(&path);
        watch(&conn);
        let db = Mutex::new(conn);
        db::with_conn(&db, "test::write", |conn| {
            insert_commodity(conn, "c-before");
            Ok(())
        })
        .unwrap();

        let incoming = dir.join("incoming.spendo");
        drop(open(&incoming));
        std::fs::rename(&incoming, &path).unwrap();

        let mut ran = false;
        let err = db::with_conn(&db, "test::write", |conn| {
            ran = true;
            insert_commodity(conn, "c-lost");
            Ok(())
        })
        .unwrap_err();
        assert_eq!(err.kind(), "bookChanged");
        assert!(!ran, "the closure must not run on a replaced book");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_deleted_file_is_detected() {
        let (dir, path) = tmp_book("deleted");
        let conn = open(&path);
        watch(&conn);
        std::fs::remove_file(&path).unwrap();
        assert!(ensure_unchanged(&conn).is_err());
        drop(conn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn in_memory_connections_are_never_watched() {
        let conn = db::open_placeholder().unwrap();
        watch(&conn);
        assert!(ensure_unchanged(&conn).is_ok());
    }
}
