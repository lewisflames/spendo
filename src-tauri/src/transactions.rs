use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{run_blocking, AppState};
use crate::fx;
use crate::sql;
use crate::utils;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitInfo {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub account_type: String,
    pub amount: f64,
    pub memo: Option<String>,
}

/// Split row enriched with fields only the transaction-detail view renders.
#[derive(Debug, Clone)]
struct SplitData {
    info: SplitInfo,
    /// Raw signed quantity (`quantity_num/denom`) — share counts for
    /// investment accounts; equals `amount` for plain currency accounts.
    quantity: f64,
    reconciled: bool,
    /// GnuCash reconcile state ('n' new, 'c' cleared, 'y' reconciled).
    reconcile_state: Option<String>,
    /// Description of the split's account (falls back to its name in the UI).
    account_description: Option<String>,
    /// Mnemonic of the split account's commodity. Drives FX conversion of the
    /// transaction total and quantity display on the detail page.
    commodity: Option<String>,
}

impl SplitData {
    fn to_detail(&self) -> SplitDetail {
        SplitDetail {
            guid: self.info.guid.clone(),
            account_guid: self.info.account_guid.clone(),
            account_name: self.info.account_name.clone(),
            account_type: self.info.account_type.clone(),
            amount: self.info.amount,
            quantity: self.quantity,
            commodity: self.commodity.clone(),
            memo: self.info.memo.clone(),
            reconciled: self.reconciled,
            reconcile_state: self.reconcile_state.clone(),
            account_description: self.account_description.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitDetail {
    pub guid: String,
    pub account_guid: String,
    pub account_name: String,
    pub account_type: String,
    pub amount: f64,
    pub quantity: f64,
    pub commodity: Option<String>,
    pub memo: Option<String>,
    pub reconciled: bool,
    pub reconcile_state: Option<String>,
    pub account_description: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRow {
    pub guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    pub splits: Vec<SplitInfo>,
    pub total_amount: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetail {
    pub guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    pub splits: Vec<SplitDetail>,
    pub total_amount: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetailResult {
    pub transaction: TransactionDetail,
    /// Neighbor following the ledger list ordering (post_date DESC, guid
    /// DESC): `prev` is the row displayed above (more recent), `next` the row
    /// below (older). Both respect the same visibility window as the list.
    pub prev_guid: Option<String>,
    pub next_guid: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    pub search: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub account_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsResult {
    pub transactions: Vec<TransactionRow>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

/// Shared transaction-list WHERE builder. `param_offset` reserves leading
/// placeholder indices already used by the surrounding query.
fn build_filter_conditions(
    filters: &Option<Filters>,
    param_offset: usize,
) -> sql::WhereBuilder {
    let mut w = sql::WhereBuilder::new(param_offset);
    w.push_raw(utils::VISIBLE_TX_FILTER);

    if let Some(ref f) = filters {
        if let Some(ref search) = f.search {
            let s = utils::escape_like(search);
            w.add(
                "(t.description LIKE '%' || ?{} ESCAPE '\\' \
                 OR t.num LIKE '%' || ?{} ESCAPE '\\' \
                 OR t.notes LIKE '%' || ?{} ESCAPE '\\' \
                 OR t.guid IN (SELECT s.tx_guid FROM splits s WHERE s.memo LIKE '%' || ?{} ESCAPE '\\'))",
                vec![
                    Box::new(s.clone()) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(s.clone()),
                    Box::new(s.clone()),
                    Box::new(s),
                ],
            );
        }
        if let Some(ref date_from) = f.date_from {
            w.add("t.post_date >= ?", vec![Box::new(date_from.clone())]);
        }
        if let Some(ref date_to) = f.date_to {
            w.add("t.post_date <= ?", vec![Box::new(date_to.clone())]);
        }
        if let Some(ref account_name) = f.account_name {
            w.add(
                "t.guid IN (SELECT s.tx_guid FROM splits s JOIN accounts a ON a.guid = s.account_guid WHERE a.name = ?)",
                vec![Box::new(account_name.clone())],
            );
        }
    }

    w
}

/// Per-transaction split rows plus the FX-converted transaction total.
type SplitMaps = (std::collections::HashMap<String, Vec<SplitData>>, std::collections::HashMap<String, f64>);

fn fetch_splits_for_txns(conn: &Connection, guids: &[String]) -> Result<SplitMaps, String> {
    if guids.is_empty() {
        return Ok((
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
        ));
    }

    let sql = format!(
        "SELECT s.guid, s.tx_guid, s.account_guid, a.name as account_name, a.account_type,
                s.memo,
                {} as amount,
                {} as signed_amount,
                t.post_date,
                c.mnemonic as commodity,
                {} as quantity,
                s.reconciled,
                a.description as account_description,
                s.reconcile_state
         FROM splits s
         JOIN accounts a ON a.guid = s.account_guid
         JOIN commodities c ON c.guid = a.commodity_guid
         JOIN transactions t ON t.guid = s.tx_guid
         WHERE s.tx_guid IN ({})
         ORDER BY s.tx_guid, ABS(s.value_num) DESC",
        utils::RAW_AMOUNT,
        utils::signed_amount_expr(utils::RAW_AMOUNT, "a.account_type"),
        utils::QUANTITY_AMOUNT,
        sql::in_clause(guids.len())
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

    let param_refs = sql::string_param_refs(guids);

    let report_currency = fx::get_report_currency_mnemonic(conn).unwrap_or_default();
    let fx_history = fx::FxHistory::load(conn)?;

    let rows: Vec<(String, SplitData, f64, String)> = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(1)?, // tx_guid
                SplitData {
                    info: SplitInfo {
                        guid: row.get(0)?,
                        account_guid: row.get(2)?,
                        account_name: row.get(3)?,
                        account_type: row.get(4)?,
                        amount: row.get(6)?,
                        memo: row.get(5)?,
                    },
                    commodity: row.get::<_, Option<String>>(9)?,
                    quantity: row.get(10)?,
                    reconciled: row.get::<_, i64>(11)? != 0,
                    account_description: row.get::<_, Option<String>>(12)?,
                    reconcile_state: row.get::<_, Option<String>>(13)?,
                },
                row.get::<_, f64>(7)?,    // signed_amount
                row.get::<_, String>(8)?, // post_date
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut map: std::collections::HashMap<String, Vec<SplitData>> =
        std::collections::HashMap::new();
    let mut totals: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for (tx_guid, split, signed_amount, post_date) in rows {
        // Sorted by |raw amount| desc, so the first split per transaction is its
        // largest-magnitude one. Its signed value is the transaction total,
        // converted to the report currency at the transaction date's FX.
        totals.entry(tx_guid.clone()).or_insert_with(|| {
            let fx_at = fx_history.map_for_date(&post_date);
            let currency = split.commodity.as_deref().unwrap_or(&report_currency);
            fx::convert(signed_amount, currency, &report_currency, &fx_at)
        });
        map.entry(tx_guid).or_default().push(split);
    }
    Ok((map, totals))
}

pub fn fetch_account_names_inner(conn: &Connection) -> Result<Vec<AccountInfo>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT a.name
             FROM accounts a
             JOIN splits s ON s.account_guid = a.guid
             ORDER BY a.name",
        )
        .map_err(|e| e.to_string())?;

    let names = stmt
        .query_map([], |row| Ok(AccountInfo { name: row.get(0)? }))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(names)
}

#[derive(Debug)]
struct TxRow {
    guid: String,
    post_date: String,
    description: Option<String>,
    num: Option<String>,
    notes: Option<String>,
}

fn map_tx_rows(conn: &Connection, rows: Vec<TxRow>) -> Result<Vec<TransactionRow>, String> {
    let guids: Vec<String> = rows.iter().map(|r| r.guid.clone()).collect();
    let (split_map, total_map) = fetch_splits_for_txns(conn, &guids)?;

    Ok(rows
        .into_iter()
        .map(|r| TransactionRow {
            splits: split_map
                .get(&r.guid)
                .map(|v| v.iter().map(|d| d.info.clone()).collect())
                .unwrap_or_default(),
            total_amount: total_map.get(&r.guid).copied().unwrap_or(0.0),
            guid: r.guid,
            post_date: r.post_date,
            description: r.description,
            num: r.num,
            notes: r.notes,
        })
        .collect())
}

pub fn fetch_transactions_inner(
    conn: &Connection,
    page: i64,
    page_size: i64,
    filters: Option<Filters>,
    sort_by: Option<&str>,
    sort_dir: Option<&str>,
) -> Result<TransactionsResult, String> {
    let w = build_filter_conditions(&filters, 0);
    let where_clause = if w.clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", w.joined())
    };

    let count_sql = format!("SELECT COUNT(*) as cnt FROM transactions t{where_clause}");
    let mut count_stmt = conn.prepare(&count_sql).map_err(|e| e.to_string())?;
    let param_refs = sql::param_refs(&w.params);

    let total: i64 = count_stmt
        .query_row(param_refs.as_slice(), |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let total_pages = utils::total_pages(total, page_size);
    let safe_page = utils::clamp_page(page, total_pages);
    let offset = (safe_page - 1) * page_size;

    // Amount sorting needs the FX-converted totals, which are computed in Rust,
    // so fetch all matching rows, sort, then paginate.
    if sort_by == Some("amount") {
        let query_sql = format!(
            "SELECT guid, post_date, description, num, notes FROM transactions t{where_clause} ORDER BY post_date DESC"
        );
        let mut query_stmt = conn.prepare(&query_sql).map_err(|e| e.to_string())?;
        let all_refs = sql::param_refs(&w.params);
        let rows: Vec<TxRow> = query_stmt
            .query_map(all_refs.as_slice(), |row| {
                Ok(TxRow {
                    guid: row.get(0)?,
                    post_date: row.get(1)?,
                    description: row.get(2)?,
                    num: row.get(3)?,
                    notes: row.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        let mut all_txns = map_tx_rows(conn, rows)?;
        let ascending = sort_dir == Some("asc");
        all_txns.sort_by(|a, b| {
            let cmp = a
                .total_amount
                .partial_cmp(&b.total_amount)
                .unwrap_or(std::cmp::Ordering::Equal);
            if ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });

        let transactions = all_txns
            .into_iter()
            .skip(offset as usize)
            .take(page_size as usize)
            .collect();

        return Ok(TransactionsResult {
            transactions,
            total,
            page: safe_page,
            page_size,
            total_pages,
        });
    }

    let order_clause = match (sort_by, sort_dir) {
        (Some("description"), Some("asc")) => {
            "ORDER BY COALESCE(LOWER(description), '') ASC, guid ASC"
        }
        (Some("description"), _) => "ORDER BY COALESCE(LOWER(description), '') DESC, guid DESC",
        (Some("date"), Some("asc")) => "ORDER BY post_date ASC, guid ASC",
        _ => "ORDER BY post_date DESC, guid DESC",
    };

    let query_sql = format!(
        "SELECT guid, post_date, description, num, notes FROM transactions t{where_clause} {order_clause} LIMIT ? OFFSET ?"
    );

    let mut query_stmt = conn.prepare(&query_sql).map_err(|e| e.to_string())?;
    let all_params: Vec<Box<dyn rusqlite::types::ToSql>> = w
        .params
        .into_iter()
        .chain(vec![
            Box::new(page_size) as Box<dyn rusqlite::types::ToSql>,
            Box::new(offset) as Box<dyn rusqlite::types::ToSql>,
        ])
        .collect();
    let all_refs = sql::param_refs(&all_params);

    let rows: Vec<TxRow> = query_stmt
        .query_map(all_refs.as_slice(), |row| {
            Ok(TxRow {
                guid: row.get(0)?,
                post_date: row.get(1)?,
                description: row.get(2)?,
                num: row.get(3)?,
                notes: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let transactions = map_tx_rows(conn, rows)?;

    Ok(TransactionsResult {
        transactions,
        total,
        page: safe_page,
        page_size,
        total_pages,
    })
}

#[tauri::command]
pub async fn fetch_account_names(state: State<'_, AppState>) -> Result<Vec<AccountInfo>, String> {
    run_blocking(state.db.clone(), fetch_account_names_inner).await
}

#[tauri::command]
pub async fn fetch_transactions(
    state: State<'_, AppState>,
    page: i64,
    page_size: i64,
    filters: Option<Filters>,
    sort_by: Option<String>,
    sort_dir: Option<String>,
) -> Result<TransactionsResult, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_transactions_inner(
            conn,
            page,
            utils::clamp_page_size(page_size),
            filters,
            sort_by.as_deref(),
            sort_dir.as_deref(),
        )
    })
    .await
}

/// Nearest neighbor of a transaction in the ledger list ordering
/// (`post_date DESC, guid DESC`), restricted to the same visibility window as
/// the list. `above = true` finds the row displayed above (more recent),
/// `false` the row below (older).
fn neighbor_guid(
    conn: &Connection,
    post_date: &str,
    guid: &str,
    above: bool,
) -> Result<Option<String>, String> {
    let (op, order) = if above {
        (">", "ORDER BY post_date ASC, guid ASC")
    } else {
        ("<", "ORDER BY post_date DESC, guid DESC")
    };
    let sql = format!(
        "SELECT guid FROM transactions
         WHERE {}
           AND (post_date {op} ?1 OR (post_date = ?1 AND guid {op} ?2))
         {order}
         LIMIT 1",
        utils::VISIBLE_TX_FILTER
    );
    conn.query_row(&sql, rusqlite::params![post_date, guid], |row| row.get(0))
        .optional()
        .map_err(|e| e.to_string())
}

pub fn fetch_transaction_inner(
    conn: &Connection,
    guid: &str,
) -> Result<Option<TransactionDetailResult>, String> {
    let base = conn
        .query_row(
            "SELECT guid, post_date, description, num, notes FROM transactions WHERE guid = ?1",
            [guid],
            |row| {
                Ok(TxRow {
                    guid: row.get(0)?,
                    post_date: row.get(1)?,
                    description: row.get(2)?,
                    num: row.get(3)?,
                    notes: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let Some(base) = base else {
        return Ok(None);
    };

    let guids = [base.guid.clone()];
    let (split_map, total_map) = fetch_splits_for_txns(conn, &guids)?;

    let transaction = TransactionDetail {
        guid: base.guid.clone(),
        post_date: base.post_date.clone(),
        description: base.description,
        num: base.num,
        notes: base.notes,
        splits: split_map
            .get(&base.guid)
            .map(|v| v.iter().map(|d| d.to_detail()).collect())
            .unwrap_or_default(),
        total_amount: total_map.get(&base.guid).copied().unwrap_or(0.0),
    };

    let prev_guid = neighbor_guid(conn, &base.post_date, &base.guid, true)?;
    let next_guid = neighbor_guid(conn, &base.post_date, &base.guid, false)?;

    Ok(Some(TransactionDetailResult {
        transaction,
        prev_guid,
        next_guid,
    }))
}

/// `None` when no transaction with that GUID exists (the frontend shows a
/// not-found state instead of an error card).
#[tauri::command]
pub async fn fetch_transaction(
    state: State<'_, AppState>,
    guid: String,
) -> Result<Option<TransactionDetailResult>, String> {
    run_blocking(state.db.clone(), move |conn| fetch_transaction_inner(conn, &guid)).await
}
