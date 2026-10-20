use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager, State};

use crate::db::{run_blocking, run_io, with_tx_err, AppError, AppState};
use crate::utils;

/// Files attached to a transaction. Up to `MAX_ATTACHMENTS_PER_TX` files per
/// transaction (PDF or images); files accumulate — adding never replaces or
/// clears existing ones. Bytes live in the `.spendo`
/// book (see the `attachments` table comment in `db.rs`) so the file stays
/// portable; deleting a transaction removes its attachments permanently.
pub const MAX_ATTACHMENT_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_ATTACHMENTS_PER_TX: usize = 10;

/// Extension → MIME allowlist. The frontend file-dialog filter mirrors this,
/// but the backend is the source of truth — never trust the caller.
const ALLOWED_TYPES: [(&str, &str); 6] = [
    ("pdf", "application/pdf"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub guid: String,
    pub filename: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentAddInput {
    pub tx_guid: String,
    pub paths: Vec<String>,
}

/// A file read from disk and validated, ready to insert.
#[derive(Debug)]
struct PreparedAttachment {
    filename: String,
    mime_type: &'static str,
    data: Vec<u8>,
}

fn mime_for_path(path: &Path) -> Result<&'static str, AppError> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    ALLOWED_TYPES
        .iter()
        .find(|(candidate, _)| *candidate == extension)
        .map(|(_, mime)| *mime)
        .ok_or_else(|| {
            AppError::Validation(if extension.is_empty() {
                format!("Unsupported file type: {}", path.display())
            } else {
                format!("Unsupported file type: .{extension}")
            })
        })
}

/// Read + validate one picked/dropped file. Runs before the DB lock is taken
/// (disk I/O never happens while holding the connection).
fn prepare_attachment(path: &str) -> Result<PreparedAttachment, AppError> {
    let source = Path::new(path);
    let filename = source
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            AppError::Validation(format!("Unreadable file name: {}", source.display()))
        })?;
    let mime_type = mime_for_path(source)?;
    let metadata = std::fs::metadata(source)
        .map_err(|e| AppError::Io(format!("Failed to read {}: {e}", source.display())))?;
    if !metadata.is_file() {
        return Err(AppError::Validation(format!(
            "Not a file: {}",
            source.display()
        )));
    }
    if metadata.len() == 0 {
        return Err(AppError::Validation(format!("{filename} is empty")));
    }
    if metadata.len() > MAX_ATTACHMENT_BYTES {
        return Err(AppError::Validation(format!(
            "{filename} is larger than {} MB",
            MAX_ATTACHMENT_BYTES / (1024 * 1024)
        )));
    }
    let data = std::fs::read(source)
        .map_err(|e| AppError::Io(format!("Failed to read {}: {e}", source.display())))?;
    // The file could have grown between `metadata` and `read`.
    if data.len() as u64 > MAX_ATTACHMENT_BYTES {
        return Err(AppError::Validation(format!(
            "{filename} is larger than {} MB",
            MAX_ATTACHMENT_BYTES / (1024 * 1024)
        )));
    }
    let filename: String = filename.chars().take(255).collect();
    Ok(PreparedAttachment {
        filename,
        mime_type,
        data,
    })
}

/// Safe basename for the cache file `open_attachment` writes before handing it
/// to the OS. Strips path separators and dot runs so a crafted attachment name
/// can never escape the preview folder.
fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let collapsed = cleaned.replace("..", "_");
    let trimmed = collapsed.trim().trim_matches('.');
    let capped: String = trimmed.chars().take(120).collect();
    if capped.is_empty() {
        "attachment".to_string()
    } else {
        capped
    }
}

