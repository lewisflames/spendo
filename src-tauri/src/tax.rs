//! Tax-year summary: income, withholding and gains for one calendar year.
//!
//! Accounts carry a tax role (`tax_roles`, inherited by descendants). Income
//! and withholding are the year's activity in role accounts, taken from the
//! cash-flow monthly buckets so totals match that report. The annual return
//! (`tax_settlement`) is paid or refunded the year after, so settlement
//! postings in year Y+1 count toward year Y. Realized gains come from the
//! portfolio lots (`realized_gains_inner`), so they match Realized gains.
//! Expenses deducted from the base are the transactions carrying the book's
//! deductible tag (book setting `tax_deductible_tag`).

use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::fx;
use crate::reports::{self, AccountMeta};
use crate::{portfolio, settings, tags, utils};

/// Years between a tax year and the postings that settle its return.
pub const SETTLEMENT_LAG_YEARS: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxRole {
    /// Gross wages (INCOME).
    EmploymentIncome,
    /// Income tax withheld from wages (EXPENSE).
    EmploymentWithholding,
    /// Employee social security (EXPENSE).
    SocialContributions,
    /// Interest, dividends and other savings income, gross (INCOME).
    SavingsIncome,
    /// Tax withheld on savings income (EXPENSE).
    SavingsWithholding,
    /// Annual return payments (positive) and refunds (negative) (EXPENSE).
    TaxSettlement,
    /// Income that isn't taxed, e.g. gifts (INCOME).
    Exempt,
}

