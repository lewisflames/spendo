use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::utils;

/// One description-match rule: the first rule (by priority, then id) whose
/// pattern matches a CSV row's description picks that row's category. Rules
/// are book-scoped user config.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRule {
    pub id: i64,
    pub priority: i64,
    pub match_type: String,
    pub pattern: String,
    pub account_guid: String,
    pub account_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRuleInput {
    pub priority: i64,
    pub match_type: String,
    pub pattern: String,
    pub account_guid: String,
}

const MAX_PATTERN_CHARS: usize = 200;

fn validate_rule_input(conn: &Connection, input: &ImportRuleInput) -> Result<(), AppError> {
    if !matches!(input.match_type.as_str(), "contains" | "equals") {
        return Err(AppError::Validation(
            "Match type must be contains or equals".to_string(),
        ));
    }
    let pattern = input.pattern.trim();
    if pattern.is_empty() {
        return Err(AppError::Validation("Pattern is required".to_string()));
    }
    if pattern.chars().count() > MAX_PATTERN_CHARS {
        return Err(AppError::Validation(format!(
            "Pattern is too long (max {MAX_PATTERN_CHARS} characters)"
        )));
    }
    // Rules post splits, so the same postability guard as the CSV pickers.
    crate::csv_import::ensure_postable_account(conn, &input.account_guid, "Rule")
}

fn fetch_rule(conn: &Connection, id: i64) -> Result<ImportRule, AppError> {
    conn.query_row(
        "SELECT r.id, r.priority, r.match_type, r.pattern, r.account_guid,
                COALESCE(a.name, '')
         FROM import_rules r
         LEFT JOIN accounts a ON a.guid = r.account_guid
         WHERE r.id = ?1",
        params![id],
        |row| {
            Ok(ImportRule {
                id: row.get(0)?,
                priority: row.get(1)?,
                match_type: row.get(2)?,
                pattern: row.get(3)?,
                account_guid: row.get(4)?,
                account_name: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(AppError::from)?
    .ok_or_else(|| AppError::NotFound("Import rule not found".to_string()))
}

fn fetch_import_rules_inner(conn: &Connection) -> Result<Vec<ImportRule>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT r.id, r.priority, r.match_type, r.pattern, r.account_guid,
                    COALESCE(a.name, '')
             FROM import_rules r
             LEFT JOIN accounts a ON a.guid = r.account_guid
             ORDER BY r.priority ASC, r.id ASC",
        )
        .map_err(AppError::from)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ImportRule {
                id: row.get(0)?,
                priority: row.get(1)?,
                match_type: row.get(2)?,
                pattern: row.get(3)?,
                account_guid: row.get(4)?,
                account_name: row.get(5)?,
            })
        })
        .map_err(AppError::from)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::from)?;
    Ok(rows)
}

pub(crate) fn create_import_rule_inner(
    conn: &Connection,
    input: &ImportRuleInput,
) -> Result<ImportRule, AppError> {
    validate_rule_input(conn, input)?;
    let pattern = input.pattern.trim().to_string();
    let id = with_tx_err(conn, || {
        conn.execute(
            "INSERT INTO import_rules (priority, match_type, pattern, account_guid, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                input.priority,
                input.match_type,
                pattern,
                input.account_guid,
                utils::iso_now()
            ],
        )?;
        Ok::<i64, AppError>(conn.last_insert_rowid())
    })?;
    fetch_rule(conn, id)
}

fn update_import_rule_inner(
    conn: &Connection,
    id: i64,
    input: &ImportRuleInput,
) -> Result<ImportRule, AppError> {
    validate_rule_input(conn, input)?;
    let pattern = input.pattern.trim().to_string();
    let changed = conn
        .execute(
            "UPDATE import_rules
             SET priority = ?2, match_type = ?3, pattern = ?4, account_guid = ?5
             WHERE id = ?1",
            params![
                id,
                input.priority,
                input.match_type,
                pattern,
                input.account_guid
            ],
        )
        .map_err(AppError::from)?;
    if changed == 0 {
        return Err(AppError::NotFound("Import rule not found".to_string()));
    }
    fetch_rule(conn, id)
}

fn delete_import_rule_inner(conn: &Connection, id: i64) -> Result<(), AppError> {
    let changed = conn
        .execute("DELETE FROM import_rules WHERE id = ?1", params![id])
        .map_err(AppError::from)?;
    if changed == 0 {
        return Err(AppError::NotFound("Import rule not found".to_string()));
    }
    Ok(())
}

/// Snapshot of the rules, ordered by priority. Load once per CSV parse and
/// call [`RuleMatcher::resolve`] per row instead of querying per row.
pub(crate) struct RuleMatcher {
    rules: Vec<(String, String, String)>,
}

impl RuleMatcher {
    pub(crate) fn load(conn: &Connection) -> Result<Self, AppError> {
        let mut stmt = conn
            .prepare(
                "SELECT match_type, pattern, account_guid
                 FROM import_rules ORDER BY priority ASC, id ASC",
            )
            .map_err(AppError::from)?;
        let rules = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(AppError::from)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(Self { rules })
    }

