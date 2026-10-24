use crate::db::AppError;
use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::prices::get_cached_prices_map;
use crate::utils;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSummary {
    pub currency: String,
    pub total_accounts: i64,
    pub total_transactions: i64,
    pub earliest_date: Option<String>,
    pub latest_date: Option<String>,
    pub net_worth: f64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeDataResult {
    pub has_data: bool,
    pub summary: Option<DataSummary>,
}

pub fn get_summary(conn: &Connection) -> Result<DataSummary, AppError> {
    let currency = fx::get_report_currency_mnemonic(conn)?.unwrap_or_else(|| "Unknown".to_string());

    let total_accounts: i64 =
        conn.query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))?;

    // Same visibility window as every other page: future-dated transactions
    // must not inflate totals, the displayed date range or net worth.
    let total_transactions: i64 = conn.query_row(
        // Alias the table: VISIBLE_TX_FILTER qualifies its column as `t`.
        &format!(
            "SELECT COUNT(*) FROM transactions t WHERE {}",
            utils::VISIBLE_TX_FILTER
        ),
        [],
        |row| row.get(0),
    )?;

    let (earliest_date, latest_date) = utils::visible_tx_window(conn)?;

    // Net worth in report currency: cash per account currency converted at latest FX,
    // investments marked to market with report-currency-normalized prices.
    let cash_rows: Vec<(String, String, f64, f64, f64)> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT c.guid, c.mnemonic,
                        COALESCE(SUM(CASE WHEN a.account_type IN ({})
                            THEN {} ELSE 0 END), 0) as cash_assets,
                        COALESCE(SUM(CASE WHEN a.account_type IN ({})
                            THEN {} ELSE 0 END), 0) as shares,
                        COALESCE(SUM(CASE WHEN a.account_type IN ({})
                            THEN {} ELSE 0 END), 0) as liabilities
                 FROM splits s
                 JOIN accounts a ON a.guid = s.account_guid
                 JOIN commodities c ON c.guid = a.commodity_guid
                 JOIN transactions t ON t.guid = s.tx_guid
                 WHERE {}
                 GROUP BY c.guid, c.mnemonic",
            utils::cash_like_types_sql(),
            utils::RAW_AMOUNT,
            utils::investment_types_sql(),
            utils::QUANTITY_AMOUNT,
            utils::liability_types_sql(),
            utils::RAW_AMOUNT,
            utils::VISIBLE_TX_FILTER
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let fx_map = fx::get_fx_map(conn, None)?;
    let unified_prices = get_cached_prices_map(conn)?;

    let net_worth: f64 = cash_rows
        .iter()
        .map(|(guid, mnemonic, cash_assets, shares, liabilities)| {
            // Liabilities keep their raw (credit-negative) sign, so debt is
            // added, not subtracted: 40 owed is a raw -40.
            let cash_val = fx::convert(*cash_assets + *liabilities, mnemonic, &currency, &fx_map);
            let inv_val = if *shares != 0.0 {
                unified_prices.get(guid).copied().unwrap_or(0.0) * shares
            } else {
                0.0
            };
            cash_val + inv_val
        })
        .sum();

    Ok(DataSummary {
        currency,
        total_accounts,
        total_transactions,
        earliest_date,
        latest_date,
        net_worth,
    })
}

#[tauri::command]
pub async fn fetch_home_data(state: State<'_, AppState>) -> Result<HomeDataResult, AppError> {
    // A book being open is not the same as a book having data: a cleared or
    // freshly seeded book still needs its shell so pages can show their empty
    // states and the import paths stay reachable. `has_data` keeps tracking
    // whether accounts exist; only `summary` is tied to the book being open.
    let book_open = state.db_path.lock()?.is_some();
    run_blocking(state.db.clone(), move |conn| {
        if !book_open {
            return Ok(HomeDataResult {
                has_data: false,
                summary: None,
            });
        }

        let cnt: i64 =
            conn.query_row("SELECT COUNT(*) as cnt FROM accounts", [], |row| row.get(0))?;

        Ok(HomeDataResult {
            has_data: cnt > 0,
            summary: Some(get_summary(conn)?),
        })
    })
    .await
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
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        for (tag, date, value) in [("t1", "2024-01-10", 5_000i64), ("t2", "2024-02-05", -1_500)] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![tag, date],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, 'checking', ?3, 100, ?3, 100)",
                params![format!("s-{tag}"), tag, value],
            )
            .unwrap();
        }
        conn
    }

    /// Guards every plain-`transactions` summary query: VISIBLE_TX_FILTER
    /// qualifies its column as `t`, so any unaliased table reference fails
    /// here instead of at user import time.
    #[test]
    fn summary_queries_run_with_visibility_filter() {
        let conn = setup();
        let summary = get_summary(&conn).expect("get_summary must not error");

        assert_eq!(summary.total_transactions, 2);
        assert_eq!(summary.earliest_date.as_deref(), Some("2024-01-10"));
        assert_eq!(summary.latest_date.as_deref(), Some("2024-02-05"));
        // 50 - 15 in EUR (the report currency).
        assert_eq!(summary.net_worth, 35.0);
    }

    /// Debt lowers net worth: a card purchase credits the liability account
    /// (negative raw value), so 40 owed takes 35 down to -5.
    #[test]
    fn liabilities_reduce_net_worth() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('card', 'Card', 'CREDIT', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t-card', '2024-02-10')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-card', 't-card', 'card', -4_000, 100, -4_000, 100)",
            [],
        )
        .unwrap();

        assert_eq!(get_summary(&conn).unwrap().net_worth, -5.0);
    }

    #[test]
    fn future_transactions_are_excluded_from_current_state() {
        let conn = setup();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t-future', '9999-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-future', 't-future', 'checking', 99_000, 100, 99_000, 100)",
            [],
        )
        .unwrap();

        let summary = get_summary(&conn).unwrap();
        assert_eq!(summary.total_transactions, 2);
        assert_eq!(summary.latest_date.as_deref(), Some("2024-02-05"));
        assert_eq!(summary.net_worth, 35.0);
    }
}
