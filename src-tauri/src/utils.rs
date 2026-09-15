/// Account types displayed with reversed sign (credit balances read positive).
/// TRADING is included: GnuCash trading books accumulate currency/commodity
/// trading gains as net credits, and the balance-sheet identity only holds if
/// they are negated like other credit-normal types (see `accounts.rs`).
pub const CREDIT_NORMAL: [&str; 6] = [
    "INCOME",
    "LIABILITY",
    "EQUITY",
    "CREDIT",
    "PAYABLE",
    "TRADING",
];

pub const INVESTMENT_TYPES: [&str; 3] = ["STOCK", "MUTUAL", "FUND"];

/// Cash-like balance-sheet types: everything asset-side except investments
/// (which are marked to market separately).
pub const CASH_LIKE_TYPES: [&str; 5] = ["ASSET", "BANK", "CASH", "CURRENCY", "RECEIVABLE"];
pub const LIABILITY_TYPES: [&str; 3] = ["LIABILITY", "CREDIT", "PAYABLE"];

/// Account type → reporting category. Mirrors the frontend
/// `$lib/features/accounts/account-types.ts` table; `ROOT` is excluded.
pub const TYPE_TO_CATEGORY: [(&str, &str); 15] = [
    ("ASSET", "ASSET"),
    ("BANK", "ASSET"),
    ("CASH", "ASSET"),
    ("STOCK", "ASSET"),
    ("MUTUAL", "ASSET"),
    ("FUND", "ASSET"),
    ("CURRENCY", "ASSET"),
    ("RECEIVABLE", "ASSET"),
    ("LIABILITY", "LIABILITY"),
    ("CREDIT", "LIABILITY"),
    ("PAYABLE", "LIABILITY"),
    ("INCOME", "INCOME"),
    ("EXPENSE", "EXPENSE"),
    ("EQUITY", "EQUITY"),
    ("TRADING", "EQUITY"),
];

/// The five root categories, balance-sheet order then P&L.
pub const CATEGORY_ORDER: [&str; 5] = ["ASSET", "LIABILITY", "EQUITY", "INCOME", "EXPENSE"];

/// Category of an account type, falling back to the raw type when unknown.
pub fn category_for_type(account_type: &str) -> &str {
    TYPE_TO_CATEGORY
        .iter()
        .find(|(ty, _)| *ty == account_type)
        .map_or(account_type, |(_, category)| category)
}

/// Every categorised account type as an SQL `IN (...)` fragment.
pub fn category_types_all_sql() -> String {
    let types: Vec<&str> = TYPE_TO_CATEGORY.iter().map(|(ty, _)| *ty).collect();
    types_sql(&types)
}

/// `'STOCK','MUTUAL','FUND'` fragment for SQL `IN (...)` lists, derived from
/// [`INVESTMENT_TYPES`] so query filters can never drift from Rust-side checks.
pub fn investment_types_sql() -> String {
    types_sql(&INVESTMENT_TYPES)
}

/// [`CASH_LIKE_TYPES`] as an SQL `IN (...)` fragment.
pub fn cash_like_types_sql() -> String {
    types_sql(&CASH_LIKE_TYPES)
}

/// [`LIABILITY_TYPES`] as an SQL `IN (...)` fragment.
pub fn liability_types_sql() -> String {
    types_sql(&LIABILITY_TYPES)
}

