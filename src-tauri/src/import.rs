use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use tauri::State;

use crate::db::{self, connect_source_db, init_db, AppError, AppState};
use crate::history;
use crate::home::{get_summary, DataSummary};
use crate::prices::{self, PriceError, PricesResult};
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

// ── Result struct ────────────────────────────────────────────────────────────

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
    pub prices: PricesResult,
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
    // Remaining pairs are HH, MM, SS… joined back with colons. An odd-length
    // tail (e.g. 9 digits) would emit a 1-char component (`"2024-01-15 1"`)
    // that sorts/compares wrong, so pass it through for the caller to handle.
    let tail = &digits[8..];
    if !tail.len().is_multiple_of(2) || tail.len() < 6 {
        return clean.to_string();
    }
    let time = tail
        .as_bytes()
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

/// Materialize source GUIDs into a TEMP table so orphan detection can use an
/// anti-join instead of a giant `IN (...)` list (SQLite caps bound parameters
/// at ~32k, which real books exceed). Inserts run in one txn instead of one
/// autocommit per GUID.
fn populate_source_guids(conn: &Connection, name: &str, guids: &[String]) -> Result<(), AppError> {
    conn.execute_batch(&format!(
        "DROP TABLE IF EXISTS {name};
         CREATE TEMP TABLE {name} (guid TEXT NOT NULL PRIMARY KEY);"
    ))
    ?;
    db::with_tx_err(conn, || {
        let mut stmt = conn
            .prepare(&format!("INSERT OR IGNORE INTO {name} (guid) VALUES (?1)"))
            ?;
        for guid in guids {
            stmt.execute([guid])?;
        }
        Ok(())
    })
}

fn drop_source_guid_tables(conn: &Connection) {
    let _ = conn.execute_batch(
        "DROP TABLE IF EXISTS tmp_src_commodities;
         DROP TABLE IF EXISTS tmp_src_accounts;
         DROP TABLE IF EXISTS tmp_src_transactions;
         DROP TABLE IF EXISTS tmp_src_splits;",
    );
}

fn count_orphaned(conn: &Connection, table: &str, src_table: &str) -> Result<i64, AppError> {
    db::validate_data_table(table)?;
    let sql =
        format!("SELECT COUNT(*) FROM {table} WHERE guid NOT IN (SELECT guid FROM {src_table})");
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(AppError::from)
}

fn delete_orphans(conn: &Connection, table: &str, src_table: &str) -> Result<i64, AppError> {
    db::validate_data_table(table)?;
    let sql = format!("DELETE FROM {table} WHERE guid NOT IN (SELECT guid FROM {src_table})");
    conn.execute(&sql, [])
        .map(|n| n as i64)
        .map_err(AppError::from)
}

// ── Merge ───────────────────────────────────────────────────────────────────

/// Destination rows keyed by GUID, loaded once per import for diffing.
struct ExistingData {
    commodities: HashMap<String, Commodity>,
    accounts: HashMap<String, Account>,
    transactions: HashMap<String, Transaction>,
    splits: HashMap<String, Split>,
}

impl ExistingData {
    fn load(conn: &Connection) -> Result<Self, AppError> {
        Ok(Self {
            commodities: read_commodity(conn)?
                .into_iter()
                .map(|c| (c.guid.clone(), c))
                .collect(),
            accounts: read_account(conn)?.into_iter().map(|a| (a.guid.clone(), a)).collect(),
            transactions: read_transaction(conn)?
                .into_iter()
                .map(|t| (t.guid.clone(), t))
                .collect(),
            splits: read_split(conn)?.into_iter().map(|s| (s.guid.clone(), s)).collect(),
        })
    }
}

/// Added/updated/orphaned tallies for one import merge, shared by the merge
/// steps and the history row.
#[derive(Default)]
struct MergeCounts {
    commodities_added: i64,
    commodities_updated: i64,
    accounts_added: i64,
    accounts_updated: i64,
    transactions_added: i64,
    transactions_updated: i64,
    splits_added: i64,
    splits_updated: i64,
    commodities_orphaned: i64,
    accounts_orphaned: i64,
    transactions_orphaned: i64,
    splits_orphaned: i64,
}

