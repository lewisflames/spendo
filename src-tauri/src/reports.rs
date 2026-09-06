use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{AppState, run_blocking};
use crate::fx;
use crate::tree;
use crate::utils;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLossRow {
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub indent: i32,
    pub current_amount: f64,
    pub previous_amount: f64,
    pub change_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitLossReport {
    pub date_from: String,
    pub date_to: String,
    pub prev_date_from: String,
    pub prev_date_to: String,
    pub report_currency: String,
    pub income_rows: Vec<ProfitLossRow>,
    pub income_root_guid: Option<String>,
    pub total_income: f64,
    pub total_income_prev: f64,
    pub expense_rows: Vec<ProfitLossRow>,
    pub expense_root_guid: Option<String>,
    pub total_expenses: f64,
    pub total_expenses_prev: f64,
    pub net_income: f64,
    pub missing_fx: Vec<String>,
}

struct RawAccountBalance {
    guid: String,
    name: String,
    description: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
    balance: f64,
}

struct AccountMeta {
    name: String,
    description: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
}

/// Full account hierarchy for ancestor injection: every non-ROOT account with
/// its parent link. P&L buckets only carry accounts with in-window splits, so
/// without this the tree walker promotes children of inactive placeholders to
/// roots (flattened tables).
fn fetch_account_meta(conn: &Connection) -> Result<HashMap<String, AccountMeta>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT guid, name, description, account_type, parent_guid, placeholder
             FROM accounts WHERE account_type != 'ROOT'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                AccountMeta {
                    name: row.get(1)?,
                    description: row.get(2)?,
                    account_type: row.get(3)?,
                    parent_guid: row.get(4)?,
                    placeholder: row.get::<_, i64>(5)? != 0,
                },
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn is_pnl_type(account_type: &str) -> bool {
    account_type == "INCOME" || account_type == "EXPENSE"
}

/// Ancestor chain of `guid` through `meta`, stopping at ROOT/missing parents.
fn ancestor_chain(guid: &str, meta: &HashMap<String, AccountMeta>) -> Vec<String> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut next = meta.get(guid).and_then(|m| m.parent_guid.clone());
    while let Some(parent) = next {
        if !seen.insert(parent.clone()) {
            break;
        }
        let Some(parent_meta) = meta.get(&parent) else {
            break;
        };
        chain.push(parent.clone());
        next = parent_meta.parent_guid.clone();
    }
    chain
}

/// Expand P&L row sets so inactive placeholder parents render with rolled-up
/// totals instead of flattening children to roots. Only ancestors of active
/// INCOME/EXPENSE accounts are added (zero-only branches stay hidden). Both
/// windows are expanded to the union of active guids so prev-only accounts
/// still render with a zero current amount (and vice versa); per-window
/// balances stay intact and root sums are unchanged.
fn augment_pnl_rows(
    current: Vec<RawAccountBalance>,
    prev: Vec<RawAccountBalance>,
    meta: &HashMap<String, AccountMeta>,
) -> (Vec<RawAccountBalance>, Vec<RawAccountBalance>) {
    let mut wanted: HashSet<String> = HashSet::new();
    for r in current.iter().chain(prev.iter()) {
        if is_pnl_type(&r.account_type) {
            wanted.insert(r.guid.clone());
        }
    }
    for guid in wanted.clone() {
        wanted.extend(ancestor_chain(&guid, meta));
    }

    let mut cur_map: HashMap<String, RawAccountBalance> =
        current.into_iter().map(|r| (r.guid.clone(), r)).collect();
    let mut prev_map: HashMap<String, RawAccountBalance> =
        prev.into_iter().map(|r| (r.guid.clone(), r)).collect();
    for guid in &wanted {
        for map in [&mut cur_map, &mut prev_map] {
            if !map.contains_key(guid) {
                if let Some(m) = meta.get(guid) {
                    map.insert(
                        guid.clone(),
                        RawAccountBalance {
                            guid: guid.clone(),
                            name: m.name.clone(),
                            description: m.description.clone(),
                            account_type: m.account_type.clone(),
                            parent_guid: m.parent_guid.clone(),
                            placeholder: m.placeholder,
                            balance: 0.0,
                        },
                    );
                }
            }
        }
    }

    let mut current: Vec<RawAccountBalance> = cur_map.into_values().collect();
    let mut prev: Vec<RawAccountBalance> = prev_map.into_values().collect();
    current.sort_by(|a, b| a.name.cmp(&b.name));
    prev.sort_by(|a, b| a.name.cmp(&b.name));
    (current, prev)
}

fn build_tree_adjacency(rows: &[RawAccountBalance], account_types: &[&str]) -> tree::TreeIndex {
    let included: Vec<&RawAccountBalance> = rows
        .iter()
        .filter(|r| account_types.contains(&r.account_type.as_str()))
        .collect();

    // Drop top-level placeholders whose parent is outside the filtered set:
    // they only group children and would render as empty roots otherwise.
    let guids: HashSet<&str> = included.iter().map(|r| r.guid.as_str()).collect();
    let filtered: Vec<&RawAccountBalance> = included
        .into_iter()
        .filter(|r| {
            !(r.placeholder
                && r.parent_guid
                    .as_ref()
                    .is_none_or(|p| !guids.contains(p.as_str())))
        })
        .collect();

    let triples = filtered
        .iter()
        .map(|r| (r.guid.clone(), r.parent_guid.clone(), r.name.clone()));
    tree::build_index(triples)
}

/// One account's signed activity in one calendar month. Sign follows
/// `signed_amount_expr` (credit-normal types negated, so INCOME reads
/// positive like everywhere else).
struct MonthlyBucket {
    guid: String,
    name: String,
    description: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
    commodity: Option<String>,
    month: String,
    raw: f64,
}