impl TaxRole {
    const ALL: [TaxRole; 7] = [
        TaxRole::EmploymentIncome,
        TaxRole::EmploymentWithholding,
        TaxRole::SocialContributions,
        TaxRole::SavingsIncome,
        TaxRole::SavingsWithholding,
        TaxRole::TaxSettlement,
        TaxRole::Exempt,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TaxRole::EmploymentIncome => "employment_income",
            TaxRole::EmploymentWithholding => "employment_withholding",
            TaxRole::SocialContributions => "social_contributions",
            TaxRole::SavingsIncome => "savings_income",
            TaxRole::SavingsWithholding => "savings_withholding",
            TaxRole::TaxSettlement => "tax_settlement",
            TaxRole::Exempt => "exempt",
        }
    }

    pub fn parse(value: &str) -> Option<TaxRole> {
        TaxRole::ALL.into_iter().find(|r| r.as_str() == value)
    }

    /// Account type the role applies to: income roles on INCOME accounts,
    /// the rest on EXPENSE accounts.
    fn account_type(self) -> &'static str {
        match self {
            TaxRole::EmploymentIncome | TaxRole::SavingsIncome | TaxRole::Exempt => "INCOME",
            _ => "EXPENSE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaxRoleAssignment {
    pub account_guid: String,
    pub role: TaxRole,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTaxRolesInput {
    pub roles: Vec<TaxRoleAssignment>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaxLine {
    pub account_guid: String,
    pub account_name: String,
    pub account_code: Option<String>,
    pub amount: f64,
}

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaxSection {
    /// Ordered by account code, then name.
    pub lines: Vec<TaxLine>,
    pub total: f64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RealizedTotals {
    pub gains: f64,
    pub losses: f64,
    pub net: f64,
    /// Lots consumed by sales in the year.
    pub disposals: i64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeductibleTotals {
    pub tag_guid: String,
    pub tag_name: String,
    /// Expense splits of the year's tagged transactions.
    pub total: f64,
    pub transactions: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxSummary {
    pub year: i32,
    /// Years with tax-relevant activity, ascending.
    pub years: Vec<i32>,
    pub report_currency: String,
    /// The year hasn't ended yet, so totals are year to date.
    pub is_partial: bool,
    /// Year whose postings settle this year's return.
    pub settlement_year: i32,
    pub cost_basis_method: String,
    pub employment_income: TaxSection,
    pub employment_withholding: TaxSection,
    pub social_contributions: TaxSection,
    pub savings_income: TaxSection,
    pub savings_withholding: TaxSection,
    pub settlement: TaxSection,
    pub exempt: TaxSection,
    pub realized: RealizedTotals,
    /// `None` while no deductible tag is set (or it was deleted).
    pub deductible: Option<DeductibleTotals>,
    /// Income accounts with activity this year and no tax role.
    pub untagged: Vec<TaxLine>,
    /// Employment + savings income + net realized gains when positive.
    pub gross_income: f64,
    /// Income tax: withholding on both bases plus the settlement.
    pub income_tax: f64,
    pub effective_rate: Option<f64>,
    pub missing_fx: Vec<String>,
}

/// Explicit role rows, account → role. Unknown role strings are skipped so a
/// newer book still opens in an older build.
fn load_roles(conn: &Connection) -> Result<HashMap<String, TaxRole>, AppError> {
    let mut stmt = conn.prepare("SELECT account_guid, role FROM tax_roles")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (guid, role) = row?;
        if let Some(role) = TaxRole::parse(&role) {
            out.insert(guid, role);
        }
    }
    Ok(out)
}

/// Role of `guid`: its own, else the nearest ancestor's.
fn effective_role(
    guid: &str,
    roles: &HashMap<String, TaxRole>,
    meta: &HashMap<String, AccountMeta>,
) -> Option<TaxRole> {
    if let Some(role) = roles.get(guid) {
        return Some(*role);
    }
    reports::ancestor_chain(guid, meta)
        .iter()
        .find_map(|a| roles.get(a).copied())
}

/// Accounts carrying a tax role, which the orphan sweep and account deletion
/// must not drop through the FK cascade.
pub(crate) fn role_account_guids(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT account_guid FROM tax_roles")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn fetch_tax_roles_inner(conn: &Connection) -> Result<Vec<TaxRoleAssignment>, AppError> {
    let mut roles: Vec<TaxRoleAssignment> = load_roles(conn)?
        .into_iter()
        .map(|(account_guid, role)| TaxRoleAssignment { account_guid, role })
        .collect();
    roles.sort_by(|a, b| a.account_guid.cmp(&b.account_guid));
    Ok(roles)
}

/// Replace every role assignment. Each account must exist and have the type
/// its role applies to.
fn save_tax_roles_inner(
    conn: &Connection,
    input: &SaveTaxRolesInput,
) -> Result<Vec<TaxRoleAssignment>, AppError> {
    let mut seen = HashSet::new();
    for r in &input.roles {
        if !seen.insert(r.account_guid.as_str()) {
            return Err(AppError::Validation(
                "Each account can have one tax role".to_string(),
            ));
        }
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT name, account_type FROM accounts WHERE guid = ?1",
                params![r.account_guid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((name, account_type)) = row else {
            return Err(AppError::NotFound(
                "One of those accounts is gone. Reload and try again".to_string(),
            ));
        };
        if account_type != r.role.account_type() {
            let kind = if r.role.account_type() == "INCOME" {
                "income"
            } else {
                "expense"
            };
            return Err(AppError::Validation(format!(
                "{name} can't take that role: it's for {kind} accounts"
            )));
        }
    }
    with_tx_err(conn, || {
        conn.execute("DELETE FROM tax_roles", [])?;
        let mut insert =
            conn.prepare_cached("INSERT INTO tax_roles (account_guid, role) VALUES (?1, ?2)")?;
        for r in &input.roles {
            insert.execute(params![r.account_guid, r.role.as_str()])?;
        }
        Ok::<_, AppError>(())
    })?;
    fetch_tax_roles_inner(conn)
}

/// Converted activity per account for one calendar year.
struct YearActivity {
    by_account: HashMap<String, f64>,
    missing_fx: Vec<String>,
    report_currency: String,
}

/// [`YearActivity`] from the cash-flow buckets (month-end FX).
fn year_activity(conn: &Connection, year: i32) -> Result<YearActivity, AppError> {
    let from = format!("{year:04}-01-01");
    let to = format!("{year:04}-12-31");
    let mut buckets = reports::fetch_monthly_buckets(conn, &from, &to)?;
    // Roles only live on income and expense accounts. Converting the rest
    // would run investment accounts (priced in a security, not a currency)
    // through FX and warn about rates that can never exist.
    buckets.retain(|b| b.account_type == "INCOME" || b.account_type == "EXPENSE");
    let (converted, missing, report) = reports::convert_buckets(conn, &buckets)?;
    let mut by_account: HashMap<String, f64> = HashMap::new();
    for b in converted {
        *by_account.entry(b.guid).or_insert(0.0) += b.amount;
    }
    Ok(YearActivity {
        by_account,
        missing_fx: missing,
        report_currency: report,
    })
}

fn line(guid: &str, amount: f64, meta: &HashMap<String, AccountMeta>) -> TaxLine {
    let m = meta.get(guid);
    TaxLine {
        account_guid: guid.to_string(),
        account_name: m
            .map(|m| m.name.clone())
            .unwrap_or_else(|| guid.to_string()),
        account_code: m.and_then(|m| m.code.clone()),
        amount,
    }
}

fn sort_lines(lines: &mut [TaxLine]) {
    lines.sort_by(|a, b| {
        (
            a.account_code.is_none(),
            &a.account_code,
            &a.account_name,
            &a.account_guid,
        )
            .cmp(&(
                b.account_code.is_none(),
                &b.account_code,
                &b.account_name,
                &b.account_guid,
            ))
    });
}

fn section(mut lines: Vec<TaxLine>) -> TaxSection {
    sort_lines(&mut lines);
    let total = lines.iter().map(|l| l.amount).sum();
    TaxSection { lines, total }
}

/// Expense total and transaction count of the year's transactions carrying
/// `tag_guid`, converted at month-end like the buckets.
fn deductible_totals(
    conn: &Connection,
    tag_guid: &str,
    year: i32,
    missing: &mut HashSet<String>,
) -> Result<(f64, i64), AppError> {
    let from = format!("{year:04}-01-01");
    let to = format!("{year:04}-12-31");
    let visible_now = utils::VISIBLE_NOW_EXPR;
    let window =
        format!("t.post_date >= ?2 AND t.post_date < min(date(?3, '+1 day'), {visible_now})");
    let tagged =
        "EXISTS(SELECT 1 FROM transaction_tags tt WHERE tt.tx_guid = t.guid AND tt.tag_guid = ?1)";

    let mut stmt = conn.prepare(&format!(
        "SELECT c.mnemonic, strftime('%Y-%m', t.post_date), SUM({amount})
         FROM splits s
         JOIN transactions t ON t.guid = s.tx_guid
         JOIN accounts a ON a.guid = s.account_guid
         LEFT JOIN commodities c ON c.guid = a.commodity_guid
         WHERE a.account_type = 'EXPENSE' AND {window} AND {tagged}
         GROUP BY c.mnemonic, strftime('%Y-%m', t.post_date)",
        amount = utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type"),
    ))?;
    let rows = stmt
        .query_map(params![tag_guid, from, to], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let report = fx::report_currency_or_empty(conn)?;
    let history = fx::FxHistory::load(conn)?;
    let mut total = 0.0;
    for (commodity, month, raw) in rows {
        total += match commodity.as_deref() {
            Some(m) => {
                let fx_at = history.map_for_date(&utils::month_end(&month)?);
                fx::convert_tracked(raw, m, &report, &fx_at, Some(missing))
            }
            None => raw,
        };
    }
    let count: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM transactions t WHERE {window} AND {tagged}"),
        params![tag_guid, from, to],
        |row| row.get(0),
    )?;
    Ok((total, count))
}

/// Years with a posting in a role account (settlements shifted back to the
/// year they settle) or a realized sale.
fn activity_years(
    conn: &Connection,
    roles: &HashMap<String, TaxRole>,
    meta: &HashMap<String, AccountMeta>,
    realized_years: &[i32],
) -> Result<Vec<i32>, AppError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT DISTINCT s.account_guid, CAST(strftime('%Y', t.post_date) AS INTEGER)
         FROM splits s JOIN transactions t ON t.guid = s.tx_guid
         WHERE {}",
        utils::VISIBLE_TX_FILTER
    ))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<i32>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut years: BTreeSet<i32> = realized_years.iter().copied().collect();
    for (guid, year) in rows {
        let Some(year) = year else { continue };
        match effective_role(&guid, roles, meta) {
            Some(TaxRole::TaxSettlement) => {
                years.insert(year - SETTLEMENT_LAG_YEARS);
            }
            Some(_) => {
                years.insert(year);
            }
            None => {}
        }
    }
    Ok(years.into_iter().collect())
}

pub(crate) fn fetch_tax_summary_inner(
    conn: &Connection,
    year: i32,
) -> Result<TaxSummary, AppError> {
    if !(1900..=2200).contains(&year) {
        return Err(AppError::Validation(format!(
            "That year doesn't look right: {year}"
        )));
    }
    let meta = reports::fetch_account_meta(conn)?;
    let roles = load_roles(conn)?;
    let settlement_year = year + SETTLEMENT_LAG_YEARS;

    let current = year_activity(conn, year)?;
    let settling = year_activity(conn, settlement_year)?;
    let report_currency = current.report_currency;
    let mut missing: HashSet<String> = current
        .missing_fx
        .into_iter()
        .chain(settling.missing_fx)
        .collect();

    let mut by_role: HashMap<TaxRole, Vec<TaxLine>> = HashMap::new();
    let mut untagged = Vec::new();
    for (guid, amount) in &current.by_account {
        if amount.abs() < 1e-9 {
            continue;
        }
        match effective_role(guid, &roles, &meta) {
            // A settlement posted this year belongs to the previous year.
            Some(TaxRole::TaxSettlement) => {}
            Some(role) => by_role
                .entry(role)
                .or_default()
                .push(line(guid, *amount, &meta)),
            None => {
                if meta.get(guid).is_some_and(|m| m.account_type == "INCOME") {
                    untagged.push(line(guid, *amount, &meta));
                }
            }
        }
    }
    let settlement_lines: Vec<TaxLine> = settling
        .by_account
        .iter()
        .filter(|(guid, amount)| {
            amount.abs() >= 1e-9
                && effective_role(guid, &roles, &meta) == Some(TaxRole::TaxSettlement)
        })
        .map(|(guid, amount)| line(guid, *amount, &meta))
        .collect();
    sort_lines(&mut untagged);
    let mut take = |role: TaxRole| section(by_role.remove(&role).unwrap_or_default());

    let gains = portfolio::realized_gains_inner(conn, year)?;
    let realized = RealizedTotals {
        gains: gains.total_gains,
        losses: gains.total_losses,
        net: gains.net_gain,
        disposals: gains.rows.len() as i64,
    };

    let book = settings::get_book_settings_inner(conn)?;
    let deductible = match book.tax_deductible_tag.as_str() {
        "" => None,
        guid => match tags::tag_name(conn, guid)? {
            Some(tag_name) => {
                let (total, transactions) = deductible_totals(conn, guid, year, &mut missing)?;
                Some(DeductibleTotals {
                    tag_guid: guid.to_string(),
                    tag_name,
                    total,
                    transactions,
                })
            }
            None => None,
        },
    };

    let employment_income = take(TaxRole::EmploymentIncome);
    let employment_withholding = take(TaxRole::EmploymentWithholding);
    let social_contributions = take(TaxRole::SocialContributions);
    let savings_income = take(TaxRole::SavingsIncome);
    let savings_withholding = take(TaxRole::SavingsWithholding);
    let exempt = take(TaxRole::Exempt);
    let settlement = section(settlement_lines);

    let gross_income = employment_income.total + savings_income.total + realized.net.max(0.0);
    let income_tax = employment_withholding.total + savings_withholding.total + settlement.total;
    let effective_rate = (gross_income > 0.0).then(|| income_tax / gross_income);
    let this_year: i32 = chrono::Datelike::year(&chrono::Local::now().date_naive());

    let mut missing_fx: Vec<String> = missing.into_iter().collect();
    missing_fx.sort();
    Ok(TaxSummary {
        year,
        years: activity_years(conn, &roles, &meta, &gains.years)?,
        report_currency,
        is_partial: year >= this_year,
        settlement_year,
        cost_basis_method: book.cost_basis_method,
        employment_income,
        employment_withholding,
        social_contributions,
        savings_income,
        savings_withholding,
        settlement,
        exempt,
        realized,
        deductible,
        untagged,
        gross_income,
        income_tax,
        effective_rate,
        missing_fx,
    })
}

#[tauri::command]
pub async fn fetch_tax_summary(
    state: State<'_, AppState>,
    year: i32,
) -> Result<TaxSummary, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_tax_summary_inner(conn, year)
    })
    .await
}