impl MergeCounts {
    fn has_changes(&self) -> bool {
        self.accounts_added != 0
            || self.transactions_added != 0
            || self.splits_added != 0
            || self.commodities_added != 0
            || self.accounts_updated != 0
            || self.transactions_updated != 0
            || self.splits_updated != 0
            || self.commodities_updated != 0
            || self.accounts_orphaned != 0
            || self.transactions_orphaned != 0
            || self.splits_orphaned != 0
            || self.commodities_orphaned != 0
    }

    fn history_detail(&self, filename: &str, source_path: &str, sha256: &str) -> String {
        serde_json::json!({
            "filename": filename,
            "filePath": source_path,
            "sha256": sha256,
            "accountsAdded": self.accounts_added,
            "transactionsAdded": self.transactions_added,
            "splitsAdded": self.splits_added,
            "commoditiesAdded": self.commodities_added,
            "accountsUpdated": self.accounts_updated,
            "transactionsUpdated": self.transactions_updated,
            "splitsUpdated": self.splits_updated,
            "accountsOrphaned": self.accounts_orphaned,
            "transactionsOrphaned": self.transactions_orphaned,
            "splitsOrphaned": self.splits_orphaned,
            "commoditiesOrphaned": self.commodities_orphaned,
        })
        .to_string()
    }
}

fn capture_snapshot(conn: &Connection, table: &'static str, guid: &str) -> Result<history::Snapshot<'static>, AppError> {
    Ok(history::Snapshot {
        table_name: table,
        row_guid: guid.to_string(),
        before_json: history::snapshot_row(conn, table, guid)?,
    })
}

/// Snapshot added rows as absent (revert deletes them) and changed rows with
/// their current image (revert restores it); unchanged rows are skipped.
fn capture_changed<T: Importable>(
    conn: &Connection,
    table: &'static str,
    src: &[T],
    existing: &HashMap<String, T>,
    snapshots: &mut Vec<history::Snapshot<'static>>,
) -> Result<(), AppError> {
    for r in src {
        match existing.get(r.guid()) {
            None => snapshots.push(capture_snapshot(conn, table, r.guid())?),
            Some(ex) if r.has_changes(ex) => snapshots.push(capture_snapshot(conn, table, r.guid())?),
            _ => {}
        }
    }
    Ok(())
}

/// Snapshot destination rows absent from the source (the merge deletes them).
fn capture_orphaned<T: Importable>(
    conn: &Connection,
    table: &'static str,
    src: &[T],
    existing: &HashMap<String, T>,
    snapshots: &mut Vec<history::Snapshot<'static>>,
) -> Result<(), AppError> {
    let src_guids: HashSet<&str> = src.iter().map(|r| r.guid()).collect();
    for guid in existing.keys() {
        if !src_guids.contains(guid.as_str()) {
            snapshots.push(capture_snapshot(conn, table, guid)?);
        }
    }
    Ok(())
}

/// Capture before-images for every row the merge is about to touch so the
/// single history row can revert this import.
fn capture_import_snapshots(
    conn: &Connection,
    src: &SourceData,
    existing: &ExistingData,
) -> Result<Vec<history::Snapshot<'static>>, AppError> {
    let mut snapshots = Vec::new();
    capture_changed(conn, "commodities", &src.commodities, &existing.commodities, &mut snapshots)?;
    capture_changed(conn, "accounts", &src.accounts, &existing.accounts, &mut snapshots)?;
    capture_changed(conn, "transactions", &src.transactions, &existing.transactions, &mut snapshots)?;
    capture_changed(conn, "splits", &src.splits, &existing.splits, &mut snapshots)?;
    capture_orphaned(conn, "commodities", &src.commodities, &existing.commodities, &mut snapshots)?;
    capture_orphaned(conn, "accounts", &src.accounts, &existing.accounts, &mut snapshots)?;
    capture_orphaned(conn, "transactions", &src.transactions, &existing.transactions, &mut snapshots)?;
    capture_orphaned(conn, "splits", &src.splits, &existing.splits, &mut snapshots)?;
    Ok(snapshots)
}

