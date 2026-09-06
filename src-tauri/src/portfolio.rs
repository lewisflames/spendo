use chrono::{NaiveDate, Utc};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::prices::{
    get_cached_prev_prices_map, get_cached_prices_with_date_map, get_last_refresh_timestamp,
    get_price_history, latest_quote_currencies, list_commodities, PriceHistoryEntry,
};
use crate::sql;
use crate::utils;

/// Upper bound on transactions returned for a single holding detail view.
const MAX_HOLDING_TRANSACTIONS: i64 = 500;

/// Approximate number of points targeted when sampling the portfolio value trend.
const TREND_SAMPLE_TARGET: i64 = 180;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingAccount {
    pub account_guid: String,
    pub account_name: String,
    pub shares: f64,
    pub cost_basis: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioHolding {
    pub commodity_guid: String,
    pub mnemonic: String,
    pub full_name: Option<String>,
    pub namespace: String,
    pub shares: f64,
    pub cost_basis: f64,
    pub market_value: f64,
    pub avg_cost_per_share: f64,
    pub gain_loss: f64,
    pub gain_loss_pct: f64,
    pub allocation_pct: f64,
    pub price: Option<f64>,
    pub prev_price: Option<f64>,
    pub price_date: Option<String>,
    pub accounts: Vec<HoldingAccount>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioValuePoint {
    pub date: String,
    pub market_value: f64,
    pub cost_basis: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioData {
    pub total_market_value: f64,
    pub total_cost_basis: f64,
    pub total_gain_loss: f64,
    pub total_gain_loss_pct: f64,
    pub last_refreshed: Option<String>,
    pub holdings: Vec<PortfolioHolding>,
    pub value_trend: Vec<PortfolioValuePoint>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingTransaction {
    pub guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub account_name: String,
    pub shares_delta: f64,
    pub value_delta: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingDetail {
    pub holding: PortfolioHolding,
    pub price_history: Vec<PriceHistoryEntry>,
    pub value_trend: Vec<PortfolioValuePoint>,
    pub transactions: Vec<HoldingTransaction>,
}

struct HoldingSplit {
    commodity_guid: String,
    mnemonic: String,
    full_name: Option<String>,
    namespace: String,
    account_guid: String,
    account_name: String,
    shares: f64,
    cost: f64,
}

struct SplitEvent {
    commodity_guid: String,
    post_date: String,
    shares_delta: f64,
    cost_delta: f64,
}

struct PriceEvent {
    mnemonic: String,
    date: String,
    price: f64,
    quote: String,
}

/// Commodity GUIDs that have at least one stored price row.
fn priced_commodity_guids(conn: &Connection) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT commodity_guid FROM prices")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|e| e.to_string());
    rows
}

/// Aggregated investment-account splits grouped by (commodity, account),
/// optionally restricted to a single commodity GUID.
fn load_holding_splits(
    conn: &Connection,
    filter: Option<&str>,
) -> Result<Vec<HoldingSplit>, String> {
    let mut sql = format!(
        "SELECT a.commodity_guid, c.mnemonic, c.fullname, c.namespace,
				s.account_guid, a.name AS account_name,
				SUM(CAST(s.quantity_num AS REAL) / CAST(s.quantity_denom AS REAL)) AS shares,
				SUM(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)) AS cost
		 FROM splits s
		 JOIN accounts a ON a.guid = s.account_guid
		 JOIN commodities c ON c.guid = a.commodity_guid
		 JOIN transactions t ON t.guid = s.tx_guid
		 WHERE a.account_type IN ({})
		   AND a.placeholder = 0
		   AND {}",
        utils::investment_types_sql(),
        utils::VISIBLE_TX_FILTER
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    if let Some(g) = filter {
        sql.push_str(" AND a.commodity_guid = ?1");
        params.push(Box::new(g.to_string()));
    }
    sql.push_str(" GROUP BY a.commodity_guid, s.account_guid ORDER BY c.mnemonic");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(sql::param_refs(&params).as_slice(), |row| {
            Ok(HoldingSplit {
                commodity_guid: row.get(0)?,
                mnemonic: row.get(1)?,
                full_name: row.get(2)?,
                namespace: row.get(3)?,
                account_guid: row.get(4)?,
                account_name: row.get(5)?,
                shares: row.get(6)?,
                cost: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

/// Build one entry per tracked commodity: every non-currency commodity that has
/// either splits in investment accounts or at least one stored price. Commodities
/// without positions surface as zero-share rows so sold-out / watch-only
/// securities remain visible (and their price history reachable).
fn build_holdings(
    conn: &Connection,
    filter: Option<&str>,
) -> Result<Vec<PortfolioHolding>, String> {
    let rows = load_holding_splits(conn, filter)?;

    // Universe of displayable commodities. When filtering for one GUID, the
    // split row itself carries the metadata; otherwise enumerate the book.
    #[derive(Clone)]
    struct CommodityMeta {
        mnemonic: String,
        full_name: Option<String>,
        namespace: String,
    }

    let mut meta_by_guid: HashMap<String, CommodityMeta> = HashMap::new();
    let mut with_splits: HashSet<String> = HashSet::new();
    for r in &rows {
        with_splits.insert(r.commodity_guid.clone());
        meta_by_guid
            .entry(r.commodity_guid.clone())
            .or_insert_with(|| CommodityMeta {
                mnemonic: r.mnemonic.clone(),
                full_name: r.full_name.clone(),
                namespace: r.namespace.clone(),
            });
    }
    if let Some(guid) = filter {
        // Filtered mode starts from a single GUID that may have no splits at
        // all, so its metadata comes straight from the commodities table.
        let meta: CommodityMeta = conn
            .query_row(
                "SELECT mnemonic, fullname, namespace FROM commodities WHERE guid = ?1",
                [guid],
                |row| {
                    Ok(CommodityMeta {
                        mnemonic: row.get(0)?,
                        full_name: row.get(1)?,
                        namespace: row.get(2)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    format!("Commodity not found: {guid}")
                }
                other => other.to_string(),
            })?;
        meta_by_guid.insert(guid.to_string(), meta);
    } else {
        for c in list_commodities(conn)? {
            meta_by_guid.entry(c.guid.clone()).or_insert_with(|| CommodityMeta {
                mnemonic: c.mnemonic,
                full_name: c.full_name,
                namespace: c.namespace,
            });
        }
    }

    let priced = priced_commodity_guids(conn)?;

    struct CommodityGroup {
        mnemonic: String,
        full_name: Option<String>,
        namespace: String,
        total_shares: f64,
        total_cost: f64,
        accounts: Vec<HoldingAccount>,
    }

    let report_currency = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_map = fx::get_fx_map(conn, None)?;
    // Cost-basis currency per commodity: its latest price quote currency.
    let quote_currencies = latest_quote_currencies(conn)?;

    let mut commodity_map: HashMap<String, CommodityGroup> = HashMap::new();

    // Seed groups: everything with splits, plus priced-only commodities.
    for guid in meta_by_guid.keys() {
        if !with_splits.contains(guid) && !priced.contains(guid) {
            continue;
        }
        let meta = &meta_by_guid[guid];
        commodity_map.insert(
            guid.clone(),
            CommodityGroup {
                mnemonic: meta.mnemonic.clone(),
                full_name: meta.full_name.clone(),
                namespace: meta.namespace.clone(),
                total_shares: 0.0,
                total_cost: 0.0,
                accounts: vec![],
            },
        );
    }

    for r in &rows {
        let quote_currency = quote_currencies
            .get(&r.mnemonic)
            .cloned()
            .unwrap_or_else(|| report_currency.clone());
        let cost_converted = fx::convert(r.cost, &quote_currency, &report_currency, &fx_map);
        let entry = commodity_map
            .entry(r.commodity_guid.clone())
            .or_insert_with(|| CommodityGroup {
                mnemonic: r.mnemonic.clone(),
                full_name: r.full_name.clone(),
                namespace: r.namespace.clone(),
                total_shares: 0.0,
                total_cost: 0.0,
                accounts: vec![],
            });
        entry.total_shares += r.shares;
        entry.total_cost += cost_converted;
        entry.accounts.push(HoldingAccount {
            account_guid: r.account_guid.clone(),
            account_name: r.account_name.clone(),
            shares: r.shares,
            cost_basis: cost_converted,
        });
    }

    let prices_with_date_map = get_cached_prices_with_date_map(conn)?;
    let prev_prices_map = get_cached_prev_prices_map(conn)?;

    let holdings: Vec<PortfolioHolding> = commodity_map
        .into_iter()
        .map(|(guid, group)| {
            let price_info = prices_with_date_map.get(&group.mnemonic);
            let price = price_info.map(|p| p.0);
            let price_date = price_info.map(|p| p.1.clone());
            let prev_price = prev_prices_map
                .get(&group.mnemonic)
                .map(|p| p.0)
                .filter(|_| price.is_some());
            let market_value = group.total_shares * price.unwrap_or(0.0);
            let avg_cost = if group.total_shares != 0.0 {
                group.total_cost / group.total_shares
            } else {
                0.0
            };
            let gain_loss = market_value - group.total_cost;
            let gain_loss_pct = if group.total_cost != 0.0 {
                (gain_loss / group.total_cost) * 100.0
            } else {
                0.0
            };

            PortfolioHolding {
                commodity_guid: guid,
                mnemonic: group.mnemonic,
                full_name: group.full_name,
                namespace: group.namespace,
                shares: group.total_shares,
                cost_basis: group.total_cost,
                market_value,
                avg_cost_per_share: avg_cost,
                gain_loss,
                gain_loss_pct,
                allocation_pct: 0.0,
                price,
                prev_price,
                price_date,
                accounts: group.accounts,
            }
        })
        .collect();

    Ok(holdings)
}

fn build_value_trend(
    conn: &Connection,
    filter: Option<&str>,
) -> Result<Vec<PortfolioValuePoint>, String> {
    let inputs = load_trend_inputs(conn, filter)?;
    if inputs.events.is_empty() {
        return Ok(vec![]);
    }
    let first_date_str = inputs.events[0]
        .post_date
        .get(..10)
        .ok_or_else(|| format!("Invalid first date: {}", inputs.events[0].post_date))?;
    let first_date = NaiveDate::parse_from_str(first_date_str, "%Y-%m-%d")
        .map_err(|e| format!("Invalid first date: {e}"))?;
    let end_date = Utc::now().date_naive();
    accumulate_trend(&inputs, sample_trend_dates(first_date, end_date))
}

/// Ordered inputs for the value trend: dated split/price events plus the maps
/// to value them in the report currency.
struct TrendInputs {
    events: Vec<SplitEvent>,
    price_events: Vec<PriceEvent>,
    fx_history: fx::FxHistory,
    report_currency: String,
    guid_to_mnemonic: HashMap<String, String>,
    guid_to_quote: HashMap<String, String>,
}

fn load_trend_inputs(conn: &Connection, filter: Option<&str>) -> Result<TrendInputs, String> {
    // Load split events ordered by date (optionally one commodity only)
    let mut events_sql = format!(
        "SELECT a.commodity_guid, t.post_date,
				{} AS shares_delta,
				{} AS cost_delta
		 FROM splits s
		 JOIN accounts a ON a.guid = s.account_guid
		 JOIN transactions t ON t.guid = s.tx_guid
		 WHERE a.account_type IN ({})
		   AND a.placeholder = 0
		   AND {}",
        utils::QUANTITY_AMOUNT,
        utils::RAW_AMOUNT,
        utils::investment_types_sql(),
        utils::VISIBLE_TX_FILTER
    );
    let mut event_params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    if let Some(g) = filter {
        events_sql.push_str(" AND a.commodity_guid = ?1");
        event_params.push(Box::new(g.to_string()));
    }
    events_sql.push_str(" ORDER BY t.post_date ASC");

    let mut stmt = conn.prepare(&events_sql).map_err(|e| e.to_string())?;
    let events: Vec<SplitEvent> = stmt
        .query_map(sql::param_refs(&event_params).as_slice(), |row| {
            Ok(SplitEvent {
                commodity_guid: row.get(0)?,
                post_date: row.get(1)?,
                shares_delta: row.get(2)?,
                cost_delta: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    // Load price records ordered by date (with their quote currency)
    let mut prices_sql = format!(
        "SELECT c.mnemonic, p.date,
				{} as price,
				cur.mnemonic as quote
		 FROM prices p
		 JOIN commodities c ON c.guid = p.commodity_guid
		 JOIN commodities cur ON cur.guid = p.currency_guid
		 WHERE c.namespace != 'CURRENCY'",
        utils::PRICE_AMOUNT
    );
    let mut price_params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    if let Some(g) = filter {
        prices_sql.push_str(" AND p.commodity_guid = ?1");
        price_params.push(Box::new(g.to_string()));
    }
    prices_sql.push_str(" ORDER BY p.date ASC");

    let mut price_stmt = conn.prepare(&prices_sql).map_err(|e| e.to_string())?;
    let price_events: Vec<PriceEvent> = price_stmt
        .query_map(sql::param_refs(&price_params).as_slice(), |row| {
            Ok(PriceEvent {
                mnemonic: row.get(0)?,
                date: row.get(1)?,
                price: row.get(2)?,
                quote: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    // FX rate history per currency (quoted in the report currency), for normalizing
    // prices and cost basis per date. Same source `account_detail.rs` uses.
    let report_currency = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_history = fx::FxHistory::load(conn)?;

    // Get commodity_guid → mnemonic for market value computation
    let mut guid_sql = format!(
        "SELECT a.commodity_guid, c.mnemonic
		 FROM accounts a
		 JOIN commodities c ON c.guid = a.commodity_guid
		 WHERE a.account_type IN ({}) AND a.placeholder = 0",
        utils::investment_types_sql()
    );
    let mut guid_params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    if let Some(g) = filter {
        guid_sql.push_str(" AND a.commodity_guid = ?1");
        guid_params.push(Box::new(g.to_string()));
    }

    let mut guid_stmt = conn.prepare(&guid_sql).map_err(|e| e.to_string())?;
    let mut guid_to_mnemonic: HashMap<String, String> = HashMap::new();
    let mappings: Vec<(String, String)> = guid_stmt
        .query_map(sql::param_refs(&guid_params).as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for (guid, mnemonic) in mappings {
        guid_to_mnemonic.insert(guid, mnemonic);
    }

    // Cost-basis currency per commodity (its price quote currency)
    let quote_currencies = latest_quote_currencies(conn)?;
    let mut guid_to_quote: HashMap<String, String> = HashMap::new();
    for (guid, mnemonic) in &guid_to_mnemonic {
        let quote = quote_currencies
            .get(mnemonic)
            .cloned()
            .unwrap_or_else(|| report_currency.clone());
        guid_to_quote.insert(guid.clone(), quote);
    }

    Ok(TrendInputs {
        events,
        price_events,
        fx_history,
        report_currency,
        guid_to_mnemonic,
        guid_to_quote,
    })
}

/// Sample long histories instead of emitting one point per day: the per-day
/// work is O(commodities) but the unbounded payload dominates. Always ends at
/// `end` so the trend stops at the current value.
fn sample_trend_dates(first: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let total_days = (end - first).num_days().max(0);
    let stride = i64::max(1, (total_days as f64 / TREND_SAMPLE_TARGET as f64).ceil() as i64);

    let mut sample_dates: Vec<NaiveDate> = Vec::new();
    let mut cursor = first;
    while cursor <= end {
        sample_dates.push(cursor);
        cursor += chrono::Duration::days(stride);
    }
    // Always include today so the trend ends at the current value.
    if sample_dates.last().copied() != Some(end) {
        sample_dates.push(end);
    }
    sample_dates
}

/// Walk the sampled dates, advancing splits/prices and valuing the running
/// position in the report currency per date.
fn accumulate_trend(
    inputs: &TrendInputs,
    sample_dates: Vec<NaiveDate>,
) -> Result<Vec<PortfolioValuePoint>, String> {
    let events = &inputs.events;
    let price_events = &inputs.price_events;
    let fx_history = &inputs.fx_history;
    let report_currency = inputs.report_currency.as_str();
    let guid_to_mnemonic = &inputs.guid_to_mnemonic;
    let guid_to_quote = &inputs.guid_to_quote;

    let mut running_shares: HashMap<String, f64> = HashMap::new();
    let mut running_cost: HashMap<String, f64> = HashMap::new();
    let mut running_prices: HashMap<String, f64> = HashMap::new();
    let mut event_idx: usize = 0;
    let mut price_idx: usize = 0;
    let mut trend: Vec<PortfolioValuePoint> = vec![];

    for day in sample_dates {
        let date_str = day.format("%Y-%m-%d").to_string();

        // Advance splits on or before this date (cost accumulates in the
        // commodity's own quote currency)
        while event_idx < events.len() {
            let prefix = events[event_idx].post_date.get(..10).ok_or_else(|| {
                format!("Invalid post_date: {}", events[event_idx].post_date)
            })?;
            if prefix > date_str.as_str() {
                break;
            }
            let e = &events[event_idx];
            *running_shares
                .entry(e.commodity_guid.clone())
                .or_insert(0.0) += e.shares_delta;
            *running_cost.entry(e.commodity_guid.clone()).or_insert(0.0) += e.cost_delta;
            event_idx += 1;
        }

        // FX rates effective on this date
        let fx_at = fx_history.map_for_date(&date_str);

        // Advance prices on or before this date, normalized to the report currency
        while price_idx < price_events.len() && price_events[price_idx].date <= date_str {
            let p = &price_events[price_idx];
            let rate = if p.quote == report_currency {
                1.0
            } else {
                fx_at.get(&p.quote).copied().unwrap_or(1.0)
            };
            running_prices.insert(p.mnemonic.clone(), p.price * rate);
            price_idx += 1;
        }

        // Compute market value (report currency)
        let mut market_value = 0.0;
        for (guid, shares) in &running_shares {
            if *shares == 0.0 {
                continue;
            }
            if let Some(mnemonic) = guid_to_mnemonic.get(guid) {
                let price = running_prices.get(mnemonic).copied().unwrap_or(0.0);
                market_value += shares * price;
            }
        }

        // Cost basis converted to the report currency at this date's FX
        let cost_basis: f64 = running_cost
            .iter()
            .map(|(guid, cost)| {
                let quote = guid_to_quote.get(guid).map(|q| q.as_str()).unwrap_or("");
                let rate = if quote == report_currency {
                    1.0
                } else {
                    fx_at.get(quote).copied().unwrap_or(1.0)
                };
                cost * rate
            })
            .sum();

        trend.push(PortfolioValuePoint {
            date: date_str,
            market_value,
            cost_basis,
        });
    }

    Ok(trend)
}

fn finalize_totals(
    mut holdings: Vec<PortfolioHolding>,
) -> (f64, f64, f64, f64, Vec<PortfolioHolding>) {
    let total_market_value: f64 = holdings.iter().map(|h| h.market_value).sum();
    let total_cost_basis: f64 = holdings.iter().map(|h| h.cost_basis).sum();
    let total_gain_loss = total_market_value - total_cost_basis;
    let total_gain_loss_pct = if total_cost_basis != 0.0 {
        (total_gain_loss / total_cost_basis) * 100.0
    } else {
        0.0
    };

    for h in &mut holdings {
        h.allocation_pct = if total_market_value != 0.0 {
            (h.market_value / total_market_value) * 100.0
        } else {
            0.0
        };
    }

    // Value desc; zero-value (closed/watch) holdings trail alphabetically.
    holdings.sort_by(|a, b| {
        b.market_value
            .partial_cmp(&a.market_value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.mnemonic.cmp(&b.mnemonic))
    });

    (
        total_market_value,
        total_cost_basis,
        total_gain_loss,
        total_gain_loss_pct,
        holdings,
    )
}

fn fetch_portfolio_data_inner(conn: &Connection) -> Result<PortfolioData, String> {
    let holdings = build_holdings(conn, None)?;
    let (total_market_value, total_cost_basis, total_gain_loss, total_gain_loss_pct, holdings) =
        finalize_totals(holdings);
    let value_trend = build_value_trend(conn, None)?;
    let last_refreshed = get_last_refresh_timestamp(conn)?;

    Ok(PortfolioData {
        total_market_value,
        total_cost_basis,
        total_gain_loss,
        total_gain_loss_pct,
        last_refreshed,
        holdings,
        value_trend,
    })
}

fn fetch_holding_transactions(
    conn: &Connection,
    commodity_guid: &str,
) -> Result<Vec<HoldingTransaction>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT t.guid, t.post_date, t.description,
					MIN(a.name) AS account_name,
					SUM({}) AS shares_delta,
					SUM({}) AS value_delta
			 FROM splits s
			 JOIN accounts a ON a.guid = s.account_guid
			 JOIN transactions t ON t.guid = s.tx_guid
			 WHERE a.commodity_guid = ?1
			   AND a.account_type IN ({})
			   AND a.placeholder = 0
			   AND {}
			 GROUP BY t.guid
			 ORDER BY t.post_date DESC
			 LIMIT {}",
            utils::QUANTITY_AMOUNT,
            utils::RAW_AMOUNT,
            utils::investment_types_sql(),
            utils::VISIBLE_TX_FILTER,
            MAX_HOLDING_TRANSACTIONS
        ))
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([commodity_guid], |row| {
            Ok(HoldingTransaction {
                guid: row.get(0)?,
                post_date: row.get(1)?,
                description: row.get(2)?,
                account_name: row.get(3)?,
                shares_delta: row.get(4)?,
                value_delta: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

fn fetch_holding_detail_inner(
    conn: &Connection,
    commodity_guid: &str,
) -> Result<HoldingDetail, String> {
    // Allocation is relative to the whole portfolio, so it is derived from the
    // unfiltered holdings rather than the single-commodity aggregation.
    let all_holdings = build_holdings(conn, None)?;
    let total_market_value: f64 = all_holdings.iter().map(|h| h.market_value).sum();

    let mut holding = all_holdings
        .into_iter()
        .find(|h| h.commodity_guid == commodity_guid)
        .ok_or_else(|| format!("Holding not found: {commodity_guid}"))?;
    holding.allocation_pct = if total_market_value != 0.0 {
        (holding.market_value / total_market_value) * 100.0
    } else {
        0.0
    };

    let price_history = get_price_history(conn, commodity_guid)?;
    let value_trend = build_value_trend(conn, Some(commodity_guid))?;
    let transactions = fetch_holding_transactions(conn, commodity_guid)?;

    Ok(HoldingDetail {
        holding,
        price_history,
        value_trend,
        transactions,
    })
}

#[tauri::command]
pub async fn fetch_portfolio_data(state: State<'_, AppState>) -> Result<PortfolioData, String> {
    run_blocking(state.db.clone(), fetch_portfolio_data_inner).await
}

#[tauri::command]
pub async fn fetch_holding_detail(
    state: State<'_, AppState>,
    commodity_guid: String,
) -> Result<HoldingDetail, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_holding_detail_inner(conn, &commodity_guid)
    })
    .await
}
