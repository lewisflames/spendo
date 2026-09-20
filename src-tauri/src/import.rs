use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use tauri::State;

use crate::db::{self, connect_source_db, init_db, AppError, AppState};
use crate::history;
use crate::home::{get_summary, DataSummary};
use crate::init;
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

    // GnuCash can carry internal accounts named after their GUID (import
    // leftovers). Drop those only when no split references them — filtering a
    // referenced account would silently drop its splits and unbalance the
    // owning transactions.
    let referenced: HashSet<&str> = splits.iter().map(|s| s.account_guid.as_str()).collect();
    accounts.retain(|a| !is_uuid(&a.name) || referenced.contains(a.guid.as_str()));
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
    // Remaining pairs are HH, MM, SS. Short tails (hour or hour+minute) are
    // padded with `:00` so SQLite's date math parses them and lexicographic
    // comparisons stay chronological; odd-length or over-long tails pass
    // through for the caller to surface.
    let tail = &digits[8..];
    if !tail.len().is_multiple_of(2) || !(2..=6).contains(&tail.len()) {
        return clean.to_string();
    }
    let mut parts: Vec<&str> = tail
        .as_bytes()
        .chunks(2)
        .map(|pair| std::str::from_utf8(pair).unwrap_or(""))
        .collect();
    while parts.len() < 3 {
        parts.push("00");
    }
    format!("{date} {}", parts.join(":"))
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
         DROP TABLE IF EXISTS tmp_src_splits;
         DROP TABLE IF EXISTS tmp_protected_accounts;",
    );
}

fn count_orphaned(
    conn: &Connection,
    table: &str,
    src_table: &str,
    excluded_table: Option<&str>,
) -> Result<i64, AppError> {
    db::validate_data_table(table)?;
    let mut sql =
        format!("SELECT COUNT(*) FROM {table} WHERE guid NOT IN (SELECT guid FROM {src_table})");
    if let Some(excluded) = excluded_table {
        sql.push_str(&format!(" AND guid NOT IN (SELECT guid FROM {excluded})"));
    }
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(AppError::from)
}

fn delete_orphans_rows(
    conn: &Connection,
    table: &str,
    src_table: &str,
    excluded_table: Option<&str>,
) -> Result<i64, AppError> {
    db::validate_data_table(table)?;
    let mut sql = format!("DELETE FROM {table} WHERE guid NOT IN (SELECT guid FROM {src_table})");
    if let Some(excluded) = excluded_table {
        sql.push_str(&format!(" AND guid NOT IN (SELECT guid FROM {excluded})"));
    }
    conn.execute(&sql, [])
        .map(|n| n as i64)
        .map_err(AppError::from)
}

