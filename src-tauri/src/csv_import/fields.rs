use super::*;

// ── Field parsers ────────────────────────────────────────────────────────────

pub(super) fn try_ymd(year: i32, month: u32, day: u32) -> Option<String> {
    chrono::NaiveDate::from_ymd_opt(year, month, day).map(|d| d.format("%Y-%m-%d").to_string())
}

pub(super) fn map_two_digit_year(yy: i32) -> i32 {
    if (0..=68).contains(&yy) {
        2000 + yy
    } else {
        1900 + yy
    }
}

/// Lenient date parse into canonical `YYYY-MM-DD`. Accepts ISO plus the bank
/// trio `DD/MM/YYYY`, `MM/DD/YYYY`, `DD.MM.YYYY`, `DD-MM-YYYY` and a few
/// textual forms. `preference` (`"mdy"` or anything else = DMY) resolves
/// ambiguous slash dates like `01/02/2026`.
pub(super) fn parse_csv_date(raw: &str, preference: &str) -> Result<String, AppError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(AppError::Validation("Add a date to every row".to_string()));
    }
    // ISO datetime prefix: `2024-03-01T…` / `2024-03-01 10:00`.
    let first_token = s.split(['T', ' ']).next().unwrap_or(s).trim();
    for fmt in ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d"] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(first_token, fmt) {
            return Ok(d.format("%Y-%m-%d").to_string());
        }
    }
    // Textual forms.
    for fmt in [
        "%d %b %Y",
        "%d %B %Y",
        "%b %d, %Y",
        "%B %d, %Y",
        "%d-%b-%Y",
        "%d-%b-%y",
    ] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(s, fmt) {
            return Ok(d.format("%Y-%m-%d").to_string());
        }
    }
    // Numeric 3-part dates.
    let normalized = first_token.replace(['.', '-'], "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() == 3 {
        let a: Option<i32> = parts[0].trim().parse().ok();
        let b: Option<i32> = parts[1].trim().parse().ok();
        let c_raw = parts[2].trim();
        let c: Option<i32> = c_raw.parse().ok();
        if let (Some(a), Some(b), Some(c)) = (a, b, c) {
            // Year-first already handled above, but tolerate `2024/3/1`.
            if parts[0].len() == 4 {
                let year = a;
                if let Some(d) = try_ymd(year, b as u32, c as u32) {
                    return Ok(d);
                }
            } else {
                let year = if parts[2].len() == 2 {
                    map_two_digit_year(c)
                } else {
                    c
                };
                let mdy_first: [(i32, i32); 2] = if preference.eq_ignore_ascii_case("mdy") {
                    [(a, b), (b, a)]
                } else {
                    [(b, a), (a, b)]
                };
                // mdy_first holds (month, day) attempts in preference order.
                for (month, day) in mdy_first {
                    if let Some(d) = try_ymd(year, month as u32, day as u32) {
                        // Guard the classic ambiguity: when both orders parse
                        // (e.g. 01/02/2026) the loop order already encodes the
                        // preference, so the first hit wins.
                        return Ok(d);
                    }
                }
            }
        }
    }
    // Compact `YYYYMMDD` (some exports).
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        let y: i32 = s[0..4].parse().unwrap_or(0);
        let m: u32 = s[4..6].parse().unwrap_or(0);
        let d: u32 = s[6..8].parse().unwrap_or(0);
        if let Some(out) = try_ymd(y, m, d) {
            return Ok(out);
        }
    }
    Err(AppError::Validation(format!(
        "Invalid date (expected YYYY-MM-DD or bank format): {s}"
    )))
}

