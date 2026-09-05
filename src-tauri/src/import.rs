use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use tauri::State;

use crate::db::{self, connect_source_db, init_db, AppState};
use crate::home::{get_summary, DataSummary};
use crate::utils;

// ── Traits ──────────────────────────────────────────────────────────────────

trait Importable {
    fn guid(&self) -> &str;
    fn has_changes(&self, existing: &Self) -> bool;
    fn upsert(&self, conn: &Connection) -> Result<(), String>;
}

fn process_entities<T: Importable>(
    conn: &Connection,
    src: &[T],
    existing: &HashMap<String, T>,
) -> Result<(i64, i64), String> {
    let mut added = 0;
    let mut updated = 0;
    for r in src {
        let Some(ex) = existing.get(r.guid()) else {
            r.upsert(conn)?;
            added += 1;
            continue;
        };
        if r.has_changes(ex) {
            r.upsert(conn)?;
            updated += 1;
        }
    }
    Ok((added, updated))
}

// ── Log structs ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportLogDetails {
    pub id: i64,
    pub filename: String,
    pub file_path: Option<String>,
    pub imported_at: String,
    pub sha256: Option<String>,
    pub accounts_added: i64,
    pub transactions_added: i64,
    pub splits_added: i64,
    pub commodities_added: i64,
    pub accounts_updated: i64,
    pub transactions_updated: i64,
    pub splits_updated: i64,
    pub accounts_orphaned: i64,
    pub transactions_orphaned: i64,
    pub splits_orphaned: i64,
    pub commodities_orphaned: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportHistoryPage {
    pub logs: Vec<ImportLogDetails>,
    pub total: i64,
    pub page_size: i64,
    pub total_pages: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub accounts_added: i64,
    pub transactions_added: i64,
    pub splits_added: i64,
    pub commodities_added: i64,
    pub accounts_updated: i64,
    pub transactions_updated: i64,
    pub splits_updated: i64,
    pub accounts_orphaned: i64,
    pub transactions_orphaned: i64,
    pub splits_orphaned: i64,
    pub commodities_orphaned: i64,
    pub total_accounts: i64,
    pub total_transactions: i64,
    pub summary: DataSummary,
}

// ── Log helpers ─────────────────────────────────────────────────────────────

