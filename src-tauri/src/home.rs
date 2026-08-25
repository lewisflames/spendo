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

pub fn get_summary(conn: &Connection) -> Result<DataSummary, String> {
    let currency = fx::get_report_currency_mnemonic(conn).unwrap_or_else(|| "Unknown".to_string());

    let total_accounts: i64 = conn
        .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    // Same visibility window as every other page: future-dated transactions
    // must not inflate totals, the displayed date range or net worth.
    let total_transactions: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM transactions WHERE {}", utils::VISIBLE_TX_FILTER),
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    let (earliest_date, latest_date): (Option<String>, Option<String>) = conn
        .query_row(
            &format!(
                "SELECT substr(MIN(post_date), 1, 10), substr(MAX(post_date), 1, 10)
                 FROM transactions WHERE {}",
                utils::VISIBLE_TX_FILTER
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| e.to_string())?;

    // Net worth in report currency: cash per account currency converted at latest FX,
    // investments marked to market with report-currency-normalized prices.
    let cash_rows: Vec<(String, f64, f64, f64)> = {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT c.mnemonic,
                        COALESCE(SUM(CASE WHEN a.account_type IN ('ASSET','BANK','CASH','CURRENCY','RECEIVABLE')
                            THEN {} ELSE 0 END), 0) as cash_assets,
                        COALESCE(SUM(CASE WHEN a.account_type IN ({})
                            THEN {} ELSE 0 END), 0) as shares,
                        COALESCE(SUM(CASE WHEN a.account_type IN ('LIABILITY','CREDIT','PAYABLE')
                            THEN {} ELSE 0 END), 0) as liabilities
                 FROM splits s
                 JOIN accounts a ON a.guid = s.account_guid
                 JOIN commodities c ON c.guid = a.commodity_guid
                 JOIN transactions t ON t.guid = s.tx_guid
                 WHERE {}
                 GROUP BY c.mnemonic",
                utils::RAW_AMOUNT,
                utils::investment_types_sql(),
                utils::QUANTITY_AMOUNT,
                utils::RAW_AMOUNT,
                utils::VISIBLE_TX_FILTER
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };

    let fx_map = fx::get_fx_map(conn, None)?;
    let unified_prices = get_cached_prices_map(conn).unwrap_or_default();

    let net_worth: f64 = cash_rows
        .iter()
        .map(|(mnemonic, cash_assets, shares, liabilities)| {
            let cash_val = fx::convert(*cash_assets - *liabilities, mnemonic, &currency, &fx_map);
            let inv_val = if *shares != 0.0 {
                unified_prices.get(mnemonic).copied().unwrap_or(0.0) * shares
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
pub async fn fetch_home_data(state: State<'_, AppState>) -> Result<HomeDataResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        let cnt: i64 = conn
            .query_row("SELECT COUNT(*) as cnt FROM accounts", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;

        if cnt == 0 {
            return Ok(HomeDataResult {
                has_data: false,
                summary: None,
            });
        }

        Ok(HomeDataResult {
            has_data: true,
            summary: Some(get_summary(conn)?),
        })
    })
    .await
}