fn types_sql(types: &[&str]) -> String {
    types
        .iter()
        .map(|t| format!("'{t}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn is_credit_normal(account_type: &str) -> bool {
    CREDIT_NORMAL.contains(&account_type)
}

/// Trimmed `None`-when-empty helper for optional text fields (create/update
/// payloads and schedule templates).
pub fn opt_text(s: &Option<String>) -> Option<String> {
    s.as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Book-wide visible transaction window (`YYYY-MM-DD`). Single `MIN,MAX`
/// query so earliest/latest can never diverge between callers.
pub fn visible_tx_window(
    conn: &rusqlite::Connection,
) -> Result<(Option<String>, Option<String>), String> {
    conn.query_row(
        &format!(
            "SELECT substr(MIN(t.post_date), 1, 10), substr(MAX(t.post_date), 1, 10)
             FROM transactions t
             WHERE {}",
            VISIBLE_TX_FILTER
        ),
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|e| e.to_string())
}

pub const RAW_AMOUNT: &str = "CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)";
pub const PRICE_AMOUNT: &str = "CAST(p.value_num AS REAL) / CAST(p.value_denom AS REAL)";
pub const QUANTITY_AMOUNT: &str = "CAST(s.quantity_num AS REAL) / CAST(s.quantity_denom AS REAL)";

/// Visibility window applied to every "current state" query: future-dated
/// transactions are excluded so net worth, PnL, budgets and portfolio agree
/// across pages for the same book.
///
/// Canonical date contract (half-open everywhere):
/// `from >= start AND to < date(end, '+1 day')`. Never use `<= YYYY-MM-DD`
/// against `post_date` (`YYYY-MM-DD HH:MM:SS`) — lexicographic `<=` drops
/// end-day timed rows. Use `< date(?, '+1 day')` or `substr(post_date,1,10)`.
pub const VISIBLE_TX_FILTER: &str = "t.post_date < date('now', '+1 day')";

/// Bare "now + 1 day" expression for queries that cap a caller-supplied upper
/// bound (e.g. PnL `min(date(?,'+1 day'), VISIBLE_NOW)`). Must stay in sync
/// with [`VISIBLE_TX_FILTER`].
pub const VISIBLE_NOW_EXPR: &str = "date('now', '+1 day')";

/// Escape `%`, `_` and `\` in user input for LIKE patterns used together
/// with `ESCAPE '\'`, so searching for a literal `%` matches a literal `%`
/// instead of acting as a wildcard.
pub fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        if c == '\\' || c == '%' || c == '_' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Total pages for a paginated result set, always at least 1 so empty result
/// sets still render page 1. Shared by every paginated command.
pub fn total_pages(total: i64, page_size: i64) -> i64 {
    (((total as f64) / (page_size as f64)).ceil() as i64).max(1)
}

pub fn signed_amount_expr(amount_col: &str, type_col: &str) -> String {
    let types = CREDIT_NORMAL
        .iter()
        .map(|t| format!("'{}'", t))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "CASE WHEN {} IN ({}) THEN -{} ELSE {} END",
        type_col, types, amount_col, amount_col
    )
}
pub fn iso_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn format_month(n: i64) -> String {
    let y = n / 12;
    let m = n % 12;
    format!("{:04}-{:02}", y, m + 1)
}

/// Last calendar day of a `YYYY-MM` month, for month-end FX lookups.
pub fn month_end(month: &str) -> Result<String, String> {
    let mut parts = month.split('-');
    let (Some(y), Some(m)) = (parts.next(), parts.next()) else {
        return Err(format!("Invalid month: {month}"));
    };
    if parts.next().is_some() {
        return Err(format!("Invalid month: {month}"));
    }
    let y: i32 = y.parse().map_err(|_| format!("Invalid month: {month}"))?;
    let m: u32 = m.parse().map_err(|_| format!("Invalid month: {month}"))?;
    let first_next = if m == 12 {
        chrono::NaiveDate::from_ymd_opt(y + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(y, m + 1, 1)
    }
    .ok_or_else(|| format!("Invalid month: {month}"))?;
    Ok((first_next - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string())
}

pub fn month_number(ym: &str) -> Result<i64, String> {
    // Accept both `YYYY-MM` (canonical) and full `YYYY-MM-DD` dates (as passed
    // by calendar widgets) by truncating dated strings to the month prefix.
    // Only truncate when the input actually looks like a full date; otherwise
    // keep it whole so malformed months still error (e.g. `2024-13x`).
    let ym = if ym.len() > 7 && ym.as_bytes().get(7) == Some(&b'-') {
        ym.get(..7).unwrap_or(ym)
    } else {
        ym
    };
    let parts: Vec<&str> = ym.split('-').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid month format: {}", ym));
    }
    let y: i64 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid year in month string: {}", ym))?;
    let m: i64 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid month in month string: {}", ym))?;
    if !(1..=12).contains(&m) {
        return Err(format!("Invalid month in month string: {}", ym));
    }
    Ok(y * 12 + (m - 1))
}

/// Clamp a client-supplied page size into a sane range (SQLite treats negative
/// LIMIT as unlimited, and huge values would materialize entire tables).
pub fn clamp_page_size(page_size: i64) -> i64 {
    page_size.clamp(1, 500)
}

/// Clamp a 1-based page number into `1..=total_pages` so out-of-range
/// requests can neither produce a negative offset nor an empty page.
pub fn clamp_page(page: i64, total_pages: i64) -> i64 {
    page.clamp(1, total_pages.max(1))
}

/// Single entry point for paginated queries: clamps `page_size` first (so
/// direct `_inner` callers/tests can never divide by zero or emit a negative
/// LIMIT/OFFSET), then derives `total_pages`, the clamped page and offset.
/// Returns `(clamped_page_size, total_pages, safe_page, offset)`.
pub fn paginate(total: i64, page: i64, page_size: i64) -> (i64, i64, i64, i64) {
    let page_size = clamp_page_size(page_size);
    let total_pages = total_pages(total, page_size);
    let safe_page = clamp_page(page, total_pages);
    let offset = (safe_page - 1) * page_size;
    (page_size, total_pages, safe_page, offset)
}

/// Validate a strict, zero-padded `YYYY-MM-DD` calendar date and return it
/// normalized. SQLite's `date()` silently yields NULL for malformed input, so
/// any caller that interpolates a date into SQL must validate first instead of
/// letting a bad string quietly zero out comparisons.
pub fn validate_iso_date(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let parsed = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .map_err(|_| format!("Invalid date (expected YYYY-MM-DD): {trimmed}"))?;
    let normalized = parsed.format("%Y-%m-%d").to_string();
    if normalized != trimmed {
        return Err(format!("Invalid date (expected YYYY-MM-DD): {trimmed}"));
    }
    Ok(normalized)
}

/// Validate that a path points at an existing `.gnucash` file.
pub fn validate_gnucash_path(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if p.extension().and_then(|e| e.to_str()) != Some("gnucash") {
        return Err(format!("Not a .gnucash file: {}", path));
    }
    if !p.is_file() {
        return Err(format!("File not found: {}", path));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_number_parses_year_month_pairs() {
        assert_eq!(month_number("0000-01").unwrap(), 0);
        assert_eq!(month_number("0000-12").unwrap(), 11);
        assert_eq!(month_number("0001-01").unwrap(), 12);
        assert_eq!(month_number("2024-01").unwrap(), 24_288);
        assert_eq!(month_number("2024-12").unwrap(), 24_299);
        assert_eq!(month_number("2025-01").unwrap(), 24_300);
        // Full calendar dates truncate to their month prefix.
        assert_eq!(month_number("2024-01-15").unwrap(), 24_288);
        assert_eq!(month_number("2024-02-05").unwrap(), 24_289);
        assert!(month_number("2024").is_err());
        assert!(month_number("2024-13x").is_err());
    }

    #[test]
    fn format_month_round_trips_month_number() {
        for n in [0i64, 11, 12, 25, 603] {
            let ym = format_month(n);
            assert_eq!(month_number(&ym).unwrap(), n);
        }
        assert_eq!(format_month(0), "0000-01");
        assert_eq!(format_month(24), "0002-01");
    }

    #[test]
    fn escape_like_escapes_metacharacters() {
        assert_eq!(escape_like("100%"), r"100\%");
        assert_eq!(escape_like("a_b"), r"a\_b");
        assert_eq!(escape_like(r"back\slash"), r"back\\slash");
        assert_eq!(escape_like("plain"), "plain");
    }

    #[test]
    fn month_end_returns_last_calendar_day() {
        assert_eq!(month_end("2024-02").unwrap(), "2024-02-29");
        assert_eq!(month_end("2023-02").unwrap(), "2023-02-28");
        assert_eq!(month_end("2024-12").unwrap(), "2024-12-31");
        assert_eq!(month_end("2024-01").unwrap(), "2024-01-31");
        assert!(month_end("2024-13").is_err());
        assert!(month_end("not-a-month").is_err());
    }

    #[test]
    fn validate_iso_date_accepts_only_strict_dates() {
        assert_eq!(validate_iso_date("2024-02-29").unwrap(), "2024-02-29");
        // Non-padded or out-of-range components must not reach SQL date math.
        assert!(validate_iso_date("2024-2-9").is_err());
        assert!(validate_iso_date("2023-02-29").is_err());
        assert!(validate_iso_date("2024-13-01").is_err());
        assert!(validate_iso_date("").is_err());
        assert!(validate_iso_date("not-a-date").is_err());
    }

    #[test]
    fn paginate_clamps_and_never_divides_by_zero() {
        // Zero/negative/huge sizes clamp to 1..=500.
        let (size, pages, safe, offset) = paginate(0, 1, 0);
        assert_eq!((size, pages, safe, offset), (1, 1, 1, 0));
        let (size, pages, safe, offset) = paginate(100, 99, 50);
        assert_eq!(size, 50);
        assert_eq!(pages, 2);
        assert_eq!(safe, 2);
        assert_eq!(offset, 50);
        let (size, _, _, _) = paginate(10, 1, 10_000);
        assert_eq!(size, 500);
        // Totals never yield an empty page range.
        assert_eq!(total_pages(0, 50), 1);
        assert_eq!(total_pages(51, 50), 2);
    }
}