/// Upsert source rows over the destination, then delete orphans.
fn apply_merge(
    conn: &Connection,
    src: &SourceData,
    existing: &ExistingData,
    counts: &mut MergeCounts,
) -> Result<(), AppError> {
    (counts.commodities_added, counts.commodities_updated) =
        process_entities(conn, &src.commodities, &existing.commodities)?;
    (counts.accounts_added, counts.accounts_updated) =
        process_entities(conn, &src.accounts, &existing.accounts)?;
    (counts.transactions_added, counts.transactions_updated) =
        process_entities(conn, &src.transactions, &existing.transactions)?;
    (counts.splits_added, counts.splits_updated) =
        process_entities(conn, &src.splits, &existing.splits)?;

    delete_orphans(conn, "splits", "tmp_src_splits")?;
    delete_orphans(conn, "transactions", "tmp_src_transactions")?;
    delete_orphans(conn, "accounts", "tmp_src_accounts")?;
    delete_orphans(conn, "commodities", "tmp_src_commodities")?;
    Ok(())
}

// ── Import ──────────────────────────────────────────────────────────────────

fn import_gnucash_inner(
    dest_conn: &Connection,
    source_path: &str,
    filename: &str,
    sha256: &str,
) -> Result<ImportResult, AppError> {
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

    let mut counts = MergeCounts::default();

    // Counts live inside the txn so the numbers in the history row match
    // the deletes they report. Temp tables live on the long-lived
    // connection; drop them after the txn instead of letting them linger
    // until the next import recreates them.
    let tx_result: Result<(), AppError> = db::with_tx_err(dest_conn, || {
        counts.commodities_orphaned =
            count_orphaned(dest_conn, "commodities", "tmp_src_commodities")?;
        counts.accounts_orphaned =
            count_orphaned(dest_conn, "accounts", "tmp_src_accounts")?;
        counts.transactions_orphaned =
            count_orphaned(dest_conn, "transactions", "tmp_src_transactions")?;
        counts.splits_orphaned = count_orphaned(dest_conn, "splits", "tmp_src_splits")?;

        let existing = ExistingData::load(dest_conn)?;
        let snapshots = capture_import_snapshots(dest_conn, &src, &existing)?;
        apply_merge(dest_conn, &src, &existing, &mut counts)?;

        if counts.has_changes() {
            history::insert_change_entry(
                dest_conn,
                "import",
                &format!("Imported {filename}"),
                "import",
                counts.transactions_added + counts.transactions_updated + counts.transactions_orphaned,
                counts.accounts_added + counts.accounts_updated + counts.accounts_orphaned,
                Some(&counts.history_detail(filename, source_path, sha256)),
                &snapshots,
            )?;
        }

        Ok(())
    });

    drop_source_guid_tables(dest_conn);
    tx_result?;

    let summary = get_summary(dest_conn)?;

    let total_accounts: i64 = dest_conn
        .query_row("SELECT COUNT(*) as cnt FROM accounts", [], |row| row.get(0))
        ?;

    let total_transactions: i64 = dest_conn
        .query_row("SELECT COUNT(*) as cnt FROM transactions", [], |row| {
            row.get(0)
        })
        ?;

    Ok(ImportResult {
        accounts_added: counts.accounts_added,
        transactions_added: counts.transactions_added,
        splits_added: counts.splits_added,
        commodities_added: counts.commodities_added,
        accounts_updated: counts.accounts_updated,
        transactions_updated: counts.transactions_updated,
        splits_updated: counts.splits_updated,
        accounts_orphaned: counts.accounts_orphaned,
        transactions_orphaned: counts.transactions_orphaned,
        splits_orphaned: counts.splits_orphaned,
        commodities_orphaned: counts.commodities_orphaned,
        total_accounts,
        total_transactions,
        summary,
        // Placeholder: the Tauri commands below replace this with a
        // best-effort Yahoo fetch after the merge commits.
        prices: prices::empty_prices_result(),
    })
}

