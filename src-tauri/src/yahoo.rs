use serde::Deserialize;

static HTTP_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
const HTTP_TIMEOUT_SECS: u64 = 15;

fn http_client() -> Result<&'static reqwest::Client, String> {
    if let Some(client) = HTTP_CLIENT.get() {
        return Ok(client);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
    // `set` only fails if another thread won the race; reuse its client then.
    let _ = HTTP_CLIENT.set(client);
    HTTP_CLIENT
        .get()
        .ok_or_else(|| "Failed to create HTTP client".to_string())
}

const NAMESPACE_YAHOO_SUFFIX: [(&str, &[&str]); 8] = [
    ("EUREX", &[".DE"]),
    ("XETRA", &[".DE"]),
    ("EURONEXT", &[".AS", ".PA"]),
    ("FUND", &[".AS", ".L", ".DE", ".PA"]),
    ("MUTF", &[".AS", ".L", ".PA"]),
    ("NYSE", &[""]),
    ("NASDAQ", &[""]),
    ("AMEX", &[""]),
];

const DEFAULT_SUFFIXES: &[&str] = &[".AS", ".L", ".DE", ".PA", ""];

/// Fiat currencies Yahoo supports as crypto quote pairs (BTC-XXX etc.).
pub const SUPPORTED_CRYPTO_FIAT: &[&str] = &[
    "EUR", "USD", "GBP", "JPY", "CHF", "CAD", "AUD", "HKD", "NZD", "SGD", "SEK", "NOK", "DKK",
    "MXN", "TRY", "ZAR", "PLN", "CZK",
];

pub fn derive_yahoo_symbols(mnemonic: &str, namespace: &str, crypto_suffix: &str) -> Vec<String> {
    if mnemonic.contains('.') || mnemonic.contains('-') {
        return vec![mnemonic.to_string()];
    }
    if namespace == "CRYPTO" {
        return vec![format!("{}{}", mnemonic, crypto_suffix)];
    }
    for (ns, suffixes) in &NAMESPACE_YAHOO_SUFFIX {
        if *ns == namespace {
            return suffixes
                .iter()
                .map(|s| {
                    if s.is_empty() {
                        mnemonic.to_string()
                    } else {
                        format!("{}{}", mnemonic, s)
                    }
                })
                .collect();
        }
    }
    DEFAULT_SUFFIXES
        .iter()
        .map(|s| {
            if s.is_empty() {
                mnemonic.to_string()
            } else {
                format!("{}{}", mnemonic, s)
            }
        })
        .collect()
}

/// Infer the quote currency of a Yahoo symbol (e.g. "AAPL" on NASDAQ -> USD,
/// "SIE.DE" -> EUR, "BTC-EUR" -> EUR). Falls back to the report currency —
/// this is only a fallback: `YahooHistory.currency` (Yahoo's `meta.currency`)
/// wins when it is present and known to the book.
pub fn quote_currency_for_symbol(sym: &str, namespace: &str, report: &str) -> String {
    if sym.ends_with(".DE") || sym.ends_with(".AS") || sym.ends_with(".PA") {
        return "EUR".to_string();
    }
    if sym.ends_with(".L") {
        return "GBP".to_string();
    }
    if sym.ends_with(".T") {
        return "JPY".to_string();
    }
    if sym.ends_with(".HK") {
        return "HKD".to_string();
    }
    if sym.ends_with(".TO") {
        return "CAD".to_string();
    }
    // Only crypto pairs encode their quote currency after the dash; for
    // equities (`BRK-B`) that segment is part of the ticker, not a currency.
    if namespace == "CRYPTO" {
        if let Some((_, quote)) = sym.split_once('-') {
            if !quote.is_empty() {
                return quote.to_string();
            }
        }
    }
    // US exchanges quote in USD regardless of dotted tickers (`BRK.B`).
    if matches!(namespace, "NYSE" | "NASDAQ" | "AMEX") {
        return "USD".to_string();
    }
    report.to_string()
}

#[derive(Debug, Deserialize)]
struct YahooChartResponse {
    chart: YahooChart,
}

#[derive(Debug, Deserialize)]
struct YahooChart {
    result: Vec<YahooChartResult>,
}

#[derive(Debug, Deserialize)]
struct YahooChartResult {
    meta: Option<YahooMeta>,
    timestamp: Option<Vec<i64>>,
    indicators: Option<YahooIndicators>,
}

#[derive(Debug, Deserialize)]
struct YahooMeta {
    currency: Option<String>,
}

/// One symbol's fetched history plus the quote currency Yahoo reported
/// (`meta.currency`), which is authoritative over suffix guessing.
#[derive(Debug, Clone, PartialEq)]
pub struct YahooHistory {
    pub prices: Vec<(i64, f64)>,
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YahooIndicators {
    quote: Vec<YahooQuote>,
}

#[derive(Debug, Deserialize)]
struct YahooQuote {
    close: Vec<Option<f64>>,
}

/// RFC 3986 percent-encoding for Yahoo path segments (unreserved set kept
/// verbatim, everything else `%XX` over UTF-8 bytes). Hand-rolled to avoid a
/// new dependency for a single call site; covered by unit tests below.
fn percent_encode(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

pub async fn fetch_single_yahoo_history(
    symbol: &str,
    period1: Option<i64>,
    period2: i64,
) -> Result<YahooHistory, String> {
    let encoded = percent_encode(symbol);
    let url = if let Some(p1) = period1 {
        format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&period1={}&period2={}",
            encoded, p1, period2
        )
    } else {
        format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=10y",
            encoded
        )
    };