/// Lenient amount parse: strips currency symbols/spaces, `(…)` negatives,
/// normalizes typographic minus signs, and both thousand/decimal conventions
/// (last separator wins when both `,` and `.` are present; lone `,` with 1–2
/// trailing digits is a decimal comma). `dot_is_thousands` is the file-wide
/// convention from [`detect_dot_thousands`]; it makes a lone dot with 3-digit
/// groups a thousands separator (`1.234` → 1234). Zero is allowed so callers
/// can treat `0.00` in an unused Debit/Credit column as absent.
pub(super) fn parse_csv_amount(raw: &str, dot_is_thousands: bool) -> Result<f64, AppError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(AppError::Validation(
            "Add an amount to every row".to_string(),
        ));
    }
    // Typographic minus/dash variants would otherwise be dropped by the filter
    // below, silently flipping a debit into a credit.
    let mut t: String = s
        .chars()
        .map(|c| match c {
            '\u{2212}' | '\u{2013}' | '\u{2014}' => '-',
            other => other,
        })
        .collect();
    let negative_parens = t.starts_with('(') && t.ends_with(')');
    if negative_parens {
        t = t[1..t.len() - 1].to_string();
    }
    // Keep digits, separators, sign, spaces; drop currency codes/symbols.
    let mut kept = String::with_capacity(t.len());
    for c in t.chars() {
        if c.is_ascii_digit() || matches!(c, ',' | '.' | '-' | '+' | ' ' | '\'' | '’') {
            kept.push(c);
        } else if c.is_whitespace() {
            kept.push(' ');
        }
    }
    let mut normalized: String = kept
        .chars()
        .filter(|c| *c != ' ' && *c != '\'' && *c != '’')
        .collect();
    if normalized.is_empty() || normalized == "-" || normalized == "+" {
        return Err(AppError::Validation(format!(
            "That amount doesn't look right: {s}"
        )));
    }
    let has_comma = normalized.contains(',');
    let has_dot = normalized.contains('.');
    if has_comma && has_dot {
        let last_comma = normalized.rfind(',').unwrap_or(0);
        let last_dot = normalized.rfind('.').unwrap_or(0);
        if last_comma > last_dot {
            // `1.234,56` → remove dots, comma becomes the decimal point.
            normalized = normalized.replace('.', "");
            normalized = normalized.replace(',', ".");
        } else {
            // `1,234.56` → remove commas.
            normalized = normalized.replace(',', "");
        }
    } else if has_comma {
        let comma_count = normalized.matches(',').count();
        if comma_count > 1 {
            normalized = normalized.replace(',', "");
        } else if let Some(pos) = normalized.rfind(',') {
            let frac_len = normalized.len() - pos - 1;
            if frac_len == 2 || frac_len == 1 {
                normalized = normalized.replace(',', ".");
            } else if frac_len == 3
                && normalized.len() > 4
                && normalized[..pos].chars().all(|c| c.is_ascii_digit())
            {
                // `1,234` thousands — drop the comma; `12,345` likewise.
                // A 3-digit fraction with a short head is ambiguous, but bank
                // exports use `1,234.00` (handled above) or `1234,00` for
                // decimals, so thousands is the safer read here.
                normalized = normalized.replace(',', "");
            } else {
                normalized = normalized.replace(',', ".");
            }
        }
    } else if has_dot && dot_is_thousands {
        // Continental file (`1.234` means 1234). Only strip dots when every
        // group after the first has exactly three digits, so `1.23` stays a
        // (malformed but better-than-mangled) decimal.
        if normalized.split('.').skip(1).all(|group| group.len() == 3) {
            normalized = normalized.replace('.', "");
        }
    }
    let mut value: f64 = normalized
        .parse()
        .map_err(|_| AppError::Validation(format!("That amount doesn't look right: {s}")))?;
    if negative_parens {
        value = -value.abs();
    }
    if !value.is_finite() {
        return Err(AppError::Validation(format!(
            "That amount doesn't look right: {s}"
        )));
    }
    if value.abs() > 1_000_000_000.0 {
        return Err(AppError::Validation(
            "That amount is huge. Keep it under 1,000,000,000".to_string(),
        ));
    }
    Ok(value)
}

/// File-wide decimal convention: true when amounts use comma decimals
/// (`1,23`, `1234,56`) and never dot decimals (`1.23`). A lone dot must then
/// be a thousands separator, so `1.234` means 1234 (continental exports).
/// Mixed files keep the dot-decimal default.
pub(super) fn detect_dot_thousands(
    data: &[Vec<String>],
    use_pair: bool,
    mapping: &CsvMapping,
) -> bool {
    fn tail_is_decimal(raw: &str, sep: char) -> bool {
        let t = raw.trim().trim_matches(|c: char| c == '(' || c == ')');
        let Some(pos) = t.rfind(sep) else {
            return false;
        };
        let frac = &t[pos + 1..];
        let digits = frac.chars().filter(char::is_ascii_digit).count();
        (digits == 1 || digits == 2) && digits == frac.chars().count()
    }

    let mut comma_decimal = false;
    let mut dot_decimal = false;
    for cells in data {
        let indexes: Vec<usize> = if use_pair {
            [mapping.debit_col, mapping.credit_col]
                .into_iter()
                .flatten()
                .collect()
        } else {
            mapping.amount_col.into_iter().collect()
        };
        for index in indexes {
            if let Some(raw) = cells.get(index) {
                comma_decimal |= tail_is_decimal(raw, ',');
                dot_decimal |= tail_is_decimal(raw, '.');
            }
        }
    }
    comma_decimal && !dot_decimal
}

pub(super) fn cell_at(cells: &[String], idx: Option<usize>) -> String {
    idx.and_then(|i| cells.get(i)).cloned().unwrap_or_default()
}

/// GUID→name for every account, used to label preview rows' categories.
pub(super) fn load_account_names(conn: &Connection) -> Result<HashMap<String, String>, AppError> {
    let mut stmt = conn
        .prepare("SELECT guid, name FROM accounts")
        .map_err(AppError::from)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(AppError::from)?;
    rows.collect::<Result<HashMap<_, _>, _>>()
        .map_err(AppError::from)
}

