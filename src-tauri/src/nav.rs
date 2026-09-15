use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::utils;

/// Counts rendered as badges on the app sidebar.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavBadges {
    /// Active schedules whose next occurrence is due today or overdue.
    pub due_schedules: i64,
    /// Unreconciled transactions in reconcilable accounts (same type set as
    /// `reconcile.rs`: cash-like + liability, non-placeholder).
    pub uncleared_transactions: i64,
}

fn nav_badges_inner(conn: &Connection) -> Result<NavBadges, String> {
    let due_schedules: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM recurring_schedules
             WHERE active = 1 AND next_due_date IS NOT NULL
               AND substr(next_due_date, 1, 10) <= date('now')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let uncleared_transactions: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT s.tx_guid)
                 FROM splits s
                 JOIN transactions t ON t.guid = s.tx_guid
                 JOIN accounts a ON a.guid = s.account_guid
                 WHERE (s.reconcile_state IS NULL OR s.reconcile_state = 'n')
                   AND a.placeholder = 0
                   AND a.account_type IN ({}, {})
                   AND {}",
                utils::cash_like_types_sql(),
                utils::liability_types_sql(),
                utils::VISIBLE_TX_FILTER
            ),
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(NavBadges {
        due_schedules,
        uncleared_transactions,
    })
}

#[tauri::command]
pub async fn fetch_nav_badges(state: State<'_, AppState>) -> Result<NavBadges, String> {
    run_blocking(state.db.clone(), nav_badges_inner).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use rusqlite::params;

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
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur'),
                    ('groceries', 'Groceries', 'EXPENSE', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn
    }

    fn add_tx(conn: &Connection, tag: &str, date: &str, reconcile_state: Option<&str>) {
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
            params![tag, date],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state)
             VALUES (?1, ?2, 'checking', 100, 100, 100, 100, ?3)",
            params![format!("s-{tag}"), tag, reconcile_state],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state)
             VALUES (?1, ?2, 'groceries', -100, 100, -100, 100, NULL)",
            params![format!("s2-{tag}"), tag],
        )
        .unwrap();
    }

    #[test]
    fn counts_only_unreconciled_reconcilable_txs() {
        let conn = setup();
        add_tx(&conn, "t1", "2024-01-10", None);
        add_tx(&conn, "t2", "2024-01-11", Some("c"));
        add_tx(&conn, "t3", "2024-01-12", Some("y"));
        // GnuCash's default "not reconciled" state counts too.
        add_tx(&conn, "t4", "2024-01-13", Some("n"));
        // Future-dated rows are excluded by the shared visibility window.
        add_tx(&conn, "t5", "9999-01-01", None);

        let badges = nav_badges_inner(&conn).unwrap();
        assert_eq!(badges.uncleared_transactions, 2);
        assert_eq!(badges.due_schedules, 0);
    }

    #[test]
    fn counts_due_schedules_only_when_active() {
        let conn = setup();
        for (guid, due, active) in [
            ("s1", "'2024-01-01'", 1),
            ("s2", "'2099-01-01'", 1),
            ("s3", "'2024-01-01'", 0),
            ("s4", "NULL", 1),
        ] {
            conn.execute(
                &format!(
                    "INSERT INTO recurring_schedules
                     (guid, name, frequency, interval_mult, start_date, next_due_date, active, template_json, created_at)
                     VALUES (?1, ?1, 'MONTHLY', 1, '2024-01-01', {due}, {active}, '{{}}', '2024-01-01T00:00:00Z')"
                ),
                params![guid],
            )
            .unwrap();
        }

        let badges = nav_badges_inner(&conn).unwrap();
        assert_eq!(badges.due_schedules, 1);
    }
}
