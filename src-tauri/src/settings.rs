use crate::db::AppError;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::State;

use crate::db::{clear_db, run_blocking, AppState};

#[tauri::command]
pub async fn clear_app_db(state: State<'_, AppState>) -> Result<(), String> {
    // Clearing the book invalidates any in-flight price refresh (which checks
    // only the generation before writing), so bump it under the DB lock.
    let generation = state.book_generation.clone();
    run_blocking(state.db.clone(), move |conn| {
        clear_db(conn).map_err(|e| e.to_string())?;
        generation.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .await
}

// ── Book-scoped settings (travel with the `.spendo` file) ──

/// Full book-settings snapshot served to the frontend (camelCase). Stored as
/// individual `TEXT` rows in `app_settings`; missing rows fall back to
/// defaults.
#[derive(Serialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BookSettings {
    pub fiscal_year_start_month: u8,
    pub default_pnl_period: String,
    pub price_auto_refresh: String,
    pub export_csv_delimiter: String,
    pub export_date_format: String,
}

fn default_book_settings() -> BookSettings {
    BookSettings {
        fiscal_year_start_month: 1,
        default_pnl_period: "last_12_months".to_string(),
        price_auto_refresh: "off".to_string(),
        export_csv_delimiter: ",".to_string(),
        export_date_format: "iso".to_string(),
    }
}

fn read_setting(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [key],
        |row| row.get(0),
    )
    .optional()
    .map_err(AppError::from)
}

fn validate_book_setting(key: &str, value: &str) -> Result<(), AppError> {
    match key {
        "fiscal_year_start_month" => {
            let month: u8 = value
                .parse()
                .map_err(|_| AppError::Validation(format!("Invalid value for {key}: {value}")))?;
            if (1..=12).contains(&month) {
                Ok(())
            } else {
                Err(AppError::Validation(format!(
                    "Invalid value for {key}: {value}"
                )))
            }
        }
        "default_pnl_period" => match value {
            "this_month" | "this_quarter" | "this_year" | "last_12_months" => Ok(()),
            _ => Err(AppError::Validation(format!(
                "Invalid value for {key}: {value}"
            ))),
        },
        "price_auto_refresh" => match value {
            "off" | "on_startup" => Ok(()),
            _ => Err(AppError::Validation(format!(
                "Invalid value for {key}: {value}"
            ))),
        },
        "export_csv_delimiter" => match value {
            "," | ";" => Ok(()),
            _ => Err(AppError::Validation(format!(
                "Invalid value for {key}: {value}"
            ))),
        },
        "export_date_format" => match value {
            "iso" | "locale" => Ok(()),
            _ => Err(AppError::Validation(format!(
                "Invalid value for {key}: {value}"
            ))),
        },
        _ => Err(AppError::Validation(format!(
            "Unknown setting: {key}. We don't know that one"
        ))),
    }
}

fn get_book_settings_inner(conn: &Connection) -> Result<BookSettings, AppError> {
    let defaults = default_book_settings();
    let pick = |key: &str, fallback: String| -> Result<String, AppError> {
        match read_setting(conn, key)? {
            Some(v) if validate_book_setting(key, &v).is_ok() => Ok(v),
            _ => Ok(fallback),
        }
    };
    let fiscal_year_start_month = pick(
        "fiscal_year_start_month",
        defaults.fiscal_year_start_month.to_string(),
    )?
    .parse()
    .unwrap_or(1);
    Ok(BookSettings {
        fiscal_year_start_month,
        default_pnl_period: pick("default_pnl_period", defaults.default_pnl_period.clone())?,
        price_auto_refresh: pick("price_auto_refresh", defaults.price_auto_refresh.clone())?,
        export_csv_delimiter: pick(
            "export_csv_delimiter",
            defaults.export_csv_delimiter.clone(),
        )?,
        export_date_format: pick("export_date_format", defaults.export_date_format.clone())?,
    })
}

fn set_book_setting_inner(
    conn: &Connection,
    key: &str,
    value: &str,
) -> Result<BookSettings, AppError> {
    validate_book_setting(key, value)?;
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    get_book_settings_inner(conn)
}

#[tauri::command]
pub async fn get_book_settings(state: State<'_, AppState>) -> Result<BookSettings, String> {
    run_blocking(state.db.clone(), |conn| {
        get_book_settings_inner(conn).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn set_book_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<BookSettings, String> {
    run_blocking(state.db.clone(), move |conn| {
        set_book_setting_inner(conn, &key, &value).map_err(String::from)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn defaults_on_fresh_book() {
        let conn = mem_db();
        assert_eq!(
            get_book_settings_inner(&conn).unwrap(),
            default_book_settings()
        );
    }

    #[test]
    fn set_round_trips_valid_values() {
        let conn = mem_db();
        let updated = set_book_setting_inner(&conn, "fiscal_year_start_month", "4").unwrap();
        assert_eq!(updated.fiscal_year_start_month, 4);
        let updated = set_book_setting_inner(&conn, "default_pnl_period", "this_year").unwrap();
        assert_eq!(updated.default_pnl_period, "this_year");
        let updated = set_book_setting_inner(&conn, "price_auto_refresh", "on_startup").unwrap();
        assert_eq!(updated.price_auto_refresh, "on_startup");
        let updated = set_book_setting_inner(&conn, "export_csv_delimiter", ";").unwrap();
        assert_eq!(updated.export_csv_delimiter, ";");
        let updated = set_book_setting_inner(&conn, "export_date_format", "locale").unwrap();
        assert_eq!(updated.export_date_format, "locale");
        // Full snapshot persists.
        assert_eq!(get_book_settings_inner(&conn).unwrap(), updated);
    }

    #[test]
    fn rejects_unknown_keys_and_invalid_values() {
        let conn = mem_db();
        assert!(set_book_setting_inner(&conn, "nope", "1")
            .unwrap_err()
            .to_string()
            .contains("Unknown"));
        for (key, value) in [
            ("fiscal_year_start_month", "0"),
            ("fiscal_year_start_month", "13"),
            ("fiscal_year_start_month", "jan"),
            ("default_pnl_period", "forever"),
            ("price_auto_refresh", "hourly"),
            ("export_csv_delimiter", "|"),
            ("export_date_format", "us"),
        ] {
            assert!(
                set_book_setting_inner(&conn, key, value).is_err(),
                "accepted {key}={value}"
            );
        }
        // Failed writes leave defaults intact.
        assert_eq!(
            get_book_settings_inner(&conn).unwrap(),
            default_book_settings()
        );
    }

    #[test]
    fn corrupt_stored_values_fall_back_to_defaults() {
        let conn = mem_db();
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES ('fiscal_year_start_month', '99')",
            [],
        )
        .unwrap();
        assert_eq!(
            get_book_settings_inner(&conn)
                .unwrap()
                .fiscal_year_start_month,
            1
        );
    }

    #[test]
    fn clear_db_resets_book_settings() {
        let conn = mem_db();
        set_book_setting_inner(&conn, "export_csv_delimiter", ";").unwrap();
        db::clear_db(&conn).unwrap();
        assert_eq!(
            get_book_settings_inner(&conn).unwrap(),
            default_book_settings()
        );
    }
}
