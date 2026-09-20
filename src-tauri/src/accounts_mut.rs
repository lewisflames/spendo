use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use std::collections::HashSet;
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::history;
use crate::utils;
use crate::utils::opt_text;

/// Creatable account types (mirrors `TYPE_TO_CATEGORY` keys minus the
/// internal ROOT, kept in sync with `$lib/features/accounts/account-types`).
pub const ACCOUNT_TYPES: [&str; 15] = [
    "ASSET",
    "BANK",
    "CASH",
    "STOCK",
    "MUTUAL",
    "FUND",
    "CURRENCY",
    "RECEIVABLE",
    "LIABILITY",
    "CREDIT",
    "PAYABLE",
    "INCOME",
    "EXPENSE",
    "EQUITY",
    "TRADING",
];

/// Shared create/update payload. `parent_guid = None` means top-level (stored
/// as the internal ROOT's guid).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInput {
    pub name: String,
    pub account_type: String,
    pub parent_guid: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub commodity_guid: Option<String>,
    pub placeholder: bool,
}

fn root_guid(conn: &Connection) -> Result<String, AppError> {
    conn.query_row(
        "SELECT guid FROM accounts WHERE account_type = 'ROOT' LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    ?
    .ok_or_else(|| AppError::NotFound("Book has no ROOT account — re-create the book".to_string()))
}

/// Trimmed name plus all scalar field constraints (required, lengths and
/// allowed account type).
fn validate_scalar_fields(input: &AccountInput) -> Result<&str, AppError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("Account name is required".to_string()));
    }
    if name.len() > 200 {
        return Err(AppError::Validation("Account name is too long (max 200)".to_string()));
    }
    if !ACCOUNT_TYPES.contains(&input.account_type.as_str()) {
        return Err(AppError::Validation(format!("Invalid account type: {}", input.account_type)));
    }
    // A postable account without a currency would be dropped from ledger/split
    // queries (INNER JOIN commodities) while still counting in balances —
    // require one. Placeholders are never posted to, so they may stay bare.
    if !input.placeholder && input.commodity_guid.is_none() {
        return Err(AppError::Validation(
            "Pick a currency for this account".to_string(),
        ));
    }
    if let Some(ref code) = input.code {
        if code.len() > 100 {
            return Err(AppError::Validation("Code is too long (max 100)".to_string()));
        }
    }
    if let Some(ref d) = input.description {
        if d.len() > 500 {
            return Err(AppError::Validation("Description is too long (max 500)".to_string()));
        }
    }
    Ok(name)
}

fn validate_commodity_exists(
    conn: &Connection,
    commodity_guid: Option<&str>,
) -> Result<(), AppError> {
    if let Some(cg) = commodity_guid {
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM commodities WHERE guid = ?1",
                params![cg],
                |_| Ok(()),
            )
            .optional()
            ?
            .is_some();
        if !exists {
            return Err(AppError::Validation("Selected currency does not exist".to_string()));
        }
    }
    Ok(())
}

/// Resolve the stored parent guid (`None` = top-level under ROOT) and reject
/// self/descendant parents with a cycle-guarded ancestor walk.
fn resolve_parent(
    conn: &Connection,
    parent_guid: Option<String>,
    self_guid: Option<&str>,
) -> Result<String, AppError> {
    match parent_guid {
        None => root_guid(conn),
        Some(p) => {
            if Some(p.as_str()) == self_guid {
                return Err(AppError::Validation("An account cannot be its own parent".to_string()));
            }
            let parent_type: Option<String> = conn
                .query_row(
                    "SELECT account_type FROM accounts WHERE guid = ?1",
                    params![p],
                    |row| row.get(0),
                )
                .optional()
                ?;
            let Some(pt) = parent_type else {
                return Err(AppError::NotFound("Parent account not found".to_string()));
            };
            if pt == "ROOT" {
                root_guid(conn)
            } else {
                // Cycle guard: walk the ancestor chain; it must never reach self.
                if let Some(self_g) = self_guid {
                    let mut current = Some(p.clone());
                    let mut visited = HashSet::new();
                    for _ in 0..100 {
                        let Some(c) = current else { break };
                        if c == self_g {
                            return Err(AppError::Validation(
                                "Cannot move an account under its own descendant".to_string()
                            ));
                        }
                        if !visited.insert(c.clone()) {
                            break;
                        }
                        current = conn
                            .query_row(
                                "SELECT parent_guid FROM accounts WHERE guid = ?1",
                                params![c],
                                |row| row.get::<_, Option<String>>(0),
                            )
                            .optional()
                            ?
                            .flatten();
                    }
                }
                Ok(p)
            }
        }
    }
}

