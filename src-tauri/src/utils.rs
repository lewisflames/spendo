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
