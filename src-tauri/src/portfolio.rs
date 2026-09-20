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

/// Approximate number of points targeted when sampling the portfolio value trend.
const TREND_SAMPLE_TARGET: i64 = 180;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingAccount {
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
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
    pub num: Option<String>,
    pub notes: Option<String>,
    pub account_name: String,
    pub account_description: Option<String>,
    pub shares_delta: f64,
    pub value_delta: f64,
    /// Cumulative share balance after this transaction (ascending date order),
    /// so paged newest-first rows still carry a globally correct balance.
    pub running_shares: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingDetail {
    pub holding: PortfolioHolding,
    pub price_history: Vec<PriceHistoryEntry>,
    pub value_trend: Vec<PortfolioValuePoint>,
    pub transactions: Vec<HoldingTransaction>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

struct HoldingSplit {
    commodity_guid: String,
    mnemonic: String,
    full_name: Option<String>,
    namespace: String,
    account_guid: String,
    account_name: String,
    account_description: Option<String>,
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
				s.account_guid, a.name AS account_name, a.description AS account_description,
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
                account_description: row.get(6)?,
                shares: row.get(7)?,
                cost: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

#[derive(Clone)]
struct CommodityMeta {
    mnemonic: String,
    full_name: Option<String>,
    namespace: String,
}

struct CommodityGroup {
    mnemonic: String,
    full_name: Option<String>,
    namespace: String,
    total_shares: f64,
    total_cost: f64,
    accounts: Vec<HoldingAccount>,
}

/// Commodity metadata plus the GUID sets used to seed the holding groups.
struct HoldingUniverse {
    meta_by_guid: HashMap<String, CommodityMeta>,
    with_splits: HashSet<String>,
    priced: HashSet<String>,
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
    let universe = resolve_holding_universe(conn, &rows, filter)?;
    let commodity_map = build_commodity_groups(conn, &rows, &universe)?;
    value_commodity_groups(conn, commodity_map)
}

/// Universe of displayable commodities. When filtering for one GUID, the
/// split row itself carries the metadata; otherwise enumerate the book.
fn resolve_holding_universe(
    conn: &Connection,
    rows: &[HoldingSplit],
    filter: Option<&str>,
) -> Result<HoldingUniverse, String> {
    let mut meta_by_guid: HashMap<String, CommodityMeta> = HashMap::new();
    let mut with_splits: HashSet<String> = HashSet::new();
    for r in rows {
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

    Ok(HoldingUniverse {
        meta_by_guid,
        with_splits,
        priced,
    })
}

/// Seed one group per displayable commodity and fold each split row into its
/// group, converting cost to the report currency.
fn build_commodity_groups(
    conn: &Connection,
    rows: &[HoldingSplit],
    universe: &HoldingUniverse,
) -> Result<HashMap<String, CommodityGroup>, String> {
    let report_currency = fx::get_report_currency_mnemonic(conn)?.unwrap_or_default();
    let fx_map = fx::get_fx_map(conn, None)?;
    // Cost-basis currency per commodity: its latest price quote currency.
    let quote_currencies = latest_quote_currencies(conn)?;

    let mut commodity_map: HashMap<String, CommodityGroup> = HashMap::new();

    // Seed groups: everything with splits, plus priced-only commodities.
    for guid in universe.meta_by_guid.keys() {
        if !universe.with_splits.contains(guid) && !universe.priced.contains(guid) {
            continue;
        }
        let meta = &universe.meta_by_guid[guid];
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

    for r in rows {
        let quote_currency = quote_currencies
            .get(&r.commodity_guid)
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
            account_description: r.account_description.clone(),
            shares: r.shares,
            cost_basis: cost_converted,
        });
    }

    Ok(commodity_map)
}

/// Valuation pass: value each group at its latest cached price and derive the
/// per-holding metrics (allocation is finalized later by `finalize_totals`).
fn value_commodity_groups(
    conn: &Connection,
    commodity_map: HashMap<String, CommodityGroup>,
) -> Result<Vec<PortfolioHolding>, String> {
    let prices_with_date_map = get_cached_prices_with_date_map(conn)?;
    let prev_prices_map = get_cached_prev_prices_map(conn)?;

    let holdings: Vec<PortfolioHolding> = commodity_map
        .into_iter()
        .map(|(guid, group)| {
            let price_info = prices_with_date_map.get(&guid);
            let price = price_info.map(|p| p.0);
            let price_date = price_info.map(|p| p.1.clone());
            let prev_price = prev_prices_map.get(&guid).map(|p| p.0).filter(|_| price.is_some());
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
    let report_currency = fx::get_report_currency_mnemonic(conn)?.unwrap_or_default();
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
    for guid in guid_to_mnemonic.keys() {
        let quote = quote_currencies
            .get(guid)
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

/// One page of a holding's buy/sell activity with pagination metadata.
struct HoldingTxPage {
    transactions: Vec<HoldingTransaction>,
    total: i64,
    safe_page: i64,
    page_size: i64,
    total_pages: i64,
}

/// Grouped per-transaction activity row before pagination.
struct HoldingTxRow {
    guid: String,
    post_date: String,
    description: Option<String>,
    num: Option<String>,
    notes: Option<String>,
    account_name: String,
    account_description: Option<String>,
    shares_delta: f64,
    value_delta: f64,
}

fn fetch_holding_transactions(
    conn: &Connection,
    commodity_guid: &str,
    page: i64,
    page_size: i64,
    search: Option<&str>,
) -> Result<HoldingTxPage, String> {
    // Trimmed, empty searches match everything (the frontend sends None, but
    // direct callers shouldn't have to).
    let search = search.map(str::trim).filter(|s| !s.is_empty());
    let mut sql = format!(
        "SELECT t.guid, t.post_date, t.description, t.num, t.notes,
					MIN(a.name) AS account_name,
					-- Description of the MIN(name) account: ordering by the combined
					-- key picks the same row, so name and description stay paired.
					MIN(a.name || CHAR(31) || IFNULL(a.description, '')) AS account_key,
					SUM({}) AS shares_delta,
					SUM({}) AS value_delta
			 FROM splits s
			 JOIN accounts a ON a.guid = s.account_guid
			 JOIN transactions t ON t.guid = s.tx_guid
			 WHERE a.commodity_guid = ?1
			   AND a.account_type IN ({})
			   AND a.placeholder = 0
			   AND {}",
        utils::QUANTITY_AMOUNT,
        utils::RAW_AMOUNT,
        utils::investment_types_sql(),
        utils::VISIBLE_TX_FILTER,
    );
    // ?1 is the commodity GUID; the search reuses ?2 three times.
    let mut params: Vec<Box<dyn rusqlite::ToSql>> =
        vec![Box::new(commodity_guid.to_string())];
    if let Some(q) = search {
        sql.push_str(
            " AND (t.description LIKE '%' || ?2 || '%' ESCAPE '\\' \
               OR t.num LIKE '%' || ?2 || '%' ESCAPE '\\' \
               OR t.notes LIKE '%' || ?2 || '%' ESCAPE '\\')",
        );
        params.push(Box::new(utils::escape_like(q)));
    }
    sql.push_str(" GROUP BY t.guid ORDER BY t.post_date ASC, t.guid ASC");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let mut rows: Vec<HoldingTxRow> = stmt
        .query_map(sql::param_refs(&params).as_slice(), |row| {
            let account_name: String = row.get(5)?;
            let account_key: String = row.get(6)?;
            let account_description = account_key
                .split_once('\u{1f}')
                .map(|(_, d)| d.to_string())
                .filter(|d| !d.is_empty());
            Ok(HoldingTxRow {
                guid: row.get(0)?,
                post_date: row.get(1)?,
                description: row.get(2)?,
                num: row.get(3)?,
                notes: row.get(4)?,
                account_name,
                account_description,
                shares_delta: row.get(7)?,
                value_delta: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let total = rows.len() as i64;
    let (page_size, total_pages, safe_page, offset) = utils::paginate(total, page, page_size);

    // Running share balance accumulates in ascending date order so every page
    // carries a globally correct balance (mirrors account_detail's
    // running_by_guid, without FX since shares are unit-less).
    let mut running = 0.0;
    let mut running_by_guid: HashMap<String, f64> = HashMap::with_capacity(rows.len());
    for r in &rows {
        running += r.shares_delta;
        running_by_guid.insert(r.guid.clone(), running);
    }

    // Newest-first display order; running balances were fixed above.
    rows.sort_by(|a, b| {
        b.post_date
            .cmp(&a.post_date)
            .then_with(|| b.guid.cmp(&a.guid))
    });

    let transactions = rows
        .into_iter()
        .skip(offset as usize)
        .take(page_size as usize)
        .map(|r| {
            let running_shares = running_by_guid.remove(&r.guid).unwrap_or(0.0);
            HoldingTransaction {
                guid: r.guid,
                post_date: r.post_date,
                description: r.description,
                num: r.num,
                notes: r.notes,
                account_name: r.account_name,
                account_description: r.account_description,
                shares_delta: r.shares_delta,
                value_delta: r.value_delta,
                running_shares,
            }
        })
        .collect();

    Ok(HoldingTxPage {
        transactions,
        total,
        safe_page,
        page_size,
        total_pages,
    })
}

fn fetch_holding_detail_inner(
    conn: &Connection,
    commodity_guid: &str,
    page: i64,
    page_size: i64,
    search: Option<&str>,
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
    let tx_page = fetch_holding_transactions(conn, commodity_guid, page, page_size, search)?;

    Ok(HoldingDetail {
        holding,
        price_history,
        value_trend,
        transactions: tx_page.transactions,
        total: tx_page.total,
        page: tx_page.safe_page,
        page_size: tx_page.page_size,
        total_pages: tx_page.total_pages,
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
    page: i64,
    page_size: i64,
    search: Option<String>,
) -> Result<HoldingDetail, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_holding_detail_inner(conn, &commodity_guid, page, page_size, search.as_deref())
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
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES
             ('sec-aapl', 'AAPL', 'NASDAQ', 'Apple Inc'),
             ('c-eur', 'EUR', 'CURRENCY', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, description, placeholder, commodity_guid) VALUES
             ('broker', 'Brokerage', 'STOCK', NULL, 'Brokerage acct', 0, 'sec-aapl')",
            [],
        )
        .unwrap();
        add_activity(&conn, "t1", "2024-01-10", "Buy AAPL", Some("B1"), Some("first buy"), 10, 1000);
        add_activity(&conn, "t2", "2024-02-05", "Buy AAPL", None, None, 5, 550);
        add_activity(&conn, "t3", "2024-03-01", "Sell AAPL", Some("S1"), Some("trim"), -8, -960);
        conn
    }

    #[allow(clippy::too_many_arguments)]
    fn add_activity(
        conn: &Connection,
        tag: &str,
        date: &str,
        description: &str,
        num: Option<&str>,
        notes: Option<&str>,
        shares: i64,
        value: i64,
    ) {
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![tag, date, description, num, notes],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES (?1, ?2, 'broker', ?3, 100, ?4, 1)",
            params![format!("s-{tag}"), tag, value * 100, shares],
        )
        .unwrap();
    }

    #[test]
    fn holding_activity_paginates_newest_first_with_running_balances() {
        let conn = setup();

        let p1 = fetch_holding_transactions(&conn, "sec-aapl", 1, 2, None).unwrap();
        assert_eq!((p1.total, p1.safe_page, p1.total_pages), (3, 1, 2));
        assert_eq!(p1.transactions.len(), 2);
        // Newest first, with globally correct running balances (10 + 5 - 8).
        assert_eq!(p1.transactions[0].guid, "t3");
        assert_eq!(p1.transactions[0].running_shares, 7.0);
        assert_eq!(p1.transactions[0].num.as_deref(), Some("S1"));
        assert_eq!(p1.transactions[0].notes.as_deref(), Some("trim"));
        assert_eq!(p1.transactions[1].guid, "t2");
        assert_eq!(p1.transactions[1].running_shares, 15.0);
        assert_eq!(p1.transactions[1].num, None);

        let p2 = fetch_holding_transactions(&conn, "sec-aapl", 2, 2, None).unwrap();
        assert_eq!(p2.transactions.len(), 1);
        assert_eq!(p2.transactions[0].guid, "t1");
        assert_eq!(p2.transactions[0].running_shares, 10.0);
        assert_eq!(p2.transactions[0].account_name, "Brokerage");
        assert_eq!(
            p2.transactions[0].account_description.as_deref(),
            Some("Brokerage acct")
        );

        // Out-of-range pages clamp to the last valid page.
        let clamped = fetch_holding_transactions(&conn, "sec-aapl", 99, 2, None).unwrap();
        assert_eq!(clamped.safe_page, 2);
        assert_eq!(clamped.transactions.len(), 1);
    }

    #[test]
    fn holding_activity_search_matches_description_num_and_notes() {
        let conn = setup();

        // Notes match.
        let page = fetch_holding_transactions(&conn, "sec-aapl", 1, 25, Some("trim")).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.transactions[0].guid, "t3");

        // Number match.
        let page = fetch_holding_transactions(&conn, "sec-aapl", 1, 25, Some("B1")).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.transactions[0].guid, "t1");

        // Description match (LIKE is ASCII case-insensitive).
        let page = fetch_holding_transactions(&conn, "sec-aapl", 1, 25, Some("buy")).unwrap();
        assert_eq!(page.total, 2);

        // LIKE metacharacters match literally, not as wildcards.
        let page = fetch_holding_transactions(&conn, "sec-aapl", 1, 25, Some("%")).unwrap();
        assert_eq!(page.total, 0);

        // No match yields an empty page.
        let page = fetch_holding_transactions(&conn, "sec-aapl", 1, 25, Some("zzz")).unwrap();
        assert_eq!(page.total, 0);
        assert!(page.transactions.is_empty());
    }

    #[test]
    fn holding_without_activity_returns_empty_page() {
        let conn = setup();
        let page = fetch_holding_transactions(&conn, "c-eur", 1, 25, None).unwrap();
        assert_eq!(page.total, 0);
        assert!(page.transactions.is_empty());
    }

    fn setup_valued_portfolio() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES
             ('c-eur', 'EUR', 'CURRENCY', NULL),
             ('c-aapl', 'AAPL', 'NASDAQ', 'Apple Inc'),
             ('c-xyz', 'XYZ', 'NASDAQ', 'XYZ Corp')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
             ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
             ('broker', 'Broker', 'STOCK', 'root', 0, 'c-aapl'),
             ('vault', 'Vault', 'STOCK', 'root', 0, 'c-xyz')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES
             ('t-aapl', '2024-02-01'), ('t-xyz', '2024-02-15')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom) VALUES
             ('s-aapl', 't-aapl', 'broker', 100000, 100, 10, 1),
             ('s-xyz', 't-xyz', 'vault', 70000, 100, 5, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES ('p-aapl-prev', 'c-aapl', 'c-eur', 14000, 100, '2024-02-01'),
                    ('p-aapl-latest', 'c-aapl', 'c-eur', 15000, 100, '2024-03-01')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn portfolio_values_priced_holdings_and_zero_values_unpriced_ones() {
        let conn = setup_valued_portfolio();
        let data = fetch_portfolio_data_inner(&conn).unwrap();

        // Value desc: priced AAPL first, unpriced XYZ trails.
        assert_eq!(data.holdings.len(), 2);
        let aapl = &data.holdings[0];
        assert_eq!(aapl.commodity_guid, "c-aapl");
        assert_eq!(aapl.shares, 10.0);
        assert_eq!(aapl.cost_basis, 1000.0);
        assert_eq!(aapl.market_value, 1500.0);
        assert_eq!(aapl.gain_loss, 500.0);
        assert_eq!(aapl.gain_loss_pct, 50.0);
        assert_eq!(aapl.price, Some(150.0));
        assert_eq!(aapl.prev_price, Some(140.0));
        assert_eq!(aapl.price_date.as_deref(), Some("2024-03-01"));
        assert_eq!(aapl.allocation_pct, 100.0);

        let xyz = &data.holdings[1];
        assert_eq!(xyz.commodity_guid, "c-xyz");
        assert_eq!(xyz.shares, 5.0);
        assert_eq!(xyz.cost_basis, 700.0);
        assert_eq!(xyz.market_value, 0.0);
        assert_eq!(xyz.gain_loss, -700.0);
        assert_eq!(xyz.price, None);
        assert_eq!(xyz.allocation_pct, 0.0);

        assert_eq!(data.total_market_value, 1500.0);
        assert_eq!(data.total_cost_basis, 1700.0);
        assert_eq!(data.total_gain_loss, -200.0);
        assert!((data.total_gain_loss_pct - (-200.0 / 1700.0) * 100.0).abs() < 1e-9);
    }
}