    /// First matching rule's account (case-insensitive `contains`/`equals` on
    /// the trimmed description).
    pub(crate) fn resolve(&self, description: &str) -> Option<&str> {
        let needle = description.trim().to_lowercase();
        if needle.is_empty() {
            return None;
        }
        for (match_type, pattern, account_guid) in &self.rules {
            let pattern = pattern.trim().to_lowercase();
            if pattern.is_empty() {
                continue;
            }
            let hit = match match_type.as_str() {
                "equals" => needle == pattern,
                _ => needle.contains(&pattern),
            };
            if hit {
                return Some(account_guid);
            }
        }
        None
    }
}

/// Convenience wrapper for callers that resolve a single description.
#[cfg(test)]
pub(crate) fn resolve_account(
    conn: &Connection,
    description: &str,
) -> Result<Option<String>, AppError> {
    Ok(RuleMatcher::load(conn)?.resolve(description).map(str::to_string))
}

/// Accounts referenced by rules. The GnuCash orphan sweep must skip them: the
/// FK cascade would silently drop the rules.
pub(crate) fn rule_account_guids(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT account_guid FROM import_rules")
        .map_err(AppError::from)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(AppError::from)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::from)?;
    Ok(rows)
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fetch_import_rules(state: State<'_, AppState>) -> Result<Vec<ImportRule>, String> {
    run_blocking(state.db.clone(), |conn| {
        fetch_import_rules_inner(conn).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn create_import_rule(
    state: State<'_, AppState>,
    input: ImportRuleInput,
) -> Result<ImportRule, String> {
    run_blocking(state.db.clone(), move |conn| {
        create_import_rule_inner(conn, &input).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn update_import_rule(
    state: State<'_, AppState>,
    id: i64,
    input: ImportRuleInput,
) -> Result<ImportRule, String> {
    run_blocking(state.db.clone(), move |conn| {
        update_import_rule_inner(conn, id, &input).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn delete_import_rule(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        delete_import_rule_inner(conn, id).map_err(String::from)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
             ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
             ('groceries', 'Groceries', 'EXPENSE', 'root', 0, 'c-eur'),
             ('coffee', 'Coffee', 'EXPENSE', 'root', 0, 'c-eur'),
             ('reserved', 'Reserved', 'EXPENSE', 'root', 1, 'c-eur')",
            [],
        )
        .unwrap();
        conn
    }

    fn rule(priority: i64, match_type: &str, pattern: &str, account: &str) -> ImportRuleInput {
        ImportRuleInput {
            priority,
            match_type: match_type.to_string(),
            pattern: pattern.to_string(),
            account_guid: account.to_string(),
        }
    }

    #[test]
    fn resolve_uses_priority_then_matches_case_insensitively() {
        let conn = setup();
        create_import_rule_inner(&conn, &rule(10, "contains", "coffee", "coffee")).unwrap();
        create_import_rule_inner(&conn, &rule(5, "contains", "STARBUCKS", "coffee")).unwrap();
        create_import_rule_inner(&conn, &rule(20, "equals", "weekly shop", "groceries")).unwrap();

        // Priority 5 (STARBUCKS) wins over priority 10 even for the same target.
        assert_eq!(
            resolve_account(&conn, "STARBUCKS #123").unwrap().as_deref(),
            Some("coffee")
        );
        assert_eq!(
            resolve_account(&conn, "Weekly Shop").unwrap().as_deref(),
            Some("groceries")
        );
        // `equals` needs the whole trimmed description.
        assert!(resolve_account(&conn, "weekly shop extra").unwrap().is_none());
        assert!(resolve_account(&conn, "").unwrap().is_none());
        assert!(resolve_account(&conn, "unrelated").unwrap().is_none());
    }

    #[test]
    fn crud_round_trips_and_validates() {
        let conn = setup();
        let created = create_import_rule_inner(&conn, &rule(1, "contains", "  coffee  ", "coffee")).unwrap();
        assert_eq!(created.pattern, "coffee");
        assert_eq!(created.account_name, "Coffee");

        let updated = update_import_rule_inner(
            &conn,
            created.id,
            &rule(2, "equals", "espresso", "groceries"),
        )
        .unwrap();
        assert_eq!(updated.match_type, "equals");
        assert_eq!(updated.account_guid, "groceries");

        assert_eq!(fetch_import_rules_inner(&conn).unwrap().len(), 1);
        delete_import_rule_inner(&conn, created.id).unwrap();
        assert!(fetch_import_rules_inner(&conn).unwrap().is_empty());
        assert!(delete_import_rule_inner(&conn, created.id).is_err());

        assert!(create_import_rule_inner(&conn, &rule(1, "regex", "x", "coffee")).is_err());
        assert!(create_import_rule_inner(&conn, &rule(1, "contains", " ", "coffee")).is_err());
        assert!(create_import_rule_inner(&conn, &rule(1, "contains", "x", "reserved")).is_err());
        assert!(create_import_rule_inner(&conn, &rule(1, "contains", "x", "missing")).is_err());
        assert!(create_import_rule_inner(&conn, &rule(1, "contains", "x", "root")).is_err());
    }

    #[test]
    fn rule_accounts_are_listed_for_sweep_protection() {
        let conn = setup();
        create_import_rule_inner(&conn, &rule(1, "contains", "a", "coffee")).unwrap();
        create_import_rule_inner(&conn, &rule(2, "contains", "b", "groceries")).unwrap();
        let mut guids = rule_account_guids(&conn).unwrap();
        guids.sort();
        assert_eq!(guids, vec!["coffee".to_string(), "groceries".to_string()]);
    }
}