/// Per-account activity per calendar month within `[date_from, date_to]`
/// (half-open, visibility-capped like every money query). Only accounts with
/// in-window splits appear — zero-activity accounts contribute nothing to P&L
/// trees, so omitting them just drops €0 rows.
fn fetch_monthly_buckets(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
) -> Result<Vec<MonthlyBucket>, String> {
    let amount = utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type");
    let visible_now = utils::VISIBLE_NOW_EXPR;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT a.guid, a.name, a.description, a.account_type, a.parent_guid, a.placeholder,
                    c.mnemonic as commodity, strftime('%Y-%m', t.post_date) as month,
                    SUM({amount}) as total
             FROM splits s
             JOIN transactions t ON t.guid = s.tx_guid
             JOIN accounts a ON a.guid = s.account_guid
             LEFT JOIN commodities c ON c.guid = a.commodity_guid
             WHERE a.account_type != 'ROOT'
               AND t.post_date >= ?1
               AND t.post_date < min(date(?2, '+1 day'), {visible_now})
             GROUP BY a.guid, a.name, a.description, a.account_type, a.parent_guid,
                      a.placeholder, c.mnemonic, month
             ORDER BY month, a.name"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([date_from, date_to], |row| {
            Ok(MonthlyBucket {
                guid: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                account_type: row.get(3)?,
                parent_guid: row.get(4)?,
                placeholder: row.get::<_, i64>(5)? != 0,
                commodity: row.get(6)?,
                month: row.get(7)?,
                raw: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Last calendar day of a `YYYY-MM` month, for month-end FX lookups.
fn month_end(month: &str) -> Result<String, String> {
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

/// A monthly bucket converted to report currency at its month-end rate.
struct ConvertedBucket {
    guid: String,
    name: String,
    description: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
    commodity: Option<String>,
    month: String,
    amount: f64,
}

/// Convert buckets at month-end rates via a single `FxHistory` load (one rate
/// map per distinct month). Returns converted buckets, the sorted missing-FX
/// list and report currency. Zero buckets convert to zero at any rate and
/// never warn.
fn convert_buckets(
    conn: &Connection,
    buckets: &[MonthlyBucket],
) -> Result<(Vec<ConvertedBucket>, Vec<String>, String), String> {
    let report = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_history = fx::FxHistory::load(conn)?;
    let mut missing = HashSet::new();
    let mut maps: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut out = Vec::with_capacity(buckets.len());
    for b in buckets {
        if b.raw == 0.0 {
            continue;
        }
        if !maps.contains_key(&b.month) {
            let end = month_end(&b.month)?;
            maps.insert(b.month.clone(), fx_history.map_for_date(&end));
        }
        let fx_at = &maps[&b.month];
        let amount = match b.commodity.as_deref() {
            Some(m) => convert_amount(b.raw, m, &report, fx_at, Some(&mut missing)),
            None => b.raw,
        };
        out.push(ConvertedBucket {
            guid: b.guid.clone(),
            name: b.name.clone(),
            description: b.description.clone(),
            account_type: b.account_type.clone(),
            parent_guid: b.parent_guid.clone(),
            placeholder: b.placeholder,
            commodity: b.commodity.clone(),
            month: b.month.clone(),
            amount,
        });
    }
    let mut missing: Vec<String> = missing.into_iter().collect();
    missing.sort();
    Ok((out, missing, report))
}

/// Roll converted buckets up to per-account totals for the tree walker,
/// ordered by account name like the old balance query.
fn roll_up_buckets(buckets: &[ConvertedBucket]) -> Vec<RawAccountBalance> {
    struct Acc<'a> {
        name: &'a str,
        description: Option<&'a str>,
        account_type: &'a str,
        parent_guid: Option<&'a str>,
        placeholder: bool,
        total: f64,
    }
    let mut map: HashMap<&str, Acc> = HashMap::new();
    for b in buckets {
        let entry = map.entry(b.guid.as_str()).or_insert(Acc {
            name: b.name.as_str(),
            description: b.description.as_deref(),
            account_type: b.account_type.as_str(),
            parent_guid: b.parent_guid.as_deref(),
            placeholder: b.placeholder,
            total: 0.0,
        });
        entry.total += b.amount;
    }
    let mut rows: Vec<RawAccountBalance> = map
        .into_iter()
        .map(|(guid, a)| RawAccountBalance {
            guid: guid.to_string(),
            name: a.name.to_string(),
            description: a.description.map(|d| d.to_string()),
            account_type: a.account_type.to_string(),
            parent_guid: a.parent_guid.map(|p| p.to_string()),
            placeholder: a.placeholder,
            balance: a.total,
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

/// Convert `amount` from `from` to the report currency, degrading to 1.0
/// parity for currencies without a rate. Pass `missing` to record gaps
/// (report warnings) or `None` for the silent investment cost leg, which is
/// either priced (market dominates) or flagged via `missing_prices` instead.
fn convert_amount(
    amount: f64,
    from: &str,
    report: &str,
    fx_map: &HashMap<String, f64>,
    missing: Option<&mut HashSet<String>>,
) -> f64 {
    // A zero balance converts to zero at any rate, so it can never skew a
    // total — don't flag its currency.
    if amount == 0.0 {
        return amount;
    }
    if report.is_empty() || from == report {
        return amount;
    }
    match fx_map.get(from) {
        Some(rate) => amount * rate,
        None => {
            if let Some(set) = missing {
                set.insert(from.to_string());
            }
            amount
        }
    }
}

fn walk_tree<R>(
    rows_by_guid: &HashMap<&str, &RawAccountBalance>,
    children: &HashMap<String, Vec<String>>,
    guid: &str,
    indent: i32,
    result: &mut Vec<R>,
    push_row: &impl Fn(&RawAccountBalance, i32, f64) -> R,
    visited: &mut HashSet<String>,
) -> f64 {
    if !visited.insert(guid.to_string()) {
        return 0.0;
    }
    let Some(r) = rows_by_guid.get(guid) else {
        return 0.0;
    };
    // Pre-order: parents precede children so the frontend can rebuild the
    // tree in one forward pass (`report-tree.ts:buildTree` contract). Children
    // accumulate into a scratch vec, then the rolled-up parent is emitted
    // first followed by its subtree.
    let mut total = r.balance;
    let mut child_rows = Vec::new();
    if let Some(kids) = children.get(guid) {
        for kid in kids {
            total += walk_tree(
                rows_by_guid,
                children,
                kid,
                indent + 1,
                &mut child_rows,
                push_row,
                visited,
            );
        }
    }
    result.push(push_row(r, indent, total));
    result.extend(child_rows);
    total
}

fn build_tree<R>(
    rows: &[RawAccountBalance],
    account_types: &[&str],
    push_row: impl Fn(&RawAccountBalance, i32, f64) -> R,
) -> Vec<R> {
    let index = build_tree_adjacency(rows, account_types);
    let rows_by_guid: HashMap<&str, &RawAccountBalance> =
        rows.iter().map(|r| (r.guid.as_str(), r)).collect();
    let mut result = Vec::new();
    let mut visited = HashSet::new();
    for root in &index.roots {
        walk_tree(
            &rows_by_guid,
            &index.children,
            root,
            0,
            &mut result,
            &push_row,
            &mut visited,
        );
    }
    result
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSheetRow {
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub indent: i32,
    pub amount: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSheetReport {
    pub as_of: String,
    pub report_currency: String,
    pub asset_rows: Vec<BalanceSheetRow>,
    pub liability_rows: Vec<BalanceSheetRow>,
    pub equity_rows: Vec<BalanceSheetRow>,
    pub total_assets: f64,
    pub total_liabilities: f64,
    pub total_equity: f64,
    pub net_income: f64,
    pub unrealized_gains: f64,
    pub balance_check: f64,
    pub missing_fx: Vec<String>,
    pub missing_prices: Vec<MissingPrice>,
}

const ASSET_TYPES: &[&str] = &[
    "ASSET",
    "BANK",
    "CASH",
    "STOCK",
    "MUTUAL",
    "FUND",
    "CURRENCY",
    "RECEIVABLE",
];
const LIABILITY_TYPES: &[&str] = &["LIABILITY", "CREDIT", "PAYABLE"];
// TRADING accounts (GnuCash currency/commodity trading books) accumulate
// trading gains/losses, which are equity-like. They must appear somewhere:
// silently dropping any type breaks the accounting identity.
const EQUITY_TYPES: &[&str] = &["EQUITY", "TRADING"];

struct ComputedBalance {
    guid: String,
    name: String,
    description: Option<String>,
    account_type: String,
    parent_guid: Option<String>,
    placeholder: bool,
    amount: f64,
    /// Market minus FX-converted cost for priced investments, else 0.
    /// Mark-to-market lifts assets with no offsetting entry, so the sheet
    /// needs this back as an equity line (like GnuCash's Unrealized Gains).
    unrealized: f64,
}

/// A security held in the book with no cached Yahoo price. Distinct from a
/// missing FX rate: refreshing only helps if the symbol resolves, otherwise
/// the user must set an explicit symbol override on the holding.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingPrice {
    pub mnemonic: String,
    pub commodity_guid: Option<String>,
}

/// Cumulative balances as of `as_of` with investment mark-to-market (mirrors
/// `accounts.rs`) and FX conversion. Returns rows, sorted missing-FX list,
/// sorted missing-price list and report currency.
fn compute_balances_as_of(
    conn: &Connection,
    as_of: &str,
) -> Result<
    (
        Vec<ComputedBalance>,
        Vec<String>,
        Vec<MissingPrice>,
        String,
    ),
    String,
> {
    use crate::prices::get_cached_prices_map;

    let visible_now = utils::VISIBLE_NOW_EXPR;
    let raw = utils::RAW_AMOUNT;
    let qty = utils::QUANTITY_AMOUNT;
    // Cumulative: everything strictly before the day after `as_of`, capped at
    // the shared visibility window so a future `as_of` never leaks future txs.
    let balance_case =
        format!("CASE WHEN t.post_date IS NULL THEN 0 WHEN t.post_date < min(date(?1, '+1 day'), {visible_now}) THEN {raw} ELSE 0 END");
    let quantity_case =
        format!("CASE WHEN t.post_date IS NULL THEN 0 WHEN t.post_date < min(date(?1, '+1 day'), {visible_now}) THEN {qty} ELSE 0 END");

    let sql = format!(
        "SELECT a.guid, a.name, a.description, a.account_type, a.parent_guid, a.placeholder,
                c.mnemonic as commodity, c.guid as commodity_guid,
                COALESCE(SUM({balance_case}), 0) as balance_raw,
                COALESCE(SUM({quantity_case}), 0) as quantity_raw
         FROM accounts a
         LEFT JOIN commodities c ON c.guid = a.commodity_guid
         LEFT JOIN splits s ON s.account_guid = a.guid
         LEFT JOIN transactions t ON t.guid = s.tx_guid
         WHERE a.account_type != 'ROOT'
         GROUP BY a.guid, a.name, a.description, a.account_type, a.parent_guid, a.placeholder, c.mnemonic, c.guid
         ORDER BY a.name"
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows: Vec<(
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        i64,
        Option<String>,
        Option<String>,
        f64,
        f64,
    )> = stmt
        .query_map([as_of], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
                row.get(9)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let report = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_map = fx::get_fx_map(conn, Some(as_of))?;
    let prices = get_cached_prices_map(conn).unwrap_or_default();
    let mut missing = HashSet::new();
    let mut missing_prices: HashMap<String, Option<String>> = HashMap::new();

    // The investment cost leg converts silently (`None`): the holding is
    // either priced (market dominates, cost is only the unrealized baseline)
    // or flagged in `missing_prices`. Either way it must never pollute the
    // genuine-currency `missing` FX set with security mnemonics.

    let mut out = Vec::with_capacity(rows.len());
    for (
        guid,
        name,
        description,
        account_type,
        parent_guid,
        placeholder,
        commodity,
        commodity_guid,
        balance_raw,
        quantity_raw,
    ) in rows
    {
        let signed_balance = if utils::is_credit_normal(&account_type) {
            -balance_raw
        } else {
            balance_raw
        };
        let (amount, unrealized) = if utils::INVESTMENT_TYPES.contains(&account_type.as_str()) {
            let signed_qty = if utils::is_credit_normal(&account_type) {
                -quantity_raw
            } else {
                quantity_raw
            };
            match commodity.as_deref().and_then(|m| prices.get(m)) {
                Some(price) => {
                    let market = signed_qty * price;
                    let cost = match commodity.as_deref() {
                        Some(m) => convert_amount(signed_balance, m, &report, &fx_map, None),
                        None => signed_balance,
                    };
                    (market, market - cost)
                }
                None => {
                    // No cached Yahoo price: fall back to cost basis so the
                    // sheet stays complete, and flag the security (not an FX
                    // gap) so the UI can point at the holding's symbol.
                    if let Some(m) = commodity.as_deref() {
                        missing_prices.entry(m.to_string()).or_insert(commodity_guid);
                        (
                            convert_amount(signed_balance, m, &report, &fx_map, None),
                            0.0,
                        )
                    } else {
                        (signed_balance, 0.0)
                    }
                }
            }
        } else {
            let cost = match commodity.as_deref() {
                Some(m) => convert_amount(signed_balance, m, &report, &fx_map, Some(&mut missing)),
                None => signed_balance,
            };
            (cost, 0.0)
        };
        out.push(ComputedBalance {
            guid,
            name,
            description,
            account_type,
            parent_guid,
            placeholder: placeholder != 0,
            amount,
            unrealized,
        });
    }

    let mut missing: Vec<String> = missing.into_iter().collect();
    missing.sort();
    let mut missing_prices: Vec<MissingPrice> = missing_prices
        .into_iter()
        .map(|(mnemonic, commodity_guid)| MissingPrice {
            mnemonic,
            commodity_guid,
        })
        .collect();
    missing_prices.sort_by(|a, b| a.mnemonic.cmp(&b.mnemonic));
    Ok((out, missing, missing_prices, report))
}

fn generate_balance_sheet_inner(
    conn: &Connection,
    as_of: &str,
) -> Result<BalanceSheetReport, String> {
    let (rows, missing_fx, missing_prices, report_currency) =
        compute_balances_as_of(conn, as_of)?;

    // walk_tree sums `balance` fields; ComputedBalance has `amount`, so adapt
    // via a RawAccountBalance view for the generic walker.
    let raw_view: Vec<RawAccountBalance> = rows
        .iter()
        .map(|r| RawAccountBalance {
            guid: r.guid.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            account_type: r.account_type.clone(),
            parent_guid: r.parent_guid.clone(),
            placeholder: r.placeholder,
            balance: r.amount,
        })
        .collect();
    let push_raw = |r: &RawAccountBalance, indent: i32, total: f64| -> BalanceSheetRow {
        BalanceSheetRow {
            account_guid: r.guid.clone(),
            account_name: r.name.clone(),
            account_description: r.description.clone(),
            indent,
            amount: total,
        }
    };
    let asset_rows = build_tree(&raw_view, ASSET_TYPES, push_raw);
    let liability_rows = build_tree(&raw_view, LIABILITY_TYPES, push_raw);
    let mut equity_rows = build_tree(&raw_view, EQUITY_TYPES, push_raw);

    // Unrealized mark-to-market gains rolled up like every other total:
    // per-account values double count children, so sum top-level rows only.
    let unreal_view: Vec<RawAccountBalance> = rows
        .iter()
        .map(|r| RawAccountBalance {
            guid: r.guid.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            account_type: r.account_type.clone(),
            parent_guid: r.parent_guid.clone(),
            placeholder: r.placeholder,
            balance: r.unrealized,
        })
        .collect();
    let unrealized_gains: f64 = build_tree(&unreal_view, &utils::INVESTMENT_TYPES, push_raw)
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.amount)
        .sum();
    // Synthetic equity row. Empty guid is the "no account page" contract (the
    // frontend renders it as plain text); it is always a leaf so it never
    // enters the expand set, and the table keys rows by guid+name so it can't
    // collide with a real account.
    equity_rows.push(BalanceSheetRow {
        account_guid: String::new(),
        account_name: "Unrealized gains".to_string(),
        account_description: Some("Mark-to-market above cost".to_string()),
        indent: 0,
        amount: unrealized_gains,
    });

    let sum_roots = |rows: &[BalanceSheetRow]| -> f64 {
        rows.iter()
            .filter(|r| r.indent == 0)
            .map(|r| r.amount)
            .sum()
    };
    let total_assets = sum_roots(&asset_rows);
    let total_liabilities = sum_roots(&liability_rows);
    let total_equity = sum_roots(&equity_rows);

    // Cumulative income minus expenses = retained earnings; needed for the
    // accounting check since INCOME/EXPENSE are excluded from the trees.
    // Per-account amounts double count children, so roll up via the same tree
    // walker and sum only top-level rows.
    let income_tree = build_tree(&raw_view, &["INCOME"], push_raw);
    let expense_tree = build_tree(&raw_view, &["EXPENSE"], push_raw);
    let income_rolled: f64 = income_tree
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.amount)
        .sum();
    let expense_rolled: f64 = expense_tree
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.amount)
        .sum();
    let net_income = income_rolled - expense_rolled;
    let balance_check =
        total_assets - (total_liabilities + total_equity + net_income);

    Ok(BalanceSheetReport {
        as_of: as_of.to_string(),
        report_currency,
        asset_rows,
        liability_rows,
        equity_rows,
        total_assets,
        total_liabilities,
        total_equity,
        net_income,
        unrealized_gains,
        balance_check,
        missing_fx,
        missing_prices,
    })
}

/// Percent change for P&L rows. `None` when both periods are zero; a new
/// account with no prior balance reports 100% (matches the frontend
/// `changePct` in `pnl-utils.ts` — keep the two in sync).
fn pnl_change_pct(current: f64, previous: f64) -> Option<f64> {
    if previous != 0.0 {
        Some((current - previous) / previous * 100.0)
    } else if current != 0.0 {
        Some(100.0)
    } else {
        None
    }
}

fn generate_pnl_report_inner(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
    prev_date_from: Option<&str>,
    prev_date_to: Option<&str>,
) -> Result<ProfitLossReport, String> {
    if date_from > date_to {
        return Err("dateFrom must not be after dateTo".to_string());
    }
    let cur_buckets = fetch_monthly_buckets(conn, date_from, date_to)?;
    let (converted_cur, missing_current, report_currency) = convert_buckets(conn, &cur_buckets)?;
    let current_rows = roll_up_buckets(&converted_cur);

    let push_pnl = |r: &RawAccountBalance, indent: i32, total: f64| -> ProfitLossRow {
        ProfitLossRow {
            account_guid: r.guid.clone(),
            account_name: r.name.clone(),
            account_description: r.description.clone(),
            indent,
            current_amount: total,
            previous_amount: 0.0,
            change_pct: None,
        }
    };

    // Previous period — subtract span to get same-length prior window
    let (prev_date_from_owned, prev_date_to_owned) = match (prev_date_from, prev_date_to) {
        (Some(pf), Some(pt)) => (pf.to_string(), pt.to_string()),
        (Some(pf), None) => (pf.to_string(), date_from.to_string()),
        (None, Some(pt)) => (date_from.to_string(), pt.to_string()),
        (None, None) => {
            let f = utils::month_number(date_from)?;
            let t = utils::month_number(date_to)?;
            let span = t - f;
            let prev_start = utils::format_month(f - span - 1);
            let prev_end = utils::format_month(f - 1);
            (prev_start, prev_end)
        }
    };

    let prev_buckets =
        fetch_monthly_buckets(conn, &prev_date_from_owned, &prev_date_to_owned)?;
    let (converted_prev, missing_prev, report_currency_prev) =
        convert_buckets(conn, &prev_buckets)?;
    let prev_rows = roll_up_buckets(&converted_prev);

    // Re-attach inactive placeholder parents (and union prev-only accounts)
    // so the trees keep their hierarchy instead of promoting children to
    // roots. Totals are unaffected: injected rows carry zero balances.
    let meta = fetch_account_meta(conn)?;
    let (current_rows, prev_rows) = augment_pnl_rows(current_rows, prev_rows, &meta);

    let income_rows = build_tree(&current_rows, &["INCOME"], push_pnl);
    let expense_rows = build_tree(&current_rows, &["EXPENSE"], push_pnl);

    let income_total: f64 = income_rows
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.current_amount)
        .sum();
    let expense_total: f64 = expense_rows
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.current_amount)
        .sum();

    // Union of currencies missing in either window; either window converts at
    // 1.0 parity, so either can skew its totals. Only currencies on
    // INCOME/EXPENSE buckets matter here: the trees below drop non-P&L types,
    // so their mnemonics must not raise a warning.
    let mut missing_set: HashSet<String> = missing_current.into_iter().collect();
    missing_set.extend(missing_prev);
    let pnl_currencies: HashSet<&str> = converted_cur
        .iter()
        .chain(converted_prev.iter())
        .filter(|b| b.account_type == "INCOME" || b.account_type == "EXPENSE")
        .filter_map(|b| b.commodity.as_deref())
        .collect();
    let mut missing_fx: Vec<String> = missing_set
        .into_iter()
        .filter(|m| pnl_currencies.contains(m.as_str()))
        .collect();
    missing_fx.sort();
    let report_currency = if report_currency.is_empty() {
        report_currency_prev
    } else {
        report_currency
    };

    let prev_income_rows = build_tree(&prev_rows, &["INCOME"], push_pnl);
    let prev_expense_rows = build_tree(&prev_rows, &["EXPENSE"], push_pnl);

    let mut prev_income_map: HashMap<String, f64> = HashMap::new();
    for r in &prev_income_rows {
        prev_income_map.insert(r.account_guid.clone(), r.current_amount);
    }
    let mut prev_expense_map: HashMap<String, f64> = HashMap::new();
    for r in &prev_expense_rows {
        prev_expense_map.insert(r.account_guid.clone(), r.current_amount);
    }

    fn merge_prev_amount(mut r: ProfitLossRow, prev_map: &HashMap<String, f64>) -> ProfitLossRow {
        r.previous_amount = *prev_map.get(&r.account_guid).unwrap_or(&0.0);
        r.change_pct = pnl_change_pct(r.current_amount, r.previous_amount);
        r
    }

    let income_rows: Vec<ProfitLossRow> = income_rows
        .into_iter()
        .map(|r| merge_prev_amount(r, &prev_income_map))
        .collect();

    let expense_rows: Vec<ProfitLossRow> = expense_rows
        .into_iter()
        .map(|r| merge_prev_amount(r, &prev_expense_map))
        .collect();

    let income_prev_total: f64 = income_rows
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.previous_amount)
        .sum();
    let expense_prev_total: f64 = expense_rows
        .iter()
        .filter(|r| r.indent == 0)
        .map(|r| r.previous_amount)
        .sum();

    let income_root_guid = income_rows
        .iter()
        .find(|r| r.indent == 0)
        .map(|r| r.account_guid.clone());
    let expense_root_guid = expense_rows
        .iter()
        .find(|r| r.indent == 0)
        .map(|r| r.account_guid.clone());

    Ok(ProfitLossReport {
        date_from: date_from.to_string(),
        date_to: date_to.to_string(),
        prev_date_from: prev_date_from_owned,
        prev_date_to: prev_date_to_owned,
        report_currency,
        income_rows,
        income_root_guid,
        total_income: income_total,
        total_income_prev: income_prev_total,
        expense_rows,
        expense_root_guid,
        total_expenses: expense_total,
        total_expenses_prev: expense_prev_total,
        net_income: income_total - expense_total,
        missing_fx,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlTrendPoint {
    pub month: String,
    pub income: f64,
    pub expenses: f64,
}

/// Monthly income/expense totals over `[date_from, date_to]` for the P&L
/// trend chart. Shares the monthly-bucket engine (and month-end FX) with the
/// statement, so chart and table always agree. Months without activity are
/// zero-filled for a continuous series.
fn generate_pnl_trend_inner(
    conn: &Connection,
    date_from: &str,
    date_to: &str,
) -> Result<Vec<PnlTrendPoint>, String> {
    if date_from > date_to {
        return Err("dateFrom must not be after dateTo".to_string());
    }
    let buckets = fetch_monthly_buckets(conn, date_from, date_to)?;
    let (converted, _, _) = convert_buckets(conn, &buckets)?;
    let mut per_month: HashMap<String, (f64, f64)> = HashMap::new();
    for b in &converted {
        if b.account_type == "INCOME" {
            per_month.entry(b.month.clone()).or_default().0 += b.amount;
        } else if b.account_type == "EXPENSE" {
            per_month.entry(b.month.clone()).or_default().1 += b.amount;
        }
    }
    let start = utils::month_number(date_from)?;
    let end = utils::month_number(date_to)?;
    let mut points = Vec::with_capacity((end - start + 1).max(0) as usize);
    for m in start..=end {
        let month = utils::format_month(m);
        let (income, expenses) = per_month.get(&month).copied().unwrap_or((0.0, 0.0));
        points.push(PnlTrendPoint {
            month,
            income,
            expenses,
        });
    }
    Ok(points)
}

#[tauri::command]
pub async fn generate_pnl_report(
    state: State<'_, AppState>,
    date_from: String,
    date_to: String,
    prev_date_from: Option<String>,
    prev_date_to: Option<String>,
) -> Result<ProfitLossReport, String> {
    run_blocking(state.db.clone(), move |conn| {
        generate_pnl_report_inner(
            conn,
            &date_from,
            &date_to,
            prev_date_from.as_deref(),
            prev_date_to.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub async fn generate_pnl_trend(
    state: State<'_, AppState>,
    date_from: String,
    date_to: String,
) -> Result<Vec<PnlTrendPoint>, String> {
    run_blocking(state.db.clone(), move |conn| {
        generate_pnl_trend_inner(conn, &date_from, &date_to)
    })
    .await
}

#[tauri::command]
pub async fn generate_balance_sheet(
    state: State<'_, AppState>,
    as_of: String,
) -> Result<BalanceSheetReport, String> {
    run_blocking(state.db.clone(), move |conn| {
        generate_balance_sheet_inner(conn, &as_of)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use rusqlite::params;

    fn setup_balance_sheet() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES
             ('c-eur', 'EUR', 'CURRENCY'), ('c-usd', 'USD', 'CURRENCY'),
             ('c-aapl', 'AAPL', 'NASDAQ')",
            [],
        )
        .unwrap();
        // ROOT commodity sets the report currency to EUR.
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur'),
                    ('loan', 'Loan', 'LIABILITY', 'root', 0, 'c-eur'),
                    ('equity', 'Opening', 'EQUITY', 'root', 0, 'c-eur'),
                    ('salary', 'Salary', 'INCOME', 'root', 0, 'c-eur'),
                    ('rent', 'Rent', 'EXPENSE', 'root', 0, 'c-eur'),
                    ('broker', 'Broker', 'STOCK', 'root', 0, 'c-aapl')",
            [],
        )
        .unwrap();
        // Opening equity 1000 -> checking; salary 2000 -> checking; rent 500.
        for (tag, date, debit, credit, amount) in [
            ("t0", "2024-01-01", "checking", "equity", 100_000i64),
            ("t1", "2024-02-01", "checking", "salary", 200_000i64),
            ("t2", "2024-03-01", "rent", "checking", 50_000i64),
        ] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![tag, date],
            )
            .unwrap();
            // Double entry like GnuCash: debit positive, credit negative.
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                params![format!("s-{tag}-d"), tag, debit, amount],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                params![format!("s-{tag}-c"), tag, credit, -amount],
            )
            .unwrap();
        }
        // Buy 10 AAPL @ 100 (value +1000, quantity +10) funded from checking.
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t3', '2024-04-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-t3-d', 't3', 'broker', 100000, 100, 1000, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-t3-c', 't3', 'checking', -100000, 100, -100000, 100)",
            [],
        )
        .unwrap();
        // AAPL marked at 120: 10 shares -> market 1200, cost 1000.
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES ('c-aapl-2024-06-01', 'c-aapl', 'c-eur', 12000, 100, '2024-06-01')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn balance_sheet_identity_holds_with_mark_to_market() {
        let conn = setup_balance_sheet();
        let report = generate_balance_sheet_inner(&conn, "2024-12-31").unwrap();
        // Checking: 1000 + 2000 - 500 - 1000 = 1500; broker at market 1200.
        assert_eq!(report.total_assets, 2700.0);
        assert_eq!(report.total_liabilities, 0.0);
        // Opening equity 1000 plus the synthetic unrealized-gains row (+200).
        assert_eq!(report.unrealized_gains, 200.0);
        assert_eq!(report.total_equity, 1200.0);
        assert!(report.equity_rows.iter().any(|r| r.account_guid.is_empty()
            && r.account_name == "Unrealized gains"
            && r.amount == 200.0));
        // Salary 2000 - rent 500 = 1500 retained.
        assert_eq!(report.net_income, 1500.0);
        assert!(report.balance_check.abs() < 0.01, "check={}", report.balance_check);
        assert_eq!(report.report_currency, "EUR");
        // Priced securities must not leak into either warning list.
        assert!(report.missing_fx.is_empty());
        assert!(report.missing_prices.is_empty());
    }

    #[test]
    fn balance_sheet_flags_unpriced_security_not_fx() {
        let conn = setup_balance_sheet();
        conn.execute("DELETE FROM prices", [],).unwrap();
        let report = generate_balance_sheet_inner(&conn, "2024-12-31").unwrap();
        // Falls back to cost (1000): assets 2500, equity 1000, still balanced.
        assert_eq!(report.total_assets, 2500.0);
        assert_eq!(report.unrealized_gains, 0.0);
        assert!(report.balance_check.abs() < 0.01, "check={}", report.balance_check);
        // The security is reported as a missing price (with holding link
        // target), never as a missing FX rate.
        assert!(report.missing_fx.is_empty());
        assert_eq!(report.missing_prices.len(), 1);
        assert_eq!(report.missing_prices[0].mnemonic, "AAPL");
        assert_eq!(
            report.missing_prices[0].commodity_guid.as_deref(),
            Some("c-aapl")
        );
    }

    #[test]
    fn balance_sheet_ignores_zero_balance_currencies() {
        let conn = setup_balance_sheet();
        // USD account with no transactions: zero converts to zero at any
        // rate, so it must not raise a missing-FX warning.
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('spare', 'Spare', 'BANK', 'root', 0, 'c-usd')",
            [],
        )
        .unwrap();
        let report = generate_balance_sheet_inner(&conn, "2024-12-31").unwrap();
        assert!(report.missing_fx.is_empty());
    }

    #[test]
    fn balance_sheet_includes_trading_accounts() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
                    ('cash', 'Cash', 'BANK', 'root', 0, 'c-eur'),
                    ('equity', 'Opening', 'EQUITY', 'root', 0, 'c-eur'),
                    ('trade', 'Trading', 'TRADING', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        // Opening 1000, then a 50 trading gain booked against cash.
        for (tag, debit, credit, amount) in [
            ("u0", "cash", "equity", 100_000i64),
            ("u1", "cash", "trade", 5_000i64),
        ] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, '2024-01-01')",
                params![tag],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                params![format!("s-{tag}-d"), tag, debit, amount],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                params![format!("s-{tag}-c"), tag, credit, -amount],
            )
            .unwrap();
        }
        let report = generate_balance_sheet_inner(&conn, "2024-12-31").unwrap();
        assert_eq!(report.total_assets, 1050.0);
        // Trading gain reads positive in equity; nothing dropped.
        assert_eq!(report.total_equity, 1050.0);
        assert!(report.balance_check.abs() < 0.01, "check={}", report.balance_check);
    }

    #[test]
    fn pnl_rejects_inverted_range() {
        let conn = setup_balance_sheet();
        assert!(
            generate_pnl_report_inner(&conn, "2024-04-01", "2024-01-01", None, None).is_err()
        );
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
    fn pnl_converts_each_month_at_its_month_end_rate() {
        let conn = setup_balance_sheet();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('consult', 'Consulting', 'INCOME', 'root', 0, 'c-usd')",
            [],
        )
        .unwrap();
        for (tag, date) in [("t-jan", "2024-01-15"), ("t-feb", "2024-02-15")] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![tag, date],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, 'checking', 10000, 100, 10000, 100)",
                params![format!("s-{tag}-d"), tag],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, 'consult', -10000, 100, -10000, 100)",
                params![format!("s-{tag}-c"), tag],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES ('p-usd-jan', 'c-usd', 'c-eur', 50, 100, '2024-01-31'),
                    ('p-usd-feb', 'c-usd', 'c-eur', 100, 100, '2024-02-28')",
            [],
        )
        .unwrap();
        // January's 100 USD converts at 0.5, February's at 1.0. A single
        // period-end rate (1.0) would report 2200 with the fixture salary;
        // month-end bucketing reports 2150. Fixture salary adds 2000 EUR in
        // February on top of the 150 USD consulting income.
        let report = generate_pnl_report_inner(
            &conn,
            "2024-01-01",
            "2024-02-29",
            Some("2023-12-01"),
            Some("2023-12-31"),
        )
        .unwrap();
        assert_eq!(report.total_income, 2150.0);
        let consult = report
            .income_rows
            .iter()
            .find(|r| r.account_name == "Consulting")
            .expect("consulting line");
        assert_eq!(consult.current_amount, 150.0);
        assert!(report.missing_fx.is_empty());
    }

    #[test]
    fn pnl_trend_buckets_months_and_zero_fills() {
        let conn = setup_balance_sheet();
        let points = generate_pnl_trend_inner(&conn, "2024-01-01", "2024-03-31").unwrap();
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].month, "2024-01");
        assert_eq!(points[0].income, 0.0);
        assert_eq!(points[0].expenses, 0.0);
        assert_eq!(points[1].month, "2024-02");
        assert_eq!(points[1].income, 2000.0);
        assert_eq!(points[2].month, "2024-03");
        assert_eq!(points[2].expenses, 500.0);
    }

    #[test]
    fn pnl_trend_rejects_inverted_range() {
        let conn = setup_balance_sheet();
        assert!(generate_pnl_trend_inner(&conn, "2024-04-01", "2024-01-01").is_err());
    }

    #[test]
    fn pnl_change_pct_matches_frontend_convention() {
        assert_eq!(pnl_change_pct(150.0, 100.0), Some(50.0));
        assert_eq!(pnl_change_pct(0.0, 0.0), None);
        // New account with no prior balance reports 100%.
        assert_eq!(pnl_change_pct(200.0, 0.0), Some(100.0));
    }

    #[test]
    fn pnl_totals_and_prev_merge() {
        let conn = setup_balance_sheet();
        let report = generate_pnl_report_inner(
            &conn,
            "2024-02-01",
            "2024-03-31",
            Some("2024-01-01"),
            Some("2024-01-31"),
        )
        .unwrap();
        // Salary 2000 in Feb, rent 500 in Mar; January has neither.
        assert_eq!(report.total_income, 2000.0);
        assert_eq!(report.total_expenses, 500.0);
        assert_eq!(report.net_income, 1500.0);
        assert_eq!(report.total_income_prev, 0.0);
        assert_eq!(report.total_expenses_prev, 0.0);
        assert_eq!(report.income_rows.len(), 1);
        assert_eq!(report.income_rows[0].current_amount, 2000.0);
        assert_eq!(report.income_rows[0].previous_amount, 0.0);
        assert_eq!(report.income_rows[0].change_pct, Some(100.0));
        assert_eq!(report.expense_rows[0].change_pct, Some(100.0));
        assert_eq!(report.report_currency, "EUR");
        assert!(report.missing_fx.is_empty());
    }

    #[test]
    fn pnl_reports_missing_fx_from_either_window() {
        let conn = setup_balance_sheet();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('consult', 'Consulting', 'INCOME', 'root', 0, 'c-usd')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t-usd', '2024-02-15')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-t-usd-d', 't-usd', 'checking', 10000, 100, 10000, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES ('s-t-usd-c', 't-usd', 'consult', -10000, 100, -10000, 100)",
            [],
        )
        .unwrap();
        let report = generate_pnl_report_inner(
            &conn,
            "2024-02-01",
            "2024-03-31",
            Some("2024-01-01"),
            Some("2024-01-31"),
        )
        .unwrap();
        // No USD->EUR price exists, so the book degrades to parity and flags it.
        assert!(report.missing_fx.contains(&"USD".to_string()));
    }

    #[test]
    fn pnl_keeps_placeholder_ancestors_and_prev_only_accounts() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
                    ('inc-top', 'Income Top', 'INCOME', 'root', 1, 'c-eur'),
                    ('wages', 'Wages', 'INCOME', 'inc-top', 1, 'c-eur'),
                    ('salary', 'Salary', 'INCOME', 'wages', 0, 'c-eur'),
                    ('bonus', 'Bonus', 'INCOME', 'wages', 0, 'c-eur'),
                    ('other-top', 'Other Top', 'INCOME', 'root', 1, 'c-eur'),
                    ('other-leaf', 'Other Leaf', 'INCOME', 'other-top', 0, 'c-eur'),
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        // Salary in the current window (Feb), bonus only in the previous
        // window (Jan). Other Leaf has no activity in either window.
        for (tag, date, amount, income) in [
            ("p1", "2024-02-10", 200_000i64, "salary"),
            ("p0", "2024-01-10", 50_000i64, "bonus"),
        ] {
            conn.execute(
                "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
                params![tag, date],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, 'checking', ?3, 100, ?3, 100)",
                params![format!("s-{tag}-d"), tag, amount],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
                params![format!("s-{tag}-c"), tag, income, -amount],
            )
            .unwrap();
        }
        let report = generate_pnl_report_inner(
            &conn,
            "2024-02-01",
            "2024-02-29",
            Some("2024-01-01"),
            Some("2024-01-31"),
        )
        .unwrap();
        // Intermediate placeholder renders as the root (the top grouping row
        // is covered by the Income total row) with rolled-up totals.
        assert_eq!(report.income_rows.len(), 3);
        assert_eq!(report.income_rows[0].account_name, "Wages");
        assert_eq!(report.income_rows[0].indent, 0);
        assert_eq!(report.income_rows[0].current_amount, 2000.0);
        assert_eq!(report.income_rows[0].previous_amount, 500.0);
        // Name-sorted children: Bonus before Salary.
        assert_eq!(report.income_rows[1].account_name, "Bonus");
        assert_eq!(report.income_rows[1].indent, 1);
        assert_eq!(report.income_rows[1].current_amount, 0.0);
        assert_eq!(report.income_rows[1].previous_amount, 500.0);
        assert_eq!(report.income_rows[2].account_name, "Salary");
        assert_eq!(report.income_rows[2].indent, 1);
        assert_eq!(report.income_rows[2].current_amount, 2000.0);
        assert_eq!(report.income_rows[2].previous_amount, 0.0);
        assert_eq!(report.total_income, 2000.0);
        assert_eq!(report.total_income_prev, 500.0);
        // Zero-only branches stay hidden.
        assert!(report.income_rows.iter().all(|r| r.account_name != "Other Leaf"
            && r.account_name != "Other Top"));
    }
}
