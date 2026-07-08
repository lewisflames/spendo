use serde::Deserialize;

static HTTP_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
const HTTP_TIMEOUT_SECS: u64 = 15;

fn http_client() -> &'static reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
            .user_agent(USER_AGENT)
            .build()
            .expect("Failed to create HTTP client")
    })
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
/// "SIE.DE" -> EUR, "BTC-EUR" -> EUR). Falls back to the report currency.
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
    if sym.contains('-') {
        if let Some((_, quote)) = sym.split_once('-') {
            if !quote.is_empty() {
                return quote.to_string();
            }
        }
    }
    if !sym.contains('.') && matches!(namespace, "NYSE" | "NASDAQ" | "AMEX") {
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
    timestamp: Option<Vec<i64>>,
    indicators: Option<YahooIndicators>,
}

#[derive(Debug, Deserialize)]
struct YahooIndicators {
    quote: Vec<YahooQuote>,
}

#[derive(Debug, Deserialize)]
struct YahooQuote {
    close: Vec<Option<f64>>,
}

fn urlencoding(s: &str) -> String {
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
) -> Result<Vec<(i64, f64)>, String> {
    let encoded = urlencoding(symbol);
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

    let client = http_client();

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
            .filter_map(|(ts, price)| price.map(|p| (*ts, p)))
            .collect();

        if prices.is_empty() {
            Err("No price data in response".to_string())
        } else {
            Ok(prices)
        }
    } else {
        Err("No result from Yahoo".to_string())
    }
}