/// Sibling-name uniqueness: same parent, different row.
fn validate_sibling_name_unique(
    conn: &Connection,
    parent: &str,
    name: &str,
    self_guid: Option<&str>,
) -> Result<(), AppError> {
    let clash: bool = conn
        .query_row(
            "SELECT 1 FROM accounts
             WHERE parent_guid = ?1 AND name = ?2 AND (?3 IS NULL OR guid != ?3)
             LIMIT 1",
            params![parent, name, self_guid],
            |_| Ok(()),
        )
        .optional()
        ?
        .is_some();
    if clash {
        return Err(AppError::Validation("An account with this name already exists under the same parent".to_string()));
    }
    Ok(())
}

fn validate_account_input(
    conn: &Connection,
    input: &AccountInput,
    self_guid: Option<&str>,
) -> Result<String, AppError> {
    let name = validate_scalar_fields(input)?;
    validate_commodity_exists(conn, input.commodity_guid.as_deref())?;
    let parent = resolve_parent(conn, opt_text(&input.parent_guid), self_guid)?;
    validate_sibling_name_unique(conn, &parent, name, self_guid)?;
    Ok(parent)
}

fn account_label(name: &str, verb: &str) -> String {
    format!("{verb} {}", utils::truncate_chars(name.trim(), 80))
}

fn create_account_inner(conn: &Connection, input: AccountInput) -> Result<String, AppError> {
    let parent = validate_account_input(conn, &input, None)?;
    let guid = uuid::Uuid::new_v4().to_string();
    let label = account_label(input.name.trim(), "Create");

    with_tx_err(conn, || {
        conn.execute(
            "INSERT INTO accounts
              (guid, name, account_type, parent_guid, code, description,
               placeholder, commodity_guid)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                guid,
                input.name.trim(),
                input.account_type,
                parent,
                opt_text(&input.code),
                opt_text(&input.description),
                input.placeholder as i64,
                opt_text(&input.commodity_guid),
            ],
        )
        ?;
        history::insert_change_entry(
            conn,
            "account_create",
            &label,
            "manual",
            0,
            1,
            None,
            &[history::Snapshot {
                table_name: "accounts",
                row_guid: guid.clone(),
                before_json: None,
            }],
        )?;
        Ok(guid.clone())
    })
}

fn update_account_inner(
    conn: &Connection,
    guid: &str,
    input: AccountInput,
) -> Result<(), AppError> {
    let existing_type: Option<String> = conn
        .query_row(
            "SELECT account_type FROM accounts WHERE guid = ?1",
            params![guid],
            |row| row.get(0),
        )
        .optional()
        ?;
    let Some(existing_type) = existing_type else {
        return Err(AppError::NotFound("Account not found — it may have been removed".to_string()));
    };
    if existing_type == "ROOT" {
        return Err(AppError::Validation("The root account cannot be edited".to_string()));
    }
    if input.account_type == "ROOT" {
        return Err(AppError::Validation("Accounts cannot be converted to ROOT".to_string()));
    }
    let parent = validate_account_input(conn, &input, Some(guid))?;
    let label = account_label(input.name.trim(), "Update");

    with_tx_err(conn, || {
        let snapshots = vec![history::Snapshot {
            table_name: "accounts",
            row_guid: guid.to_string(),
            before_json: history::snapshot_row(conn, "accounts", guid)?,
        }];
        conn.execute(
            "UPDATE accounts SET name = ?1, account_type = ?2, parent_guid = ?3,
                    code = ?4, description = ?5, placeholder = ?6,
                    commodity_guid = ?7
             WHERE guid = ?8",
            params![
                input.name.trim(),
                input.account_type,
                parent,
                opt_text(&input.code),
                opt_text(&input.description),
                input.placeholder as i64,
                opt_text(&input.commodity_guid),
                guid,
            ],
        )
        ?;
        history::insert_change_entry(
            conn,
            "account_update",
            &label,
            "manual",
            0,
            1,
            None,
            &snapshots,
        )?;
        Ok(())
    })
}

