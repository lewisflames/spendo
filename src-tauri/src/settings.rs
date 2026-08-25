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

/// Reads only the precomputed `db_path` — no DB access, no lock needed.
#[tauri::command]
pub async fn get_data_info(state: State<'_, AppState>) -> Result<DataInfo, String> {
    Ok(DataInfo {
        db_path: state.db_path.to_string_lossy().to_string(),
    })
}
