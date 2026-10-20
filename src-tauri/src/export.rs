use rusqlite::Connection;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tauri::State;

use crate::db::{run_blocking, run_io, AppError, AppState};
use crate::tree;
use crate::utils;

/// One CSV row per split, mirroring GnuCash's multiline export so the file can
/// be re-imported via `File → Import → Import Transactions from CSV`
/// (Multi-split checked, `GnuCash Export Settings` preset). Transaction-level
/// fields repeat on every split row — the importer tolerates repeats and it
/// keeps the CSV self-describing without blank-continuation parsing.
///
/// This is a complete dump: unlike the ledger queries it deliberately does NOT
/// apply the visibility window, so future-dated transactions survive the
/// export. A round trip back into Spendo flags those rows as errors (the CSV
/// importer keeps its future-date block).
#[derive(Debug, Clone)]
pub struct ExportSplitRow {
    pub tx_guid: String,
    pub split_guid: String,
    /// `YYYY-MM-DD` (date part of `post_date`).
    pub post_date: String,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    /// Mnemonic of the split account's commodity.
    pub commodity: Option<String>,
    pub memo: Option<String>,
    /// Colon-joined path excluding the internal ROOT, e.g. `Assets:Bank:Checking`.
    pub full_account_name: String,
    /// Leaf account name.
    pub account_name: String,
    /// Raw native amount (`value_num/value_denom`): no FX, no sign flips.
    pub amount: f64,
    /// GnuCash reconcile state ('n'/'c'/'y') or blank.
    pub reconcile_state: Option<String>,
    /// `amount/quantity` for investments, else `1`.
    pub price: f64,
}

/// One CSV row per account in parent-before-child (pre-order DFS) order so a
/// GnuCash account import can recreate parents first. Balances are
/// deliberately omitted: they are report-currency converted snapshots and
/// would corrupt a re-import — balances rebuild from the transactions file.
#[derive(Debug, Clone)]
pub struct ExportAccountRow {
    pub guid: String,
    /// `None` for top-level accounts (parent is the internal ROOT or missing).
    pub parent_guid: Option<String>,
    pub account_type: String,
    /// Colon-joined path excluding the internal ROOT.
    pub full_name: String,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub commodity: Option<String>,
    pub commodity_namespace: Option<String>,
    pub placeholder: bool,
}

fn price_for(amount: f64, quantity: f64) -> f64 {
    if quantity.abs() > 1e-12 && amount.is_finite() && quantity.is_finite() {
        amount / quantity
    } else {
        1.0
    }
}

fn fetch_export_transactions_inner(conn: &Connection) -> Result<Vec<ExportSplitRow>, AppError> {
    let metas = tree::load_account_metas(conn)?;
    let mut path_cache: HashMap<String, String> = HashMap::new();
    for guid in metas.keys() {
        path_cache.insert(guid.clone(), tree::full_account_path(guid, &metas, ":"));
    }

    let sql = format!(
        "SELECT t.guid, substr(t.post_date, 1, 10), t.description, t.num, t.notes,
                s.guid, s.account_guid, s.memo, s.reconcile_state,
                {} as amount,
                {} as quantity,
                a.name as account_name,
                c.mnemonic as commodity
         FROM splits s
         JOIN transactions t ON t.guid = s.tx_guid
         JOIN accounts a ON a.guid = s.account_guid
         LEFT JOIN commodities c ON c.guid = a.commodity_guid
         ORDER BY t.post_date DESC, t.guid DESC,
                  ABS(CAST(s.value_num AS REAL) / CAST(s.value_denom AS REAL)) DESC,
                  s.guid ASC",
        utils::RAW_AMOUNT,
        utils::QUANTITY_AMOUNT,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, f64>(9)?,
                row.get::<_, f64>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, Option<String>>(12)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows
        .into_iter()
        .map(
            |(
                tx_guid,
                post_date,
                description,
                num,
                notes,
                split_guid,
                account_guid,
                memo,
                reconcile_state,
                amount,
                quantity,
                account_name,
                commodity,
            )| {
                let full_account_name = path_cache
                    .get(&account_guid)
                    .cloned()
                    .unwrap_or_else(|| account_name.clone());
                ExportSplitRow {
                    tx_guid,
                    split_guid,
                    post_date,
                    description,
                    num,
                    notes,
                    commodity,
                    memo,
                    full_account_name,
                    account_name,
                    amount,
                    reconcile_state,
                    price: price_for(amount, quantity),
                }
            },
        )
        .collect())
}