fn delete_account_inner(conn: &Connection, guid: &str) -> Result<(), AppError> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT name, account_type FROM accounts WHERE guid = ?1",
            params![guid],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        ?;
    let Some((name, account_type)) = row else {
        return Err(AppError::NotFound("Account not found — it may have been removed".to_string()));
    };
    if account_type == "ROOT" {
        return Err(AppError::Validation("The root account cannot be deleted".to_string()));
    }
    let children: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM accounts WHERE parent_guid = ?1",
            params![guid],
            |row| row.get(0),
        )
        ?;
    if children > 0 {
        return Err(AppError::Validation(format!(
            "Cannot delete — it has {children} sub-account{}. Move or delete them first.",
            if children == 1 { "" } else { "s" }
        )));
    }
    let splits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM splits WHERE account_guid = ?1",
            params![guid],
            |row| row.get(0),
        )
        ?;
    if splits > 0 {
        return Err(AppError::Validation(format!(
            "Cannot delete — {splits} split{} reference it. Delete or move those transactions first.",
            if splits == 1 { "" } else { "s" }
        )));
    }
    let budgets: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM budgets WHERE account_guid = ?1",
            params![guid],
            |row| row.get(0),
        )
        ?;
    if budgets > 0 {
        return Err(AppError::Validation("Cannot delete — budgets reference it. Remove the budgets first.".to_string()));
    }
    // Schedule templates embed account GUIDs in JSON (no FK), so deleting would
    // leave them failing at posting time.
    let schedules: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM recurring_schedules WHERE instr(template_json, ?1) > 0",
            params![guid],
            |row| row.get(0),
        )
        ?;
    if schedules > 0 {
        return Err(AppError::Validation(
            "Cannot delete — schedules reference it. Update or remove those schedules first."
                .to_string(),
        ));
    }

    let label = account_label(&name, "Delete");
    with_tx_err(conn, || {
        let snapshots = vec![history::Snapshot {
            table_name: "accounts",
            row_guid: guid.to_string(),
            before_json: history::snapshot_row(conn, "accounts", guid)?,
        }];
        conn.execute("DELETE FROM accounts WHERE guid = ?1", params![guid])
            ?;
        history::insert_change_entry(
            conn,
            "account_delete",
            &label,
            "manual",
            0,
            1,
            None,
            &snapshots,
        )?;
        Ok(())
    })
}