fn get_import_logs(
    conn: &Connection,
    limit: i64,
    offset: i64,
) -> Result<Vec<ImportLogDetails>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, filename, file_path, imported_at, sha256,
			        accounts_added, transactions_added, splits_added, commodities_added,
			        accounts_updated, transactions_updated, splits_updated,
			        accounts_orphaned, transactions_orphaned, splits_orphaned, commodities_orphaned
			 FROM import_log ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(rusqlite::params![limit, offset], |row| {
            Ok(ImportLogDetails {
                id: row.get(0)?,
                filename: row.get(1)?,
                file_path: row.get(2)?,
                imported_at: row.get(3)?,
                sha256: row.get(4)?,
                accounts_added: row.get(5)?,
                transactions_added: row.get(6)?,
                splits_added: row.get(7)?,
                commodities_added: row.get(8)?,
                accounts_updated: row.get(9)?,
                transactions_updated: row.get(10)?,
                splits_updated: row.get(11)?,
                accounts_orphaned: row.get(12)?,
                transactions_orphaned: row.get(13)?,
                splits_orphaned: row.get(14)?,
                commodities_orphaned: row.get(15)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

fn count_import_logs(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM import_log", [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

// ── SHA256 ──────────────────────────────────────────────────────────────────

fn file_sha256(path: &str) -> Result<String, String> {
    // Stream instead of slurping so large books don't need to fit in memory.
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Failed to hash file: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect())
}

// ── Source data: shared read+filter logic for preview and import ────────────

struct SourceData {
    commodities: Vec<Commodity>,
    accounts: Vec<Account>,
    splits: Vec<Split>,
    transactions: Vec<Transaction>,
}

fn read_source_data(source: &Connection) -> Result<SourceData, String> {
    let mut accounts = read_account(source)?;
    let mut splits = read_split(source)?;

    accounts.retain(|a| !is_uuid(&a.name));
    let valid_account_guids: HashSet<String> = accounts.iter().map(|a| a.guid.clone()).collect();
    splits.retain(|s| valid_account_guids.contains(&s.account_guid));

    Ok(SourceData {
        commodities: read_commodity(source)?,
        accounts,
        splits,
        transactions: read_source_transaction(source)?,
    })
}

// ── Row structs + Importable impls ──────────────────────────────────────────

#[derive(Clone)]
struct Commodity {
    guid: String,
    mnemonic: String,
    namespace: String,
    fullname: Option<String>,
}

impl Importable for Commodity {
    fn guid(&self) -> &str {
        &self.guid
    }

    fn has_changes(&self, existing: &Self) -> bool {
        existing.mnemonic != self.mnemonic
            || existing.namespace != self.namespace
            || existing.fullname != self.fullname
    }

    fn upsert(&self, conn: &Connection) -> Result<(), String> {
        conn.prepare_cached(
			"INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES (?1, ?2, ?3, ?4)
			 ON CONFLICT(guid) DO UPDATE SET mnemonic=excluded.mnemonic, namespace=excluded.namespace, fullname=excluded.fullname",
		)
		.map_err(|e| e.to_string())?
		.execute(rusqlite::params![self.guid, self.mnemonic, self.namespace, self.fullname])
		.map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[derive(Clone)]
struct Account {
    guid: String,
    name: String,
    account_type: String,
    parent_guid: Option<String>,
    code: Option<String>,
    description: Option<String>,
    placeholder: i64,
    commodity_guid: Option<String>,
}

impl Importable for Account {
    fn guid(&self) -> &str {
        &self.guid
    }

    fn has_changes(&self, existing: &Self) -> bool {
        existing.name != self.name
            || existing.account_type != self.account_type
            || existing.parent_guid != self.parent_guid
            || existing.code != self.code
            || existing.description != self.description
            || existing.placeholder != self.placeholder
            || existing.commodity_guid != self.commodity_guid
    }

    fn upsert(&self, conn: &Connection) -> Result<(), String> {
        conn.prepare_cached(
			"INSERT INTO accounts (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
			 ON CONFLICT(guid) DO UPDATE SET name=excluded.name, account_type=excluded.account_type, parent_guid=excluded.parent_guid, code=excluded.code, description=excluded.description, placeholder=excluded.placeholder, commodity_guid=excluded.commodity_guid",
		)
		.map_err(|e| e.to_string())?
		.execute(rusqlite::params![self.guid, self.name, self.account_type, self.parent_guid, self.code, self.description, self.placeholder, self.commodity_guid])
		.map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[derive(Clone)]
struct Transaction {
    guid: String,
    post_date: String,
    description: Option<String>,
    num: Option<String>,
    /// Free-form note from the source book's `slots` table (GnuCash stores
    /// transaction notes as a 'notes' string slot on the tx GUID).
    notes: Option<String>,
}

impl Importable for Transaction {
    fn guid(&self) -> &str {
        &self.guid
    }

    fn has_changes(&self, existing: &Self) -> bool {
        existing.post_date != self.post_date
            || existing.description != self.description
            || existing.num != self.num
            || existing.notes != self.notes
    }

    fn upsert(&self, conn: &Connection) -> Result<(), String> {
        conn.prepare_cached(
            "INSERT INTO transactions (guid, post_date, description, num, notes) VALUES (?1, ?2, ?3, ?4, ?5)
			 ON CONFLICT(guid) DO UPDATE SET post_date=excluded.post_date, description=excluded.description, num=excluded.num, notes=excluded.notes",
        )
        .map_err(|e| e.to_string())?
        .execute(rusqlite::params![
            self.guid,
            self.post_date,
            self.description,
            self.num,
            self.notes
        ])
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[derive(Clone)]
struct Split {
    guid: String,
    tx_guid: String,
    account_guid: String,
    value_num: i64,
    value_denom: i64,
    quantity_num: i64,
    quantity_denom: i64,
    reconcile_state: Option<String>,
    memo: Option<String>,
}

impl Importable for Split {
    fn guid(&self) -> &str {
        &self.guid
    }

    fn has_changes(&self, existing: &Self) -> bool {
        existing.tx_guid != self.tx_guid
            || existing.account_guid != self.account_guid
            || existing.value_num != self.value_num
            || existing.value_denom != self.value_denom
            || existing.quantity_num != self.quantity_num
            || existing.quantity_denom != self.quantity_denom
            || existing.memo != self.memo
            || existing.reconcile_state != self.reconcile_state
    }

    fn upsert(&self, conn: &Connection) -> Result<(), String> {
        let reconciled = self
            .reconcile_state
            .as_deref()
            .is_some_and(|s| s == "c" || s == "y");
        conn.prepare_cached(
			"INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconciled, reconcile_state, memo) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
			 ON CONFLICT(guid) DO UPDATE SET tx_guid=excluded.tx_guid, account_guid=excluded.account_guid, value_num=excluded.value_num, value_denom=excluded.value_denom, quantity_num=excluded.quantity_num, quantity_denom=excluded.quantity_denom, reconciled=excluded.reconciled, reconcile_state=excluded.reconcile_state, memo=excluded.memo",
		)
		.map_err(|e| e.to_string())?
		.execute(rusqlite::params![self.guid, self.tx_guid, self.account_guid, self.value_num, self.value_denom, self.quantity_num, self.quantity_denom, reconciled as i64, self.reconcile_state, self.memo])
		.map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ── UUID detection ──────────────────────────────────────────────────────────

fn is_uuid(s: &str) -> bool {
    if s.len() == 32 {
        s.chars().all(|c| c.is_ascii_hexdigit())
    } else if s.len() == 36 {
        let hyphenated = s.as_bytes();
        hyphenated[8] == b'-'
            && hyphenated[13] == b'-'
            && hyphenated[18] == b'-'
            && hyphenated[23] == b'-'
            && s.chars().all(|c| c == '-' || c.is_ascii_hexdigit())
    } else {
        false
    }
}

// ── Read functions ──────────────────────────────────────────────────────────

fn read_commodity(conn: &Connection) -> Result<Vec<Commodity>, String> {
    let mut stmt = conn
        .prepare("SELECT guid, mnemonic, namespace, fullname FROM commodities")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Commodity {
                guid: row.get(0)?,
                mnemonic: row.get(1)?,
                namespace: row.get(2)?,
                fullname: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn read_account(conn: &Connection) -> Result<Vec<Account>, String> {
    let mut stmt = conn
		.prepare(
			"SELECT guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid FROM accounts",
		)
		.map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Account {
                guid: row.get(0)?,
                name: row.get(1)?,
                account_type: row.get(2)?,
                parent_guid: row.get(3)?,
                code: row.get(4)?,
                description: row.get(5)?,
                placeholder: row.get(6)?,
                commodity_guid: row.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// App-DB loader (also used for change detection during merge).
fn read_transaction(conn: &Connection) -> Result<Vec<Transaction>, String> {
    let mut stmt = conn
        .prepare("SELECT guid, post_date, description, num, notes FROM transactions")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let num: Option<String> = row.get(3)?;
            Ok(Transaction {
                guid: row.get(0)?,
                post_date: row.get(1)?,
                description: row.get(2)?,
                num: non_empty(num),
                notes: non_empty(row.get(4)?),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Normalize a source-book `post_date` to a lexicographically comparable
/// ISO form. GnuCash normally stores `YYYY-MM-DD HH:MM:SS`, but some
/// producers write the compact `YYYYMMDDHHMMSS` form, which would silently
/// break every date-range comparison (digits sort after `-`). Anything not
/// composed of dates/times characters is passed through unchanged.
fn normalize_post_date(raw: &str) -> String {
    let clean = raw.trim();
    let digits: String = clean.chars().filter(|c| c.is_ascii_digit()).collect();
    let plausible = clean
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '-' | ':' | ' '))
        && digits.len() >= 8;
    if !plausible {
        return clean.to_string();
    }
    let date = format!("{}-{}-{}", &digits[..4], &digits[4..6], &digits[6..8]);
    if digits.len() == 8 {
        return date;
    }
    // Remaining pairs are HH, MM, SS… joined back with colons.
    let time = digits.as_bytes()[8..]
        .chunks(2)
        .map(|pair| std::str::from_utf8(pair).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(":");
    format!("{date} {time}")
}

/// Source-book loader: joins GnuCash's `slots` table, where transaction notes
/// live ('notes' string slot keyed by the tx GUID).
fn read_source_transaction(source: &Connection) -> Result<Vec<Transaction>, String> {
    let mut stmt = source
        .prepare(
            "SELECT t.guid, t.post_date, t.description, t.num,
                    (SELECT s.string_val FROM slots s
                     WHERE s.obj_guid = t.guid AND s.name = 'notes' LIMIT 1) as notes
             FROM transactions t",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let num: Option<String> = row.get(3)?;
            let notes: Option<String> = row.get(4)?;
            Ok(Transaction {
                guid: row.get(0)?,
                post_date: normalize_post_date(row.get::<_, String>(1)?.trim()),
                description: row.get(2)?,
                num: non_empty(num),
                notes: non_empty(notes),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn read_split(conn: &Connection) -> Result<Vec<Split>, String> {
    let mut stmt = conn
		.prepare(
			"SELECT guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state, memo FROM splits",
		)
		.map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let memo: Option<String> = row.get(8)?;
            Ok(Split {
                guid: row.get(0)?,
                tx_guid: row.get(1)?,
                account_guid: row.get(2)?,
                value_num: row.get(3)?,
                value_denom: row.get(4)?,
                quantity_num: row.get(5)?,
                quantity_denom: row.get(6)?,
                reconcile_state: row.get(7)?,
                memo: non_empty(memo),
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty())
}

// ── Orphan helpers ──────────────────────────────────────────────────────────

const VALID_TABLES: &[&str] = &["commodities", "accounts", "transactions", "splits"];

fn validate_table(table: &str) -> Result<(), String> {
    if VALID_TABLES.contains(&table) {
        Ok(())
    } else {
        Err(format!("Invalid table name: {}", table))
    }
}

/// Materialize source GUIDs into a TEMP table so orphan detection can use an
/// anti-join instead of a giant `IN (...)` list (SQLite caps bound parameters
/// at ~32k, which real books exceed).
fn populate_source_guids(conn: &Connection, name: &str, guids: &[String]) -> Result<(), String> {
    conn.execute_batch(&format!(
        "DROP TABLE IF EXISTS {name};
         CREATE TEMP TABLE {name} (guid TEXT NOT NULL PRIMARY KEY);"
    ))
    .map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(&format!("INSERT OR IGNORE INTO {name} (guid) VALUES (?1)"))
        .map_err(|e| e.to_string())?;
    for guid in guids {
        stmt.execute([guid]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn count_orphaned(conn: &Connection, table: &str, src_table: &str) -> Result<i64, String> {
    validate_table(table)?;
    let sql =
        format!("SELECT COUNT(*) FROM {table} WHERE guid NOT IN (SELECT guid FROM {src_table})");
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

fn delete_orphans(conn: &Connection, table: &str, src_table: &str) -> Result<i64, String> {
    validate_table(table)?;
    let sql = format!("DELETE FROM {table} WHERE guid NOT IN (SELECT guid FROM {src_table})");
    conn.execute(&sql, [])
        .map(|n| n as i64)
        .map_err(|e| e.to_string())
}

// ── Import ──────────────────────────────────────────────────────────────────

fn import_gnucash_inner(
    dest_conn: &Connection,
    source_path: &str,
    filename: &str,
    sha256: &str,
) -> Result<ImportResult, String> {
    let source = connect_source_db(&std::path::PathBuf::from(source_path))
        .map_err(|e| format!("Failed to open source DB: {}", e))?;

    init_db(dest_conn).map_err(|e| format!("Failed to init dest DB: {}", e))?;

    let src = read_source_data(&source)?;

    let commodity_guids: Vec<String> = src.commodities.iter().map(|r| r.guid.clone()).collect();
    let account_guids: Vec<String> = src.accounts.iter().map(|r| r.guid.clone()).collect();
    let tx_guids: Vec<String> = src.transactions.iter().map(|r| r.guid.clone()).collect();
    let split_guids: Vec<String> = src.splits.iter().map(|r| r.guid.clone()).collect();

    populate_source_guids(dest_conn, "tmp_src_commodities", &commodity_guids)?;
    populate_source_guids(dest_conn, "tmp_src_accounts", &account_guids)?;
    populate_source_guids(dest_conn, "tmp_src_transactions", &tx_guids)?;
    populate_source_guids(dest_conn, "tmp_src_splits", &split_guids)?;

    let commodities_orphaned = count_orphaned(dest_conn, "commodities", "tmp_src_commodities")?;
    let accounts_orphaned = count_orphaned(dest_conn, "accounts", "tmp_src_accounts")?;
    let transactions_orphaned = count_orphaned(dest_conn, "transactions", "tmp_src_transactions")?;
    let splits_orphaned = count_orphaned(dest_conn, "splits", "tmp_src_splits")?;

    let mut commodities_added = 0i64;
    let mut commodities_updated = 0i64;
    let mut accounts_added = 0i64;
    let mut accounts_updated = 0i64;
    let mut transactions_added = 0i64;
    let mut transactions_updated = 0i64;
    let mut splits_added = 0i64;
    let mut splits_updated = 0i64;

    let tx_result = (|| -> Result<(), String> {
        dest_conn
            .execute_batch("BEGIN TRANSACTION")
            .map_err(|e| e.to_string())?;

        let existing_commodities: HashMap<String, Commodity> = read_commodity(dest_conn)?
            .into_iter()
            .map(|c| (c.guid.clone(), c))
            .collect();
        let existing_accounts: HashMap<String, Account> = read_account(dest_conn)?
            .into_iter()
            .map(|a| (a.guid.clone(), a))
            .collect();
        let existing_txns: HashMap<String, Transaction> = read_transaction(dest_conn)?
            .into_iter()
            .map(|t| (t.guid.clone(), t))
            .collect();
        let existing_splits: HashMap<String, Split> = read_split(dest_conn)?
            .into_iter()
            .map(|s| (s.guid.clone(), s))
            .collect();

        (commodities_added, commodities_updated) =
            process_entities(dest_conn, &src.commodities, &existing_commodities)?;
        (accounts_added, accounts_updated) =
            process_entities(dest_conn, &src.accounts, &existing_accounts)?;
        (transactions_added, transactions_updated) =
            process_entities(dest_conn, &src.transactions, &existing_txns)?;
        (splits_added, splits_updated) =
            process_entities(dest_conn, &src.splits, &existing_splits)?;

        delete_orphans(dest_conn, "splits", "tmp_src_splits")?;
        delete_orphans(dest_conn, "transactions", "tmp_src_transactions")?;
        delete_orphans(dest_conn, "accounts", "tmp_src_accounts")?;
        delete_orphans(dest_conn, "commodities", "tmp_src_commodities")?;

        let has_changes = accounts_added != 0
            || transactions_added != 0
            || splits_added != 0
            || commodities_added != 0
            || accounts_updated != 0
            || transactions_updated != 0
            || splits_updated != 0
            || commodities_updated != 0
            || accounts_orphaned != 0
            || transactions_orphaned != 0
            || splits_orphaned != 0
            || commodities_orphaned != 0;

        if has_changes {
            let now = utils::iso_now();
            dest_conn
				.execute(
					"INSERT INTO import_log (filename, file_path, imported_at, sha256, accounts_added, transactions_added, splits_added, commodities_added, accounts_updated, transactions_updated, splits_updated, accounts_orphaned, transactions_orphaned, splits_orphaned, commodities_orphaned) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
					rusqlite::params![filename, source_path, now, sha256, accounts_added, transactions_added, splits_added, commodities_added, accounts_updated, transactions_updated, splits_updated, accounts_orphaned, transactions_orphaned, splits_orphaned, commodities_orphaned],
				)
				.map_err(|e| e.to_string())?;
        }

        dest_conn
            .execute_batch("COMMIT")
            .map_err(|e| e.to_string())?;

        // Temp tables live on the long-lived connection; drop them now instead
        // of letting them linger until the next import recreates them.
        let _ = dest_conn.execute_batch(
            "DROP TABLE IF EXISTS tmp_src_commodities;
             DROP TABLE IF EXISTS tmp_src_accounts;
             DROP TABLE IF EXISTS tmp_src_transactions;
             DROP TABLE IF EXISTS tmp_src_splits;",
        );

        Ok(())
    })();

    if let Err(e) = tx_result {
        dest_conn.execute_batch("ROLLBACK").ok();
        return Err(e);
    }

    let summary = get_summary(dest_conn)?;

    let total_accounts: i64 = dest_conn
        .query_row("SELECT COUNT(*) as cnt FROM accounts", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let total_transactions: i64 = dest_conn
        .query_row("SELECT COUNT(*) as cnt FROM transactions", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;

    Ok(ImportResult {
        accounts_added,
        transactions_added,
        splits_added,
        commodities_added,
        accounts_updated,
        transactions_updated,
        splits_updated,
        accounts_orphaned,
        transactions_orphaned,
        splits_orphaned,
        commodities_orphaned,
        total_accounts,
        total_transactions,
        summary,
    })
}

// ── Tauri commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn import_gnucash(
    state: State<'_, AppState>,
    path: String,
    filename: String,
) -> Result<ImportResult, String> {
    utils::validate_gnucash_path(&path)?;
    // Hash before acquiring the DB lock so the lock isn't held during file I/O.
    let sha256 = file_sha256(&path)?;
    db::run_blocking(state.db.clone(), move |conn| {
        import_gnucash_inner(conn, &path, &filename, &sha256)
    })
    .await
}

#[tauri::command]
pub async fn fetch_import_history(
    state: State<'_, AppState>,
    page: i64,
    page_size: i64,
) -> Result<ImportHistoryPage, String> {
    db::run_blocking(state.db.clone(), move |conn| {
        let page_size = utils::clamp_page_size(page_size);
        let total = count_import_logs(conn)?;
        let total_pages = utils::total_pages(total, page_size);
        let safe_page = utils::clamp_page(page, total_pages);
        let logs = get_import_logs(conn, page_size, (safe_page - 1) * page_size)?;
        Ok(ImportHistoryPage {
            logs,
            total,
            page_size,
            total_pages,
        })
    })
    .await
}

#[tauri::command]
pub async fn reimport_last(state: State<'_, AppState>) -> Result<ImportResult, String> {
    // Look up the last import outside the blocking closure, then hash + merge.
    let (file_path, filename) = db::run_blocking(state.db.clone(), |conn| {
        conn.query_row(
            "SELECT file_path, filename FROM import_log WHERE file_path IS NOT NULL ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|e| format!("Failed to look up previous import: {}", e))
    })
    .await?;
    utils::validate_gnucash_path(&file_path)?;
    let sha256 = file_sha256(&file_path)?;

    db::run_blocking(state.db.clone(), move |conn| {
        import_gnucash_inner(conn, &file_path, &filename, &sha256)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_uuid_matches_both_formats() {
        assert!(is_uuid("0123456789abcdef0123456789abcdef"));
        assert!(is_uuid("01234567-89ab-cdef-0123-456789abcdef"));
        assert!(!is_uuid("Checking Account"));
        assert!(!is_uuid("0123456789ABCDEF0123456789ABCDEFZ"));
        assert!(!is_uuid(""));
    }

    #[test]
    fn normalize_post_date_handles_known_shapes() {
        assert_eq!(
            normalize_post_date("2024-01-15 10:30:00"),
            "2024-01-15 10:30:00"
        );
        assert_eq!(normalize_post_date("2024-01-15"), "2024-01-15");
        assert_eq!(
            normalize_post_date("20240115103000"),
            "2024-01-15 10:30:00"
        );
        assert_eq!(normalize_post_date(" 20240115 "), "2024-01-15");
    }

    #[test]
    fn normalize_post_date_passthrough_unexpected_shapes() {
        assert_eq!(normalize_post_date("not-a-date"), "not-a-date");
        assert_eq!(normalize_post_date(""), "");
        assert_eq!(normalize_post_date("2024-01"), "2024-01");
    }
}
