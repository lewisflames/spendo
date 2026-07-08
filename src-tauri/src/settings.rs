use serde::Serialize;
use tauri::State;

use crate::db::{clear_db, get_conn, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataInfo {
    pub db_path: String,
}

#[tauri::command]
pub async fn clear_app_db(state: State<'_, AppState>) -> Result<(), String> {
    let conn = get_conn(&state)?;
    clear_db(&conn).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_data_info(state: State<'_, AppState>) -> Result<DataInfo, String> {
    Ok(DataInfo {
        db_path: state.db_path.to_string_lossy().to_string(),
    })
}
