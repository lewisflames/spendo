use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{self, AppState};
use crate::fx;
use crate::utils;
use crate::yahoo::{
    derive_yahoo_symbols, fetch_single_yahoo_history, quote_currency_for_symbol,
    SUPPORTED_CRYPTO_FIAT,
};

/// Denominator used when storing fetched prices as integer fractions.
/// 10^4 keeps four decimal places (sub-penny quotes quantize to zero).
const PRICE_SCALE: i64 = 10_000;

/// Upper bound on simultaneous Yahoo history requests.
const MAX_CONCURRENT_FETCHES: usize = 5;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommodityInfo {
    pub guid: String,
    pub mnemonic: String,
    pub namespace: String,
    pub full_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceEntry {
    pub mnemonic: String,
    pub price: f64,
    pub price_date: String,
    pub currency: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricesResult {
    pub prices: Vec<PriceEntry>,
    pub fetched_at: String,
    pub errors: Vec<PriceError>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceError {
    pub mnemonic: String,
    pub error: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceHistoryEntry {
    pub date: String,
    pub price: f64,
    pub currency: String,
}

pub fn list_commodities(conn: &Connection) -> Result<Vec<CommodityInfo>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT guid, mnemonic, namespace, fullname
             FROM commodities
             WHERE namespace != 'CURRENCY'
             AND namespace != 'template'
             ORDER BY namespace, mnemonic",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(CommodityInfo {
                guid: row.get(0)?,
                mnemonic: row.get(1)?,
                namespace: row.get(2)?,
                full_name: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

fn latest_n_prices_raw(
    conn: &Connection,
    rn_filter: u32,
) -> Result<Vec<(String, f64, String, String)>, String> {
    // (mnemonic, price, date, quote_currency_mnemonic)
    let mut stmt = conn
        .prepare(&format!(
            "SELECT mnemonic, price, date, quote FROM (
                    SELECT c.mnemonic,
                        {} as price,
                        p.date,
                        cur.mnemonic as quote,
                        ROW_NUMBER() OVER (PARTITION BY c.mnemonic ORDER BY p.date DESC, p.guid DESC) as rn
                    FROM prices p
                    JOIN commodities c ON c.guid = p.commodity_guid
                    JOIN commodities cur ON cur.guid = p.currency_guid
                    WHERE c.namespace != 'CURRENCY'
                 ) WHERE rn = ?1",
            utils::PRICE_AMOUNT
        ))
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([rn_filter], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

/// Normalize a price quoted in `quote_currency` to the report currency as of `date`.
fn normalize_price(
    price: f64,
    quote_currency: &str,
    date: &str,
    report_guid: &str,
    report_mnemonic: &str,
    conn: &Connection,
) -> Result<f64, String> {
    if quote_currency == report_mnemonic {
        return Ok(price);
    }
    Ok(price * fx::get_fx_rate_at(conn, quote_currency, report_guid, date)?)
}

fn get_latest_live_prices(conn: &Connection) -> Result<Vec<PriceEntry>, String> {
    Ok(latest_n_prices_raw(conn, 1)?
        .into_iter()
        .map(|(mnemonic, price, date, currency)| PriceEntry {
            mnemonic,
            price,
            price_date: date,
            currency,
        })
        .collect())
}

pub fn get_last_refresh_timestamp(conn: &Connection) -> Result<Option<String>, String> {
    let result: Option<String> = conn
        .query_row("SELECT MAX(refreshed_at) FROM refresh_log", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    Ok(result)
}

pub fn get_price_history(
    conn: &Connection,
    commodity_guid: &str,
) -> Result<Vec<PriceHistoryEntry>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT p.date,
                    {} as price,
                    cur.mnemonic as currency
             FROM prices p
             JOIN commodities cur ON cur.guid = p.currency_guid
             WHERE p.commodity_guid = ?1
             ORDER BY p.date ASC",
            utils::PRICE_AMOUNT
        ))
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([commodity_guid], |row| {
            Ok(PriceHistoryEntry {
                date: row.get(0)?,
                price: row.get(1)?,
                currency: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolOverride {
    pub commodity_guid: String,
    pub mnemonic: String,
    pub namespace: String,
    pub full_name: Option<String>,
    pub symbol: Option<String>,
}

/// Normalize free-text Yahoo symbols: trim + ASCII-uppercase (`aapl` →
/// `AAPL`, `btc-eur` → `BTC-EUR`). Rejects empty, overlong (>32) or values
/// containing whitespace, `/`, `?` or `#` (URL path breakers).
fn normalize_symbol(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Symbol must not be empty".to_string());
    }
    if trimmed.len() > 32 {
        return Err("Symbol must be 32 characters or fewer".to_string());
    }
    if trimmed
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '/' | '?' | '#'))
    {
        return Err("Symbol must not contain spaces, '/', '?' or '#'".to_string());
    }
    Ok(trimmed.to_ascii_uppercase())
}

/// commodity_guid -> override symbol. Tolerates pre-migration DBs (missing
/// table) by returning an empty map; `init_db` creates the table on startup.
fn symbol_override_map(conn: &Connection) -> HashMap<String, String> {
    let mut stmt = match conn.prepare("SELECT commodity_guid, symbol FROM symbol_overrides") {
        Ok(s) => s,
        Err(_) => return HashMap::new(),
    };
    let rows = match stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };
    rows.flatten().collect()
}

fn list_symbol_overrides_inner(conn: &Connection) -> Result<Vec<SymbolOverride>, String> {
    let commodities = list_commodities(conn)?;
    let overrides = symbol_override_map(conn);
    Ok(commodities
        .into_iter()
        .map(|c| SymbolOverride {
            commodity_guid: c.guid.clone(),
            mnemonic: c.mnemonic,
            namespace: c.namespace,
            full_name: c.full_name,
            symbol: overrides.get(&c.guid).cloned(),
        })
        .collect())
}

pub(crate) struct PriceFetchPlan {
    commodities: Vec<CommodityInfo>,
    already_fetched: HashSet<String>,
    latest_dates: HashMap<String, String>,
    report_currency_guid: Option<String>,
    report_currency: Option<String>,
    currencies: Vec<(String, String)>,
    currency_guid_map: HashMap<String, String>,
    existing_live_prices: Vec<PriceEntry>,
    overrides: HashMap<String, String>,
}

pub(crate) struct PendingPriceWrites {
    symbol_map: HashMap<String, String>,
    yahoo_history: HashMap<String, Vec<(i64, f64)>>,
    assigned: HashSet<String>,
    report_currency_guid: Option<String>,
    report_currency: Option<String>,
    currency_guid_map: HashMap<String, String>,
    commodities: Vec<CommodityInfo>,
    fx_prices: Vec<(String, Vec<(i64, f64)>)>,
}

fn read_price_data(conn: &Connection) -> Result<PriceFetchPlan, String> {
    let commodities = list_commodities(conn)?;

    let already_fetched: HashSet<String> = {
        let mut stmt = conn
            .prepare("SELECT DISTINCT commodity_guid FROM prices")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };

    // Latest stored date per commodity drives incremental fetching (period1).
    // Failures here must propagate: silently degrading to empty dates turns an
    // incremental refresh into a full 10-year refetch for every symbol.
    let latest_dates: HashMap<String, String> = {
        let mut stmt = conn
            .prepare("SELECT commodity_guid, MAX(date) FROM prices GROUP BY commodity_guid")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<HashMap<_, _>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };

    let report_currency_guid = fx::get_report_currency_guid(conn);
    let report_currency = fx::get_report_currency_mnemonic(conn);
    let currencies = fx::list_book_currencies(conn);
    let currency_guid_map = fx::currency_guid_map(conn);

    let existing_live_prices = get_latest_live_prices(conn).unwrap_or_default();
    let overrides = symbol_override_map(conn);

    Ok(PriceFetchPlan {
        commodities,
        already_fetched,
        latest_dates,
        report_currency_guid,
        report_currency,
        currencies,
        currency_guid_map,
        existing_live_prices,
        overrides,
    })
}

/// Parse a stored date (either "YYYY-MM-DD" or "YYYY-MM-DD HH:MM:SS") into a
/// Unix timestamp for the Yahoo `period1` parameter.
fn date_to_period1(d: &str) -> Option<i64> {
    let day = if d.len() >= 10 { &d[..10] } else { d };
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .and_then(|nd| nd.and_hms_opt(0, 0, 0))
        .map(|ndt| ndt.and_utc().timestamp())
}

/// Insert one price per day for a commodity, replacing any existing same-day rows.
/// Single range DELETE (instead of N per-day deletes); the per-day guid
/// (`commodity-date`) keeps the subsequent INSERTs idempotent.
fn write_history_prices(
    conn: &Connection,
    commodity_guid: &str,
    currency_guid: &str,
    history: &[(i64, f64)],
) -> Result<(), String> {
    if history.is_empty() {
        return Ok(());
    }
    let now = utils::iso_now();
    let dates: Vec<String> = history
        .iter()
        .map(|(ts, _)| {
            chrono::DateTime::from_timestamp(*ts, 0)
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| now.clone())
        })
        .collect();
    let (min_date, max_date) = dates.iter().fold((dates[0].clone(), dates[0].clone()), |(mn, mx), d| {
        (mn.min(d.clone()), mx.max(d.clone()))
    });
    conn.execute(
        "DELETE FROM prices WHERE commodity_guid = ?1 AND substr(date, 1, 10) BETWEEN ?2 AND ?3",
        rusqlite::params![commodity_guid, min_date, max_date],
    )
    .map_err(|e| e.to_string())?;

    let insert_sql = format!(
        "INSERT OR REPLACE INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
         VALUES (?1, ?2, ?3, ?4, {PRICE_SCALE}, ?5)"
    );
    let mut insert = conn.prepare(&insert_sql).map_err(|e| e.to_string())?;

    for ((_, price), date_str) in history.iter().zip(dates.iter()) {
        let price_guid = format!("{}-{}", commodity_guid, date_str);
        let value_num = (*price * PRICE_SCALE as f64).round() as i64;
        insert
            .execute(rusqlite::params![
                price_guid,
                commodity_guid,
                currency_guid,
                value_num,
                date_str
            ])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_price_data(conn: &Connection, data: &PendingPriceWrites) -> Result<(), String> {
    let PendingPriceWrites {
        symbol_map,
        yahoo_history,
        assigned,
        report_currency_guid,
        report_currency,
        currency_guid_map,
        commodities,
        fx_prices,
    } = data;

    for (sym, mnemonic) in symbol_map {
        if !assigned.contains(mnemonic) {
            continue;
        }
        let Some(history) = yahoo_history.get(sym) else {
            continue;
        };
        let Some(commodity) = commodities.iter().find(|c| c.mnemonic == *mnemonic) else {
            continue;
        };

        // Store prices in their actual quote currency so normalization at read time
        // can convert to the report currency. Fall back to the report currency GUID
        // when the quote currency isn't present in the book.
        let quote = quote_currency_for_symbol(
            sym,
            &commodity.namespace,
            report_currency.as_deref().unwrap_or(""),
        );
        let currency_guid = currency_guid_map
            .get(&quote)
            .cloned()
            .or_else(|| report_currency_guid.clone());
        let Some(cg) = currency_guid else {
            continue;
        };

        write_history_prices(conn, &commodity.guid, &cg, history)?;
    }

    // FX rates: each book currency quoted in the report currency.
    let Some(report_guid) = report_currency_guid else {
        return Ok(());
    };
    for (mnemonic, history) in fx_prices {
        let Some(commodity_guid) = conn
            .query_row(
                "SELECT guid FROM commodities WHERE mnemonic = ?1 AND namespace = 'CURRENCY'",
                [mnemonic],
                |row| row.get::<_, String>(0),
            )
            .ok()
        else {
            continue;
        };

        write_history_prices(conn, &commodity_guid, report_guid, history)?;
    }

    Ok(())
}

/// Register a Yahoo symbol for a commodity, keeping the earliest `period1` across
/// all commodities that share the symbol.
fn register_symbol(
    symbol_map: &mut HashMap<String, String>,
    all_symbols: &mut Vec<String>,
    symbol_period1: &mut HashMap<String, Option<i64>>,
    sym: String,
    mnemonic: String,
    period1: Option<i64>,
) {
    if !symbol_map.contains_key(&sym) {
        symbol_map.insert(sym.clone(), mnemonic);
        all_symbols.push(sym.clone());
        symbol_period1.insert(sym, period1);
    } else if let Some(entry) = symbol_period1.get_mut(&sym) {
        match (*entry, period1) {
            (None, Some(p)) => *entry = Some(p),
            (Some(e), Some(p)) if p < e => *entry = Some(p),
            _ => {}
        }
    }
}

async fn fetch_prices_remote(pre: PriceFetchPlan) -> Result<(PricesResult, PendingPriceWrites), String> {
    let PriceFetchPlan {
        commodities,
        already_fetched,
        latest_dates,
        report_currency_guid,
        report_currency,
        currencies,
        currency_guid_map,
        existing_live_prices,
        overrides,
    } = pre;

    let mut assigned: HashSet<String> = HashSet::new();
    let now = utils::iso_now();

    // Crypto pairs quote in the report currency when supported, else EUR (converted via FX at read time).
    let crypto_suffix = match &report_currency {
        Some(r) if SUPPORTED_CRYPTO_FIAT.contains(&r.as_str()) => format!("-{}", r),
        _ => "-EUR".to_string(),
    };

    // period2 = tomorrow at 00:00 UTC (ensures today's data is included)
    let today = chrono::Utc::now().date_naive();
    let tomorrow = today.succ_opt().unwrap_or(today);
    let period2 = tomorrow
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "Failed to construct period2 timestamp".to_string())?
        .and_utc()
        .timestamp();

    let mut symbol_map: HashMap<String, String> = HashMap::new();
    let mut all_symbols: Vec<String> = vec![];
    // Earliest period1 per Yahoo symbol (across all commodities sharing that symbol)
    let mut symbol_period1: HashMap<String, Option<i64>> = HashMap::new();

    for c in &commodities {
        let period1_for_c: Option<i64> = latest_dates.get(&c.guid).and_then(|d| date_to_period1(d));

        // Per-commodity free-text override wins verbatim (single Yahoo fetch,
        // no heuristic suffix expansion).
        if let Some(ov) = overrides.get(&c.guid) {
            register_symbol(
                &mut symbol_map,
                &mut all_symbols,
                &mut symbol_period1,
                ov.clone(),
                c.mnemonic.clone(),
                period1_for_c,
            );
            continue;
        }
        let candidates = derive_yahoo_symbols(&c.mnemonic, &c.namespace, &crypto_suffix);
        for sym in candidates {
            register_symbol(
                &mut symbol_map,
                &mut all_symbols,
                &mut symbol_period1,
                sym,
                c.mnemonic.clone(),
                period1_for_c,
            );
        }
    }

    // FX symbols: each book currency quoted in the report currency.
    // Yahoo convention: "USDEUR=X" = price of 1 USD in EUR. Fetch both directions
    // and prefer the direct quote, falling back to the inverted pair.
    let mut fx_symbols: Vec<(String, String, bool)> = vec![]; // (symbol, currency_mnemonic, is_inverse)
    if let Some(report_mnemonic) = &report_currency {
        for (guid, mnemonic) in &currencies {
            if mnemonic == report_mnemonic {
                continue;
            }
            let primary = format!("{}{}=X", mnemonic, report_mnemonic);
            let inverse = format!("{}{}=X", report_mnemonic, mnemonic);
            let period1_for_currency: Option<i64> =
                latest_dates.get(guid).and_then(|d| date_to_period1(d));
            if !symbol_map.contains_key(&primary) && !all_symbols.contains(&primary) {
                all_symbols.push(primary.clone());
                symbol_period1.insert(primary.clone(), period1_for_currency);
                fx_symbols.push((primary, mnemonic.clone(), false));
            }
            if !symbol_map.contains_key(&inverse) && !all_symbols.contains(&inverse) {
                all_symbols.push(inverse.clone());
                symbol_period1.insert(inverse.clone(), period1_for_currency);
                fx_symbols.push((inverse, mnemonic.clone(), true));
            }
        }
    }

    // Remove duplicates
    all_symbols.sort();
    all_symbols.dedup();

    let mut yahoo_history: HashMap<String, Vec<(i64, f64)>> = HashMap::new();
    let mut yahoo_errors: HashMap<String, String> = HashMap::new();

    // Fetch prices concurrently with a concurrency limit
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_FETCHES));
    let mut handles = Vec::new();
    for sym in &all_symbols {
        let sym = sym.clone();
        let p1 = symbol_period1.get(&sym).copied().flatten();
        let p2 = period2;
        let permit = std::sync::Arc::clone(&semaphore);
        handles.push(tokio::spawn(async move {
            // A closed semaphore would mean the runtime is shutting down; map
            // it into the result tuple instead of panicking the task.
            match permit.acquire().await {
                Ok(_guard) => (sym.clone(), fetch_single_yahoo_history(&sym, p1, p2).await),
                Err(e) => (sym, Err(format!("Fetch scheduler unavailable: {e}"))),
            }
        }));
    }

    let results = futures::future::join_all(handles).await;
    for result in results {
        match result {
            Ok((sym, Ok(prices))) => {
                yahoo_history.insert(sym, prices);
            }
            Ok((sym, Err(e))) => {
                yahoo_errors.insert(sym, e);
            }
            Err(e) => {
                yahoo_errors.insert("unknown".to_string(), format!("Task failed: {}", e));
            }
        }
    }

    // FX rates in report currency per currency mnemonic.
    let mut fx_primary_done: HashSet<String> = HashSet::new();
    let mut fx_history: HashMap<String, Vec<(i64, f64)>> = HashMap::new();
    for (sym, mnemonic, is_inverse) in &fx_symbols {
        let Some(history) = yahoo_history.get(sym) else {
            continue;
        };
        if *is_inverse {
            if fx_primary_done.contains(mnemonic) {
                continue;
            }
            let inverted: Vec<(i64, f64)> = history.iter().map(|(ts, p)| (*ts, 1.0 / *p)).collect();
            fx_history.insert(mnemonic.clone(), inverted);
        } else {
            fx_history.insert(mnemonic.clone(), history.clone());
            fx_primary_done.insert(mnemonic.clone());
        }
    }

    let mut latest_prices: Vec<PriceEntry> = vec![];
    let mut errors: Vec<PriceError> = vec![];

    for (sym, mnemonic) in &symbol_map {
        if let Some(history) = yahoo_history.get(sym) {
            if let Some((_, latest_price)) = history.last() {
                let namespace = commodities
                    .iter()
                    .find(|c| c.mnemonic == *mnemonic)
                    .map(|c| c.namespace.as_str())
                    .unwrap_or("");
                let quote = quote_currency_for_symbol(
                    sym,
                    namespace,
                    report_currency.as_deref().unwrap_or(""),
                );
                latest_prices.push(PriceEntry {
                    mnemonic: mnemonic.clone(),
                    price: *latest_price,
                    price_date: now.clone(),
                    currency: quote,
                });
            }
            assigned.insert(mnemonic.clone());
        }
    }

    // Include existing DB prices for commodities that were already fetched
    let new_mnemonics: HashSet<String> = latest_prices.iter().map(|p| p.mnemonic.clone()).collect();
    for ep in existing_live_prices {
        if !new_mnemonics.contains(&ep.mnemonic) {
            latest_prices.push(ep);
        }
    }

    for c in &commodities {
        if !assigned.contains(&c.mnemonic) && !already_fetched.contains(&c.guid) {
            let candidates = match overrides.get(&c.guid) {
                Some(ov) => vec![ov.clone()],
                None => derive_yahoo_symbols(&c.mnemonic, &c.namespace, &crypto_suffix),
            };
            let first_error = candidates
                .iter()
                .find_map(|sym| yahoo_errors.get(sym))
                .cloned()
                .unwrap_or_else(|| format!("No price found (tried: {})", candidates.join(", ")));
            errors.push(PriceError {
                mnemonic: c.mnemonic.clone(),
                error: first_error,
            });
        }
    }

    // Surface FX failures so missing conversions are visible.
    if let Some(report_mnemonic) = &report_currency {
        for (_, mnemonic) in &currencies {
            if mnemonic == report_mnemonic {
                continue;
            }
            if fx_history.contains_key(mnemonic) {
                continue;
            }
            let primary = format!("{}{}=X", mnemonic, report_mnemonic);
            let inverse = format!("{}{}=X", report_mnemonic, mnemonic);
            let first_error = yahoo_errors
                .get(&primary)
                .or_else(|| yahoo_errors.get(&inverse))
                .cloned()
                .unwrap_or_else(|| format!("No FX rate found (tried: {}, {})", primary, inverse));
            errors.push(PriceError {
                mnemonic: format!("{} (FX)", mnemonic),
                error: first_error,
            });
        }
    }

    let fx_prices: Vec<(String, Vec<(i64, f64)>)> = fx_history.into_iter().collect();

    Ok((
        PricesResult {
            prices: latest_prices,
            fetched_at: now,
            errors,
        },
        PendingPriceWrites {
            symbol_map,
            yahoo_history,
            assigned,
            report_currency_guid,
            report_currency,
            currency_guid_map,
            commodities,
            fx_prices,
        },
    ))
}

pub fn get_cached_prices_map(conn: &Connection) -> Result<HashMap<String, f64>, String> {
    let rows = latest_n_prices_raw(conn, 1)?;
    if rows.is_empty() {
        return Ok(HashMap::new());
    }
    let (report_guid, report_mnemonic) = report_currency_pair(conn);
    let map: HashMap<String, f64> = rows
        .into_iter()
        .map(|(m, p, d, quote)| {
            Ok((
                m,
                normalize_price(p, &quote, &d, &report_guid, &report_mnemonic, conn)?,
            ))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    Ok(map)
}

pub fn get_cached_prices_with_date_map(
    conn: &Connection,
) -> Result<HashMap<String, (f64, Option<String>)>, String> {
    let rows = latest_n_prices_raw(conn, 1)?;
    let (report_guid, report_mnemonic) = report_currency_pair(conn);
    let map: HashMap<String, (f64, Option<String>)> = rows
        .into_iter()
        .map(|(m, p, d, quote)| {
            Ok((
                m,
                (
                    normalize_price(p, &quote, &d, &report_guid, &report_mnemonic, conn)?,
                    Some(d),
                ),
            ))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    Ok(map)
}

/// Second-latest price per commodity, normalized to the report currency at that
/// price's own date. Powers day-change comparisons in the portfolio table.
pub fn get_cached_prev_prices_map(
    conn: &Connection,
) -> Result<HashMap<String, (f64, Option<String>)>, String> {
    let rows = latest_n_prices_raw(conn, 2)?;
    let (report_guid, report_mnemonic) = report_currency_pair(conn);
    let map: HashMap<String, (f64, Option<String>)> = rows
        .into_iter()
        .map(|(m, p, d, quote)| {
            Ok((
                m,
                (
                    normalize_price(p, &quote, &d, &report_guid, &report_mnemonic, conn)?,
                    Some(d),
                ),
            ))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    Ok(map)
}

fn report_currency_pair(conn: &Connection) -> (String, String) {
    (
        fx::get_report_currency_guid(conn).unwrap_or_default(),
        fx::get_report_currency_mnemonic(conn).unwrap_or_default(),
    )
}

/// Quote currency mnemonic per commodity (from each commodity's latest price),
/// used as the cost-basis currency proxy for stock accounts. One query instead
/// of one lookup per commodity.
pub fn latest_quote_currencies(conn: &Connection) -> Result<HashMap<String, String>, String> {
    Ok(latest_n_prices_raw(conn, 1)?
        .into_iter()
        .map(|(mnemonic, _, _, quote)| (mnemonic, quote))
        .collect())
}

#[tauri::command]
pub async fn list_symbol_overrides(
    state: State<'_, AppState>,
) -> Result<Vec<SymbolOverride>, String> {
    db::run_blocking(state.db.clone(), list_symbol_overrides_inner).await
}

#[tauri::command]
pub async fn set_symbol_override(
    state: State<'_, AppState>,
    commodity_guid: String,
    symbol: String,
) -> Result<SymbolOverride, String> {
    let normalized = normalize_symbol(&symbol)?;
    db::run_blocking(state.db.clone(), move |conn| {
        // Guard against typos: the commodity must exist and be priceable
        // (non-CURRENCY, non-template).
        let (mnemonic, namespace, full_name): (String, String, Option<String>) = conn
            .query_row(
                "SELECT mnemonic, namespace, fullname FROM commodities WHERE guid = ?1",
                [&commodity_guid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| format!("Unknown commodity: {commodity_guid}"))?;
        if namespace == "CURRENCY" || namespace == "template" {
            return Err("Symbol overrides apply to securities only".to_string());
        }
        conn.execute(
            "INSERT INTO symbol_overrides (commodity_guid, symbol) VALUES (?1, ?2)
             ON CONFLICT(commodity_guid) DO UPDATE SET symbol = excluded.symbol",
            rusqlite::params![commodity_guid, normalized],
        )
        .map_err(|e| e.to_string())?;
        Ok(SymbolOverride {
            commodity_guid,
            mnemonic,
            namespace,
            full_name,
            symbol: Some(normalized),
        })
    })
    .await
}

#[tauri::command]
pub async fn delete_symbol_override(
    state: State<'_, AppState>,
    commodity_guid: String,
) -> Result<(), String> {
    db::run_blocking(state.db.clone(), move |conn| {
        conn.execute(
            "DELETE FROM symbol_overrides WHERE commodity_guid = ?1",
            [&commodity_guid],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn refresh_prices(state: State<'_, AppState>) -> Result<PricesResult, String> {
    let pre = db::run_blocking(state.db.clone(), read_price_data).await?;

    let (result, write_data) = fetch_prices_remote(pre).await?;

    db::run_blocking(state.db.clone(), move |conn| {
        // Single transaction so prices + refresh_log commit atomically; a
        // mid-write failure must not leave partial prices behind.
        conn.execute_batch("BEGIN").map_err(|e| e.to_string())?;
        let write_result = (|| -> Result<(), String> {
            write_price_data(conn, &write_data)?;
            conn.execute(
                "INSERT INTO refresh_log (refreshed_at) VALUES (?1)",
                rusqlite::params![utils::iso_now()],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })();
        match write_result {
            Ok(()) => conn.execute_batch("COMMIT").map_err(|e| e.to_string()),
            Err(e) => {
                conn.execute_batch("ROLLBACK").ok();
                Err(e)
            }
        }
    })
    .await?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn setup_commodities() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES
             ('c-aapl', 'AAPL', 'NASDAQ', 'Apple'),
             ('c-eur', 'EUR', 'CURRENCY', NULL)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn normalize_symbol_uppercases_and_trims() {
        assert_eq!(normalize_symbol("  aapl ").unwrap(), "AAPL");
        assert_eq!(normalize_symbol("btc-eur").unwrap(), "BTC-EUR");
        assert_eq!(normalize_symbol("^GSPC").unwrap(), "^GSPC");
        assert!(normalize_symbol("").is_err());
        assert!(normalize_symbol("   ").is_err());
        assert!(normalize_symbol("AA PL").is_err());
        assert!(normalize_symbol("A/B").is_err());
        assert!(normalize_symbol(&"A".repeat(33)).is_err());
    }

    #[test]
    fn override_map_empty_without_rows() {
        let conn = setup_commodities();
        assert!(symbol_override_map(&conn).is_empty());
        let list = list_symbol_overrides_inner(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].commodity_guid, "c-aapl");
        assert_eq!(list[0].symbol, None);
    }

    #[test]
    fn override_upsert_round_trip() {
        let conn = setup_commodities();
        conn.execute(
            "INSERT INTO symbol_overrides (commodity_guid, symbol) VALUES ('c-aapl', 'AAPL')",
            [],
        )
        .unwrap();
        assert_eq!(
            symbol_override_map(&conn).get("c-aapl").cloned(),
            Some("AAPL".to_string())
        );
        conn.execute(
            "INSERT INTO symbol_overrides (commodity_guid, symbol) VALUES ('c-aapl', 'AAPL.DE')
             ON CONFLICT(commodity_guid) DO UPDATE SET symbol = excluded.symbol",
            [],
        )
        .unwrap();
        assert_eq!(
            symbol_override_map(&conn).get("c-aapl").cloned(),
            Some("AAPL.DE".to_string())
        );
    }
}
