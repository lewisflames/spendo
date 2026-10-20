use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use tauri::State;

use crate::db::{self, AppError, AppState};
use crate::fx;
use crate::utils;
use crate::yahoo::{
    derive_yahoo_symbols, fetch_single_yahoo_history, quote_currency_for_symbol, YahooHistory,
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

pub fn list_commodities(conn: &Connection) -> Result<Vec<CommodityInfo>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT guid, mnemonic, namespace, fullname
             FROM commodities
             WHERE namespace != 'CURRENCY'
             AND namespace != 'template'
             ORDER BY namespace, mnemonic",
    )?;

    let rows = stmt
        .query_map([], |row| {
            Ok(CommodityInfo {
                guid: row.get(0)?,
                mnemonic: row.get(1)?,
                namespace: row.get(2)?,
                full_name: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// One stored price row: `price` is in the `quote` currency as of `date`.
struct RawPrice {
    commodity_guid: String,
    mnemonic: String,
    price: f64,
    date: String,
    /// Quote currency mnemonic.
    quote: String,
}

/// Latest `rn`-th price row per commodity. Keyed by commodity GUID (a
/// mnemonic can repeat across namespaces, e.g. the same ticker on two
/// exchanges) with the mnemonic kept for display only.
fn nth_price_raw(conn: &Connection, rn: u32) -> Result<Vec<RawPrice>, AppError> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT guid, mnemonic, price, date, quote FROM (
                    SELECT c.guid, c.mnemonic,
                        {} as price,
                        p.date,
                        cur.mnemonic as quote,
                        ROW_NUMBER() OVER (PARTITION BY c.guid ORDER BY p.date DESC, p.guid DESC) as rn
                    FROM prices p
                    JOIN commodities c ON c.guid = p.commodity_guid
                    JOIN commodities cur ON cur.guid = p.currency_guid
                    WHERE c.namespace != 'CURRENCY'
                 ) WHERE rn = ?1",
            utils::PRICE_AMOUNT
        ))?;

    let rows = stmt
        .query_map([rn], |row| {
            Ok(RawPrice {
                commodity_guid: row.get(0)?,
                mnemonic: row.get(1)?,
                price: row.get(2)?,
                date: row.get(3)?,
                quote: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

fn get_latest_live_prices(conn: &Connection) -> Result<Vec<PriceEntry>, AppError> {
    Ok(nth_price_raw(conn, 1)?
        .into_iter()
        .map(|p| PriceEntry {
            mnemonic: p.mnemonic,
            price: p.price,
            price_date: p.date,
            currency: p.quote,
        })
        .collect())
}

pub fn get_last_refresh_timestamp(conn: &Connection) -> Result<Option<String>, AppError> {
    let result: Option<String> =
        conn.query_row("SELECT MAX(refreshed_at) FROM refresh_log", [], |row| {
            row.get(0)
        })?;
    Ok(result)
}

pub fn get_price_history(
    conn: &Connection,
    commodity_guid: &str,
) -> Result<Vec<PriceHistoryEntry>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT p.date,
                    {} as price,
                    cur.mnemonic as currency
             FROM prices p
             JOIN commodities cur ON cur.guid = p.currency_guid
             WHERE p.commodity_guid = ?1
             ORDER BY p.date ASC",
        utils::PRICE_AMOUNT
    ))?;

    let rows = stmt
        .query_map([commodity_guid], |row| {
            Ok(PriceHistoryEntry {
                date: row.get(0)?,
                price: row.get(1)?,
                currency: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

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
fn normalize_symbol(input: &str) -> Result<String, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(
            "Enter a Yahoo symbol, like AAPL".to_string(),
        ));
    }
    if trimmed.len() > 32 {
        return Err(AppError::Validation(
            "Keep the symbol under 32 characters".to_string(),
        ));
    }
    if trimmed
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '/' | '?' | '#'))
    {
        return Err(AppError::Validation(
            "Symbols can't have spaces, '/', '?' or '#'".to_string(),
        ));
    }
    Ok(trimmed.to_ascii_uppercase())
}

/// commodity_guid -> override symbol. Errors propagate: the table is created
/// by `init_db`, so a failure here means a real DB problem, not a legacy book.
fn symbol_override_map(conn: &Connection) -> Result<HashMap<String, String>, AppError> {
    let mut stmt = conn.prepare("SELECT commodity_guid, symbol FROM symbol_overrides")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
}

fn fetch_symbol_overrides_inner(conn: &Connection) -> Result<Vec<SymbolOverride>, AppError> {
    let commodities = list_commodities(conn)?;
    let overrides = symbol_override_map(conn)?;
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
    pub(crate) commodities: Vec<CommodityInfo>,
    already_fetched: HashSet<String>,
    latest_dates: HashMap<String, String>,
    report_currency_guid: Option<String>,
    report_currency: Option<String>,
    currencies: Vec<(String, String)>,
    currency_guid_map: HashMap<String, String>,
    existing_live_prices: Vec<PriceEntry>,
    overrides: HashMap<String, String>,
}

/// One commodity's winning history, ready to write. Quote currency is already
/// resolved to a book currency GUID.
struct PendingPriceWrite {
    commodity_guid: String,
    currency_guid: String,
    prices: Vec<(i64, f64)>,
}

pub(crate) struct PendingPriceWrites {
    /// Exactly one entry per commodity that fetched successfully, keyed by
    /// GUID so same-ticker commodities can never overwrite each other.
    writes: Vec<PendingPriceWrite>,
    report_currency_guid: Option<String>,
    fx_prices: Vec<(String, Vec<(i64, f64)>)>,
}

pub(crate) fn read_price_data(conn: &Connection) -> Result<PriceFetchPlan, AppError> {
    let commodities = list_commodities(conn)?;

    let already_fetched: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT DISTINCT commodity_guid FROM prices")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        rows
    };

    // Latest stored date per commodity drives incremental fetching (period1).
    // Failures here must propagate: silently degrading to empty dates turns an
    // incremental refresh into a full 10-year refetch for every symbol.
    let latest_dates: HashMap<String, String> = {
        let mut stmt =
            conn.prepare("SELECT commodity_guid, MAX(date) FROM prices GROUP BY commodity_guid")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<HashMap<_, _>, _>>()?;
        rows
    };

    let report_currency_guid = fx::get_report_currency_guid(conn)?;
    let report_currency = fx::get_report_currency_mnemonic(conn)?;
    let currencies = fx::list_book_currencies(conn)?;
    let currency_guid_map = fx::currency_guid_map(conn)?;

    let existing_live_prices = get_latest_live_prices(conn)?;
    let overrides = symbol_override_map(conn)?;

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
    // Byte-safe prefix: `d` is a stored date string, but slicing must never
    // panic if a hand-edited book holds multibyte junk.
    let day = d.get(..10).unwrap_or(d);
    chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .and_then(|nd| nd.and_hms_opt(0, 0, 0))
        .map(|ndt| ndt.and_utc().timestamp())
}

/// Insert one price per day for a commodity, replacing any existing same-day rows.
/// Single range DELETE (instead of N per-day deletes); the per-day guid
/// (`commodity-date`) keeps the subsequent INSERTs idempotent. Returns whether
/// at least one valid row was written.
fn write_history_prices(
    conn: &Connection,
    commodity_guid: &str,
    currency_guid: &str,
    history: &[(i64, f64)],
) -> Result<bool, AppError> {
    if history.is_empty() {
        return Ok(false);
    }
    // Drop points whose timestamp can't be converted instead of stamping them
    // with today's date — a fabricated date is worse than a missing quote.
    let dated: Vec<(String, f64)> = history
        .iter()
        .filter_map(|(ts, price)| {
            chrono::DateTime::from_timestamp(*ts, 0)
                .map(|dt| (dt.format("%Y-%m-%d").to_string(), *price))
        })
        .collect();
    if dated.is_empty() {
        return Ok(false);
    }
    let (min_date, max_date) = dated.iter().fold(
        (dated[0].0.clone(), dated[0].0.clone()),
        |(mn, mx), (d, _)| (mn.min(d.clone()), mx.max(d.clone())),
    );
    conn.execute(
        "DELETE FROM prices WHERE commodity_guid = ?1 AND substr(date, 1, 10) BETWEEN ?2 AND ?3",
        rusqlite::params![commodity_guid, min_date, max_date],
    )?;

    let insert_sql = format!(
        "INSERT OR REPLACE INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
         VALUES (?1, ?2, ?3, ?4, {PRICE_SCALE}, ?5)"
    );
    let mut insert = conn.prepare(&insert_sql)?;

    let mut wrote = false;
    for (date_str, price) in &dated {
        // Yahoo occasionally returns 0/NaN closes; casting those to the scaled
        // integer would store `i64::MAX` (inf) or a bogus 0 and poison every
        // later conversion.
        if !price.is_finite() || *price <= 0.0 {
            continue;
        }
        let price_guid = format!("{}-{}", commodity_guid, date_str);
        let value_num = (*price * PRICE_SCALE as f64).round() as i64;
        insert.execute(rusqlite::params![
            price_guid,
            commodity_guid,
            currency_guid,
            value_num,
            date_str
        ])?;
        wrote = true;
    }
    Ok(wrote)
}

pub(crate) fn write_price_data(
    conn: &Connection,
    data: &PendingPriceWrites,
) -> Result<bool, AppError> {
    let PendingPriceWrites {
        writes,
        report_currency_guid,
        fx_prices,
    } = data;

    let mut wrote = false;
    for w in writes {
        wrote |= write_history_prices(conn, &w.commodity_guid, &w.currency_guid, &w.prices)?;
    }

    // FX rates: each book currency quoted in the report currency.
    let Some(report_guid) = report_currency_guid.as_deref() else {
        return Ok(wrote);
    };
    for (mnemonic, history) in fx_prices {
        let Some(commodity_guid) = conn
            .query_row(
                "SELECT guid FROM commodities WHERE mnemonic = ?1 AND namespace = 'CURRENCY'",
                [mnemonic],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        else {
            continue;
        };

        wrote |= write_history_prices(conn, &commodity_guid, report_guid, history)?;
    }

    Ok(wrote)
}

/// Register one candidate Yahoo symbol for a commodity, keeping the earliest
/// `period1` across every commodity that shares the symbol. Candidate order is
/// preserved per commodity so `assemble_result` can pick a deterministic winner.
fn register_symbol(
    candidates_by_commodity: &mut HashMap<String, Vec<String>>,
    all_symbols: &mut Vec<String>,
    symbol_period1: &mut HashMap<String, Option<i64>>,
    commodity_guid: &str,
    sym: String,
    period1: Option<i64>,
) {
    all_symbols.push(sym.clone());
    match symbol_period1.get_mut(&sym) {
        Some(entry) => match (*entry, period1) {
            (None, Some(p)) => *entry = Some(p),
            (Some(e), Some(p)) if p < e => *entry = Some(p),
            _ => {}
        },
        None => {
            symbol_period1.insert(sym.clone(), period1);
        }
    }
    candidates_by_commodity
        .entry(commodity_guid.to_string())
        .or_default()
        .push(sym);
}

pub(crate) async fn fetch_prices_remote(
    pre: PriceFetchPlan,
) -> Result<(PricesResult, PendingPriceWrites), AppError> {
    let now = utils::iso_now();
    let symbols = plan_symbols(&pre)?;
    let outcome = fetch_all_histories(&symbols).await;
    Ok(assemble_result(pre, symbols, outcome, now))
}

/// Everything the fetch phase needs: Yahoo symbol candidates per commodity
/// (in preference order), plus FX pairs.
struct SymbolPlan {
    /// Commodity GUID -> candidate Yahoo symbols in preference order. Keeping
    /// this per commodity (instead of a symbol -> mnemonic map) is what makes
    /// same-ticker commodities independent and the winning candidate
    /// deterministic.
    candidates_by_commodity: HashMap<String, Vec<String>>,
    all_symbols: Vec<String>,
    /// Earliest period1 per Yahoo symbol (across commodities sharing it).
    symbol_period1: HashMap<String, Option<i64>>,
    /// (symbol, currency_mnemonic, is_inverse) FX pairs.
    fx_symbols: Vec<(String, String, bool)>,
    period2: i64,
}

fn plan_symbols(pre: &PriceFetchPlan) -> Result<SymbolPlan, AppError> {
    // Crypto pairs quote in the report currency when supported, else EUR (converted via FX at read time).
    let crypto_suffix = match &pre.report_currency {
        Some(r) if SUPPORTED_CRYPTO_FIAT.contains(&r.as_str()) => format!("-{}", r),
        _ => "-EUR".to_string(),
    };

    // period2 = tomorrow at 00:00 UTC (ensures today's data is included)
    let today = chrono::Local::now().date_naive();
    let tomorrow = today.succ_opt().unwrap_or(today);
    let period2 = tomorrow
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "Failed to construct period2 timestamp".to_string())?
        .and_utc()
        .timestamp();

    let mut candidates_by_commodity: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_symbols: Vec<String> = vec![];
    // Earliest period1 per Yahoo symbol (across all commodities sharing that symbol)
    let mut symbol_period1: HashMap<String, Option<i64>> = HashMap::new();

    for c in &pre.commodities {
        let period1_for_c: Option<i64> = pre
            .latest_dates
            .get(&c.guid)
            .and_then(|d| date_to_period1(d));

        // Per-commodity free-text override wins verbatim (single Yahoo fetch,
        // no heuristic suffix expansion).
        let candidates = match pre.overrides.get(&c.guid) {
            Some(ov) => vec![ov.clone()],
            None => derive_yahoo_symbols(&c.mnemonic, &c.namespace, &crypto_suffix),
        };
        for sym in candidates {
            register_symbol(
                &mut candidates_by_commodity,
                &mut all_symbols,
                &mut symbol_period1,
                &c.guid,
                sym,
                period1_for_c,
            );
        }
    }

    // FX symbols: each book currency quoted in the report currency.
    // Yahoo convention: "USDEUR=X" = price of 1 USD in EUR. Fetch both directions
    // and prefer the direct quote, falling back to the inverted pair.
    let mut fx_symbols: Vec<(String, String, bool)> = vec![]; // (symbol, currency_mnemonic, is_inverse)
    if let Some(report_mnemonic) = &pre.report_currency {
        for (guid, mnemonic) in &pre.currencies {
            if mnemonic == report_mnemonic {
                continue;
            }
            let primary = format!("{}{}=X", mnemonic, report_mnemonic);
            let inverse = format!("{}{}=X", report_mnemonic, mnemonic);
            let period1_for_currency: Option<i64> =
                pre.latest_dates.get(guid).and_then(|d| date_to_period1(d));
            if !all_symbols.contains(&primary) {
                all_symbols.push(primary.clone());
                symbol_period1.insert(primary.clone(), period1_for_currency);
                fx_symbols.push((primary, mnemonic.clone(), false));
            }
            if !all_symbols.contains(&inverse) {
                all_symbols.push(inverse.clone());
                symbol_period1.insert(inverse.clone(), period1_for_currency);
                fx_symbols.push((inverse, mnemonic.clone(), true));
            }
        }
    }

    // Remove duplicates
    all_symbols.sort();
    all_symbols.dedup();

    Ok(SymbolPlan {
        candidates_by_commodity,
        all_symbols,
        symbol_period1,
        fx_symbols,
        period2,
    })
}

/// Fetched Yahoo histories and per-symbol failures.
struct FetchOutcome {
    yahoo_history: HashMap<String, YahooHistory>,
    yahoo_errors: HashMap<String, String>,
}

/// Fetch all planned symbols concurrently with a concurrency limit.
async fn fetch_all_histories(symbols: &SymbolPlan) -> FetchOutcome {
    let mut yahoo_history: HashMap<String, YahooHistory> = HashMap::new();
    let mut yahoo_errors: HashMap<String, String> = HashMap::new();

    // Fetch prices concurrently with a concurrency limit
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_FETCHES));
    let mut handles = Vec::new();
    for sym in &symbols.all_symbols {
        let sym = sym.clone();
        let p1 = symbols.symbol_period1.get(&sym).copied().flatten();
        let p2 = symbols.period2;
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

    FetchOutcome {
        yahoo_history,
        yahoo_errors,
    }
}

/// Fold fetched histories into the user-facing result and the DB write set:
/// direct-quote FX wins over inverted pairs; commodities with neither fresh
/// nor previously-fetched prices surface errors (`already_fetched` only
/// suppresses the error — the stale price is kept).
fn assemble_result(
    pre: PriceFetchPlan,
    symbols: SymbolPlan,
    outcome: FetchOutcome,
    now: String,
) -> (PricesResult, PendingPriceWrites) {
    let PriceFetchPlan {
        commodities,
        already_fetched,
        report_currency_guid,
        report_currency,
        currencies,
        currency_guid_map,
        existing_live_prices,
        ..
    } = pre;
    let SymbolPlan {
        candidates_by_commodity,
        fx_symbols,
        ..
    } = symbols;
    let FetchOutcome {
        yahoo_history,
        yahoo_errors,
    } = outcome;

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
            // Guard the division even though the fetch path filters: a zero
            // rate would become `inf` and poison every conversion.
            let inverted: Vec<(i64, f64)> = history
                .prices
                .iter()
                .filter(|entry| entry.1.is_finite() && entry.1 > 0.0)
                .map(|(ts, p)| (*ts, 1.0 / *p))
                .collect();
            fx_history.insert(mnemonic.clone(), inverted);
        } else {
            fx_history.insert(mnemonic.clone(), history.prices.clone());
            fx_primary_done.insert(mnemonic.clone());
        }
    }

    let mut assigned_guids: HashSet<String> = HashSet::new();
    let mut latest_prices: Vec<PriceEntry> = vec![];
    let mut writes: Vec<PendingPriceWrite> = vec![];
    let mut errors: Vec<PriceError> = vec![];

    for c in &commodities {
        let candidates = candidates_by_commodity
            .get(&c.guid)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        // Exactly one candidate wins: the first (in declaration order) that
        // returned a history. Writing every successful candidate would
        // interleave two exchanges' series into one commodity.
        let winner = candidates
            .iter()
            .find_map(|sym| yahoo_history.get(sym).map(|h| (sym.as_str(), h)));
        let Some((sym, history)) = winner else {
            if !already_fetched.contains(&c.guid) {
                let first_error = candidates
                    .iter()
                    .find_map(|sym| yahoo_errors.get(sym))
                    .cloned()
                    .unwrap_or_else(|| {
                        format!("No price found (tried: {})", candidates.join(", "))
                    });
                errors.push(PriceError {
                    mnemonic: c.mnemonic.clone(),
                    error: first_error,
                });
            }
            continue;
        };
        assigned_guids.insert(c.guid.clone());

        if let Some((_, latest_price)) = history.prices.last() {
            // Yahoo's meta currency beats the symbol-shape heuristic.
            let quote = history.currency.clone().unwrap_or_else(|| {
                quote_currency_for_symbol(
                    sym,
                    &c.namespace,
                    report_currency.as_deref().unwrap_or(""),
                )
            });
            latest_prices.push(PriceEntry {
                mnemonic: c.mnemonic.clone(),
                price: *latest_price,
                price_date: now.clone(),
                currency: quote,
            });
        }

        if history.prices.is_empty() {
            continue;
        }
        // Store prices in their actual quote currency so normalization at read
        // time can convert to the report currency. Yahoo's `meta.currency` is
        // authoritative; the symbol-shape heuristic is only a fallback. Fall
        // back to the report currency GUID when the quote isn't in the book.
        let quote = history
            .currency
            .clone()
            .filter(|q| currency_guid_map.contains_key(q))
            .unwrap_or_else(|| {
                quote_currency_for_symbol(
                    sym,
                    &c.namespace,
                    report_currency.as_deref().unwrap_or(""),
                )
            });
        let currency_guid = currency_guid_map
            .get(&quote)
            .cloned()
            .or_else(|| report_currency_guid.clone());
        if let Some(cg) = currency_guid {
            writes.push(PendingPriceWrite {
                commodity_guid: c.guid.clone(),
                currency_guid: cg,
                prices: history.prices.clone(),
            });
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
        if !assigned_guids.contains(&c.guid) && !already_fetched.contains(&c.guid) {
            let candidates = candidates_by_commodity
                .get(&c.guid)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
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

    (
        PricesResult {
            prices: latest_prices,
            fetched_at: now,
            errors,
        },
        PendingPriceWrites {
            writes,
            report_currency_guid,
            fx_prices,
        },
    )
}

/// Latest price per commodity GUID, normalized to the report currency.
pub fn get_cached_prices_map(conn: &Connection) -> Result<HashMap<String, f64>, AppError> {
    Ok(latest_prices_normalized(conn, 1)?
        .into_iter()
        .map(|p| (p.commodity_guid, p.price))
        .collect())
}

/// Price map keyed by commodity GUID: (price, price date).
type PriceDateMap = HashMap<String, (f64, String)>;

/// Latest and second-latest price maps in one pass: one `nth_price_raw` scan
/// per rank and a single `FxHistory::load` shared by both normalizations.
pub fn get_cached_price_maps(conn: &Connection) -> Result<(PriceDateMap, PriceDateMap), AppError> {
    let latest = nth_price_raw(conn, 1)?;
    let prev = nth_price_raw(conn, 2)?;
    if latest.is_empty() && prev.is_empty() {
        return Ok((HashMap::new(), HashMap::new()));
    }
    let report_mnemonic = fx::report_currency_or_empty(conn)?;
    let fx_history = fx::FxHistory::load(conn)?;
    // One rate map per distinct price date instead of one query per commodity.
    let mut rates_by_date: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let normalize = |rows: Vec<RawPrice>,
                     rates_by_date: &mut HashMap<String, HashMap<String, f64>>|
     -> PriceDateMap {
        rows.into_iter()
            .map(|p| {
                let normalized = if p.quote == report_mnemonic {
                    p.price
                } else {
                    let rates = rates_by_date
                        .entry(p.date.clone())
                        .or_insert_with(|| fx_history.map_for_date(&p.date));
                    p.price * rates.get(&p.quote).copied().unwrap_or(1.0)
                };
                (p.commodity_guid, (normalized, p.date))
            })
            .collect()
    };
    let latest_map = normalize(latest, &mut rates_by_date);
    let prev_map = normalize(prev, &mut rates_by_date);
    Ok((latest_map, prev_map))
}

/// Latest-price rows with `price` normalized to the report currency. Single
/// window-function scan plus one FX-history scan shared by every cached-price
/// map above.
fn latest_prices_normalized(conn: &Connection, rn: u32) -> Result<Vec<RawPrice>, AppError> {
    let rows = nth_price_raw(conn, rn)?;
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let report_mnemonic = fx::report_currency_or_empty(conn)?;
    let fx_history = fx::FxHistory::load(conn)?;
    // One rate map per distinct price date instead of one query per commodity.
    let mut rates_by_date: HashMap<String, HashMap<String, f64>> = HashMap::new();
    rows.into_iter()
        .map(|p| {
            let price = if p.quote == report_mnemonic {
                p.price
            } else {
                let rates = rates_by_date
                    .entry(p.date.clone())
                    .or_insert_with(|| fx_history.map_for_date(&p.date));
                p.price * rates.get(&p.quote).copied().unwrap_or(1.0)
            };
            Ok(RawPrice { price, ..p })
        })
        .collect()
}

/// Quote currency mnemonic per commodity GUID (from each commodity's latest
/// price), used as the cost-basis currency proxy for stock accounts. One query
/// instead of one lookup per commodity.
pub fn latest_quote_currencies(conn: &Connection) -> Result<HashMap<String, String>, AppError> {
    Ok(nth_price_raw(conn, 1)?
        .into_iter()
        .map(|p| (p.commodity_guid, p.quote))
        .collect())
}

#[tauri::command]
pub async fn fetch_symbol_overrides(
    state: State<'_, AppState>,
) -> Result<Vec<SymbolOverride>, String> {
    db::run_blocking(state.db.clone(), |conn| {
        fetch_symbol_overrides_inner(conn).map_err(String::from)
    })
    .await
}

/// Guard against typos: the commodity must exist and be priceable
/// (non-CURRENCY, non-template).
fn set_symbol_override_inner(
    conn: &Connection,
    commodity_guid: String,
    normalized: String,
) -> Result<SymbolOverride, AppError> {
    let (mnemonic, namespace, full_name): (String, String, Option<String>) = conn
        .query_row(
            "SELECT mnemonic, namespace, fullname FROM commodities WHERE guid = ?1",
            [&commodity_guid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound(format!("Unknown commodity: {commodity_guid}")))?;
    if namespace == "CURRENCY" || namespace == "template" {
        return Err(AppError::Validation(
            "Symbol overrides apply to securities only".to_string(),
        ));
    }
    conn.execute(
        "INSERT INTO symbol_overrides (commodity_guid, symbol) VALUES (?1, ?2)
         ON CONFLICT(commodity_guid) DO UPDATE SET symbol = excluded.symbol",
        rusqlite::params![commodity_guid, normalized],
    )?;
    Ok(SymbolOverride {
        commodity_guid,
        mnemonic,
        namespace,
        full_name,
        symbol: Some(normalized),
    })
}

#[tauri::command]
pub async fn set_symbol_override(
    state: State<'_, AppState>,
    commodity_guid: String,
    symbol: String,
) -> Result<SymbolOverride, String> {
    let normalized = normalize_symbol(&symbol)?;
    db::run_blocking(state.db.clone(), move |conn| {
        set_symbol_override_inner(conn, commodity_guid, normalized).map_err(String::from)
    })
    .await
}

fn delete_symbol_override_inner(conn: &Connection, commodity_guid: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM symbol_overrides WHERE commodity_guid = ?1",
        [commodity_guid],
    )?;
    Ok(())
}

#[tauri::command]
pub async fn delete_symbol_override(
    state: State<'_, AppState>,
    commodity_guid: String,
) -> Result<(), String> {
    db::run_blocking(state.db.clone(), move |conn| {
        delete_symbol_override_inner(conn, &commodity_guid).map_err(String::from)
    })
    .await
}

/// Empty result used when there is nothing priceable to fetch (e.g. a fresh
/// import without securities). Not an error — callers treat it as a no-op.
pub(crate) fn empty_prices_result() -> PricesResult {
    PricesResult {
        prices: vec![],
        fetched_at: utils::iso_now(),
        errors: vec![],
    }
}

/// Single transaction so prices + refresh_log commit atomically; a mid-write
/// failure must not leave partial prices behind. The log row is only stamped
/// when at least one price actually landed — a refresh that fetched nothing
/// (Yahoo down, no matches) must not look fresh to the 24h auto-refresh gate.
pub(crate) fn write_prices_and_log(
    conn: &Connection,
    write_data: &PendingPriceWrites,
) -> Result<(), AppError> {
    db::with_tx_err(conn, || {
        let wrote = write_price_data(conn, write_data)?;
        if wrote {
            conn.execute(
                "INSERT INTO refresh_log (refreshed_at) VALUES (?1)",
                rusqlite::params![utils::iso_now()],
            )?;
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn refresh_prices(state: State<'_, AppState>) -> Result<PricesResult, String> {
    let generation = state.book_generation.load(Ordering::SeqCst);
    let pre = db::run_blocking(state.db.clone(), |conn| {
        read_price_data(conn).map_err(String::from)
    })
    .await?;

    let (result, write_data) = fetch_prices_remote(pre).await?;

    // Discard the write if the active book changed while the fetch was in
    // flight; the check runs while the DB lock is held, so it cannot race the
    // swap (which bumps the generation under the same lock).
    let gen_check = state.book_generation.clone();
    db::run_blocking(state.db.clone(), move |conn| {
        if gen_check.load(Ordering::SeqCst) != generation {
            return Err(
                "Book changed while prices were refreshing. Those results were discarded"
                    .to_string(),
            );
        }
        write_prices_and_log(conn, &write_data).map_err(String::from)
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

    /// Regression: zero/non-finite closes must never reach the integer price
    /// columns (`inf` saturates to `i64::MAX`, `NaN` becomes 0).
    #[test]
    fn write_history_prices_skips_non_positive_and_non_finite() {
        let conn = setup_commodities();
        let ts = 1_700_000_000i64;
        let history = vec![
            (ts, 0.0),
            (ts + 86_400, f64::NAN),
            (ts + 172_800, f64::INFINITY),
            (ts + 259_200, 12.5),
        ];
        write_history_prices(&conn, "c-aapl", "c-eur", &history).unwrap();

        let rows: Vec<(String, i64)> = {
            let mut stmt = conn
                .prepare("SELECT date, value_num FROM prices ORDER BY date")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, (12.5 * PRICE_SCALE as f64).round() as i64);
    }

    /// Regression: cached prices were keyed by mnemonic, so two commodities
    /// sharing a ticker (same symbol on different exchanges) collapsed into one
    /// price; the window function also emitted only one row per mnemonic.
    #[test]
    fn cached_prices_keep_same_mnemonic_commodities_separate() {
        let conn = setup_commodities();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES
             ('c-bhp-us', 'BHP', 'NYSE', 'BHP NYSE'),
             ('c-bhp-au', 'BHP', 'ASX', 'BHP ASX')",
            [],
        )
        .unwrap();
        for (guid, value_num) in [("c-bhp-us", 5_000i64), ("c-bhp-au", 7_000)] {
            conn.execute(
                "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
                 VALUES (?1, ?2, 'c-eur', ?3, 100, '2024-01-05')",
                rusqlite::params![format!("p-{guid}"), guid, value_num],
            )
            .unwrap();
        }

        let map = get_cached_prices_map(&conn).unwrap();
        assert!((map.get("c-bhp-us").copied().unwrap() - 50.0).abs() < 1e-9);
        assert!((map.get("c-bhp-au").copied().unwrap() - 70.0).abs() < 1e-9);
    }

    #[test]
    fn override_map_empty_without_rows() {
        let conn = setup_commodities();
        assert!(symbol_override_map(&conn).unwrap().is_empty());
        let list = fetch_symbol_overrides_inner(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].commodity_guid, "c-aapl");
        assert_eq!(list[0].symbol, None);
    }

    #[test]
    fn override_set_overwrites_and_delete_removes() {
        let conn = setup_commodities();
        set_symbol_override_inner(&conn, "c-aapl".to_string(), "AAPL".to_string()).unwrap();
        assert_eq!(
            symbol_override_map(&conn).unwrap().get("c-aapl").cloned(),
            Some("AAPL".to_string())
        );

        set_symbol_override_inner(&conn, "c-aapl".to_string(), "AAPL.DE".to_string()).unwrap();
        assert_eq!(
            symbol_override_map(&conn).unwrap().get("c-aapl").cloned(),
            Some("AAPL.DE".to_string())
        );

        delete_symbol_override_inner(&conn, "c-aapl").unwrap();
        assert!(symbol_override_map(&conn).unwrap().is_empty());
    }

    #[test]
    fn override_rejects_currencies_and_unknown_commodities() {
        let conn = setup_commodities();
        assert!(
            set_symbol_override_inner(&conn, "c-eur".to_string(), "EUR=X".to_string()).is_err()
        );
        assert!(
            set_symbol_override_inner(&conn, "missing".to_string(), "AAPL".to_string()).is_err()
        );
    }

    /// Regression: the write path used to resolve the commodity by mnemonic and
    /// write every successful candidate, so same-ticker commodities overwrote
    /// one another and multi-candidate fetches interleaved series.
    #[test]
    fn write_price_data_keeps_same_mnemonic_commodities_separate() {
        let conn = setup_commodities();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES
             ('c-bhp-us', 'BHP', 'NYSE', 'BHP NYSE'),
             ('c-bhp-au', 'BHP', 'ASX', 'BHP ASX')",
            [],
        )
        .unwrap();
        let data = PendingPriceWrites {
            writes: vec![
                PendingPriceWrite {
                    commodity_guid: "c-bhp-us".to_string(),
                    currency_guid: "c-eur".to_string(),
                    prices: vec![(1_700_000_000, 5.0)],
                },
                PendingPriceWrite {
                    commodity_guid: "c-bhp-au".to_string(),
                    currency_guid: "c-eur".to_string(),
                    prices: vec![(1_700_000_000, 7.0)],
                },
            ],
            report_currency_guid: None,
            fx_prices: vec![],
        };
        assert!(write_price_data(&conn, &data).unwrap());

        let map = get_cached_prices_map(&conn).unwrap();
        assert!((map.get("c-bhp-us").copied().unwrap() - 5.0).abs() < 1e-9);
        assert!((map.get("c-bhp-au").copied().unwrap() - 7.0).abs() < 1e-9);
    }

    /// Regression: every successful candidate symbol used to be written to the
    /// commodity; exactly one (the first in declaration order) must win.
    #[test]
    fn assemble_result_writes_only_the_first_successful_candidate() {
        let pre = PriceFetchPlan {
            commodities: vec![CommodityInfo {
                guid: "c-foo".to_string(),
                mnemonic: "FOO".to_string(),
                namespace: "EURONEXT".to_string(),
                full_name: None,
            }],
            already_fetched: HashSet::new(),
            latest_dates: HashMap::new(),
            report_currency_guid: Some("c-eur".to_string()),
            report_currency: Some("EUR".to_string()),
            currencies: vec![],
            currency_guid_map: HashMap::from([("EUR".to_string(), "c-eur".to_string())]),
            existing_live_prices: vec![],
            overrides: HashMap::new(),
        };
        let symbols = SymbolPlan {
            candidates_by_commodity: HashMap::from([(
                "c-foo".to_string(),
                vec!["FOO.AS".to_string(), "FOO.PA".to_string()],
            )]),
            all_symbols: vec![],
            symbol_period1: HashMap::new(),
            fx_symbols: vec![],
            period2: 0,
        };
        let outcome = FetchOutcome {
            yahoo_history: HashMap::from([
                (
                    "FOO.AS".to_string(),
                    YahooHistory {
                        prices: vec![(1_700_000_000, 10.0)],
                        currency: Some("EUR".to_string()),
                    },
                ),
                (
                    "FOO.PA".to_string(),
                    YahooHistory {
                        prices: vec![(1_700_000_000, 20.0)],
                        currency: Some("EUR".to_string()),
                    },
                ),
            ]),
            yahoo_errors: HashMap::new(),
        };

        let (result, writes) = assemble_result(pre, symbols, outcome, "now".to_string());
        assert!(result.errors.is_empty());
        // One write, from the first candidate only.
        assert_eq!(writes.writes.len(), 1);
        assert_eq!(writes.writes[0].commodity_guid, "c-foo");
        assert_eq!(writes.writes[0].prices, vec![(1_700_000_000, 10.0)]);
        assert_eq!(result.prices.len(), 1);
        assert!((result.prices[0].price - 10.0).abs() < 1e-9);
    }

    #[test]
    fn write_prices_and_log_only_logs_a_nonempty_write() {
        let conn = setup_commodities();
        let empty = PendingPriceWrites {
            writes: vec![],
            report_currency_guid: Some("c-eur".to_string()),
            fx_prices: vec![],
        };
        write_prices_and_log(&conn, &empty).unwrap();
        assert_eq!(get_last_refresh_timestamp(&conn).unwrap(), None);

        let with_write = PendingPriceWrites {
            writes: vec![PendingPriceWrite {
                commodity_guid: "c-aapl".to_string(),
                currency_guid: "c-eur".to_string(),
                prices: vec![(1_700_000_000, 12.5)],
            }],
            report_currency_guid: Some("c-eur".to_string()),
            fx_prices: vec![],
        };
        write_prices_and_log(&conn, &with_write).unwrap();
        assert!(get_last_refresh_timestamp(&conn).unwrap().is_some());
    }
}