// ── Post-import price fetch ─────────────────────────────────────────────────

/// Best-effort Yahoo fetch after a successful merge. Never fails the import:
/// hard failures degrade to a synthetic `PriceError` so the frontend can show
/// a warning toast while keeping the imported data.
async fn fetch_prices_best_effort(
    db: std::sync::Arc<std::sync::Mutex<Connection>>,
    book_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    expected_generation: u64,
) -> PricesResult {
    fn failed(message: String) -> PricesResult {
        PricesResult {
            prices: vec![],
            fetched_at: utils::iso_now(),
            errors: vec![PriceError {
                mnemonic: "Yahoo".to_string(),
                error: message,
            }],
        }
    }

    // Each stage runs sequentially without holding the DB lock across the
    // network fetch (same discipline as `refresh_prices`).
    let pre = match db::run_blocking(db.clone(), prices::read_price_data).await {
        Ok(pre) => pre,
        Err(e) => return failed(format!("Failed to read price plan: {e}")),
    };
    if pre.commodities.is_empty() {
        return prices::empty_prices_result();
    }
    let (mut result, write_data) = match prices::fetch_prices_remote(pre).await {
        Ok(v) => v,
        Err(e) => return failed(e),
    };
    if let Err(e) = db::run_blocking(db, move |conn| {
        if book_generation.load(std::sync::atomic::Ordering::SeqCst) != expected_generation {
            return Err(
                "Book changed while prices were refreshing — results discarded".to_string(),
            );
        }
        prices::write_prices_and_log(conn, &write_data).map_err(String::from)
    })
    .await
    {
        result.errors.push(PriceError {
            mnemonic: "Yahoo".to_string(),
            error: format!("Failed to store fetched prices: {e}"),
        });
    }
    result
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
    // Capture the active book before merging: if the user switches books while
    // the import (or its price fetch) is in flight, the price write is dropped.
    let generation = state.book_generation.load(std::sync::atomic::Ordering::SeqCst);
    let mut result = db::run_blocking(state.db.clone(), move |conn| {
        import_gnucash_inner(conn, &path, &filename, &sha256).map_err(String::from)
    })
    .await?;
    result.prices = fetch_prices_best_effort(
        state.db.clone(),
        state.book_generation.clone(),
        generation,
    )
    .await;
    Ok(result)
}

