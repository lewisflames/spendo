use super::*;

// ── Mapping presets ──────────────────────────────────────────────────────────

/// Stable key for "the same bank statement layout": the normalized header row.
/// Headerless files have no stable key, so they don't get presets.
pub(super) fn preset_signature(headers: &[String], has_header: bool) -> Option<String> {
    if !has_header {
        return None;
    }
    let normalized: Vec<String> = headers.iter().map(|h| h.trim().to_lowercase()).collect();
    if normalized.iter().all(|h| h.is_empty()) {
        return None;
    }
    Some(bytes_sha256(normalized.join("\u{1f}").as_bytes()))
}

/// Human-readable label for the detect notice.
pub(super) fn preset_name(headers: &[String]) -> String {
    let mut name = headers
        .iter()
        .map(|h| h.trim())
        .filter(|h| !h.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" · ");
    if name.chars().count() > 80 {
        name = name.chars().take(80).collect();
    }
    if name.is_empty() {
        name = "CSV mapping".to_string();
    }
    name
}

pub(super) fn load_preset(
    conn: &Connection,
    signature: &str,
) -> Result<Option<CsvPreset>, AppError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT mapping_json FROM csv_import_presets WHERE signature = ?1",
            params![signature],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::from)?;
    match json {
        Some(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| AppError::Validation(format!("We couldn't read that saved mapping: {e}"))),
        None => Ok(None),
    }
}

pub(super) fn save_preset(conn: &Connection, preset: &CsvPreset) -> Result<(), AppError> {
    let json = serde_json::to_string(preset)
        .map_err(|e| AppError::Validation(format!("We couldn't remember that mapping: {e}")))?;
    conn.execute(
        "INSERT INTO csv_import_presets (signature, name, mapping_json, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(signature) DO UPDATE SET
           name = excluded.name,
           mapping_json = excluded.mapping_json,
           updated_at = excluded.updated_at",
        params![preset.signature, preset.name, json, utils::iso_now()],
    )
    .map_err(AppError::from)?;
    Ok(())
}

pub(super) fn delete_preset(conn: &Connection, signature: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM csv_import_presets WHERE signature = ?1",
        params![signature],
    )
    .map_err(AppError::from)?;
    Ok(())
}