#[tauri::command]
pub async fn fetch_tax_roles(
    state: State<'_, AppState>,
) -> Result<Vec<TaxRoleAssignment>, AppError> {
    run_blocking(state.db.clone(), fetch_tax_roles_inner).await
}

#[tauri::command]
pub async fn save_tax_roles(
    state: State<'_, AppState>,
    input: SaveTaxRolesInput,
) -> Result<Vec<TaxRoleAssignment>, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        save_tax_roles_inner(conn, &input)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    /// The shape of a Spanish payroll book: gross salary with IRPF and
    /// social security split out, interest with its withholding, a gift, and
    /// the annual return settled the following June.
    /// (guid, post date, splits as (account, value in cents)).
    type SeedTx<'a> = (&'a str, &'a str, &'a [(&'a str, i64)]);

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        init_db(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY');
             INSERT INTO accounts (guid, name, account_type, parent_guid, code, placeholder, commodity_guid) VALUES
               ('root', 'Root', 'ROOT', NULL, NULL, 0, 'c-eur'),
               ('bank', 'bank', 'BANK', 'root', '1150', 0, 'c-eur'),
               ('income', 'income', 'INCOME', 'root', '4000', 1, 'c-eur'),
               ('payroll', 'payroll', 'INCOME', 'income', '4400', 0, 'c-eur'),
               ('interests', 'interests', 'INCOME', 'income', '4300', 1, 'c-eur'),
               ('revolut', 'revolut', 'INCOME', 'interests', '4330', 0, 'c-eur'),
               ('gifts', 'gifts', 'INCOME', 'income', '4200', 0, 'c-eur'),
               ('other', 'other', 'INCOME', 'income', '4900', 0, 'c-eur'),
               ('taxes', 'taxes', 'EXPENSE', 'root', '5600', 1, 'c-eur'),
               ('irpf', 'payroll-irpf', 'EXPENSE', 'taxes', '5640', 0, 'c-eur'),
               ('ss', 'payroll-ss', 'EXPENSE', 'taxes', '5650', 0, 'c-eur'),
               ('rev-irpf', 'revolut-irpf', 'EXPENSE', 'taxes', '5660', 0, 'c-eur'),
               ('settle', 'irpf-settlement', 'EXPENSE', 'taxes', '5670', 0, 'c-eur'),
               ('food', 'food', 'EXPENSE', 'root', '5300', 0, 'c-eur');",
        )
        .unwrap();
        let txs: &[SeedTx] = &[
            // Payroll: gross 2,260.62 = net 1,703.33 + IRPF 384.53 + SS 172.76.
            (
                "pay1",
                "2025-01-27",
                &[
                    ("bank", 170_333),
                    ("payroll", -226_062),
                    ("irpf", 38_453),
                    ("ss", 17_276),
                ],
            ),
            (
                "pay2",
                "2025-02-26",
                &[
                    ("bank", 170_333),
                    ("payroll", -226_062),
                    ("irpf", 38_453),
                    ("ss", 17_276),
                ],
            ),
            (
                "int1",
                "2025-03-22",
                &[("bank", 562), ("revolut", -694), ("rev-irpf", 132)],
            ),
            ("gift", "2025-05-01", &[("bank", 5_000), ("gifts", -5_000)]),
            ("misc", "2025-06-01", &[("bank", 1_000), ("other", -1_000)]),
            // 2024's return paid in 2025: must not count toward 2025.
            (
                "ret24",
                "2025-06-30",
                &[("bank", -63_577), ("settle", 63_577)],
            ),
            // 2025's return paid in 2026.
            (
                "ret25",
                "2026-06-30",
                &[("bank", -7_743), ("settle", 7_743)],
            ),
            ("lunch", "2025-07-01", &[("bank", -2_000), ("food", 2_000)]),
            ("lunch2", "2025-08-01", &[("bank", -500), ("food", 500)]),
        ];
        for (guid, date, splits) in txs {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![guid, date],
            )
            .unwrap();
            for (i, (account, value)) in splits.iter().enumerate() {
                conn.execute(
                    "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                     VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                    params![format!("{guid}-{i}"), guid, account, value],
                )
                .unwrap();
            }
        }
        let roles = [
            ("payroll", TaxRole::EmploymentIncome),
            ("irpf", TaxRole::EmploymentWithholding),
            ("ss", TaxRole::SocialContributions),
            // Set on the parent: `revolut` inherits it.
            ("interests", TaxRole::SavingsIncome),
            ("rev-irpf", TaxRole::SavingsWithholding),
            ("settle", TaxRole::TaxSettlement),
            ("gifts", TaxRole::Exempt),
        ];
        save_tax_roles_inner(
            &conn,
            &SaveTaxRolesInput {
                roles: roles
                    .iter()
                    .map(|(guid, role)| TaxRoleAssignment {
                        account_guid: guid.to_string(),
                        role: *role,
                    })
                    .collect(),
            },
        )
        .unwrap();
        conn
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn summarizes_one_year_with_next_years_settlement() {
        let conn = setup();
        let s = fetch_tax_summary_inner(&conn, 2025).unwrap();

        assert!(close(s.employment_income.total, 4_521.24));
        assert!(close(s.employment_withholding.total, 769.06));
        assert!(close(s.social_contributions.total, 345.52));
        assert!(close(s.savings_income.total, 6.94));
        assert_eq!(s.savings_income.lines[0].account_name, "revolut");
        assert!(close(s.savings_withholding.total, 1.32));
        assert!(close(s.exempt.total, 50.0));
        // The 2026-06-30 payment settles 2025; the 2025-06-30 one doesn't.
        assert_eq!(s.settlement_year, 2026);
        assert!(close(s.settlement.total, 77.43));
        assert!(close(s.income_tax, 769.06 + 1.32 + 77.43));
        assert!(close(s.gross_income, 4_521.24 + 6.94));
        assert!(close(
            s.effective_rate.unwrap(),
            s.income_tax / s.gross_income
        ));
        // Untagged income is flagged; expenses never are.
        assert_eq!(s.untagged.len(), 1);
        assert_eq!(s.untagged[0].account_name, "other");
        assert_eq!(s.years, vec![2024, 2025]);
        assert!(s.deductible.is_none());
    }

    #[test]
    fn refund_reduces_the_settlement() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO transactions (guid, post_date) VALUES ('ret23', '2024-04-16');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom) VALUES
               ('ret23-0', 'ret23', 'bank', 40152, 100, 40152, 100),
               ('ret23-1', 'ret23', 'settle', -40152, 100, -40152, 100);",
        )
        .unwrap();
        let s = fetch_tax_summary_inner(&conn, 2023).unwrap();
        assert!(close(s.settlement.total, -401.52));
        assert!(close(s.income_tax, -401.52));
        assert_eq!(s.effective_rate, None, "no income recorded for 2023");
    }

    #[test]
    fn deductible_tag_sums_expense_splits_of_tagged_transactions() {
        let conn = setup();
        tags::set_transaction_tags(&conn, "lunch", &["deductible".into()]).unwrap();
        let tag = tags::ensure_tag(&conn, "deductible").unwrap();
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES ('tax_deductible_tag', ?1)",
            [&tag],
        )
        .unwrap();
        let s = fetch_tax_summary_inner(&conn, 2025).unwrap();
        let d = s.deductible.unwrap();
        assert_eq!(d.tag_name, "deductible");
        assert!(close(d.total, 20.0));
        assert_eq!(d.transactions, 1);

        // A deleted tag reads as unset instead of failing the page.
        conn.execute("DELETE FROM tags", []).unwrap();
        assert!(fetch_tax_summary_inner(&conn, 2025)
            .unwrap()
            .deductible
            .is_none());
    }

    #[test]
    fn save_rejects_roles_on_the_wrong_account_type() {
        let conn = setup();
        let err = save_tax_roles_inner(
            &conn,
            &SaveTaxRolesInput {
                roles: vec![TaxRoleAssignment {
                    account_guid: "food".into(),
                    role: TaxRole::EmploymentIncome,
                }],
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
        // The failed save left the previous roles alone.
        assert_eq!(fetch_tax_roles_inner(&conn).unwrap().len(), 7);
    }

    #[test]
    fn investment_accounts_never_warn_about_fx() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-etf', 'IWDA.AS', 'ETF');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
               ('etf', 'ibkr-iwda', 'STOCK', 'root', 0, 'c-etf');
             INSERT INTO transactions (guid, post_date) VALUES ('buy', '2025-04-01');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom) VALUES
               ('buy-0', 'buy', 'bank', -10000, 100, -10000, 100),
               ('buy-1', 'buy', 'etf', 10000, 100, 100, 100);",
        )
        .unwrap();
        let s = fetch_tax_summary_inner(&conn, 2025).unwrap();
        assert!(s.missing_fx.is_empty(), "got {:?}", s.missing_fx);
    }

    #[test]
    fn future_rows_stay_out() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO transactions (guid, post_date) VALUES ('fut', '2099-01-01');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom) VALUES
               ('fut-0', 'fut', 'bank', 100000, 100, 100000, 100),
               ('fut-1', 'fut', 'payroll', -100000, 100, -100000, 100);",
        )
        .unwrap();
        let s = fetch_tax_summary_inner(&conn, 2099).unwrap();
        assert_eq!(s.employment_income.total, 0.0);
        assert!(!s.years.contains(&2099));
    }
}