pub(super) fn require_col(idx: usize, column_count: usize, name: &str) -> Result<(), AppError> {
    if idx >= column_count {
        return Err(AppError::Validation(format!(
            "Column mapping out of range: {name} is column {}, but the file has {column_count}",
            idx + 1
        )));
    }
    Ok(())
}

pub(super) fn validate_mapping(
    mapping: &CsvMapping,
    column_count: usize,
) -> Result<bool, AppError> {
    require_col(mapping.date_col, column_count, "date")?;
    require_col(mapping.desc_col, column_count, "description")?;
    let single = mapping.amount_col.is_some();
    let pair = mapping.debit_col.is_some() || mapping.credit_col.is_some();
    if single == pair {
        return Err(AppError::Validation(
            "Map either one Amount column or the Debit+Credit pair".to_string(),
        ));
    }
    if single {
        require_col(mapping.amount_col.unwrap_or(0), column_count, "amount")?;
        return Ok(false);
    }
    match (mapping.debit_col, mapping.credit_col) {
        (Some(d), Some(c)) => {
            require_col(d, column_count, "debit")?;
            require_col(c, column_count, "credit")?;
            if d == c {
                return Err(AppError::Validation(
                    "Debit and credit must be different columns".to_string(),
                ));
            }
            Ok(true)
        }
        _ => Err(AppError::Validation(
            "Debit+Credit mode needs both columns mapped".to_string(),
        )),
    }
}

/// Reject accounts that must not hold splits: the internal ROOT (posting to it
/// corrupts the balance-sheet identity) and placeholders. Shared by the CSV
/// pickers and the import-rule editor.
pub(crate) fn ensure_postable_account(
    conn: &Connection,
    guid: &str,
    label: &str,
) -> Result<(), AppError> {
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT COALESCE(placeholder, 0), account_type FROM accounts WHERE guid = ?1",
            params![guid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(AppError::from)?;
    let Some((placeholder, account_type)) = row else {
        return Err(AppError::NotFound(format!(
            "{label} account not found. It may have been removed"
        )));
    };
    if account_type == "ROOT" {
        return Err(AppError::Validation(format!(
            "{label} account is the root account. Pick one below it"
        )));
    }
    if placeholder != 0 {
        return Err(AppError::Validation(format!(
            "{label} account just groups things. Pick one that holds money"
        )));
    }
    Ok(())
}

pub(super) fn validate_accounts(
    conn: &Connection,
    target: &str,
    offset: &str,
) -> Result<(), AppError> {
    if target.trim().is_empty() || offset.trim().is_empty() {
        return Err(AppError::Validation(
            "Pick both accounts for the import".to_string(),
        ));
    }
    if target == offset {
        return Err(AppError::Validation(
            "Pick two different accounts. Money needs somewhere to go".to_string(),
        ));
    }
    // The internal ROOT is not a real account: posting to it corrupts the
    // balance-sheet identity (the picker never offers it).
    ensure_postable_account(conn, target, "Target")?;
    ensure_postable_account(conn, offset, "Offset")?;
    Ok(())
}

/// Stable duplicate key: `date | value_num (4dp) | lower(trim(desc))`.
pub(super) fn csv_row_hash(post_date: &str, value_num: i64, description: &str) -> String {
    let normalized = format!(
        "{}|{}|{}",
        post_date,
        value_num,
        description.trim().to_lowercase()
    );
    utils::hex_digest(Sha256::digest(normalized.as_bytes()))
}

pub(super) fn amount_to_value_num(amount: f64) -> i64 {
    (amount * transactions::VALUE_DENOM as f64).round() as i64
}

/// Existing rows in the target account for the parsed date window, keyed by
/// [`csv_row_hash`] with the number of copies the book holds. Counts (not a
/// set) let a file that legitimately contains more copies of a same-day charge
/// than the book import the surplus.
pub(super) fn load_existing_counts(
    conn: &Connection,
    target_guid: &str,
    min_date: &str,
    max_date: &str,
) -> Result<HashMap<String, i64>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT substr(t.post_date, 1, 10), s.value_num, COALESCE(t.description, ''), COUNT(*)
             FROM splits s JOIN transactions t ON t.guid = s.tx_guid
             WHERE s.account_guid = ?1
               AND substr(t.post_date, 1, 10) BETWEEN ?2 AND ?3
             GROUP BY 1, 2, 3",
        )
        .map_err(AppError::from)?;
    let rows = stmt
        .query_map(params![target_guid, min_date, max_date], |row| {
            let date: String = row.get(0)?;
            let value_num: i64 = row.get(1)?;
            let desc: String = row.get(2)?;
            let count: i64 = row.get(3)?;
            Ok((date, value_num, desc, count))
        })
        .map_err(AppError::from)?;
    let mut out = HashMap::new();
    for row in rows {
        let (date, value_num, desc, count) = row.map_err(AppError::from)?;
        out.insert(csv_row_hash(&date, value_num, &desc), count);
    }
    Ok(out)
}