fn fetch_export_accounts_inner(conn: &Connection) -> Result<Vec<ExportAccountRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT a.guid, a.parent_guid, a.name, a.account_type, a.code, a.description,
                    a.placeholder, c.mnemonic, c.namespace
             FROM accounts a
             LEFT JOIN commodities c ON c.guid = a.commodity_guid
             WHERE a.account_type != 'ROOT'",
    )?;
    struct Raw {
        guid: String,
        parent_guid: Option<String>,
        name: String,
        account_type: String,
        code: Option<String>,
        description: Option<String>,
        placeholder: bool,
        commodity: Option<String>,
        commodity_namespace: Option<String>,
    }
    let raws = stmt
        .query_map([], |row| {
            Ok(Raw {
                guid: row.get(0)?,
                parent_guid: row.get(1)?,
                name: row.get(2)?,
                account_type: row.get(3)?,
                code: row.get(4)?,
                description: row.get(5)?,
                placeholder: row.get::<_, i64>(6)? != 0,
                commodity: row.get(7)?,
                commodity_namespace: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // Full paths need the complete map (including the excluded ROOT row).
    let metas = tree::load_account_metas(conn)?;
    // Null the parent link when it points at the internal ROOT or a missing
    // row so top-level accounts import as roots instead of dangling.
    let exportable_parent = |guid: &str, parent: &Option<String>| -> Option<String> {
        let p = parent.clone()?;
        match metas.get(&p) {
            Some(meta) if meta.account_type != "ROOT" && p != guid => Some(p),
            _ => None,
        }
    };

    let mut by_guid: HashMap<String, Raw> = HashMap::new();
    for r in raws {
        by_guid.insert(r.guid.clone(), r);
    }
    // Parent→children adjacency over the exported rows only; orphans whose
    // parent isn't exported (ROOT/missing) become roots.
    let index = tree::build_index(by_guid.values().map(|r| {
        let parent = match &r.parent_guid {
            Some(p) if by_guid.contains_key(p) => Some(p.clone()),
            _ => None,
        };
        (r.guid.clone(), parent, r.name.clone())
    }));

    fn visit(
        guid: &str,
        by_guid: &HashMap<String, Raw>,
        index: &tree::TreeIndex,
        metas: &HashMap<String, tree::AccountMeta>,
        exportable_parent: &impl Fn(&str, &Option<String>) -> Option<String>,
        out: &mut Vec<ExportAccountRow>,
        visited: &mut HashSet<String>,
    ) {
        if !visited.insert(guid.to_string()) {
            return;
        }
        if let Some(r) = by_guid.get(guid) {
            out.push(ExportAccountRow {
                parent_guid: exportable_parent(guid, &r.parent_guid),
                full_name: tree::full_account_path(guid, metas, ":"),
                guid: r.guid.clone(),
                account_type: r.account_type.clone(),
                name: r.name.clone(),
                code: r.code.clone(),
                description: r.description.clone(),
                commodity: r.commodity.clone(),
                commodity_namespace: r.commodity_namespace.clone(),
                placeholder: r.placeholder,
            });
        }
        if let Some(kids) = index.children.get(guid) {
            for kid in kids {
                visit(kid, by_guid, index, metas, exportable_parent, out, visited);
            }
        }
    }

    let mut out = Vec::with_capacity(by_guid.len());
    let mut visited = HashSet::new();
    for root in &index.roots {
        visit(
            root,
            &by_guid,
            &index,
            &metas,
            &exportable_parent,
            &mut out,
            &mut visited,
        );
    }
    // Cycle orphans never reachable from a root: emit them by name so no
    // account silently disappears from the export.
    let mut leftover: Vec<&String> = by_guid.keys().filter(|g| !visited.contains(*g)).collect();
    leftover.sort();
    for guid in leftover {
        visit(
            guid,
            &by_guid,
            &index,
            &metas,
            &exportable_parent,
            &mut out,
            &mut visited,
        );
    }
    Ok(out)
}

// ── CSV assembly + wire inputs ───────────────────────────────────────────────

/// Column order of the transactions export (GnuCash multi-split shape).
const TRANSACTIONS_EXPORT_HEADERS: [&str; 13] = [
    "Date",
    "Transaction ID",
    "Number",
    "Description",
    "Notes",
    "Commodity/Currency",
    "Memo",
    "Full Account Name",
    "Account Name",
    "Amount",
    "Reconciled",
    "Price",
    "Split ID",
];

/// Column order of the account-tree export, parents before children.
const ACCOUNTS_EXPORT_HEADERS: [&str; 10] = [
    "Type",
    "Full Name",
    "Name",
    "Code",
    "Description",
    "Commodity",
    "Commodity Namespace",
    "Placeholder",
    "GUID",
    "Parent GUID",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsExportInput {
    pub path: String,
    pub delimiter: String,
    /// `"iso"` keeps `YYYY-MM-DD`; `"locale"` renders via `date_parts` /
    /// `date_separator` (derived from `Intl.DateTimeFormat` in the webview so
    /// the file matches the locale the user sees).
    pub date_format: String,
    pub date_parts: Option<String>,
    pub date_separator: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsExportInput {
    pub path: String,
    pub delimiter: String,
}

fn validate_delimiter(raw: &str) -> Result<u8, AppError> {
    match raw {
        "," => Ok(b','),
        ";" => Ok(b';'),
        other => Err(AppError::Validation(format!(
            "That delimiter doesn't look right: {other:?}. Use , or ;"
        ))),
    }
}

fn validate_export_path(path: &str) -> Result<(), AppError> {
    if path.trim().is_empty() {
        return Err(AppError::Validation(
            "Pick where to save that export".to_string(),
        ));
    }
    Ok(())
}

/// `None` = ISO passthrough. Locale parts must be one of the three numeric
/// orders with a short separator.
fn validate_date_options(
    date_format: &str,
    parts: Option<&str>,
    separator: Option<&str>,
) -> Result<Option<(String, String)>, AppError> {
    match date_format {
        "iso" => Ok(None),
        "locale" => {
            let parts = parts.unwrap_or("dmy").to_string();
            if !matches!(parts.as_str(), "dmy" | "mdy" | "ymd") {
                return Err(AppError::Validation(format!(
                    "Unsupported date part order: {parts}"
                )));
            }
            let separator = separator.unwrap_or("/").to_string();
            if separator.is_empty() || separator.chars().count() > 2 {
                return Err(AppError::Validation(
                    "Unsupported date separator".to_string(),
                ));
            }
            Ok(Some((parts, separator)))
        }
        other => Err(AppError::Validation(format!(
            "That date format doesn't look right: {other}"
        ))),
    }
}

/// Render an already-validated `YYYY-MM-DD` date; malformed input passes
/// through untouched rather than dropping data.
fn format_export_date(iso: &str, locale: Option<&(String, String)>) -> String {
    let Some((parts, separator)) = locale else {
        return iso.to_string();
    };
    let mut it = iso.splitn(3, '-');
    let (Some(year), Some(month), Some(day)) = (it.next(), it.next(), it.next()) else {
        return iso.to_string();
    };
    match parts.as_str() {
        "mdy" => format!("{month}{separator}{day}{separator}{year}"),
        "ymd" => format!("{year}{separator}{month}{separator}{day}"),
        _ => format!("{day}{separator}{month}{separator}{year}"),
    }
}

/// BOM + `csv::Writer` output. The writer's default quoting (delimiter, quote,
/// newline) matches the previous frontend `escapeCell` behaviour.
fn build_csv_bytes(
    delimiter: u8,
    headers: &[&str],
    rows: &[Vec<String>],
) -> Result<Vec<u8>, AppError> {
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());
    writer
        .write_record(headers)
        .map_err(|e| AppError::Validation(format!("We couldn't put that CSV together: {e}")))?;
    for row in rows {
        writer
            .write_record(row)
            .map_err(|e| AppError::Validation(format!("We couldn't put that CSV together: {e}")))?;
    }
    let mut bytes = writer
        .into_inner()
        .map_err(|e| AppError::Validation(format!("We couldn't put that CSV together: {e}")))?;
    let mut out = Vec::with_capacity(bytes.len() + 3);
    out.extend_from_slice(b"\xEF\xBB\xBF");
    out.append(&mut bytes);
    Ok(out)
}

fn split_row_cells(row: &ExportSplitRow, locale: Option<&(String, String)>) -> Vec<String> {
    vec![
        format_export_date(&row.post_date, locale),
        row.tx_guid.clone(),
        row.num.clone().unwrap_or_default(),
        row.description.clone().unwrap_or_default(),
        row.notes.clone().unwrap_or_default(),
        row.commodity.clone().unwrap_or_default(),
        row.memo.clone().unwrap_or_default(),
        row.full_account_name.clone(),
        row.account_name.clone(),
        format!("{}", row.amount),
        row.reconcile_state.clone().unwrap_or_default(),
        format!("{}", row.price),
        row.split_guid.clone(),
    ]
}

fn account_row_cells(row: &ExportAccountRow) -> Vec<String> {
    vec![
        row.account_type.clone(),
        row.full_name.clone(),
        row.name.clone(),
        row.code.clone().unwrap_or_default(),
        row.description.clone().unwrap_or_default(),
        row.commodity.clone().unwrap_or_default(),
        row.commodity_namespace.clone().unwrap_or_default(),
        if row.placeholder { "TRUE" } else { "FALSE" }.to_string(),
        row.guid.clone(),
        row.parent_guid.clone().unwrap_or_default(),
    ]
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Full-book transactions CSV written straight to the user's chosen path: rows
/// are read under the DB lock, the file write happens on `run_io` without it.
#[tauri::command]
pub async fn export_transactions_csv(
    state: State<'_, AppState>,
    input: TransactionsExportInput,
) -> Result<(), AppError> {
    let delimiter = validate_delimiter(&input.delimiter)?;
    validate_export_path(&input.path)?;
    state.ensure_not_active_book(&PathBuf::from(input.path.trim()))?;
    let locale = validate_date_options(
        &input.date_format,
        input.date_parts.as_deref(),
        input.date_separator.as_deref(),
    )?;

    let rows = run_blocking(state.db.clone(), |conn| {
        fetch_export_transactions_inner(conn)
    })
    .await?;
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| split_row_cells(r, locale.as_ref()))
        .collect();
    let bytes = build_csv_bytes(delimiter, &TRANSACTIONS_EXPORT_HEADERS, &cells)?;
    let path = input.path;
    run_io(move || {
        std::fs::write(&path, bytes).map_err(|e| AppError::Io(format!("Failed to write file: {e}")))
    })
    .await
}

/// Full account-tree CSV written straight to the user's chosen path.
#[tauri::command]
pub async fn export_accounts_csv(
    state: State<'_, AppState>,
    input: AccountsExportInput,
) -> Result<(), AppError> {
    let delimiter = validate_delimiter(&input.delimiter)?;
    validate_export_path(&input.path)?;
    state.ensure_not_active_book(&PathBuf::from(input.path.trim()))?;

    let rows = run_blocking(state.db.clone(), fetch_export_accounts_inner).await?;
    let cells: Vec<Vec<String>> = rows.iter().map(account_row_cells).collect();
    let bytes = build_csv_bytes(delimiter, &ACCOUNTS_EXPORT_HEADERS, &cells)?;
    let path = input.path;
    run_io(move || {
        std::fs::write(&path, bytes).map_err(|e| AppError::Io(format!("Failed to write file: {e}")))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES
             ('c-eur', 'EUR', 'CURRENCY'), ('c-aapl', 'AAPL', 'NASDAQ')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid, code, description) VALUES
             ('root', 'Root Account', 'ROOT', NULL, 0, 'c-eur', NULL, NULL),
             ('assets', 'Assets', 'ASSET', 'root', 1, 'c-eur', NULL, NULL),
             ('checking', 'Checking', 'BANK', 'assets', 0, 'c-eur', '1001', 'Everyday'),
             ('broker', 'Broker', 'STOCK', 'assets', 0, 'c-aapl', NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes) VALUES
             ('t1', '2024-03-01 10:00:00', 'Groceries', '12', 'weekly'),
             ('t-future', '2099-01-01 00:00:00', 'Future', NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state, memo) VALUES
             ('s1', 't1', 'checking', -5000, 100, -5000, 100, 'c', 'card'),
             ('s2', 't1', 'broker', 100000, 100, 10, 1, 'n', NULL),
             ('s-future', 't-future', 'checking', 100, 100, 100, 100, 'n', NULL)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn split_rows_include_future_dates_and_full_paths() {
        let conn = setup();
        let rows = fetch_export_transactions_inner(&conn).unwrap();
        // Complete dump: the future-dated transaction is included.
        assert_eq!(rows.len(), 3);
        let future = rows.iter().find(|r| r.tx_guid == "t-future").unwrap();
        assert_eq!(future.post_date, "2099-01-01");

        let t1: Vec<&ExportSplitRow> = rows.iter().filter(|r| r.tx_guid == "t1").collect();
        assert_eq!(t1.len(), 2);
        // Largest-magnitude split first.
        assert_eq!(t1[0].account_name, "Broker");
        assert_eq!(t1[0].full_account_name, "Assets:Broker");
        assert_eq!(t1[0].post_date, "2024-03-01");
        assert_eq!(t1[0].amount, 1000.0);
        assert_eq!(t1[0].price, 100.0);
        assert_eq!(t1[0].commodity.as_deref(), Some("AAPL"));
        assert_eq!(t1[1].full_account_name, "Assets:Checking");
        assert_eq!(t1[1].amount, -50.0);
        assert_eq!(t1[1].price, 1.0);
        assert_eq!(t1[1].memo.as_deref(), Some("card"));
        assert_eq!(t1[1].reconcile_state.as_deref(), Some("c"));
        // Transaction-level fields repeat on every split row.
        assert!(t1
            .iter()
            .all(|r| r.description.as_deref() == Some("Groceries")));
        assert!(t1.iter().all(|r| r.num.as_deref() == Some("12")));
    }

    #[test]
    fn export_csv_bytes_have_bom_headers_and_escaping() {
        let rows = vec![vec![
            "a,b".to_string(),
            "line\nbreak".to_string(),
            "plain".to_string(),
        ]];
        let bytes = build_csv_bytes(b',', &["H1", "H2", "H3"], &rows).unwrap();
        assert!(bytes.starts_with(b"\xEF\xBB\xBF"));
        let text = String::from_utf8(bytes[3..].to_vec())
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(text, "H1,H2,H3\n\"a,b\",\"line\nbreak\",plain\n");
    }

    #[test]
    fn export_csv_supports_semicolon() {
        let bytes = build_csv_bytes(b';', &["A", "B"], &[vec!["1".into(), "2".into()]]).unwrap();
        let text = String::from_utf8(bytes[3..].to_vec())
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(text, "A;B\n1;2\n");
    }

    #[test]
    fn export_date_options_validate_and_format() {
        assert!(validate_date_options("iso", None, None).unwrap().is_none());
        let locale = validate_date_options("locale", Some("dmy"), Some("/")).unwrap();
        assert_eq!(
            format_export_date("2024-03-01", locale.as_ref()),
            "01/03/2024"
        );
        let locale = validate_date_options("locale", Some("mdy"), Some(".")).unwrap();
        assert_eq!(
            format_export_date("2024-03-01", locale.as_ref()),
            "03.01.2024"
        );
        let locale = validate_date_options("locale", Some("ymd"), Some("-")).unwrap();
        assert_eq!(
            format_export_date("2024-03-01", locale.as_ref()),
            "2024-03-01"
        );
        // Malformed input passes through rather than dropping the date.
        assert_eq!(format_export_date("not-a-date", None), "not-a-date");
        assert!(validate_date_options("us", None, None).is_err());
        assert!(validate_date_options("locale", Some("ydm"), None).is_err());
        assert!(validate_date_options("locale", None, Some("")).is_err());
        assert!(validate_delimiter("|").is_err());
    }

    #[test]
    fn account_rows_emit_parents_before_children_with_full_names() {
        let conn = setup();
        let rows = fetch_export_accounts_inner(&conn).unwrap();
        assert_eq!(rows.len(), 3);
        // Pre-order DFS: parent first, children name-sorted.
        assert_eq!(
            rows.iter().map(|r| r.guid.as_str()).collect::<Vec<_>>(),
            vec!["assets", "broker", "checking"]
        );
        let assets = &rows[0];
        assert_eq!(assets.full_name, "Assets");
        assert_eq!(assets.parent_guid, None);
        assert!(assets.placeholder);
        let broker = &rows[1];
        assert_eq!(broker.full_name, "Assets:Broker");
        assert_eq!(broker.parent_guid.as_deref(), Some("assets"));
        assert_eq!(broker.commodity.as_deref(), Some("AAPL"));
        assert_eq!(broker.commodity_namespace.as_deref(), Some("NASDAQ"));
        let checking = &rows[2];
        assert_eq!(checking.full_name, "Assets:Checking");
        assert_eq!(checking.code.as_deref(), Some("1001"));
        assert_eq!(checking.description.as_deref(), Some("Everyday"));
    }
}
