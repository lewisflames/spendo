use crate::db::AppError;
use chrono::Local;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::prices::{get_cached_prices_map, PriceHistory};
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

/// One month-end point of the net worth trend, in report currency.
/// `liabilities` is the amount owed (positive when in debt).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetWorthPoint {
    /// YYYY-MM-DD: the month's last day, or today for the current month.
    pub date: String,
    pub assets: f64,
    pub liabilities: f64,
    pub net_worth: f64,
}

/// Running per-commodity position: cash-like raw value, share quantity and
/// liability raw value (credit-negative).
#[derive(Default)]
struct Position {
    mnemonic: String,
    cash: f64,
    shares: f64,
    liabilities: f64,
}

/// Month-end net worth from the first visible transaction through today,
/// using the same account buckets as `get_summary`. Unlike the account
/// balance trend, each point is valued as of its own date: cash at that
/// date's FX rates and holdings at that date's prices, so the last point
/// matches the summary's net worth.
fn fetch_net_worth_trend_inner(conn: &Connection) -> Result<Vec<NetWorthPoint>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT substr(t.post_date, 1, 7) as ym, c.guid, c.mnemonic,
                COALESCE(SUM(CASE WHEN a.account_type IN ({cash}) THEN {raw} ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN a.account_type IN ({inv}) THEN {qty} ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN a.account_type IN ({liab}) THEN {raw} ELSE 0 END), 0)
         FROM splits s
         JOIN accounts a ON a.guid = s.account_guid
         JOIN commodities c ON c.guid = a.commodity_guid
         JOIN transactions t ON t.guid = s.tx_guid
         WHERE {visible}
           AND a.account_type IN ({cash}, {inv}, {liab})
         GROUP BY ym, c.guid, c.mnemonic
         ORDER BY ym ASC",
        cash = utils::cash_like_types_sql(),
        inv = utils::investment_types_sql(),
        liab = utils::liability_types_sql(),
        raw = utils::RAW_AMOUNT,
        qty = utils::QUANTITY_AMOUNT,
        visible = utils::VISIBLE_TX_FILTER
    ))?;
    let rows: Vec<(String, String, String, f64, f64, f64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let Some(first) = rows.first() else {
        return Ok(vec![]);
    };

    let today = Local::now().format("%Y-%m-%d").to_string();
    let first_month = utils::month_number(&first.0)?;
    let last_month = utils::month_number(&today)?;
    let report_currency = fx::report_currency_or_empty(conn)?;
    let fx_history = fx::FxHistory::load(conn)?;
    let price_history = PriceHistory::load(conn)?;

    let mut positions: HashMap<String, Position> = HashMap::new();
    let mut rows = rows.into_iter().peekable();
    let mut trend = Vec::with_capacity((last_month - first_month + 1).max(0) as usize);
    for n in first_month..=last_month {
        let ym = utils::format_month(n);
        while let Some((_, guid, mnemonic, cash, shares, liabilities)) = rows.next_if(|r| r.0 == ym)
        {
            let pos = positions.entry(guid).or_insert_with(|| Position {
                mnemonic,
                ..Default::default()
            });
            pos.cash += cash;
            pos.shares += shares;
            pos.liabilities += liabilities;
        }

        let date = if n == last_month {
            today.clone()
        } else {
            utils::month_end(&ym)?
        };
        let fx_map = fx_history.map_for_date(&date);
        let (mut assets, mut liabilities) = (0.0, 0.0);
        for (guid, pos) in &positions {
            assets += fx::convert(pos.cash, &pos.mnemonic, &report_currency, &fx_map);
            liabilities -= fx::convert(pos.liabilities, &pos.mnemonic, &report_currency, &fx_map);
            if pos.shares != 0.0 {
                let price = price_history
                    .price_as_of(guid, &date, &report_currency, &fx_history)
                    .unwrap_or(0.0);
                assets += price * pos.shares;
            }
        }
        trend.push(NetWorthPoint {
            date,
            assets,
            liabilities,
            net_worth: assets - liabilities,
        });
    }
    Ok(trend)
}