#[tauri::command]
pub async fn reimport_last(state: State<'_, AppState>) -> Result<ImportResult, String> {
    // Look up the last import outside the blocking closure, then hash + merge.
    // Reads the uniform change history first, falling back to the legacy
    // import log so books imported before the rewrite can still reimport.
    let (file_path, filename) =
        db::run_blocking(state.db.clone(), history::last_import_source).await?;
    utils::validate_gnucash_path(&file_path)?;
    let sha256 = file_sha256(&file_path)?;
    let generation = state.book_generation.load(std::sync::atomic::Ordering::SeqCst);

    let mut result = db::run_blocking(state.db.clone(), move |conn| {
        import_gnucash_inner(conn, &file_path, &filename, &sha256).map_err(String::from)
    })
    .await?;
    result.prices = fetch_prices_best_effort(
        state.db.clone(),
        state.book_generation.clone(),
        generation,
    )
    .await;
    Ok(result)
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
        // Odd-length time tails would emit a 1-char component — pass through.
        assert_eq!(normalize_post_date("202401151"), "202401151");
        assert_eq!(normalize_post_date("2024011510"), "2024011510");
    }

    /// End-to-end: import a minimal GnuCash source into a book with local-only
    /// rows, asserting the single history row, the reimport pointer, the
    /// no-op second import, and a full revert (orphans restored, added
    /// rows removed).
    #[test]
    fn import_logs_single_row_and_reverts() {
        let dir =
            std::env::temp_dir().join(format!("spendo-import-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("book.gnucash");
        {
            let src = Connection::open(&src_path).unwrap();
            src.execute_batch(
                "CREATE TABLE commodities (guid TEXT PRIMARY KEY, mnemonic TEXT, namespace TEXT, fullname TEXT);
                 CREATE TABLE accounts (guid TEXT PRIMARY KEY, name TEXT, account_type TEXT, parent_guid TEXT, code TEXT, description TEXT, placeholder INTEGER, commodity_guid TEXT);
                 CREATE TABLE transactions (guid TEXT PRIMARY KEY, post_date TEXT, description TEXT, num TEXT);
                 CREATE TABLE splits (guid TEXT PRIMARY KEY, tx_guid TEXT, account_guid TEXT, value_num INTEGER, value_denom INTEGER, quantity_num INTEGER, quantity_denom INTEGER, reconcile_state TEXT, memo TEXT);
                 CREATE TABLE slots (obj_guid TEXT, name TEXT, string_val TEXT);
                 INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY');
                 INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
                   VALUES ('root', 'Root Account', 'ROOT', NULL, 0, 'c-eur'),
                          ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur');
                 INSERT INTO transactions (guid, post_date, description) VALUES ('t1', '2024-01-15 10:30:00', 'Groceries');
                 INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state)
                   VALUES ('s1', 't1', 'checking', 5000, 100, 5000, 100, 'n');",
            )
            .unwrap();
        }
        let src_str = src_path.to_string_lossy().to_string();

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        // Local-only rows absent from the source: the import orphans them.
        dest.execute_batch(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('x-com', 'XCO', 'CURRENCY');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               VALUES ('x-root', 'X Root', 'ROOT', NULL, 0, 'x-com');
             INSERT INTO transactions (guid, post_date, description) VALUES ('t-extra', '2024-03-01', 'Local only');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
               VALUES ('s-extra', 't-extra', 'x-root', 100, 100, 100, 100);",
        )
        .unwrap();

        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "deadbeef").unwrap();
        assert_eq!(result.commodities_added, 1);
        assert_eq!(result.commodities_orphaned, 1);
        assert_eq!(result.accounts_added, 2);
        assert_eq!(result.accounts_orphaned, 1);
        assert_eq!(result.transactions_added, 1);
        assert_eq!(result.transactions_orphaned, 1);
        assert_eq!(result.splits_added, 1);
        assert_eq!(result.splits_orphaned, 1);

        // Exactly one history row; orphans are gone from the data.
        let page = history::fetch_change_history_inner(&dest, 1, 50).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.entries[0].kind, "import");
        assert_eq!(page.entries[0].label, "Imported book.gnucash");
        assert_eq!(page.entries[0].transactions_affected, 2);
        assert_eq!(page.entries[0].accounts_affected, 3);
        assert_eq!(page.revertible_id, Some(page.entries[0].id));
        let count = |table: &str| -> i64 {
            dest.query_row(
                &format!("SELECT COUNT(*) FROM {table}"),
                [],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_eq!(count("transactions"), 1);
        assert_eq!(
            dest.query_row(
                "SELECT description FROM transactions WHERE guid = 't1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "Groceries"
        );

        // The reimport pointer survives the rewrite via detail_json.
        assert_eq!(
            history::last_import_source(&dest).unwrap(),
            (src_str.clone(), "book.gnucash".to_string())
        );

        // Identical reimport is a no-op and logs nothing new.
        let again = import_gnucash_inner(&dest, &src_str, "book.gnucash", "deadbeef").unwrap();
        assert_eq!(again.transactions_added, 0);
        assert_eq!(again.transactions_orphaned, 0);
        assert_eq!(
            history::fetch_change_history_inner(&dest, 1, 50)
                .unwrap()
                .total,
            1
        );

        // Revert removes added rows and restores the orphaned local rows.
        history::revert_history_entry_inner(&dest, page.entries[0].id).unwrap();
        assert_eq!(count("transactions"), 1);
        assert_eq!(
            dest.query_row(
                "SELECT description FROM transactions WHERE guid = 't-extra'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "Local only"
        );
        assert_eq!(count("accounts"), 1);
        assert_eq!(count("commodities"), 1);

        std::fs::remove_dir_all(&dir).ok();
    }
}
