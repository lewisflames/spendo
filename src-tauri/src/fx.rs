use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

use crate::utils;

/// Detect the book's report (display) currency GUID.
///
/// Priority:
/// 1. The ROOT account's commodity (GnuCash book default currency)
/// 2. The currency with the most split value
/// 3. The first CURRENCY commodity
pub fn get_report_currency_guid(conn: &Connection) -> Option<String> {
    let root: Option<String> = conn
        .query_row(
            "SELECT c.guid
             FROM accounts a
             JOIN commodities c ON c.guid = a.commodity_guid
             WHERE a.account_type = 'ROOT' AND c.namespace = 'CURRENCY'",
            [],
            |row| row.get(0),
        )
        .ok();
    if root.is_some() {
        return root;
    }

    let weighted: Option<String> = conn
        .query_row(
            &format!(
                "SELECT c.guid
                 FROM splits s
                 JOIN accounts a ON a.guid = s.account_guid
                 JOIN commodities c ON c.guid = a.commodity_guid
                 WHERE c.namespace = 'CURRENCY'
                 GROUP BY c.guid
                 ORDER BY SUM(ABS({})) DESC
                 LIMIT 1",
                utils::RAW_AMOUNT
            ),
            [],
            |row| row.get(0),
        )
        .ok();
    if weighted.is_some() {
        return weighted;
    }

    conn.query_row(
        "SELECT guid FROM commodities WHERE namespace = 'CURRENCY' ORDER BY mnemonic LIMIT 1",
        [],
        |row| row.get(0),
    )
    .ok()
}

pub fn get_report_currency_mnemonic(conn: &Connection) -> Option<String> {
    let guid = get_report_currency_guid(conn)?;
    conn.query_row(
        "SELECT mnemonic FROM commodities WHERE guid = ?1",
        [guid],
        |row| row.get(0),
    )
    .ok()
}

/// All CURRENCY-namespace commodities as (guid, mnemonic).
pub fn list_book_currencies(conn: &Connection) -> Vec<(String, String)> {
    let mut stmt = match conn.prepare(
        "SELECT guid, mnemonic FROM commodities WHERE namespace = 'CURRENCY' ORDER BY mnemonic",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok();
    match rows {
        Some(rows) => rows.flatten().collect(),
        None => vec![],
    }
}

/// CURRENCY mnemonic -> GUID for all currency commodities in the book.
pub fn currency_guid_map(conn: &Connection) -> HashMap<String, String> {
    list_book_currencies(conn)
        .into_iter()
        .map(|(guid, mnemonic)| (mnemonic, guid))
        .collect()
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
    let Some(report_guid) = get_report_currency_guid(conn) else {
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

/// FX rate for a single currency as of a date (latest rate on or before the date).
/// Returns 1.0 only when no qualifying rate row exists; DB errors propagate.
pub fn get_fx_rate_at(
    conn: &Connection,
    currency_mnemonic: &str,
    report_guid: &str,
    date: &str,
) -> Result<f64, String> {
    let rate: Option<f64> = conn
        .query_row(
            &format!(
                "SELECT {}
                 FROM prices p
                 JOIN commodities c ON c.guid = p.commodity_guid
                 WHERE c.namespace = 'CURRENCY'
                    AND c.mnemonic = ?1
                    AND p.currency_guid = ?2
                    AND p.date <= ?3
                 ORDER BY p.date DESC, p.guid DESC
                 LIMIT 1",
                utils::PRICE_AMOUNT
            ),
            rusqlite::params![currency_mnemonic, report_guid, date],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(rate.unwrap_or(1.0))
}

/// Convert an amount from its own currency to the report currency.
pub fn convert(
    amount: f64,
    from_currency: &str,
    report_currency: &str,
    fx: &HashMap<String, f64>,
) -> f64 {
    if from_currency == report_currency {
        amount
    } else {
        amount * fx.get(from_currency).copied().unwrap_or(1.0)
    }
}

/// Date-ordered FX rate history loaded once per request, replacing repeated
/// `get_fx_map` calls (one window-function scan per distinct date).
pub struct FxHistory {
    /// (mnemonic, ascending [(date, rate)]) with one entry per date.
    events: Vec<(String, Vec<(String, f64)>)>,
}

impl FxHistory {
    pub fn load(conn: &Connection) -> Result<FxHistory, String> {
        let Some(report_guid) = get_report_currency_guid(conn) else {
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
