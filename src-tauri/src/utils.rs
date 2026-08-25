pub const CREDIT_NORMAL: [&str; 5] = ["INCOME", "LIABILITY", "EQUITY", "CREDIT", "PAYABLE"];

pub const INVESTMENT_TYPES: [&str; 3] = ["STOCK", "MUTUAL", "FUND"];

/// `'STOCK','MUTUAL','FUND'` fragment for SQL `IN (...)` lists, derived from
/// [`INVESTMENT_TYPES`] so query filters can never drift from Rust-side checks.
pub fn investment_types_sql() -> String {
    INVESTMENT_TYPES
        .iter()
        .map(|t| format!("'{t}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn is_credit_normal(account_type: &str) -> bool {
    CREDIT_NORMAL.contains(&account_type)
}

pub const RAW_AMOUNT: &str = "CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)";
pub const PRICE_AMOUNT: &str = "CAST(p.value_num AS REAL) / CAST(p.value_denom AS REAL)";
pub const QUANTITY_AMOUNT: &str = "CAST(s.quantity_num AS REAL) / CAST(s.quantity_denom AS REAL)";

/// Visibility window applied to every "current state" query: future-dated
/// transactions are excluded so net worth, PnL, budgets and portfolio agree
/// across pages for the same book.
pub const VISIBLE_TX_FILTER: &str = "t.post_date < date('now', '+1 day')";

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

pub fn month_number(ym: &str) -> Result<i64, String> {
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
    Ok(y * 12 + (m - 1))
}

pub fn month_range(year: i32, month: i32) -> (String, String) {
    let this = format!("{:04}-{:02}", year, month);
    let next = if month == 12 {
        format!("{:04}-01", year + 1)
    } else {
        format!("{:04}-{:02}", year, month + 1)
    };
    (this, next)
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

/// Validate a `YYYY-MM-DD` string at the IPC boundary.
pub fn validate_date(s: &str) -> Result<(), String> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| format!("Invalid date '{}': expected YYYY-MM-DD", s))
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
    fn month_range_spans_into_next_year() {
        assert_eq!(
            month_range(2024, 1),
            ("2024-01".to_string(), "2024-02".to_string())
        );
        assert_eq!(
            month_range(2024, 12),
            ("2024-12".to_string(), "2025-01".to_string())
        );
    }

    #[test]
    fn escape_like_escapes_metacharacters() {
        assert_eq!(escape_like("100%"), r"100\%");
        assert_eq!(escape_like("a_b"), r"a\_b");
        assert_eq!(escape_like(r"back\slash"), r"back\\slash");
        assert_eq!(escape_like("plain"), "plain");
    }

    #[test]
    fn total_pages_always_at_least_one() {
        assert_eq!(total_pages(0, 50), 1);
        assert_eq!(total_pages(1, 50), 1);
        assert_eq!(total_pages(51, 50), 2);
        assert_eq!(total_pages(100, 50), 2);
    }
}