/// Every account that has at least one budget row. The sweep must not delete
/// these: `budgets.account_guid` cascades and budgets are outside the history
/// snapshots, so the delete would be permanent. Mirrors the manual-delete
/// guard in `accounts_mut.rs`.
fn accounts_with_budgets(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT DISTINCT account_guid FROM budgets")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Canonical default roots the source book doesn't cover, as local GUIDs.
/// These survive the orphan sweep (and its revert snapshots) so re-imports
/// stay no-ops and their GUIDs stay stable; source-provided roots of the same
/// category replace them through the normal merge/sweep path. Only roots whose
/// parent ROOT survives the import are protected — the seeded ROOT of a fresh
/// book is swept in favor of the source's ROOT, taking its roots with it.
fn protected_default_roots(conn: &Connection, src: &SourceData) -> Result<Vec<String>, AppError> {
    let src_roots: HashSet<&str> = src
        .accounts
        .iter()
        .filter(|a| a.account_type == "ROOT")
        .map(|a| a.guid.as_str())
        .collect();
    let covered: HashSet<&str> = src
        .accounts
        .iter()
        .filter(|a| {
            a.account_type != "ROOT"
                && a.parent_guid
                    .as_deref()
                    .is_none_or(|parent| src_roots.contains(parent))
        })
        .map(|a| utils::category_for_type(&a.account_type))
        .collect();

    let src_guids: HashSet<&str> = src.accounts.iter().map(|a| a.guid.as_str()).collect();
    let mut protected = Vec::new();
    for (name, category) in init::DEFAULT_ROOTS {
        if covered.contains(category) {
            continue;
        }
        let mut stmt = conn.prepare(
            "SELECT guid, parent_guid FROM accounts
             WHERE name = ?1 AND account_type = ?2
               AND parent_guid IN (SELECT guid FROM accounts WHERE account_type = 'ROOT')",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![name, category], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (guid, parent) in rows {
            let parent_survives = parent
                .as_deref()
                .is_some_and(|p| src_guids.contains(p));
            if parent_survives && !src_guids.contains(guid.as_str()) {
                protected.push(guid);
            }
        }
    }
    Ok(protected)
}

// ── Plan / preview ───────────────────────────────────────────────────────────

/// Read-only tally of what an import would add, update and delete. Mirrors the
/// merge exactly (`process_entities` + orphan sweep) so the confirmation dialog
/// and the applied `ImportResult` agree; a test pins the two together.
fn plan_import(src: &SourceData, existing: &ExistingData, protected: &HashSet<String>) -> MergeCounts {
    fn tally<T: Importable>(
        src: &[T],
        existing: &HashMap<String, T>,
        skip: &HashSet<String>,
        added: &mut i64,
        updated: &mut i64,
        orphaned: &mut i64,
    ) {
        let mut src_guids: HashSet<&str> = HashSet::new();
        for r in src {
            src_guids.insert(r.guid());
            match existing.get(r.guid()) {
                None => *added += 1,
                Some(ex) if r.has_changes(ex) => *updated += 1,
                _ => {}
            }
        }
        *orphaned = existing
            .keys()
            .filter(|g| !src_guids.contains(g.as_str()) && !skip.contains(g.as_str()))
            .count() as i64;
    }

    let mut counts = MergeCounts::default();
    let no_skip = HashSet::new();
    tally(
        &src.commodities,
        &existing.commodities,
        &no_skip,
        &mut counts.commodities_added,
        &mut counts.commodities_updated,
        &mut counts.commodities_orphaned,
    );
    tally(
        &src.accounts,
        &existing.accounts,
        protected,
        &mut counts.accounts_added,
        &mut counts.accounts_updated,
        &mut counts.accounts_orphaned,
    );
    tally(
        &src.transactions,
        &existing.transactions,
        &no_skip,
        &mut counts.transactions_added,
        &mut counts.transactions_updated,
        &mut counts.transactions_orphaned,
    );
    tally(
        &src.splits,
        &existing.splits,
        &no_skip,
        &mut counts.splits_added,
        &mut counts.splits_updated,
        &mut counts.splits_orphaned,
    );
    counts
}

/// Names of existing accounts the sweep would delete, sorted and capped for
/// display in the confirmation dialog.
fn orphan_account_names(
    src: &SourceData,
    existing: &ExistingData,
    protected: &HashSet<String>,
    limit: usize,
) -> Vec<String> {
    let src_guids: HashSet<&str> = src.accounts.iter().map(|a| a.guid.as_str()).collect();
    let mut names: Vec<String> = existing
        .accounts
        .iter()
        .filter(|(g, _)| !src_guids.contains(g.as_str()) && !protected.contains(*g))
        .map(|(_, a)| a.name.clone())
        .collect();
    names.sort();
    names.truncate(limit);
    names
}

/// Wire shape of the pre-import confirmation: exact counts plus the source
/// identity needed to run the verified import afterwards.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GnuCashPreview {
    pub path: String,
    pub filename: String,
    pub sha256: String,
    pub accounts_added: i64,
    pub accounts_updated: i64,
    pub accounts_orphaned: i64,
    pub transactions_added: i64,
    pub transactions_updated: i64,
    pub transactions_orphaned: i64,
    pub splits_added: i64,
    pub splits_updated: i64,
    pub splits_orphaned: i64,
    pub commodities_added: i64,
    pub commodities_updated: i64,
    pub commodities_orphaned: i64,
    /// Sample of local account names the sweep would remove.
    pub orphan_account_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GnuCashImportInput {
    pub path: String,
    pub filename: String,
    /// Hash returned by `preview_gnucash_import`; the merge refuses to run when
    /// the file changed after the confirmation.
    pub sha256: String,
    /// When false, rows absent from the source survive (no orphan sweep).
    pub delete_orphans: bool,
}

/// Accounts the orphan sweep must keep: canonical roots for categories the
/// source doesn't cover, accounts carrying budgets (the FK cascade would
/// destroy budgets, which are outside undo snapshots), and accounts referenced
/// by import rules (same cascade problem).
fn protected_accounts(conn: &Connection, src: &SourceData) -> Result<HashSet<String>, AppError> {
    let mut protected: HashSet<String> =
        protected_default_roots(conn, src)?.into_iter().collect();
    protected.extend(accounts_with_budgets(conn)?);
    protected.extend(crate::import_rules::rule_account_guids(conn)?);
    Ok(protected)
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
/// `skip` defends app-managed rows the sweep is told to keep (protected
/// default roots) from being captured as orphans.
fn capture_orphaned<T: Importable>(
    conn: &Connection,
    table: &'static str,
    src: &[T],
    existing: &HashMap<String, T>,
    skip: &HashSet<String>,
    snapshots: &mut Vec<history::Snapshot<'static>>,
) -> Result<(), AppError> {
    let src_guids: HashSet<&str> = src.iter().map(|r| r.guid()).collect();
    for guid in existing.keys() {
        if !src_guids.contains(guid.as_str()) && !skip.contains(guid) {
            snapshots.push(capture_snapshot(conn, table, guid)?);
        }
    }
    Ok(())
}

/// Capture before-images for every row the merge is about to touch so the
/// single history row can revert this import. With `delete_orphans` off the
/// sweep doesn't run, so there are no orphan deletes to snapshot.
fn capture_import_snapshots(
    conn: &Connection,
    src: &SourceData,
    existing: &ExistingData,
    protected: &HashSet<String>,
    delete_orphans: bool,
) -> Result<Vec<history::Snapshot<'static>>, AppError> {
    let mut snapshots = Vec::new();
    capture_changed(conn, "commodities", &src.commodities, &existing.commodities, &mut snapshots)?;
    capture_changed(conn, "accounts", &src.accounts, &existing.accounts, &mut snapshots)?;
    capture_changed(conn, "transactions", &src.transactions, &existing.transactions, &mut snapshots)?;
    capture_changed(conn, "splits", &src.splits, &existing.splits, &mut snapshots)?;
    if delete_orphans {
        capture_orphaned(conn, "commodities", &src.commodities, &existing.commodities, &HashSet::new(), &mut snapshots)?;
        capture_orphaned(conn, "accounts", &src.accounts, &existing.accounts, protected, &mut snapshots)?;
        capture_orphaned(conn, "transactions", &src.transactions, &existing.transactions, &HashSet::new(), &mut snapshots)?;
        capture_orphaned(conn, "splits", &src.splits, &existing.splits, &HashSet::new(), &mut snapshots)?;
    }
    Ok(snapshots)
}

/// Upsert source rows over the destination, then delete orphans (protected
/// default roots excepted). `delete_orphans: false` keeps local-only rows.
fn apply_merge(
    conn: &Connection,
    src: &SourceData,
    existing: &ExistingData,
    counts: &mut MergeCounts,
    accounts_excluded: Option<&str>,
    delete_orphans: bool,
) -> Result<(), AppError> {
    (counts.commodities_added, counts.commodities_updated) =
        process_entities(conn, &src.commodities, &existing.commodities)?;
    (counts.accounts_added, counts.accounts_updated) =
        process_entities(conn, &src.accounts, &existing.accounts)?;
    (counts.transactions_added, counts.transactions_updated) =
        process_entities(conn, &src.transactions, &existing.transactions)?;
    (counts.splits_added, counts.splits_updated) =
        process_entities(conn, &src.splits, &existing.splits)?;

    if delete_orphans {
        delete_orphans_rows(conn, "splits", "tmp_src_splits", None)?;
        delete_orphans_rows(conn, "transactions", "tmp_src_transactions", None)?;
        delete_orphans_rows(conn, "accounts", "tmp_src_accounts", accounts_excluded)?;
        delete_orphans_rows(conn, "commodities", "tmp_src_commodities", None)?;
    }
    Ok(())
}

// ── Import ──────────────────────────────────────────────────────────────────

fn import_gnucash_inner(
    dest_conn: &Connection,
    source_path: &str,
    filename: &str,
    sha256: &str,
    delete_orphans: bool,
) -> Result<ImportResult, AppError> {
    let source = connect_source_db(&std::path::PathBuf::from(source_path))
        .map_err(|e| format!("Failed to open source DB: {}", e))?;

    init_db(dest_conn).map_err(|e| format!("Failed to init dest DB: {}", e))?;

    let src = read_source_data(&source)?;

    // The orphan sweep treats the source as authoritative and deletes rows it
    // doesn't cover, so an empty source set (`NOT IN (empty)`) would wipe the
    // whole book. Abort instead — there is nothing to import.
    if src.accounts.is_empty() {
        return Err(AppError::Validation(
            "Source book has no accounts — nothing to import".to_string(),
        ));
    }

    let commodity_guids: Vec<String> = src.commodities.iter().map(|r| r.guid.clone()).collect();
    let account_guids: Vec<String> = src.accounts.iter().map(|r| r.guid.clone()).collect();
    let tx_guids: Vec<String> = src.transactions.iter().map(|r| r.guid.clone()).collect();
    let split_guids: Vec<String> = src.splits.iter().map(|r| r.guid.clone()).collect();

    populate_source_guids(dest_conn, "tmp_src_commodities", &commodity_guids)?;
    populate_source_guids(dest_conn, "tmp_src_accounts", &account_guids)?;
    populate_source_guids(dest_conn, "tmp_src_transactions", &tx_guids)?;
    populate_source_guids(dest_conn, "tmp_src_splits", &split_guids)?;

    // Materialize sweep-protected GUIDs before the transaction so the orphan
    // sweep (and its revert snapshots) can skip them. Empty set = no exclusion.
    let protected = protected_accounts(dest_conn, &src)?;
    let protected_guids: Vec<String> = protected.iter().cloned().collect();
    populate_source_guids(dest_conn, "tmp_protected_accounts", &protected_guids)?;
    let accounts_excluded = if protected.is_empty() {
        None
    } else {
        Some("tmp_protected_accounts")
    };

    let mut counts = MergeCounts::default();

    // Counts live inside the txn so the numbers in the history row match
    // the deletes they report. Temp tables live on the long-lived
    // connection; drop them after the txn instead of letting them linger
    // until the next import recreates them.
    let tx_result: Result<(), AppError> = db::with_tx_err(dest_conn, || {
        if delete_orphans {
            counts.commodities_orphaned =
                count_orphaned(dest_conn, "commodities", "tmp_src_commodities", None)?;
            counts.accounts_orphaned =
                count_orphaned(dest_conn, "accounts", "tmp_src_accounts", accounts_excluded)?;
            counts.transactions_orphaned =
                count_orphaned(dest_conn, "transactions", "tmp_src_transactions", None)?;
            counts.splits_orphaned =
                count_orphaned(dest_conn, "splits", "tmp_src_splits", None)?;
        }

        let existing = ExistingData::load(dest_conn)?;
        let mut snapshots =
            capture_import_snapshots(dest_conn, &src, &existing, &protected, delete_orphans)?;
        apply_merge(
            dest_conn,
            &src,
            &existing,
            &mut counts,
            accounts_excluded,
            delete_orphans,
        )?;

        // Categories the source never covered: create the canonical root now
        // and record it as added, so reverting this import removes it again.
        for guid in init::ensure_default_roots(dest_conn)? {
            snapshots.push(history::Snapshot {
                table_name: "accounts",
                row_guid: guid,
                before_json: None,
            });
        }

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

/// Read-only dry run: same source read and protected-account logic as the
/// merge, but nothing is written. The returned hash is echoed back to
/// `import_gnucash` so a file changed after confirmation is rejected.
#[tauri::command]
pub async fn preview_gnucash_import(
    state: State<'_, AppState>,
    path: String,
    filename: String,
) -> Result<GnuCashPreview, String> {
    let generation = state.book_generation.load(std::sync::atomic::Ordering::SeqCst);
    let hash_path = path.clone();
    let sha256 = db::run_io(move || {
        utils::validate_gnucash_path(&hash_path)?;
        file_sha256(&hash_path)
    })
    .await?;
    let generation_handle = state.book_generation.clone();
    db::run_blocking(state.db.clone(), move |conn| {
        db::ensure_same_book(&generation_handle, generation)?;
        preview_gnucash_inner(conn, &path, &filename, &sha256).map_err(String::from)
    })
    .await
}

fn preview_gnucash_inner(
    dest_conn: &Connection,
    source_path: &str,
    filename: &str,
    sha256: &str,
) -> Result<GnuCashPreview, AppError> {
    let source = connect_source_db(&std::path::PathBuf::from(source_path))
        .map_err(|e| format!("Failed to open source DB: {}", e))?;
    let src = read_source_data(&source)?;
    if src.accounts.is_empty() {
        return Err(AppError::Validation(
            "Source book has no accounts — nothing to import".to_string(),
        ));
    }
    let protected = protected_accounts(dest_conn, &src)?;
    let existing = ExistingData::load(dest_conn)?;
    let counts = plan_import(&src, &existing, &protected);
    Ok(GnuCashPreview {
        path: source_path.to_string(),
        filename: filename.to_string(),
        sha256: sha256.to_string(),
        accounts_added: counts.accounts_added,
        accounts_updated: counts.accounts_updated,
        accounts_orphaned: counts.accounts_orphaned,
        transactions_added: counts.transactions_added,
        transactions_updated: counts.transactions_updated,
        transactions_orphaned: counts.transactions_orphaned,
        splits_added: counts.splits_added,
        splits_updated: counts.splits_updated,
        splits_orphaned: counts.splits_orphaned,
        commodities_added: counts.commodities_added,
        commodities_updated: counts.commodities_updated,
        commodities_orphaned: counts.commodities_orphaned,
        orphan_account_names: orphan_account_names(&src, &existing, &protected, 20),
    })
}

#[tauri::command]
pub async fn import_gnucash(
    state: State<'_, AppState>,
    input: GnuCashImportInput,
) -> Result<ImportResult, String> {
    // Capture the active book before any await: the merge must never land in a
    // book the user switched to while the file was being hashed.
    let generation = state.book_generation.load(std::sync::atomic::Ordering::SeqCst);
    // Validate + hash on a blocking thread, before the DB lock is touched.
    let hash_path = input.path.clone();
    let current_sha = db::run_io(move || {
        utils::validate_gnucash_path(&hash_path)?;
        file_sha256(&hash_path)
    })
    .await?;
    if current_sha != input.sha256 {
        return Err("Source file changed since the preview — run the import again".to_string());
    }
    let generation_handle = state.book_generation.clone();
    let GnuCashImportInput {
        path,
        filename,
        sha256,
        delete_orphans,
    } = input;
    let mut result = db::run_blocking(state.db.clone(), move |conn| {
        db::ensure_same_book(&generation_handle, generation)?;
        import_gnucash_inner(conn, &path, &filename, &sha256, delete_orphans).map_err(String::from)
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
        // Compact hour / hour+minute tails are padded so SQLite can parse them.
        assert_eq!(normalize_post_date("2024011510"), "2024-01-15 10:00:00");
        assert_eq!(normalize_post_date("202401151030"), "2024-01-15 10:30:00");
    }

    #[test]
    fn normalize_post_date_passthrough_unexpected_shapes() {
        assert_eq!(normalize_post_date("not-a-date"), "not-a-date");
        assert_eq!(normalize_post_date(""), "");
        assert_eq!(normalize_post_date("2024-01"), "2024-01");
        // Odd-length and over-long time tails pass through for the caller.
        assert_eq!(normalize_post_date("202401151"), "202401151");
        assert_eq!(normalize_post_date("2024011510300012"), "2024011510300012");
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

        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "deadbeef", true).unwrap();
        assert_eq!(result.commodities_added, 1);
        assert_eq!(result.commodities_orphaned, 1);
        assert_eq!(result.accounts_added, 2);
        assert_eq!(result.accounts_orphaned, 1);
        assert_eq!(result.transactions_added, 1);
        assert_eq!(result.transactions_orphaned, 1);
        assert_eq!(result.splits_added, 1);
        assert_eq!(result.splits_orphaned, 1);

        // Exactly one history row; orphans are gone from the data.
        let log = history::fetch_change_history_inner(&dest).unwrap();
        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries[0].kind, "import");
        assert_eq!(log.entries[0].label, "Imported book.gnucash");
        assert_eq!(log.entries[0].transactions_affected, 2);
        assert_eq!(log.entries[0].accounts_affected, 3);
        assert_eq!(log.undoable_id, Some(log.entries[0].id));
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

        // Identical reimport is a no-op and logs nothing new.
        let again = import_gnucash_inner(&dest, &src_str, "book.gnucash", "deadbeef", true).unwrap();
        assert_eq!(again.transactions_added, 0);
        assert_eq!(again.transactions_orphaned, 0);
        assert_eq!(
            history::fetch_change_history_inner(&dest)
                .unwrap()
                .entries
                .len(),
            1
        );

        // Revert removes added rows and restores the orphaned local rows.
        history::undo_history_entry_inner(&dest, log.entries[0].id).unwrap();
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

    /// A source book that only covers part of the canonical categories: the
    /// seeded ROOT/roots are replaced on the first import, the uncovered
    /// categories are recreated under the surviving ROOT, and a re-import is
    /// a no-op that keeps the recreated roots' GUIDs stable.
    #[test]
    fn import_keeps_uncovered_default_roots_across_reimports() {
        let dir =
            std::env::temp_dir().join(format!("spendo-import-roots-{}", uuid::Uuid::new_v4()));
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
                 INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-src', 'EUR', 'CURRENCY');
                 INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
                   VALUES ('src-root', 'Root Account', 'ROOT', NULL, 0, 'c-src'),
                          ('src-income', 'Income', 'INCOME', 'src-root', 1, 'c-src'),
                          ('src-expenses', 'Expenses', 'EXPENSE', 'src-root', 1, 'c-src');",
            )
            .unwrap();
        }
        let src_str = src_path.to_string_lossy().to_string();

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        init::seed_empty_book(&dest, "EUR").unwrap();

        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "cafe", true).unwrap();
        // Seeded ROOT + five roots are all absent from the source: the ROOT is
        // replaced wholesale, the uncovered categories are recreated after.
        assert_eq!(result.accounts_orphaned, 6);
        assert_eq!(result.accounts_added, 3);

        let roots = |conn: &Connection| -> Vec<(String, String)> {
            let mut stmt = conn
                .prepare(
                    "SELECT name, guid FROM accounts
                     WHERE parent_guid IN (SELECT guid FROM accounts WHERE account_type = 'ROOT')",
                )
                .unwrap();
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        let after_first = roots(&dest);
        assert_eq!(after_first.len(), 5);
        for (name, _) in init::DEFAULT_ROOTS {
            assert!(
                after_first.iter().any(|(n, _)| n == name),
                "missing {name} after import"
            );
        }

        // Re-import: the recreated roots are protected, so nothing changes and
        // their GUIDs stay stable.
        let again = import_gnucash_inner(&dest, &src_str, "book.gnucash", "cafe", true).unwrap();
        assert_eq!(again.accounts_orphaned, 0);
        assert_eq!(roots(&dest), after_first);
        assert_eq!(
            history::fetch_change_history_inner(&dest)
                .unwrap()
                .entries
                .len(),
            1
        );

        // Revert restores the seeded book: five canonical roots under one ROOT.
        let log = history::fetch_change_history_inner(&dest).unwrap();
        history::undo_history_entry_inner(&dest, log.entries[0].id).unwrap();
        let root_count: i64 = dest
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE account_type = 'ROOT'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root_count, 1);
        assert_eq!(roots(&dest).len(), 5);
        let total: i64 = dest
            .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(total, 6);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Minimal GnuCash-shaped source DB: the tables `read_source_data` touches.
    fn write_source_db(path: &std::path::Path, extra_sql: &str) {
        let src = Connection::open(path).unwrap();
        src.execute_batch(&format!(
            "CREATE TABLE commodities (guid TEXT PRIMARY KEY, mnemonic TEXT, namespace TEXT, fullname TEXT);
             CREATE TABLE accounts (guid TEXT PRIMARY KEY, name TEXT, account_type TEXT, parent_guid TEXT, code TEXT, description TEXT, placeholder INTEGER, commodity_guid TEXT);
             CREATE TABLE transactions (guid TEXT PRIMARY KEY, post_date TEXT, description TEXT, num TEXT);
             CREATE TABLE splits (guid TEXT PRIMARY KEY, tx_guid TEXT, account_guid TEXT, value_num INTEGER, value_denom INTEGER, quantity_num INTEGER, quantity_denom INTEGER, reconcile_state TEXT, memo TEXT);
             CREATE TABLE slots (obj_guid TEXT, name TEXT, string_val TEXT);
             {extra_sql}"
        ))
        .unwrap();
    }

    fn tmp_source(tag: &str, extra_sql: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("spendo-import-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.gnucash");
        write_source_db(&path, extra_sql);
        let path_str = path.to_string_lossy().to_string();
        (dir, path_str)
    }

    /// Regression: an empty source produced empty `tmp_src_*` tables, and
    /// `NOT IN (empty set)` made the orphan sweep delete the whole book.
    #[test]
    fn import_aborts_when_source_has_no_accounts() {
        let (dir, src_str) = tmp_source(
            "empty",
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY');",
        );

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        init::seed_empty_book(&dest, "EUR").unwrap();
        let accounts_before: i64 = dest
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .unwrap();

        let err = import_gnucash_inner(&dest, &src_str, "book.gnucash", "deadbeef", true).unwrap_err();
        assert!(err.to_string().contains("no accounts"), "unexpected: {err}");

        let accounts_after: i64 = dest
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(accounts_after, accounts_before);
        assert!(history::fetch_change_history_inner(&dest)
            .unwrap()
            .entries
            .is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Regression: UUID-named accounts referenced by splits used to be filtered
    /// out, silently dropping their splits and unbalancing the transaction.
    #[test]
    fn import_keeps_uuid_named_accounts_referenced_by_splits() {
        let (dir, src_str) = tmp_source(
            "uuid",
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               VALUES ('root', 'Root Account', 'ROOT', NULL, 0, 'c-eur'),
                      ('uuid-acct', '01234567-89ab-cdef-0123-456789abcdef', 'BANK', 'root', 0, 'c-eur'),
                      ('unused-uuid', 'deadbeefdeadbeefdeadbeefdeadbeef', 'BANK', 'root', 0, 'c-eur');
             INSERT INTO transactions (guid, post_date, description)
               VALUES ('t1', '2024-01-15', 'Groceries');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
               VALUES ('s1', 't1', 'uuid-acct', 5000, 100, 5000, 100);",
        );

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "beef", true).unwrap();

        // Referenced UUID-named account + ROOT import; the unreferenced one is filtered.
        assert_eq!(result.accounts_added, 2);
        assert_eq!(result.splits_added, 1);
        let split_account: String = dest
            .query_row("SELECT account_guid FROM splits WHERE guid = 's1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(split_account, "uuid-acct");
        let unused: i64 = dest
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE guid = 'unused-uuid'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(unused, 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Regression: sweeping a budgeted account cascades the budget rows away
    /// (budgets are outside undo snapshots), so the sweep must skip them.
    #[test]
    fn import_preserves_budgets_on_orphaned_accounts() {
        let (dir, src_str) = tmp_source(
            "budget",
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               VALUES ('root', 'Root Account', 'ROOT', NULL, 0, 'c-eur'),
                      ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur');",
        );

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        init::seed_empty_book(&dest, "EUR").unwrap();
        // Local-only account with a budget, absent from the source.
        dest.execute_batch(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               SELECT 'budgeted', 'Local Savings', 'ASSET', guid, 0, commodity_guid
               FROM accounts WHERE account_type = 'ROOT';
             INSERT INTO budgets (guid, account_guid, year, amount, created_at)
               VALUES ('b1', 'budgeted', 2024, 1200.0, '2024-01-01T00:00:00Z');",
        )
        .unwrap();

        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "beef", true).unwrap();

        // The budgeted account survived the sweep (only the 6 seeded rows are
        // counted as orphaned) and its budget escaped the FK cascade.
        assert_eq!(result.accounts_orphaned, 6);
        let budgeted: i64 = dest
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE guid = 'budgeted'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(budgeted, 1);
        let budgets: i64 = dest
            .query_row("SELECT COUNT(*) FROM budgets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(budgets, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    /// The preview must be a pure read and its counts must match what the
    /// import actually applies (the confirmation dialog depends on both).
    #[test]
    fn preview_matches_import_and_does_not_mutate() {
        let (dir, src_str) = tmp_source(
            "preview",
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               VALUES ('root', 'Root Account', 'ROOT', NULL, 0, 'c-eur'),
                      ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur');
             INSERT INTO transactions (guid, post_date, description) VALUES ('t1', '2024-01-15', 'Groceries');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
               VALUES ('s1', 't1', 'checking', 5000, 100, 5000, 100);",
        );

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        // Local-only rows the sweep would remove.
        dest.execute_batch(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               VALUES ('local', 'Local Only', 'ASSET', NULL, 0, NULL);
             INSERT INTO transactions (guid, post_date, description) VALUES ('t-local', '2024-02-01', 'Local');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
               VALUES ('s-local', 't-local', 'local', 100, 100, 100, 100);",
        )
        .unwrap();
        let before = (
            count(&dest, "accounts"),
            count(&dest, "transactions"),
            count(&dest, "splits"),
            count(&dest, "commodities"),
        );

        let preview = preview_gnucash_inner(&dest, &src_str, "book.gnucash", "hash").unwrap();
        assert_eq!(preview.sha256, "hash");
        assert_eq!(preview.filename, "book.gnucash");
        assert_eq!(preview.commodities_added, 1);
        assert_eq!(preview.accounts_added, 2);
        assert_eq!(preview.accounts_orphaned, 1);
        assert_eq!(preview.transactions_added, 1);
        assert_eq!(preview.transactions_orphaned, 1);
        assert_eq!(preview.splits_added, 1);
        assert_eq!(preview.splits_orphaned, 1);
        assert_eq!(preview.orphan_account_names, vec!["Local Only".to_string()]);
        assert_eq!(
            before,
            (
                count(&dest, "accounts"),
                count(&dest, "transactions"),
                count(&dest, "splits"),
                count(&dest, "commodities"),
            )
        );

        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "hash", true).unwrap();
        assert_eq!(result.commodities_added, preview.commodities_added);
        assert_eq!(result.accounts_added, preview.accounts_added);
        assert_eq!(result.accounts_orphaned, preview.accounts_orphaned);
        assert_eq!(result.transactions_added, preview.transactions_added);
        assert_eq!(result.transactions_orphaned, preview.transactions_orphaned);
        assert_eq!(result.splits_added, preview.splits_added);
        assert_eq!(result.splits_orphaned, preview.splits_orphaned);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Opting out of the sweep keeps rows the source doesn't cover.
    #[test]
    fn import_without_sweep_keeps_local_rows() {
        let (dir, src_str) = tmp_source(
            "no-sweep",
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               VALUES ('root', 'Root Account', 'ROOT', NULL, 0, 'c-eur'),
                      ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur');",
        );

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        dest.execute_batch(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               VALUES ('local', 'Local Only', 'ASSET', NULL, 0, NULL);
             INSERT INTO transactions (guid, post_date, description) VALUES ('t-local', '2024-02-01', 'Local');
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
               VALUES ('s-local', 't-local', 'local', 100, 100, 100, 100);",
        )
        .unwrap();

        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "beef", false).unwrap();
        assert_eq!(result.accounts_orphaned, 0);
        assert_eq!(result.transactions_orphaned, 0);
        assert_eq!(result.splits_orphaned, 0);
        assert_eq!(result.commodities_orphaned, 0);
        assert_eq!(result.accounts_added, 2);
        // local + source root/checking + the four uncovered canonical roots
        // `ensure_default_roots` always heals in.
        assert_eq!(count(&dest, "accounts"), 7);
        assert_eq!(count(&dest, "transactions"), 1);
        assert_eq!(
            dest.query_row("SELECT description FROM transactions WHERE guid = 't-local'", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Local"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