#[tauri::command]
pub async fn fetch_net_worth_trend(
    state: State<'_, AppState>,
) -> Result<Vec<NetWorthPoint>, AppError> {
    run_blocking(state.db.clone(), fetch_net_worth_trend_inner).await
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

    fn add_split(conn: &Connection, tag: &str, date: &str, account: &str, value: i64, qty: i64) {
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
            params![tag, date],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES (?1, ?2, ?3, ?4, 100, ?5, 100)",
            params![format!("s-{tag}"), tag, account, value, qty],
        )
        .unwrap();
    }

    fn add_price(
        conn: &Connection,
        guid: &str,
        commodity: &str,
        currency: &str,
        date: &str,
        cents: i64,
    ) {
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES (?1, ?2, ?3, ?4, 100, ?5)",
            params![guid, commodity, currency, cents, date],
        )
        .unwrap();
    }

    /// Book on top of `setup()` (checking: 50 in Jan, 35 from Feb) with a
    /// card owing 40 from Feb, a USD account holding 100 USD from Mar, and
    /// 2 shares of ACME (quoted in USD) bought in Jan.
    fn setup_trend() -> Connection {
        let conn = setup();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES
             ('c-usd', 'USD', 'CURRENCY'), ('c-acme', 'ACME', 'NASDAQ')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
             ('card', 'Card', 'CREDIT', 'root', 0, 'c-eur'),
             ('usd', 'USD cash', 'BANK', 'root', 0, 'c-usd'),
             ('acme', 'ACME', 'STOCK', 'root', 0, 'c-acme')",
            [],
        )
        .unwrap();
        add_split(&conn, "t-card", "2024-02-10", "card", -4_000, -4_000);
        add_split(&conn, "t-usd", "2024-03-03", "usd", 10_000, 10_000);
        add_split(&conn, "t-acme", "2024-01-15", "acme", 2_000, 200);
        // USD → EUR: 0.5 through February, 0.8 from March.
        add_price(&conn, "fx1", "c-usd", "c-eur", "2024-01-01", 50);
        add_price(&conn, "fx2", "c-usd", "c-eur", "2024-03-01", 80);
        // ACME in USD: 10 in January, 20 from March.
        add_price(&conn, "p1", "c-acme", "c-usd", "2024-01-20", 1_000);
        add_price(&conn, "p2", "c-acme", "c-usd", "2024-03-20", 2_000);
        conn
    }

    #[test]
    fn net_worth_trend_is_empty_without_transactions() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        assert!(fetch_net_worth_trend_inner(&conn).unwrap().is_empty());
    }

    /// Each month is valued as of its own end: past FX rates and prices,
    /// liabilities as the positive amount owed.
    #[test]
    fn net_worth_trend_values_each_month_as_of_its_end() {
        let conn = setup_trend();
        let trend = fetch_net_worth_trend_inner(&conn).unwrap();

        let dates: Vec<&str> = trend.iter().take(3).map(|p| p.date.as_str()).collect();
        assert_eq!(dates, ["2024-01-31", "2024-02-29", "2024-03-31"]);

        // Jan: 50 cash + 2 × 10 USD × 0.5.
        assert_eq!(
            (trend[0].assets, trend[0].liabilities, trend[0].net_worth),
            (60.0, 0.0, 60.0)
        );
        // Feb: 35 cash + 10 shares value, 40 owed.
        assert_eq!(
            (trend[1].assets, trend[1].liabilities, trend[1].net_worth),
            (45.0, 40.0, 5.0)
        );
        // Mar: 35 + 100 USD × 0.8 + 2 × 20 USD × 0.8, 40 owed.
        assert_eq!((trend[2].assets, trend[2].liabilities), (147.0, 40.0));
        assert_eq!(trend[2].net_worth, 107.0);
    }

    /// One point per month through today, the last one dated today and equal
    /// to the Overview's net worth KPI.
    #[test]
    fn net_worth_trend_ends_today_at_the_summary_value() {
        let conn = setup_trend();
        let trend = fetch_net_worth_trend_inner(&conn).unwrap();
        let today = Local::now().format("%Y-%m-%d").to_string();

        let months = utils::month_number(&today).unwrap() - utils::month_number("2024-01").unwrap();
        assert_eq!(trend.len() as i64, months + 1);
        let last = trend.last().unwrap();
        assert_eq!(last.date, today);
        assert_eq!(last.net_worth, get_summary(&conn).unwrap().net_worth);
    }

    /// Shares bought before their price history starts are valued at the
    /// earliest known price rather than zero.
    #[test]
    fn net_worth_trend_falls_back_to_the_earliest_price() {
        let conn = setup_trend();
        conn.execute("DELETE FROM prices WHERE guid = 'p1'", [])
            .unwrap();
        let trend = fetch_net_worth_trend_inner(&conn).unwrap();
        // Jan: 50 cash + 2 × 20 USD, converted at the price's own date (0.8).
        assert_eq!(trend[0].assets, 82.0);
    }

    #[test]
    fn net_worth_trend_skips_future_transactions() {
        let conn = setup_trend();
        add_split(&conn, "t-future", "9999-01-01", "checking", 99_000, 99_000);
        let trend = fetch_net_worth_trend_inner(&conn).unwrap();
        assert_eq!(
            trend.last().unwrap().net_worth,
            get_summary(&conn).unwrap().net_worth
        );
        assert!(trend.iter().all(|p| p.date.as_str() < "9999"));
    }
}