    let client = http_client()?;

    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if response.status().as_u16() == 429 {
        return Err("Rate limited by Yahoo Finance".to_string());
    }
    if !response.status().is_success() {
        return Err(format!("HTTP {} from Yahoo", response.status().as_u16()));
    }

    let data: YahooChartResponse = response
        .json()
        .await
        .map_err(|e| format!("Yahoo Finance API error: {}", e))?;

    if let Some(result) = data.chart.result.first() {
        let currency = result
            .meta
            .as_ref()
            .and_then(|meta| meta.currency.clone())
            .filter(|c| !c.trim().is_empty());
        let timestamps = result.timestamp.as_deref().unwrap_or(&[]);
        let closes = result
            .indicators
            .as_ref()
            .and_then(|i| i.quote.first())
            .map(|q| q.close.as_slice())
            .unwrap_or(&[]);

        let prices: Vec<(i64, f64)> = timestamps
            .iter()
            .zip(closes.iter())
            .filter_map(|(ts, price)| match price {
                // Zero/NaN closes poison every later FX conversion (inverse
                // rates become infinite, prices saturate on write).
                Some(p) if p.is_finite() && *p > 0.0 => Some((*ts, *p)),
                _ => None,
            })
            .collect();

        if prices.is_empty() {
            Err("No price data in response".to_string())
        } else {
            Ok(YahooHistory { prices, currency })
        }
    } else {
        Err("No result from Yahoo".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_symbols_by_namespace() {
        assert_eq!(derive_yahoo_symbols("SIE", "XETRA", "-EUR"), vec!["SIE.DE"]);
        assert_eq!(
            derive_yahoo_symbols("AIR", "EURONEXT", "-EUR"),
            vec!["AIR.AS", "AIR.PA"]
        );
        assert_eq!(derive_yahoo_symbols("AAPL", "NASDAQ", "-EUR"), vec!["AAPL"]);
        assert_eq!(derive_yahoo_symbols("BTCB", "CRYPTO", "-USD"), vec!["BTCB-USD"]);
        // Unknown namespaces try the default suffix list.
        assert_eq!(
            derive_yahoo_symbols("XYZ", "LSE", "-EUR"),
            vec!["XYZ.AS", "XYZ.L", "XYZ.DE", "XYZ.PA", "XYZ"]
        );
    }

    #[test]
    fn dotted_or_dashed_mnemonics_pass_through() {
        // Already Yahoo-shaped (or crypto-like) mnemonics are used verbatim.
        assert_eq!(derive_yahoo_symbols("BRK.B", "NYSE", "-EUR"), vec!["BRK.B"]);
        assert_eq!(derive_yahoo_symbols("SOME-PAIR", "CRYPTO", "-EUR"), vec!["SOME-PAIR"]);
    }

    #[test]
    fn percent_encode_leaves_unreserved_verbatim() {
        assert_eq!(percent_encode("AAPL"), "AAPL");
        assert_eq!(percent_encode("BRK.B"), "BRK.B");
        assert_eq!(percent_encode("BTC-EUR"), "BTC-EUR");
        assert_eq!(percent_encode("A/B C"), "A%2FB%20C");
    }

    #[test]
    fn quote_currency_from_symbol_shape() {
        assert_eq!(quote_currency_for_symbol("AAPL", "NASDAQ", "EUR"), "USD");
        // Dotted/dashed US tickers are still USD — the post-dash segment of
        // `BRK-B` is part of the ticker, not a quote currency.
        assert_eq!(quote_currency_for_symbol("BRK.B", "NYSE", "EUR"), "USD");
        assert_eq!(quote_currency_for_symbol("BRK-B", "NYSE", "EUR"), "USD");
        assert_eq!(quote_currency_for_symbol("SIE.DE", "XETRA", "USD"), "EUR");
        assert_eq!(quote_currency_for_symbol("BP.L", "LSE", "EUR"), "GBP");
        assert_eq!(quote_currency_for_symbol("7203.T", "JPX", "EUR"), "JPY");
        assert_eq!(quote_currency_for_symbol("0700.HK", "HKEX", "EUR"), "HKD");
        assert_eq!(quote_currency_for_symbol("RY.TO", "TSX", "EUR"), "CAD");
        assert_eq!(quote_currency_for_symbol("BTC-EUR", "CRYPTO", "USD"), "EUR");
        // No signal at all -> report currency.
        assert_eq!(quote_currency_for_symbol("WEIRD", "OTHER", "CHF"), "CHF");
    }
}
