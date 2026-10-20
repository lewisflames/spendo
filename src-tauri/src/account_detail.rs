use chrono::Local;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};

use tauri::State;

use crate::accounts;
use crate::accounts::{AccountNode, ChildAccountInfo, ParentAccountInfo};
use crate::db::{run_blocking, AppError, AppState};
use crate::fx;
use crate::prices::get_cached_prices_map;
use crate::sql;
use crate::utils;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailSplit {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub account_description: Option<String>,
    pub account_type: String,
    pub amount: f64,
    pub memo: Option<String>,
    // Internal only: aggregated into `DetailTransaction.reconcile_state`
    // (y wins over c). Skipped on the wire; the per-tx status is what the UI consumes.
    #[serde(skip)]
    pub reconcile_state: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailTransaction {
    pub guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    pub splits: Vec<DetailSplit>,
    /// Names of the accounts on the other side of the transaction (outside
    /// the viewed account's subtree), largest-magnitude split first.
    pub counter_accounts: Vec<String>,
    /// Signed effect on the viewed subtree, converted to the report currency
    /// at the transaction's date (the delta of `running_balance`).
    pub amount: f64,
    pub running_balance: f64,
    /// GnuCash reconcile state ('n'/'c'/'y') of the transaction's splits
    /// within the viewed subtree ('y' wins over 'c').
    pub reconcile_state: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDetailInfo {
    pub guid: String,
    pub name: String,
    pub account_type: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub placeholder: bool,
    pub commodity: Option<String>,
    pub commodity_guid: Option<String>,
    pub parent_guid: Option<String>,
    pub balance: f64,
    /// Whole subtree size minus self (`subaccount_guids` includes the account
    /// itself), i.e. the number of descendants — not direct children.
    pub descendants_count: i64,
    /// Direct children with tree-consistent balances, for the child table.
    pub child_accounts: Vec<ChildAccountInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub total_transactions: i64,
    pub total_income: f64,
    pub total_expenses: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDetailResult {
    pub account: AccountDetailInfo,
    pub summary: AccountSummary,
    pub transactions: Vec<DetailTransaction>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    /// The account node with its full subtree (tree-consistent balances), for
    /// the balance-composition chart; `None` when the account is missing.
    pub subtree: Option<AccountNode>,
    /// The account's parent, for the pie's parent-ratio label; `None` for
    /// top-level accounts (the ROOT node is excluded from the tree).
    pub parent: Option<ParentAccountInfo>,
}

/// Ledger query parameters shared by the count and page queries. Balance
/// views (header, summary, children, pie) always reflect current state; the
/// transaction ledger keeps its own `date_from`/`date_to` filters.
struct TxQuery<'a> {
    page: i64,
    page_size: i64,
    date_from: Option<&'a str>,
    date_to: Option<&'a str>,
    search: Option<&'a str>,
    sort_by: Option<&'a str>,
    sort_dir: Option<&'a str>,
}

/// Grouped per-transaction effects keyed by GUID: `(post_date, description,
/// num, [(currency, signed effect)])`, in ascending (post_date, guid) order.
type TxGroups = HashMap<String, (String, Option<String>, Option<String>, Vec<(String, f64)>)>;

/// Per-request currency context shared by the balance and summary passes so
/// FX data is loaded exactly once per IPC call.
struct FxCtx {
    report_currency: String,
    fx_map: HashMap<String, f64>,
}

struct AccountHeader {
    guid: String,
    name: String,
    account_type: String,
    code: Option<String>,
    description: Option<String>,
    placeholder: bool,
    commodity: Option<String>,
    commodity_guid: Option<String>,
    parent_guid: Option<String>,
}

fn load_account_header(conn: &Connection, guid: &str) -> Result<AccountHeader, String> {
    conn.query_row(
        "SELECT a.guid, a.name, a.account_type, a.code, a.description, a.placeholder,
                c.mnemonic, a.commodity_guid, a.parent_guid
         FROM accounts a
         LEFT JOIN commodities c ON c.guid = a.commodity_guid
         WHERE a.guid = ?1",
        [guid],
        |row| {
            Ok(AccountHeader {
                guid: row.get(0)?,
                name: row.get(1)?,
                account_type: row.get(2)?,
                code: row.get(3)?,
                description: row.get(4)?,
                placeholder: row.get::<_, i64>(5)? != 0,
                commodity: row.get(6)?,
                commodity_guid: row.get(7)?,
                parent_guid: row.get(8)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => format!("Account not found: {}", guid),
        other => other.to_string(),
    })
}

/// Shared transaction-filter WHERE builder for this module's queries. All
/// queries here alias `transactions` as `t`; date/search semantics come from
/// `sql::push_tx_date_search`.
fn filter_clauses(
    date_from: Option<&str>,
    date_to: Option<&str>,
    search: Option<&str>,
    param_offset: usize,
    include_baseline: bool,
) -> Result<sql::WhereBuilder, AppError> {
    let mut w = sql::WhereBuilder::new(param_offset);
    if include_baseline {
        w.push_raw(utils::VISIBLE_TX_FILTER);
    }
    let from = utils::validate_optional_iso_date(date_from).map_err(AppError::Validation)?;
    let to = utils::validate_optional_iso_date(date_to).map_err(AppError::Validation)?;
    sql::push_tx_date_search(&mut w, from.as_deref(), to.as_deref(), search);
    Ok(w)
}

/// Income/expense totals across transactions in the subtree that match
/// `search`. Effects are converted to the report currency and bucketed by the
/// split account's category: EXPENSE is debit-normal, so spending (a positive
/// signed effect) counts as an expense there, while balance-sheet/income
/// accounts keep the sign heuristic (inflow = income).
fn compute_summary(
    conn: &Connection,
    ctx: &FxCtx,
    sub_guids: &[String],
    search: Option<&str>,
) -> Result<AccountSummary, String> {
    let in_list = sql::in_clause(sub_guids.len());
    let sub_params = sql::string_param_refs(sub_guids);

    let w = filter_clauses(None, None, search, sub_guids.len(), false)?;
    let sum_extra_sql = w.and_clause("AND");

    let summary_sql = format!(
        "SELECT t.guid, c.mnemonic, a.account_type, SUM({}) as effect
         FROM transactions t
         JOIN splits s ON s.tx_guid = t.guid
         JOIN accounts a ON a.guid = s.account_guid
         JOIN commodities c ON c.guid = a.commodity_guid
         WHERE s.account_guid IN ({})
         AND {}
         {}
         GROUP BY t.guid, c.mnemonic, a.account_type",
        utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type"),
        in_list,
        utils::VISIBLE_TX_FILTER,
        sum_extra_sql
    );

    let mut summary_stmt = conn.prepare(&summary_sql).map_err(|e| e.to_string())?;
    let summary_params: Vec<&dyn rusqlite::types::ToSql> = sub_params
        .iter()
        .copied()
        .chain(w.params.iter().map(|p| p.as_ref()))
        .collect();

    let summary_rows: Vec<(String, String, String, f64)> = summary_stmt
        .query_map(summary_params.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut total_txns: i64 = 0;
    let mut pos_effect = 0.0;
    let mut neg_effect = 0.0;
    let mut seen: HashSet<String> = HashSet::new();
    for (tx_guid, mnemonic, account_type, effect) in &summary_rows {
        if seen.insert(tx_guid.clone()) {
            total_txns += 1;
        }
        let converted = fx::convert(*effect, mnemonic, &ctx.report_currency, &ctx.fx_map);
        if utils::category_for_type(account_type) == "EXPENSE" {
            // Debit-normal: a spend increases the expense, a refund reverses it.
            if converted >= 0.0 {
                neg_effect += converted;
            } else {
                pos_effect += -converted;
            }
        } else if converted >= 0.0 {
            pos_effect += converted;
        } else {
            neg_effect += -converted;
        }
    }

    Ok(AccountSummary {
        total_transactions: total_txns,
        total_income: pos_effect,
        total_expenses: neg_effect,
    })
}

fn fetch_account_detail_inner(
    conn: &Connection,
    guid: &str,
    query: TxQuery<'_>,
) -> Result<AccountDetailResult, String> {
    let header = load_account_header(conn, guid)?;
    let sub_guids = accounts::get_subaccount_guids(conn, guid)?;

    if sub_guids.is_empty() {
        return Err("Account has no sub-account entries".to_string());
    }

    let descendants_count = (sub_guids.len() as i64) - 1;
    let (child_accounts, subtree, parent) = accounts::fetch_children_and_subtree(conn, guid)?;
    let ctx = FxCtx {
        report_currency: fx::get_report_currency_mnemonic(conn)?.unwrap_or_default(),
        fx_map: fx::get_fx_map(conn, None)?,
    };

    // Single source of truth: the KPI balance is the tree-consistent subtree
    // balance (same valuation as the Accounts list, pie and child table), so
    // the three can never diverge. `None` only happens for accounts missing
    // from the tree (e.g. ROOT, which the tree excludes).
    let balance = subtree.as_ref().map(|n| n.balance).unwrap_or(0.0);

    let summary = compute_summary(conn, &ctx, &sub_guids, query.search)?;
    let tx_page = load_tx_page(conn, &ctx, &sub_guids, &query)?;

    Ok(AccountDetailResult {
        account: AccountDetailInfo {
            guid: header.guid,
            name: header.name,
            account_type: header.account_type,
            code: header.code,
            description: header.description,
            placeholder: header.placeholder,
            commodity: header.commodity,
            commodity_guid: header.commodity_guid,
            parent_guid: header.parent_guid,
            balance,
            descendants_count,
            child_accounts,
        },
        summary,
        transactions: tx_page.transactions,
        total: tx_page.total,
        page: tx_page.safe_page,
        page_size: tx_page.page_size,
        total_pages: tx_page.total_pages,
        subtree,
        parent,
    })
}

/// One page of subtree transactions with running balances, counter-accounts,
/// reconcile state and split details.
struct TxPage {
    transactions: Vec<DetailTransaction>,
    total: i64,
    safe_page: i64,
    total_pages: i64,
    page_size: i64,
}

#[derive(Debug, Clone)]
struct TxEffect {
    guid: String,
    post_date: String,
    description: Option<String>,
    num: Option<String>,
    currency: String,
    balance_effect: f64,
}

fn load_tx_page(
    conn: &Connection,
    ctx: &FxCtx,
    sub_guids: &[String],
    query: &TxQuery<'_>,
) -> Result<TxPage, String> {
    let TxQuery {
        page,
        page_size,
        sort_by,
        sort_dir,
        ..
    } = *query;

    let total = count_subtree_transactions(conn, sub_guids, query)?;
    let (page_size, total_pages, safe_page, _) = utils::paginate(total, page, page_size);

    let (tx_map, tx_order) = group_tx_effects(fetch_tx_effects(conn, sub_guids, query)?);
    let balances = accumulate_running_balances(conn, ctx, &tx_order, &tx_map)?;

    let page_guids = order_page_guids(
        &tx_order,
        &tx_map,
        &balances.amount_by_guid,
        sort_by,
        sort_dir,
        safe_page,
        page_size,
    );

    let transactions = enrich_page_transactions(
        conn,
        &page_guids,
        &tx_map,
        &balances.running_by_guid,
        &balances.amount_by_guid,
        sub_guids,
    )?;

    Ok(TxPage {
        transactions,
        total,
        safe_page,
        total_pages,
        page_size,
    })
}

/// Count for pagination: distinct transactions matching the ledger filters in
/// the subtree.
fn count_subtree_transactions(
    conn: &Connection,
    sub_guids: &[String],
    query: &TxQuery<'_>,
) -> Result<i64, String> {
    let placeholders_str = sql::in_clause(sub_guids.len());
    let sub_params = sql::string_param_refs(sub_guids);

    let w = filter_clauses(
        query.date_from,
        query.date_to,
        query.search,
        sub_guids.len(),
        true,
    )?;
    let count_sql = format!(
        "SELECT COUNT(DISTINCT t.guid) as cnt
         FROM transactions t
         JOIN splits s ON s.tx_guid = t.guid
         WHERE s.account_guid IN ({})
         AND {}",
        placeholders_str,
        w.joined()
    );

    let mut count_stmt = conn.prepare(&count_sql).map_err(|e| e.to_string())?;
    let count_params: Vec<&dyn rusqlite::types::ToSql> = sub_params
        .iter()
        .copied()
        .chain(w.params.iter().map(|p| p.as_ref()))
        .collect();

    count_stmt
        .query_row(count_params.as_slice(), |row| row.get(0))
        .map_err(|e| e.to_string())
}

/// Per-currency transaction effects from the subtree CTE, in
/// (post_date, guid) order. The running balance is accumulated in report
/// currency in Rust so multi-currency subtrees convert each currency at the
/// transaction's own date. The recursive CTE re-derives the subtree starting
/// at ?1, so filter params start at ?2.
fn fetch_tx_effects(
    conn: &Connection,
    sub_guids: &[String],
    query: &TxQuery<'_>,
) -> Result<Vec<TxEffect>, String> {
    let w = filter_clauses(query.date_from, query.date_to, query.search, 1, true)?;

    let grouped_balance = utils::signed_amount_expr("t.raw_amount", "t.account_type");

    let tx_sql = format!(
        "WITH RECURSIVE sub AS (
            SELECT guid FROM accounts WHERE guid = ?1
            UNION
            SELECT a.guid FROM accounts a JOIN sub ON a.parent_guid = sub.guid
        ),
        tx_splits AS (
            SELECT t.guid, t.post_date, t.description, t.num,
                {} as raw_amount,
                a.account_type,
                c.mnemonic as currency
            FROM transactions t
            JOIN splits s ON s.tx_guid = t.guid
            JOIN accounts a ON a.guid = s.account_guid
            JOIN commodities c ON c.guid = a.commodity_guid
            WHERE s.account_guid IN (SELECT guid FROM sub)
            AND {}
        ),
        tx_grouped AS (
            SELECT t.guid, t.post_date, t.description, t.num, t.currency,
                SUM({}) as balance_effect
            FROM tx_splits t
            GROUP BY t.guid, t.post_date, t.description, t.num, t.currency
        )
        SELECT guid, post_date, description, num, currency, balance_effect
        FROM tx_grouped
        ORDER BY post_date ASC, guid ASC",
        utils::RAW_AMOUNT,
        w.joined(),
        grouped_balance
    );

    let mut tx_stmt = conn.prepare(&tx_sql).map_err(|e| e.to_string())?;

    let mut all_tx_params: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(sub_guids.first().cloned().unwrap_or_default())];
    all_tx_params.extend(w.params);

    let tx_refs: Vec<&dyn rusqlite::types::ToSql> =
        all_tx_params.iter().map(|p| p.as_ref()).collect();

    let tx_effects: Vec<TxEffect> = tx_stmt
        .query_map(tx_refs.as_slice(), |row| {
            Ok(TxEffect {
                guid: row.get(0)?,
                post_date: row.get(1)?,
                description: row.get(2)?,
                num: row.get(3)?,
                currency: row.get(4)?,
                balance_effect: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(tx_effects)
}

/// Group per transaction, preserving (post_date, guid) order.
fn group_tx_effects(tx_effects: Vec<TxEffect>) -> (TxGroups, Vec<String>) {
    let mut tx_map: TxGroups = HashMap::new();
    let mut tx_order: Vec<String> = vec![];
    for e in tx_effects {
        match tx_map.get_mut(&e.guid) {
            Some(entry) => entry.3.push((e.currency, e.balance_effect)),
            None => {
                tx_order.push(e.guid.clone());
                tx_map.insert(
                    e.guid.clone(),
                    (
                        e.post_date.clone(),
                        e.description.clone(),
                        e.num.clone(),
                        vec![(e.currency, e.balance_effect)],
                    ),
                );
            }
        }
    }
    (tx_map, tx_order)
}

/// Report-currency running balance and per-transaction delta, keyed by GUID.
struct RunningBalances {
    running_by_guid: HashMap<String, f64>,
    amount_by_guid: HashMap<String, f64>,
}

/// Running balance in report currency: convert each currency at the transaction date.
fn accumulate_running_balances(
    conn: &Connection,
    ctx: &FxCtx,
    tx_order: &[String],
    tx_map: &TxGroups,
) -> Result<RunningBalances, String> {
    let fx_history = fx::FxHistory::load(conn)?;
    let mut running = 0.0;
    let mut running_by_guid: HashMap<String, f64> = HashMap::new();
    let mut amount_by_guid: HashMap<String, f64> = HashMap::new();
    for guid in tx_order {
        let (post_date, _, _, effects) = &tx_map[guid];
        let fx_at = fx_history.map_for_date(post_date);
        let prev = running;
        running += effects
            .iter()
            .map(|(mn, v)| fx::convert(*v, mn, &ctx.report_currency, &fx_at))
            .sum::<f64>();
        running_by_guid.insert(guid.clone(), running);
        amount_by_guid.insert(guid.clone(), running - prev);
    }
    Ok(RunningBalances {
        running_by_guid,
        amount_by_guid,
    })
}

/// Sort transaction GUIDs and select the requested page. Default order is
/// date descending (running balance accumulates ascending, reversed for
/// newest-first display).
fn order_page_guids(
    tx_order: &[String],
    tx_map: &TxGroups,
    amount_by_guid: &HashMap<String, f64>,
    sort_by: Option<&str>,
    sort_dir: Option<&str>,
    safe_page: i64,
    page_size: i64,
) -> Vec<String> {
    let offset = ((safe_page - 1) * page_size) as usize;
    let take = page_size as usize;

    #[derive(Clone, Copy, PartialEq)]
    enum SortKey {
        Date,
        Description,
        Amount,
    }

    let key = match sort_by.unwrap_or("date") {
        "description" => SortKey::Description,
        "amount" => SortKey::Amount,
        _ => SortKey::Date,
    };
    let ascending = match (key, sort_dir) {
        (SortKey::Date, Some("asc")) => true,
        (SortKey::Date, _) => false,
        (SortKey::Description, Some("desc")) => false,
        (SortKey::Description, _) => true,
        (SortKey::Amount, Some("desc")) => false,
        (SortKey::Amount, _) => true,
    };

    // Precompute lowercase keys once instead of allocating per comparison.
    let desc_keys: HashMap<&String, String> = match key {
        SortKey::Description => tx_order
            .iter()
            .map(|g| (g, tx_map[g].1.as_deref().unwrap_or("").to_lowercase()))
            .collect(),
        _ => HashMap::new(),
    };

    let mut ordered: Vec<&String> = tx_order.iter().collect();
    ordered.sort_by(|a, b| {
        let cmp = match key {
            SortKey::Date => tx_map[*a].0.cmp(&tx_map[*b].0).then_with(|| a.cmp(b)),
            SortKey::Description => desc_keys[a].cmp(&desc_keys[b]).then_with(|| a.cmp(b)),
            SortKey::Amount => amount_by_guid[*a]
                .total_cmp(&amount_by_guid[*b])
                .then_with(|| a.cmp(b)),
        };
        if ascending {
            cmp
        } else {
            cmp.reverse()
        }
    });

    ordered
        .into_iter()
        .skip(offset)
        .take(take)
        .map(|s| s.to_string())
        .collect()
}

fn enrich_page_transactions(
    conn: &Connection,
    page_guids: &[String],
    tx_map: &TxGroups,
    running_by_guid: &HashMap<String, f64>,
    amount_by_guid: &HashMap<String, f64>,
    sub_guids: &[String],
) -> Result<Vec<DetailTransaction>, String> {
    let notes_map = load_page_notes(conn, page_guids)?;
    let mut splits_by_guid = load_page_splits(conn, page_guids)?;

    let sub_set: HashSet<&str> = sub_guids.iter().map(|s| s.as_str()).collect();

    let mut transactions: Vec<DetailTransaction> = vec![];
    for tx_guid in page_guids {
        let (post_date, description, num, _) = &tx_map[tx_guid];
        let splits = splits_by_guid.remove(tx_guid).unwrap_or_default();

        let (counter_accounts, reconcile_state) =
            split_counter_and_reconcile_state(&splits, &sub_set);

        transactions.push(DetailTransaction {
            guid: tx_guid.clone(),
            post_date: post_date.clone(),
            description: description.clone(),
            num: num.clone(),
            notes: notes_map.get(tx_guid).cloned().flatten(),
            splits,
            counter_accounts,
            amount: amount_by_guid[tx_guid],
            running_balance: running_by_guid[tx_guid],
            reconcile_state,
        });
    }

    Ok(transactions)
}

/// Notes for the page's transactions (kept out of the main CTE to avoid
/// widening the grouping query).
fn load_page_notes(
    conn: &Connection,
    page_guids: &[String],
) -> Result<HashMap<String, Option<String>>, String> {
    let mut notes_map: HashMap<String, Option<String>> = HashMap::new();
    if !page_guids.is_empty() {
        let notes_sql = format!(
            "SELECT guid, notes FROM transactions WHERE guid IN ({})",
            sql::in_clause(page_guids.len())
        );
        let mut notes_stmt = conn.prepare(&notes_sql).map_err(|e| e.to_string())?;
        let rows: Vec<(String, Option<String>)> = notes_stmt
            .query_map(sql::string_param_refs(page_guids).as_slice(), |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        notes_map.extend(rows);
    }
    Ok(notes_map)
}

/// Split details for the page's transactions in one chunked query instead of
/// one query per row.
fn load_page_splits(
    conn: &Connection,
    page_guids: &[String],
) -> Result<HashMap<String, Vec<DetailSplit>>, String> {
    let mut splits_by_guid: HashMap<String, Vec<DetailSplit>> = HashMap::new();
    for chunk in page_guids.chunks(sql::MAX_IN_PARAMS) {
        let split_sql = format!(
            "SELECT s.guid, s.account_guid, a.name as account_name, a.description as account_description,
                    a.account_type,
                    s.memo,
                    {} as amount,
                    s.reconcile_state,
                    s.tx_guid
             FROM splits s
             JOIN accounts a ON a.guid = s.account_guid
             WHERE s.tx_guid IN ({})
             ORDER BY s.tx_guid, ABS(s.value_num) DESC",
            utils::RAW_AMOUNT,
            sql::in_clause(chunk.len()),
        );
        let mut split_stmt = conn.prepare(&split_sql).map_err(|e| e.to_string())?;
        let rows = split_stmt
            .query_map(sql::string_param_refs(chunk).as_slice(), |row| {
                Ok((
                    row.get::<_, String>(8)?, // tx_guid
                    DetailSplit {
                        guid: row.get(0)?,
                        account_guid: row.get(1)?,
                        account_name: row.get(2)?,
                        account_description: row.get(3)?,
                        account_type: row.get(4)?,
                        amount: row.get(6)?,
                        memo: row.get(5)?,
                        reconcile_state: row.get(7)?,
                    },
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for (tx_guid, split) in rows {
            splits_by_guid.entry(tx_guid).or_default().push(split);
        }
    }
    Ok(splits_by_guid)
}

/// Counter-account labels and aggregated reconcile state for one transaction:
/// distinct descriptions (falling back to name) of splits outside the viewed
/// subtree in largest-magnitude-first order, plus the subtree's state
/// ('y' wins over 'c').
fn split_counter_and_reconcile_state(
    splits: &[DetailSplit],
    sub_set: &HashSet<&str>,
) -> (Vec<String>, Option<String>) {
    let mut counter_accounts: Vec<String> = vec![];
    let mut seen_accounts: HashSet<&str> = HashSet::new();
    let mut has_y = false;
    let mut has_c = false;
    for s in splits {
        if sub_set.contains(s.account_guid.as_str()) {
            match s.reconcile_state.as_deref() {
                Some("y") => has_y = true,
                Some("c") => has_c = true,
                _ => {}
            }
        } else if seen_accounts.insert(s.account_guid.as_str()) {
            let label = s.account_description.as_deref().unwrap_or(&s.account_name);
            counter_accounts.push(label.to_string());
        }
    }
    let reconcile_state = if has_y {
        Some("y".to_string())
    } else if has_c {
        Some("c".to_string())
    } else {
        splits
            .iter()
            .filter(|s| sub_set.contains(s.account_guid.as_str()))
            .find_map(|s| s.reconcile_state.clone())
    };
    (counter_accounts, reconcile_state)
}

#[tauri::command]
// The flat argument list IS the IPC contract — the frontend invokes with
// individual camelCase keys, so it can't be grouped without changing the wire format.
#[allow(clippy::too_many_arguments)]
pub async fn fetch_account_detail(
    state: State<'_, AppState>,
    guid: String,
    page: i64,
    page_size: i64,
    date_from: Option<String>,
    date_to: Option<String>,
    search: Option<String>,
    sort_by: Option<String>,
    sort_dir: Option<String>,
) -> Result<AccountDetailResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_account_detail_inner(
            conn,
            &guid,
            TxQuery {
                page,
                page_size,
                date_from: date_from.as_deref(),
                date_to: date_to.as_deref(),
                search: search.as_deref(),
                sort_by: sort_by.as_deref(),
                sort_dir: sort_dir.as_deref(),
            },
        )
    })
    .await
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountBalancePoint {
    /// Day as `YYYY-MM-DD` (named `date` like the other trend points).
    pub date: String,
    pub balance: f64,
}

/// Per-(day, currency) signed split-value deltas for the account subtree.
/// Amounts carry GnuCash display signs (credit-normal accounts negated) via
/// `signed_amount_expr`, so running sums can be aggregated as-is.
/// Half-open dates (`>= start AND < date(end, '+1 day')`): `post_date` carries
/// `HH:MM:SS`, so `<= YYYY-MM-DD` would drop end-day timed rows.
/// Returns `(post_date, commodity_guid, mnemonic, summed delta)`. The GUID
/// keys price lookups (mnemonics can repeat across namespaces); the mnemonic
/// is for FX/display.
fn subtree_deltas_by_day(
    conn: &Connection,
    sub_guids: &[String],
    amount_expr: &str,
    type_filter: &str,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<(String, String, String, f64)>, String> {
    let n = sub_guids.len();
    let sql = format!(
        "SELECT substr(t.post_date, 1, 10) as day, c.guid, c.mnemonic, COALESCE(SUM({}), 0)
         FROM splits s
         JOIN accounts a ON a.guid = s.account_guid
         JOIN commodities c ON c.guid = a.commodity_guid
         JOIN transactions t ON t.guid = s.tx_guid
         WHERE s.account_guid IN ({}) {}
           AND t.post_date >= ?{}
           AND t.post_date < min(date(?{}, '+1 day'), {})
         GROUP BY day, c.guid, c.mnemonic
         ORDER BY day ASC",
        amount_expr,
        sql::in_clause(n),
        type_filter,
        n + 1,
        n + 2,
        utils::VISIBLE_NOW_EXPR
    );
    let mut params = sql::string_param_refs(sub_guids);
    params.push(&start_date);
    params.push(&end_date);

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// End-of-day balance history for an account and its sub-accounts. Mirrors
/// the net worth trend approach in `home.rs`: all-time daily deltas are
/// accumulated per currency; share quantities are valued at the latest cached
/// prices and cash currencies converted with historical FX rates.
fn fetch_account_balance_trend_inner(
    conn: &Connection,
    guid: &str,
) -> Result<Vec<AccountBalancePoint>, String> {
    let sub_guids = accounts::get_subaccount_guids(conn, guid)?;
    if sub_guids.is_empty() {
        return Err(format!("Account not found: {}", guid));
    }

    // Full history through today: the chart always shows all data, with the
    // as-of cutoff (if any) drawn as a marker line by the frontend.
    let end_date = Local::now().format("%Y-%m-%d").to_string();
    let start_date = "0000-01-01";
    let signed_cash = utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type");
    let cash_filter = format!(
        "AND a.account_type NOT IN ({})",
        utils::investment_types_sql()
    );
    let signed_shares = utils::signed_amount_expr(utils::QUANTITY_AMOUNT, "a.account_type");
    let shares_filter = format!("AND a.account_type IN ({})", utils::investment_types_sql());

    let report_currency = fx::get_report_currency_mnemonic(conn)?.unwrap_or_default();
    let fx_history = fx::FxHistory::load(conn)?;
    let unified_prices = get_cached_prices_map(conn)?;

    let mut cash_by_day: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    for (day, _guid, mnemonic, delta) in subtree_deltas_by_day(
        conn,
        &sub_guids,
        &signed_cash,
        &cash_filter,
        start_date,
        &end_date,
    )? {
        cash_by_day.entry(day).or_default().push((mnemonic, delta));
    }
    let mut shares_by_day: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    for (day, guid, _mnemonic, delta) in subtree_deltas_by_day(
        conn,
        &sub_guids,
        &signed_shares,
        &shares_filter,
        start_date,
        &end_date,
    )? {
        shares_by_day.entry(day).or_default().push((guid, delta));
    }

    let days: BTreeSet<String> = cash_by_day
        .keys()
        .chain(shares_by_day.keys())
        .cloned()
        .collect();

    if days.is_empty() {
        return Ok(vec![AccountBalancePoint {
            date: end_date,
            balance: 0.0,
        }]);
    }

    let mut cash_running: HashMap<String, f64> = HashMap::new();
    let mut shares_running: HashMap<String, f64> = HashMap::new();
    let mut trend: Vec<AccountBalancePoint> = Vec::with_capacity(days.len());

    for day in days {
        if let Some(deltas) = cash_by_day.get(&day) {
            for (mn, d) in deltas {
                *cash_running.entry(mn.clone()).or_insert(0.0) += d;
            }
        }
        if let Some(deltas) = shares_by_day.get(&day) {
            for (mn, d) in deltas {
                *shares_running.entry(mn.clone()).or_insert(0.0) += d;
            }
        }
        let fx_map = fx_history.map_for_date(&day);
        let cash_total: f64 = cash_running
            .iter()
            .map(|(mn, v)| fx::convert(*v, mn, &report_currency, &fx_map))
            .sum();
        let shares_total: f64 = shares_running
            .iter()
            .map(|(guid, q)| unified_prices.get(guid).unwrap_or(&0.0) * q)
            .sum();
        trend.push(AccountBalancePoint {
            date: day,
            balance: cash_total + shares_total,
        });
    }

    Ok(trend)
}

#[tauri::command]
pub async fn fetch_account_balance_trend(
    state: State<'_, AppState>,
    guid: String,
) -> Result<Vec<AccountBalancePoint>, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_account_balance_trend_inner(conn, &guid)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use rusqlite::params;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES
             ('c-eur', 'EUR', 'CURRENCY'), ('c-usd', 'USD', 'CURRENCY')",
            [],
        )
        .unwrap();
        // ROOT account pins the report currency to EUR.
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
             ('assets',   'Assets',      'ASSET',  'root', 1, 'c-eur'),
             ('checking', 'Checking',    'BANK',   'assets', 0, 'c-eur'),
             ('card',     'Credit Card', 'CREDIT', 'root', 0, 'c-eur'),
             ('empty',    'Empty',       'BANK',   'root', 0, 'c-eur')",
            [],
        )
        .unwrap();

        add_split(&conn, "t1", "2024-01-10", "checking", 10_000);
        add_split(&conn, "t2", "2024-02-05", "checking", -3_000);
        // Credit-normal account: a 40 spend must display as -40.
        add_split(&conn, "t3", "2024-02-20", "card", 4_000);
        conn
    }

    fn add_split(conn: &Connection, tag: &str, date: &str, account: &str, value_num: i64) {
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES (?1, ?2)",
            params![tag, date],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
             VALUES (?1, ?2, ?3, ?4, 100, ?4, 100)",
            params![format!("s-{tag}"), tag, account, value_num],
        )
        .unwrap();
    }

    /// Regression: EXPENSE is debit-normal, so a spend's signed effect is
    /// positive — it must count as an expense, not as income.
    #[test]
    fn expense_accounts_bucket_spending_as_expenses() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('groceries', 'Groceries', 'EXPENSE', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        add_split(&conn, "e1", "2024-03-01", "groceries", 4_210);
        add_split(&conn, "e2", "2024-03-02", "groceries", -500);

        let result = fetch_account_detail_inner(
            &conn,
            "groceries",
            TxQuery {
                page: 1,
                page_size: 50,
                date_from: None,
                date_to: None,
                search: None,
                sort_by: None,
                sort_dir: None,
            },
        )
        .unwrap();

        assert_eq!(result.summary.total_transactions, 2);
        assert!((result.summary.total_expenses - 42.10).abs() < 1e-9);
        assert!((result.summary.total_income - 5.0).abs() < 1e-9);
    }

    /// Regression: malformed ledger filter dates used to silently return an
    /// empty page instead of an error.
    #[test]
    fn detail_rejects_malformed_date_filters() {
        let conn = setup();
        let err = fetch_account_detail_inner(
            &conn,
            "checking",
            TxQuery {
                page: 1,
                page_size: 50,
                date_from: Some("2024-1-5"),
                date_to: None,
                search: None,
                sort_by: None,
                sort_dir: None,
            },
        )
        .unwrap_err();
        assert!(err.contains("Invalid date"), "unexpected: {err}");
    }

    #[test]
    fn trend_accumulates_subtree_with_display_signs() {
        let conn = setup();

        let sub = accounts::get_subaccount_guids(&conn, "assets").unwrap();
        assert_eq!(sub, vec!["assets".to_string(), "checking".to_string()]);

        // Parent placeholder aggregates only its own subtree (checking).
        let trend = fetch_account_balance_trend_inner(&conn, "assets").unwrap();
        assert_eq!(
            trend,
            vec![
                AccountBalancePoint {
                    date: "2024-01-10".into(),
                    balance: 100.0
                },
                AccountBalancePoint {
                    date: "2024-02-05".into(),
                    balance: 70.0
                },
            ]
        );

        // Root includes the credit card; its spend shows as negative.
        let trend = fetch_account_balance_trend_inner(&conn, "root").unwrap();
        let last = trend.last().unwrap();
        assert_eq!(last.date, "2024-02-20");
        assert_eq!(last.balance, 30.0);
    }

    /// Regression: imported books keep `HH:MM:SS` in `post_date`; the trend
    /// must group by calendar day, not by timestamp.
    #[test]
    fn trend_groups_timed_imported_rows_by_day() {
        let conn = setup();
        add_split(&conn, "t-timed", "2024-01-10 14:30:00", "checking", 2_500);

        let trend = fetch_account_balance_trend_inner(&conn, "checking").unwrap();
        assert_eq!(
            trend,
            vec![
                AccountBalancePoint {
                    date: "2024-01-10".into(),
                    balance: 125.0
                },
                AccountBalancePoint {
                    date: "2024-02-05".into(),
                    balance: 95.0
                },
            ]
        );
    }

    #[test]
    fn account_without_activity_returns_single_zero_point() {
        let conn = setup();
        let trend = fetch_account_balance_trend_inner(&conn, "empty").unwrap();
        assert_eq!(trend.len(), 1);
        assert_eq!(trend[0].balance, 0.0);
    }

    #[test]
    fn detail_parent_links_to_parent_and_none_at_root() {
        let conn = setup();

        // 'checking' sits under 'assets', whose subtree balance is 70 (100 - 30).
        let (_, _, parent) = accounts::fetch_children_and_subtree(&conn, "checking").unwrap();
        let parent = parent.unwrap();
        assert_eq!(parent.guid, "assets");
        assert_eq!(parent.balance, 70.0);

        // 'assets' is top-level: its parent is the excluded ROOT node.
        let (_, _, parent) = accounts::fetch_children_and_subtree(&conn, "assets").unwrap();
        assert!(parent.is_none());

        let (_, subtree, parent) = accounts::fetch_children_and_subtree(&conn, "nope").unwrap();
        assert!(subtree.is_none());
        assert!(parent.is_none());
    }

    #[test]
    fn unknown_account_errors() {
        let conn = setup();
        assert!(fetch_account_balance_trend_inner(&conn, "nope").is_err());
    }

    #[test]
    fn tx_page_amounts_match_running_balance_deltas() {
        let conn = setup();
        let ctx = FxCtx {
            report_currency: "EUR".into(),
            fx_map: HashMap::new(),
        };

        let page = load_tx_page(
            &conn,
            &ctx,
            &accounts::get_subaccount_guids(&conn, "assets").unwrap(),
            &TxQuery {
                page: 1,
                page_size: 50,
                date_from: None,
                date_to: None,
                search: None,
                sort_by: None,
                sort_dir: None,
            },
        )
        .unwrap();

        // Default sort is date descending: t2 (2024-02-05) first, then t1 (2024-01-10).
        assert_eq!(page.transactions.len(), 2);
        let (t2, t1) = (&page.transactions[0], &page.transactions[1]);
        assert_eq!(t1.amount, 100.0);
        assert_eq!(t1.running_balance, 100.0);
        assert_eq!(t2.amount, -30.0);
        assert_eq!(t2.running_balance, 70.0);

        // Credit-normal card spend shows as a negative amount under root.
        let page = load_tx_page(
            &conn,
            &ctx,
            &accounts::get_subaccount_guids(&conn, "root").unwrap(),
            &TxQuery {
                page: 1,
                page_size: 50,
                date_from: None,
                date_to: None,
                search: None,
                sort_by: None,
                sort_dir: None,
            },
        )
        .unwrap();
        assert_eq!(page.transactions.len(), 3);
        let t3 = &page.transactions[0]; // 2024-02-20, newest
        assert_eq!(t3.amount, -40.0);
        assert_eq!(t3.running_balance, 30.0);
    }

    #[test]
    fn detail_children_match_tree_and_trend_spans_full_history() {
        let conn = setup();

        // Child balances agree with the tree-consistent subtree balance.
        let (children, subtree, _) = accounts::fetch_children_and_subtree(&conn, "assets").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].guid, "checking");
        assert_eq!(children[0].balance, 70.0);
        assert_eq!(subtree.unwrap().balance, 70.0);

        // The trend always spans the full history.
        let trend = fetch_account_balance_trend_inner(&conn, "assets").unwrap();
        assert_eq!(
            trend,
            vec![
                AccountBalancePoint {
                    date: "2024-01-10".into(),
                    balance: 100.0
                },
                AccountBalancePoint {
                    date: "2024-02-05".into(),
                    balance: 70.0
                },
            ]
        );
    }

    #[test]
    fn tx_page_sorts_by_amount() {
        let conn = setup();
        let ctx = FxCtx {
            report_currency: "EUR".into(),
            fx_map: HashMap::new(),
        };

        let page = load_tx_page(
            &conn,
            &ctx,
            &accounts::get_subaccount_guids(&conn, "assets").unwrap(),
            &TxQuery {
                page: 1,
                page_size: 50,
                date_from: None,
                date_to: None,
                search: None,
                sort_by: Some("amount"),
                sort_dir: Some("desc"),
            },
        )
        .unwrap();

        // Largest effect first: t1 (+100) before t2 (-30).
        let amounts: Vec<f64> = page.transactions.iter().map(|t| t.amount).collect();
        assert_eq!(amounts, vec![100.0, -30.0]);

        let page = load_tx_page(
            &conn,
            &ctx,
            &accounts::get_subaccount_guids(&conn, "assets").unwrap(),
            &TxQuery {
                page: 1,
                page_size: 50,
                date_from: None,
                date_to: None,
                search: None,
                sort_by: Some("amount"),
                sort_dir: Some("asc"),
            },
        )
        .unwrap();
        let amounts: Vec<f64> = page.transactions.iter().map(|t| t.amount).collect();
        assert_eq!(amounts, vec![-30.0, 100.0]);
    }

    fn setup_investments() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES
             ('c-eur', 'EUR', 'CURRENCY'),
             ('c-aapl', 'AAPL', 'NASDAQ'),
             ('c-xyz', 'XYZ', 'NASDAQ')",
            [],
        )
        .unwrap();
        // ROOT pins the report currency to EUR.
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
             ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
             ('assets', 'Assets', 'ASSET', 'root', 1, 'c-eur'),
             ('checking', 'Checking', 'BANK', 'assets', 0, 'c-eur'),
             ('broker', 'Broker', 'STOCK', 'assets', 0, 'c-aapl'),
             ('vault', 'Vault', 'STOCK', 'assets', 0, 'c-xyz')",
            [],
        )
        .unwrap();
        // Cash 500.
        add_split(&conn, "t-cash", "2024-01-10", "checking", 50_000);
        // Buy 10 AAPL for 1000 (value) and 5 XYZ for 700.
        conn.execute(
            "INSERT INTO transactions (guid, post_date) VALUES ('t-aapl', '2024-02-01'), ('t-xyz', '2024-02-15')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom) VALUES
             ('s-aapl', 't-aapl', 'broker', 100000, 100, 10, 1),
             ('s-xyz', 't-xyz', 'vault', 70000, 100, 5, 1)",
            [],
        )
        .unwrap();
        // Only AAPL is priced: 10 shares @ 150 EUR = 1500 market.
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date)
             VALUES ('c-aapl-2024-03-01', 'c-aapl', 'c-eur', 15000, 100, '2024-03-01')",
            [],
        )
        .unwrap();
        conn
    }

    fn detail_query() -> TxQuery<'static> {
        TxQuery {
            page: 1,
            page_size: 50,
            date_from: None,
            date_to: None,
            search: None,
            sort_by: None,
            sort_dir: None,
        }
    }

    #[test]
    fn detail_kpi_matches_tree_for_priced_investments() {
        let conn = setup_investments();

        // Cash 500 + AAPL market 1500 + unpriced XYZ 0.
        let result = fetch_account_detail_inner(&conn, "assets", detail_query()).unwrap();
        assert_eq!(result.account.balance, 2000.0);
        assert_eq!(result.subtree.as_ref().unwrap().balance, 2000.0);

        let by_guid: HashMap<&str, f64> = result
            .account
            .child_accounts
            .iter()
            .map(|c| (c.guid.as_str(), c.balance))
            .collect();
        assert_eq!(by_guid["checking"], 500.0);
        assert_eq!(by_guid["broker"], 1500.0);
        assert_eq!(by_guid["vault"], 0.0);

        // Leaf pages agree with the tree as well.
        let broker = fetch_account_detail_inner(&conn, "broker", detail_query()).unwrap();
        assert_eq!(broker.account.balance, 1500.0);
        let vault = fetch_account_detail_inner(&conn, "vault", detail_query()).unwrap();
        assert_eq!(vault.account.balance, 0.0);
    }

    #[test]
    fn detail_kpi_excludes_unpriced_cost_from_total() {
        // Regression for the old `raw - cost + market` helper, which reported
        // cash + (market - cost) = 500 + 500 = 1000 here instead of 2000.
        let conn = setup_investments();
        let result = fetch_account_detail_inner(&conn, "assets", detail_query()).unwrap();
        assert_eq!(result.account.balance, 2000.0);
        assert!(
            (result.account.balance - 1000.0).abs() > 0.01,
            "must not be the old market-minus-cost total"
        );
    }
}
