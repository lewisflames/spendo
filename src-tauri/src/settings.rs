use serde::Serialize;
use tauri::State;

use crate::db::{clear_db, run_blocking, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataInfo {
    pub db_path: String,
}

#[tauri::command]
pub async fn clear_app_db(state: State<'_, AppState>) -> Result<(), String> {
    run_blocking(state.db.clone(), |conn| {
        clear_db(conn).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}

/// Writes text (e.g. a CSV export) to a user-chosen path. The frontend shows
/// the Tauri save dialog first and passes the confirmed path here; this
/// command never touches the DB.
#[tauri::command]
pub async fn write_text_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|e| format!("Failed to write file: {e}"))?;
    Ok(())
}
/// Reads only the precomputed active-book path — no DB access. While no book
/// is open yet (fresh install) it falls back to the legacy default path so
/// the UI still has something to display; prefer `books::fetch_active_book`
/// when nullability matters.
#[tauri::command]
pub async fn fetch_data_info(state: State<'_, AppState>) -> Result<DataInfo, String> {
    let guard = state.db_path.lock().map_err(|e| e.to_string())?;
    let path = guard
        .clone()
        .unwrap_or_else(|| crate::db::get_db_path(&state.app_data_dir));
    Ok(DataInfo {
        db_path: path.to_string_lossy().to_string(),
    })
}