fn list_attachments(conn: &Connection, tx_guid: &str) -> Result<Vec<AttachmentInfo>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT guid, filename, mime_type, byte_size, created_at
         FROM attachments
         WHERE tx_guid = ?1
         ORDER BY created_at ASC, guid ASC",
    )?;
    let rows = stmt
        .query_map(params![tx_guid], |row| {
            Ok(AttachmentInfo {
                guid: row.get(0)?,
                filename: row.get(1)?,
                mime_type: row.get(2)?,
                byte_size: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Insert validated files for one transaction: existence and per-transaction
/// count checks run inside the same transaction as the inserts, so a batch
/// that would exceed the limit fails atomically. Files accumulate — existing
/// rows are never deleted here.
fn insert_attachments(
    conn: &Connection,
    tx_guid: &str,
    prepared: Vec<PreparedAttachment>,
) -> Result<Vec<AttachmentInfo>, AppError> {
    if prepared.is_empty() {
        return Err(AppError::Validation(
            "Pick a file to attach first".to_string(),
        ));
    }
    if prepared.len() > MAX_ATTACHMENTS_PER_TX {
        return Err(AppError::Validation(format!(
            "Keep it to at most {MAX_ATTACHMENTS_PER_TX} attachments per transaction"
        )));
    }
    with_tx_err(conn, || {
        let exists = conn
            .query_row(
                "SELECT 1 FROM transactions WHERE guid = ?1",
                params![tx_guid],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Err(AppError::NotFound(
                "Transaction not found. It may have been removed".to_string(),
            ));
        }

        let existing: i64 = conn.query_row(
            "SELECT COUNT(*) FROM attachments WHERE tx_guid = ?1",
            params![tx_guid],
            |row| row.get(0),
        )?;
        if existing as usize + prepared.len() > MAX_ATTACHMENTS_PER_TX {
            return Err(AppError::Validation(format!(
                "Keep it to at most {MAX_ATTACHMENTS_PER_TX} attachments per transaction"
            )));
        }

        let mut infos = Vec::with_capacity(prepared.len());
        for attachment in prepared {
            let guid = uuid::Uuid::new_v4().to_string();
            let byte_size = attachment.data.len() as i64;
            let created_at = utils::iso_now();
            let insert = conn.execute(
                "INSERT INTO attachments
                 (guid, tx_guid, filename, mime_type, byte_size, data, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    &guid,
                    tx_guid,
                    &attachment.filename,
                    attachment.mime_type,
                    byte_size,
                    attachment.data,
                    &created_at,
                ],
            );
            if let Err(e) = insert {
                // Books created before the multi-attachment schema still carry
                // `UNIQUE(tx_guid)` (no migrations by design). Surface that as
                // guidance instead of a raw constraint message.
                let extended_code = match &e {
                    rusqlite::Error::SqliteFailure(err, _) => Some(err.extended_code),
                    _ => None,
                };
                if extended_code == Some(rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE) {
                    return Err(AppError::Validation(
                        "This book holds one attachment per transaction. Clear your data or start a new book to add more"
                            .to_string(),
                    ));
                }
                return Err(AppError::from(e));
            }
            infos.push(AttachmentInfo {
                guid,
                filename: attachment.filename,
                mime_type: attachment.mime_type.to_string(),
                byte_size,
                created_at,
            });
        }
        Ok(infos)
    })
}

fn delete_attachment_inner(conn: &Connection, guid: &str) -> Result<(), AppError> {
    let deleted = conn.execute("DELETE FROM attachments WHERE guid = ?1", params![guid])?;
    if deleted == 0 {
        return Err(AppError::NotFound(
            "Attachment not found. It may have been removed".to_string(),
        ));
    }
    Ok(())
}

/// `(filename, mime_type, bytes)` for one attachment, or `NotFound`.
fn fetch_blob(conn: &Connection, guid: &str) -> Result<(String, String, Vec<u8>), AppError> {
    conn.query_row(
        "SELECT filename, mime_type, data FROM attachments WHERE guid = ?1",
        params![guid],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound("Attachment not found. It may have been removed".to_string()))
}

#[tauri::command]
pub async fn fetch_attachments(
    state: State<'_, AppState>,
    tx_guid: String,
) -> Result<Vec<AttachmentInfo>, String> {
    run_blocking(state.db.clone(), move |conn| {
        list_attachments(conn, &tx_guid).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn add_attachments(
    state: State<'_, AppState>,
    input: AttachmentAddInput,
) -> Result<Vec<AttachmentInfo>, String> {
    let tx_guid = input.tx_guid;
    // Disk I/O first (house rule): read + validate the file before touching
    // the DB lock; a bad file fails with its own message.
    let prepared = run_io(move || {
        input
            .paths
            .iter()
            .map(|path| prepare_attachment(path))
            .collect::<Result<Vec<_>, _>>()
            .map_err(String::from)
    })
    .await?;
    run_blocking(state.db.clone(), move |conn| {
        insert_attachments(conn, &tx_guid, prepared).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn delete_attachment(state: State<'_, AppState>, guid: String) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        delete_attachment_inner(conn, &guid).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn export_attachment(
    state: State<'_, AppState>,
    guid: String,
    dest_path: String,
) -> Result<(), String> {
    state
        .ensure_not_active_book(Path::new(dest_path.trim()))
        .map_err(String::from)?;
    let (_, _, data) = run_blocking(state.db.clone(), move |conn| {
        fetch_blob(conn, &guid).map_err(String::from)
    })
    .await?;
    run_io(move || {
        std::fs::write(&dest_path, data).map_err(|e| format!("Failed to write file: {e}"))
    })
    .await
}

/// Write the bytes to the app cache and hand the file to the OS default
/// application. The opener runs in Rust, so no new capability scopes are
/// needed for arbitrary user-chosen paths.
#[tauri::command]
pub async fn open_attachment(
    app: AppHandle,
    state: State<'_, AppState>,
    guid: String,
) -> Result<(), String> {
    let blob_guid = guid.clone();
    let (filename, _, data) = run_blocking(state.db.clone(), move |conn| {
        fetch_blob(conn, &blob_guid).map_err(String::from)
    })
    .await?;
    let path = run_io(move || {
        let dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| format!("Failed to locate the cache folder: {e}"))?
            .join("attachment-previews");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create preview folder: {e}"))?;
        let file = dir.join(format!("{guid}-{}", sanitize_file_name(&filename)));
        std::fs::write(&file, data).map_err(|e| format!("Failed to write preview file: {e}"))?;
        Ok(file)
    })
    .await?;
    tauri_plugin_opener::open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use std::path::PathBuf;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t1', '2024-01-10')",
            [],
        )
        .unwrap();
        conn
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("spendo-attachments-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> String {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().to_string()
    }

    fn prepared(path: &str) -> PreparedAttachment {
        prepare_attachment(path).expect("file must prepare")
    }

    #[test]
    fn add_list_fetch_and_delete_round_trip() {
        let conn = setup();
        let dir = tmp_dir("roundtrip");
        let path = write_file(&dir, "receipt.pdf", b"fake-pdf-bytes");

        let infos = insert_attachments(&conn, "t1", vec![prepared(&path)]).unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].filename, "receipt.pdf");
        assert_eq!(infos[0].mime_type, "application/pdf");
        assert_eq!(infos[0].byte_size, 14);

        let listed = list_attachments(&conn, "t1").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], infos[0]);

        let (filename, mime_type, data) = fetch_blob(&conn, &infos[0].guid).unwrap();
        assert_eq!(filename, "receipt.pdf");
        assert_eq!(mime_type, "application/pdf");
        assert_eq!(data, b"fake-pdf-bytes");

        delete_attachment_inner(&conn, &infos[0].guid).unwrap();
        assert!(list_attachments(&conn, "t1").unwrap().is_empty());
        let err = delete_attachment_inner(&conn, &infos[0].guid).unwrap_err();
        assert!(err.to_string().contains("not found"), "unexpected: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prepare_rejects_unsupported_empty_missing_and_oversized() {
        let dir = tmp_dir("reject");
        // Unsupported extensions (text is not accepted; PDF + images are).
        let txt = write_file(&dir, "notes.txt", b"hi");
        assert!(prepare_attachment(&txt)
            .unwrap_err()
            .to_string()
            .contains("Unsupported"));
        // Accepted image types prepare fine.
        for name in [
            "photo.png",
            "photo.jpg",
            "photo.jpeg",
            "anim.gif",
            "img.webp",
        ] {
            let path = write_file(&dir, name, b"hi");
            let ok = prepared(&path);
            assert!(!ok.filename.is_empty(), "unexpected empty name for {name}");
        }
        // Empty file.
        let empty = write_file(&dir, "empty.pdf", b"");
        assert!(prepare_attachment(&empty)
            .unwrap_err()
            .to_string()
            .contains("empty"));
        // Missing file.
        let missing = dir.join("nope.pdf");
        assert!(prepare_attachment(&missing.to_string_lossy()).is_err());
        // Oversized file: sparse on disk, no need to materialize 10 MB.
        let big = dir.join("big.pdf");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MAX_ATTACHMENT_BYTES + 1)
            .unwrap();
        assert!(prepare_attachment(&big.to_string_lossy())
            .unwrap_err()
            .to_string()
            .contains("larger than"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn add_enforces_tx_existence_limit_and_appends() {
        let conn = setup();
        let dir = tmp_dir("append");
        let first = write_file(&dir, "a.pdf", b"x");
        let second = write_file(&dir, "b.png", b"yy");

        let err = insert_attachments(&conn, "nope", vec![prepared(&first)]).unwrap_err();
        assert!(err.to_string().contains("not found"), "unexpected: {err}");

        // More than the per-transaction limit in a single call is rejected.
        let too_many: Vec<PreparedAttachment> = (0..MAX_ATTACHMENTS_PER_TX + 1)
            .map(|_| prepared(&first))
            .collect();
        let err = insert_attachments(&conn, "t1", too_many).unwrap_err();
        assert!(err.to_string().contains("at most"), "unexpected: {err}");

        // Adding files accumulates instead of replacing.
        let infos = insert_attachments(&conn, "t1", vec![prepared(&first)]).unwrap();
        assert_eq!(infos.len(), 1);
        let more = insert_attachments(&conn, "t1", vec![prepared(&second)]).unwrap();
        assert_eq!(more.len(), 1);
        assert_eq!(more[0].filename, "b.png");
        assert_eq!(more[0].mime_type, "image/png");
        let listed = list_attachments(&conn, "t1").unwrap();
        assert_eq!(listed.len(), 2);

        // Filling up to the limit works; one more is rejected.
        for i in 2..MAX_ATTACHMENTS_PER_TX {
            let path = write_file(&dir, &format!("f{i}.pdf"), b"z");
            insert_attachments(&conn, "t1", vec![prepared(&path)]).unwrap();
        }
        assert_eq!(
            list_attachments(&conn, "t1").unwrap().len(),
            MAX_ATTACHMENTS_PER_TX
        );
        let err = insert_attachments(&conn, "t1", vec![prepared(&first)]).unwrap_err();
        assert!(err.to_string().contains("at most"), "unexpected: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_transaction_cascades_to_attachments() {
        let conn = setup();
        let dir = tmp_dir("cascade");
        let path = write_file(&dir, "a.pdf", b"x");
        insert_attachments(&conn, "t1", vec![prepared(&path)]).unwrap();

        conn.execute("DELETE FROM transactions WHERE guid = 't1'", [])
            .unwrap();
        assert!(list_attachments(&conn, "t1").unwrap().is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sanitize_file_name_never_yields_path_parts() {
        assert_eq!(sanitize_file_name("receipt.pdf"), "receipt.pdf");
        for name in ["../../etc/passwd", "..", "a/b\\c:d.pdf", "", "   ", "..."] {
            let out = sanitize_file_name(name);
            assert!(!out.is_empty(), "empty sanitized name for {name:?}");
            assert!(
                !out.contains('/') && !out.contains('\\') && !out.contains(".."),
                "unsafe sanitized name {out:?} for {name:?}"
            );
        }
    }
}
