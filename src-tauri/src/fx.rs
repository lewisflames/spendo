use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use crate::utils;

/// Detect the book's report (display) currency GUID.
///
/// Priority:
/// 1. The ROOT account's commodity (GnuCash book default currency)
/// 2. The currency with the most split value
/// 3. The first CURRENCY commodity
///
/// `Ok(None)` means the book genuinely has no currency commodity; a database
/// failure propagates instead of masquerading as "no currency".
pub fn get_report_currency_guid(conn: &Connection) -> Result<Option<String>, String> {
    let root: Option<String> = conn
        .query_row(
            "SELECT c.guid
             FROM accounts a
             JOIN commodities c ON c.guid = a.commodity_guid
             WHERE a.account_type = 'ROOT' AND c.namespace = 'CURRENCY'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if root.is_some() {
        return Ok(root);
    }

    let weighted: Option<String> = conn
        .query_row(
            &format!(
                "SELECT c.guid
                 FROM splits s
                 JOIN accounts a ON a.guid = s.account_guid
                 JOIN commodities c ON c.guid = a.commodity_guid
                 JOIN transactions t ON t.guid = s.tx_guid
                 WHERE c.namespace = 'CURRENCY' AND {}
                 GROUP BY c.guid
                 ORDER BY SUM(ABS({})) DESC
                 LIMIT 1",
                utils::VISIBLE_TX_FILTER,
                utils::RAW_AMOUNT
            ),
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if weighted.is_some() {
        return Ok(weighted);
    }

    conn.query_row(
        "SELECT guid FROM commodities WHERE namespace = 'CURRENCY' ORDER BY mnemonic LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn get_report_currency_mnemonic(conn: &Connection) -> Result<Option<String>, String> {
    let Some(guid) = get_report_currency_guid(conn)? else {
        return Ok(None);
    };
    conn.query_row(
        "SELECT mnemonic FROM commodities WHERE guid = ?1",
        [guid],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Report currency mnemonic, or `""` when the book has none yet.
pub fn report_currency_or_empty(conn: &Connection) -> Result<String, String> {
    Ok(get_report_currency_mnemonic(conn)?.unwrap_or_default())
}

/// All CURRENCY-namespace commodities as (guid, mnemonic).
pub fn list_book_currencies(conn: &Connection) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT guid, mnemonic FROM commodities WHERE namespace = 'CURRENCY' ORDER BY mnemonic",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// CURRENCY mnemonic -> GUID for all currency commodities in the book.
pub fn currency_guid_map(conn: &Connection) -> Result<HashMap<String, String>, String> {
    Ok(list_book_currencies(conn)?
        .into_iter()
        .map(|(guid, mnemonic)| (mnemonic, guid))
        .collect())
}

/// Latest FX rate per currency (1 unit of currency = N units of report currency),
/// optionally as of a date. Rates live in the `prices` table as CURRENCY commodities
/// quoted in the report currency.
///
/// Missing-rate contract: not an error — callers degrade to 1.0 parity via
/// `convert()`/`unwrap_or(1.0)`. That keeps books with partial FX history usable,
/// but a missing row silently skews totals. Price refresh surfaces FX failures
/// as `PriceError`s; money queries intentionally do not fail the whole report.
pub fn get_fx_map(conn: &Connection, as_of: Option<&str>) -> Result<HashMap<String, f64>, String> {
    let Some(report_guid) = get_report_currency_guid(conn)? else {
        return Ok(HashMap::new());
    };

    let sql = format!(
        "SELECT mnemonic, price FROM (
            SELECT c.mnemonic,
                {} as price,
                ROW_NUMBER() OVER (PARTITION BY c.guid ORDER BY p.date DESC, p.guid DESC) as rn
            FROM prices p
            JOIN commodities c ON c.guid = p.commodity_guid
            WHERE c.namespace = 'CURRENCY'
                AND p.currency_guid = ?1
                AND (?2 IS NULL OR p.date <= ?2)
         ) WHERE rn = 1",
        utils::PRICE_AMOUNT
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(rusqlite::params![report_guid, as_of], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    });
    let mut map = HashMap::new();
    for row in rows.map_err(|e| e.to_string())? {
        let (mnemonic, price) = row.map_err(|e| e.to_string())?;
        map.insert(mnemonic, price);
    }
    Ok(map)
}

/// Convert an amount from its own currency to the report currency.
/// Missing rates degrade to 1.0 parity (see `convert_tracked`).
pub fn convert(
    amount: f64,
    from_currency: &str,
    report_currency: &str,
    fx: &HashMap<String, f64>,
) -> f64 {
    convert_tracked(amount, from_currency, report_currency, fx, None)
}

/// Convert an amount from its own currency to the report currency, degrading
/// to 1.0 parity for currencies without a rate. Pass `missing` to record gaps
/// (report warnings) or `None` for silent legs — e.g. investment cost, which
/// is either priced (market dominates) or flagged via `missing_prices`.
pub fn convert_tracked(
    amount: f64,
    from_currency: &str,
    report_currency: &str,
    fx: &HashMap<String, f64>,
    missing: Option<&mut HashSet<String>>,
) -> f64 {
    // A zero balance converts to zero at any rate, so it can never skew a
    // total — don't flag its currency.
    if amount == 0.0 {
        return amount;
    }
    if report_currency.is_empty() || from_currency == report_currency {
        return amount;
    }
    match fx.get(from_currency) {
        Some(rate) => amount * rate,
        None => {
            if let Some(set) = missing {
                set.insert(from_currency.to_string());
            }
            amount
        }
    }
}

/// Convert an amount whose currency may be absent (e.g. splits on accounts
/// with no commodity): `None` passes through unconverted.
pub fn convert_optional(
    amount: f64,
    commodity: Option<&str>,
    report_currency: &str,
    fx: &HashMap<String, f64>,
) -> f64 {
    match commodity {
        Some(mn) => convert(amount, mn, report_currency, fx),
        None => amount,
    }
}

/// A security held in the book with no cached price. Distinct from a missing
/// FX rate: refreshing only helps if the symbol resolves, otherwise the user
/// must set an explicit symbol override on the holding.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingPrice {
    pub mnemonic: String,
    pub commodity_guid: Option<String>,
}

/// Date-ordered FX rate history loaded once per request, replacing repeated
/// `get_fx_map` calls (one window-function scan per distinct date).
pub struct FxHistory {
    /// (mnemonic, ascending [(date, rate)]) with one entry per date.
    events: Vec<(String, Vec<(String, f64)>)>,
}

impl FxHistory {
    pub fn load(conn: &Connection) -> Result<FxHistory, String> {
        let Some(report_guid) = get_report_currency_guid(conn)? else {
            return Ok(FxHistory { events: vec![] });
        };
        let mut stmt = conn
            .prepare(&format!(
                "SELECT c.mnemonic, p.date, p.guid,
					{} as rate
				 FROM prices p
				 JOIN commodities c ON c.guid = p.commodity_guid
				 WHERE c.namespace = 'CURRENCY' AND p.currency_guid = ?1
				 ORDER BY c.mnemonic ASC, p.date ASC, p.guid ASC",
                utils::PRICE_AMOUNT
            ))
            .map_err(|e| e.to_string())?;
        let rows: Vec<(String, String, String, f64)> = stmt
            .query_map([report_guid], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let mut by_mnemonic: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        for (mnemonic, date, _guid, rate) in rows {
            let evs = by_mnemonic.entry(mnemonic).or_default();
            // Same-day duplicates (mixed sources): the later guid wins.
            match evs.last_mut() {
                Some((d, r)) if *d == date => *r = rate,
                _ => evs.push((date, rate)),
            }
        }
        let mut events: Vec<(String, Vec<(String, f64)>)> = by_mnemonic.into_iter().collect();
        events.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(FxHistory { events })
    }

    /// Rates as of `date`: latest rate on or before the date per currency.
    /// Currencies without a qualifying rate are omitted (call sites fall back
    /// to 1.0, matching `get_fx_map` semantics).
    pub fn map_for_date(&self, date: &str) -> HashMap<String, f64> {
        let mut out = HashMap::with_capacity(self.events.len());
        for (mnemonic, evs) in &self.events {
            let idx = evs.partition_point(|(d, _)| d.as_str() <= date);
            if idx > 0 {
                out.insert(mnemonic.clone(), evs[idx - 1].1);
            }
        }
        out
    }
}