#[tauri::command]
pub async fn create_account(state: State<'_, AppState>, input: AccountInput) -> Result<String, String> {
    run_blocking(state.db.clone(), move |conn| {
        create_account_inner(conn, input).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn update_account(
    state: State<'_, AppState>,
    guid: String,
    input: AccountInput,
) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        update_account_inner(conn, &guid, input).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn delete_account(state: State<'_, AppState>, guid: String) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        delete_account_inner(conn, &guid).map_err(String::from)
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
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
                    ('assets', 'Assets', 'ASSET', 'root', 1, 'c-eur'),
                    ('checking', 'Checking', 'BANK', 'assets', 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn
    }

    fn input(name: &str, parent: Option<&str>) -> AccountInput {
        AccountInput {
            name: name.to_string(),
            account_type: "EXPENSE".to_string(),
            parent_guid: parent.map(str::to_string),
            code: None,
            description: None,
            commodity_guid: Some("c-eur".to_string()),
            placeholder: false,
        }
    }

    #[test]
    fn create_requires_currency_for_postable_accounts() {
        let conn = setup();
        let mut no_currency = input("Food", None);
        no_currency.commodity_guid = None;
        let err = create_account_inner(&conn, no_currency).unwrap_err();
        assert!(err.to_string().contains("currency"), "unexpected: {err}");

        // Placeholders are never posted to, so they may stay currency-less.
        let mut placeholder = input("Reserved", None);
        placeholder.commodity_guid = None;
        placeholder.placeholder = true;
        assert!(create_account_inner(&conn, placeholder).is_ok());
    }

    #[test]
    fn delete_blocks_accounts_referenced_by_schedules() {
        let conn = setup();
        let guid = create_account_inner(&conn, input("Food", Some("assets"))).unwrap();
        conn.execute(
            "INSERT INTO recurring_schedules
               (guid, name, frequency, start_date, template_json, created_at)
             VALUES ('s1', 'Rent', 'monthly', '2024-01-01', ?1, '2024-01-01T00:00:00Z')",
            params![format!(
                r#"{{"splits":[{{"accountGuid":"{guid}","amount":-100.0}}]}}"#
            )],
        )
        .unwrap();

        let err = delete_account_inner(&conn, &guid).unwrap_err();
        assert!(err.to_string().contains("schedules"), "unexpected: {err}");
    }

    #[test]
    fn create_validates_and_logs_history() {
        let conn = setup();
        assert!(create_account_inner(&conn, input("", None)).is_err());
        let mut bad = input("Food", None);
        bad.account_type = "NOPE".to_string();
        assert!(create_account_inner(&conn, bad).is_err());
        // Sibling-name clash.
        let g1 = create_account_inner(&conn, input("Food", Some("assets"))).expect("create runs");
        assert!(create_account_inner(&conn, input("Food", Some("assets"))).is_err());
        // Same name under a different parent is fine.
        create_account_inner(&conn, input("Food", None)).expect("create runs");
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM change_log WHERE kind = 'account_create'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        // Update round-trips.
        let upd = input("Food", Some("assets"));
        update_account_inner(&conn, &g1, upd).expect("update runs");
        let name: String = conn
            .query_row("SELECT name FROM accounts WHERE guid = ?1", params![g1], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "Food");
    }

    #[test]
    fn move_rejects_cycles_and_delete_guards_splits_children_budgets() {
        let conn = setup();
        let parent = create_account_inner(&conn, input("Parent", Some("assets"))).expect("create");
        let child = create_account_inner(&conn, input("Child", Some(&parent))).expect("create");
        // Self-parent and descendant-parent both blocked.
        assert!(update_account_inner(&conn, &parent, input("Parent", Some(&parent))).is_err());
        assert!(update_account_inner(&conn, &parent, input("Parent", Some(&child))).is_err());
        // Delete blocked while children exist.
        assert!(delete_account_inner(&conn, &parent).is_err());
        // Delete blocked with splits.
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t1', '2024-01-10')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s1', 't1', 'checking', 100, 100, 100, 100)",
            [],
        )
        .unwrap();
        assert!(delete_account_inner(&conn, "checking").unwrap_err().to_string().contains("split"));
        // Delete blocked with budgets (would CASCADE unsnapshotted rows).
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, year, amount, created_at)
             VALUES ('b1', 'assets', 2024, 100, '2024-01-01')",
            [],
        )
        .unwrap();
        // `assets` has children too; use a fresh leaf with a budget instead.
        let leaf = create_account_inner(&conn, input("Leaf", Some("assets"))).expect("create");
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, year, amount, created_at)
             VALUES ('b2', ?1, 2024, 50, '2024-01-01')",
            params![leaf],
        )
        .unwrap();
        assert!(delete_account_inner(&conn, &leaf).unwrap_err().to_string().contains("budget"));
        // Clean leaf deletes and logs.
        delete_account_inner(&conn, &child).expect("delete runs");
        let kinds: Vec<String> = {
            let mut stmt = conn.prepare("SELECT kind FROM change_log ORDER BY id").unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(kinds.contains(&"account_delete".to_string()));
    }
}
