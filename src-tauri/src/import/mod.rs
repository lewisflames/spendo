use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use tauri::State;

use crate::db::{self, connect_source_db, init_db, AppError, AppState};
use crate::home::{get_summary, DataSummary};
use crate::init;
use crate::prices::{self, PriceError, PricesResult};
use crate::schedules::{self, ImportedSchedule, ScheduleInput, ScheduleMerge};
use crate::transactions::{SplitAccountInfo, SplitInput};
use crate::utils;

mod orphans;
mod rows;
mod sx;

use orphans::*;
use rows::*;
use sx::*;

// ── Traits ──────────────────────────────────────────────────────────────────

trait Importable {
    fn guid(&self) -> &str;
    fn has_changes(&self, existing: &Self) -> bool;
    fn upsert(&self, conn: &Connection) -> Result<(), AppError>;
}

fn process_entities<T: Importable>(
    conn: &Connection,
    src: &[T],
    existing: &HashMap<String, T>,
) -> Result<(i64, i64), AppError> {
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
    pub schedules_added: i64,
    pub schedules_updated: i64,
    pub schedules_skipped: i64,
    pub total_accounts: i64,
    pub total_transactions: i64,
    pub summary: DataSummary,
    pub prices: PricesResult,
}

// ── SHA256 ──────────────────────────────────────────────────────────────────

fn file_sha256(path: &str) -> Result<String, AppError> {
    // Stream instead of slurping so large books don't need to fit in memory.
    use std::io::Read;
    let file =
        fs::File::open(path).map_err(|e| AppError::Io(format!("Failed to read file: {}", e)))?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| AppError::Io(format!("Failed to hash file: {}", e)))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(utils::hex_digest(hasher.finalize()))
}

// ── Source data: shared read+filter logic for preview and import ────────────

struct SourceData {
    commodities: Vec<Commodity>,
    accounts: Vec<Account>,
    splits: Vec<Split>,
    transactions: Vec<Transaction>,
    /// Scheduled transactions converted to Spendo schedules, ready to merge by
    /// GUID. Template accounts/transactions/splits are deliberately NOT part of
    /// the ordinary entity lists above.
    schedules: Vec<ImportedSchedule>,
    /// Schedules the source carries but Spendo cannot represent (readable
    /// reasons for the confirmation dialog).
    skipped_schedules: Vec<SkippedSchedule>,
}

/// A source scheduled transaction Spendo can't import (or converts only
/// partially), surfaced in the preview so nothing is dropped silently.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedSchedule {
    pub name: String,
    pub reason: String,
}

fn read_source_data(source: &Connection) -> Result<SourceData, AppError> {
    let source_schedules = read_source_schedules(source)?;

    let mut accounts = read_account(source)?;
    let mut splits = read_split(source)?;
    let transactions = read_source_transaction(source)?;

    // Template accounts (each named after its SX GUID) and everything under the
    // book's template root are GnuCash plumbing, not user data: their splits
    // are the schedule templates and their transactions are template rows.
    // Convert them to schedules first, then drop them from the entity merge
    // (importing them would add junk accounts and zero-value transactions).
    let excluded = excluded_template_accounts(source, &accounts, &source_schedules)?;
    let template_tx_guids: HashSet<String> = splits
        .iter()
        .filter(|s| excluded.contains(&s.account_guid))
        .map(|s| s.tx_guid.clone())
        .collect();
    let (schedules, skipped_schedules) = if source_schedules.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let slots = read_template_slots(source)?;
        let last_instances = read_last_instance_dates(source)?;
        build_import_schedules(
            &source_schedules,
            &splits,
            &transactions,
            &slots,
            &last_instances,
        )
    };

    accounts.retain(|a| !excluded.contains(&a.guid));

    // GnuCash can carry internal accounts named after their GUID (import
    // leftovers). Drop those only when no split references them — filtering a
    // referenced account would silently drop its splits and unbalance the
    // owning transactions.
    let referenced: HashSet<&str> = splits.iter().map(|s| s.account_guid.as_str()).collect();
    accounts.retain(|a| !is_uuid(&a.name) || referenced.contains(a.guid.as_str()));
    let valid_account_guids: HashSet<String> = accounts.iter().map(|a| a.guid.clone()).collect();
    splits.retain(|s| valid_account_guids.contains(&s.account_guid));

    // Template transactions are dropped unless something else still references
    // them (a malformed source could attach a template split to a real tx).
    let surviving_tx: HashSet<&str> = splits.iter().map(|s| s.tx_guid.as_str()).collect();
    let transactions = transactions
        .into_iter()
        .filter(|t| !template_tx_guids.contains(&t.guid) || surviving_tx.contains(t.guid.as_str()))
        .collect();

    Ok(SourceData {
        commodities: read_commodity(source)?,
        accounts,
        splits,
        transactions,
        schedules,
        skipped_schedules,
    })
}

// ── Plan / preview ───────────────────────────────────────────────────────────

/// Read-only tally of what an import would add, update and delete. Mirrors the
/// merge exactly (`process_entities` + orphan sweep) so the confirmation dialog
/// and the applied `ImportResult` agree; a test pins the two together.
fn plan_import(
    conn: &Connection,
    src: &SourceData,
    existing: &ExistingData,
    protected: &HashSet<String>,
) -> Result<(MergeCounts, Vec<SkippedSchedule>), AppError> {
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

    let (added, updated, skipped) = plan_schedules(conn, src, existing);
    counts.schedules_added = added;
    counts.schedules_updated = updated;
    counts.schedules_skipped = skipped.len() as i64;
    Ok((counts, skipped))
}

/// Schedule half of the preview. Validates each source template with the same
/// rules the merge will apply, against destination ∪ source accounts (the
/// merge writes accounts before schedules). Builder-level skips are already in
/// `src.skipped_schedules`; validation failures join them here.
fn plan_schedules(
    conn: &Connection,
    src: &SourceData,
    existing: &ExistingData,
) -> (i64, i64, Vec<SkippedSchedule>) {
    let map = import_account_map(existing, src);
    let lookup =
        |guid: &str| -> Result<Option<SplitAccountInfo>, AppError> { Ok(map.get(guid).cloned()) };
    let mut added = 0;
    let mut updated = 0;
    let mut skipped = src.skipped_schedules.clone();
    for schedule in &src.schedules {
        match schedules::plan_imported_schedule(conn, schedule, &lookup) {
            Ok(ScheduleMerge::Added) => added += 1,
            Ok(ScheduleMerge::Updated) => updated += 1,
            Ok(ScheduleMerge::Unchanged) => {}
            Err(e) => skipped.push(SkippedSchedule {
                name: schedule.input.name.clone(),
                reason: e.to_string(),
            }),
        }
    }
    (added, updated, skipped)
}

/// Account facts used to validate source templates: destination rows plus the
/// source rows about to be merged (the preview runs before either exists).
fn import_account_map(
    existing: &ExistingData,
    src: &SourceData,
) -> HashMap<String, SplitAccountInfo> {
    let mut map: HashMap<String, SplitAccountInfo> = existing
        .accounts
        .iter()
        .map(|(guid, a)| {
            (
                guid.clone(),
                SplitAccountInfo {
                    placeholder: a.placeholder,
                    account_type: a.account_type.clone(),
                },
            )
        })
        .collect();
    for a in &src.accounts {
        map.insert(
            a.guid.clone(),
            SplitAccountInfo {
                placeholder: a.placeholder,
                account_type: a.account_type.clone(),
            },
        );
    }
    map
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
    /// Scheduled transactions merged by GUID (never swept).
    pub schedules_added: i64,
    pub schedules_updated: i64,
    /// Sample of local account names the sweep would remove.
    pub orphan_account_names: Vec<String>,
    /// Source schedules Spendo can't represent, with readable reasons.
    pub skipped_schedules: Vec<SkippedSchedule>,
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
/// destroy budgets), accounts referenced by import rules (same cascade
/// problem), and accounts referenced by schedules.
fn protected_accounts(conn: &Connection, src: &SourceData) -> Result<HashSet<String>, AppError> {
    let mut protected: HashSet<String> = protected_default_roots(conn, src)?.into_iter().collect();
    protected.extend(accounts_with_budgets(conn)?);
    protected.extend(crate::import_rules::rule_account_guids(conn)?);
    protected.extend(schedules::schedule_account_guids(conn)?);
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
            accounts: read_account(conn)?
                .into_iter()
                .map(|a| (a.guid.clone(), a))
                .collect(),
            transactions: read_transaction(conn)?
                .into_iter()
                .map(|t| (t.guid.clone(), t))
                .collect(),
            splits: read_split(conn)?
                .into_iter()
                .map(|s| (s.guid.clone(), s))
                .collect(),
        })
    }
}

/// Added/updated/orphaned tallies for one import merge.
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
    schedules_added: i64,
    schedules_updated: i64,
    schedules_skipped: i64,
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

    // Schedules: insert or refresh definitions. Rows the preview reported as
    // skipped fail validation here too (the account set is identical once the
    // account merge above has run) and are skipped rather than failing the
    // whole import.
    counts.schedules_skipped = src.skipped_schedules.len() as i64;
    for schedule in &src.schedules {
        match schedules::apply_imported_schedule(conn, schedule) {
            Ok(ScheduleMerge::Added) => counts.schedules_added += 1,
            Ok(ScheduleMerge::Updated) => counts.schedules_updated += 1,
            Ok(ScheduleMerge::Unchanged) => {}
            Err(_) => counts.schedules_skipped += 1,
        }
    }

    if delete_orphans {
        delete_orphans_rows(conn, "splits", "tmp_src_splits", None)?;
        delete_orphans_rows(conn, "transactions", "tmp_src_transactions", None)?;
        delete_orphans_rows(conn, "accounts", "tmp_src_accounts", accounts_excluded)?;
        delete_orphans_rows(conn, "commodities", "tmp_src_commodities", None)?;
    }
    Ok(())
}

// ── Import ──────────────────────────────────────────────────────────────────

/// Path-based import for tests; the `#[tauri::command]` reads the source
/// before taking the DB lock and calls [`import_source_inner`].
#[cfg(test)]
pub(crate) fn import_gnucash_inner(
    dest_conn: &Connection,
    source_path: &str,
    _filename: &str,
    _sha256: &str,
    delete_orphans: bool,
) -> Result<ImportResult, AppError> {
    let source = connect_source_db(&std::path::PathBuf::from(source_path))
        .map_err(|e| AppError::Io(format!("Failed to open source DB: {}", e)))?;
    let src = read_source_data(&source)?;
    import_source_inner(dest_conn, src, _filename, delete_orphans)
}

/// Merge an already-read source into the destination. Callers read the source
/// **before** taking the destination DB lock; this is the lock-holding half.
fn import_source_inner(
    dest_conn: &Connection,
    src: SourceData,
    _filename: &str,
    delete_orphans: bool,
) -> Result<ImportResult, AppError> {
    init_db(dest_conn).map_err(|e| AppError::Io(format!("Failed to init dest DB: {}", e)))?;

    // The orphan sweep treats the source as authoritative and deletes rows it
    // doesn't cover, so an empty source set (`NOT IN (empty)`) would wipe the
    // whole book. Abort instead — there is nothing to import.
    if src.accounts.is_empty() {
        return Err(AppError::Validation(
            "Source book has no accounts. Nothing to import".to_string(),
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
    // sweep can skip them. Empty set = no exclusion.
    let protected = protected_accounts(dest_conn, &src)?;
    let protected_guids: Vec<String> = protected.iter().cloned().collect();
    populate_source_guids(dest_conn, "tmp_protected_accounts", &protected_guids)?;
    let accounts_excluded = if protected.is_empty() {
        None
    } else {
        Some("tmp_protected_accounts")
    };

    let mut counts = MergeCounts::default();

    // Counts live inside the txn so the numbers the caller reports match the
    // deletes they describe. Temp tables live on the long-lived connection;
    // drop them after the txn instead of letting them linger until the next
    // import recreates them.
    let tx_result: Result<(), AppError> = db::with_tx_err(dest_conn, || {
        if delete_orphans {
            counts.commodities_orphaned =
                count_orphaned(dest_conn, "commodities", "tmp_src_commodities", None)?;
            counts.accounts_orphaned =
                count_orphaned(dest_conn, "accounts", "tmp_src_accounts", accounts_excluded)?;
            counts.transactions_orphaned =
                count_orphaned(dest_conn, "transactions", "tmp_src_transactions", None)?;
            counts.splits_orphaned = count_orphaned(dest_conn, "splits", "tmp_src_splits", None)?;
        }

        let existing = ExistingData::load(dest_conn)?;
        apply_merge(
            dest_conn,
            &src,
            &existing,
            &mut counts,
            accounts_excluded,
            delete_orphans,
        )?;

        // Categories the source never covered: create the canonical root now.
        init::ensure_default_roots(dest_conn)?;

        Ok(())
    });

    drop_source_guid_tables(dest_conn);
    tx_result?;

    let summary = get_summary(dest_conn)?;

    let total_accounts: i64 =
        dest_conn.query_row("SELECT COUNT(*) as cnt FROM accounts", [], |row| row.get(0))?;

    let total_transactions: i64 =
        dest_conn.query_row("SELECT COUNT(*) as cnt FROM transactions", [], |row| {
            row.get(0)
        })?;

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
        schedules_added: counts.schedules_added,
        schedules_updated: counts.schedules_updated,
        schedules_skipped: counts.schedules_skipped,
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
        Err(e) => return failed(e.to_string()),
    };
    if let Err(e) = db::run_blocking(db, move |conn| {
        if book_generation.load(std::sync::atomic::Ordering::SeqCst) != expected_generation {
            return Err(AppError::Validation(
                "Book changed while prices were refreshing. Those results were discarded"
                    .to_string(),
            ));
        }
        prices::write_prices_and_log(conn, &write_data)
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
    let generation = state
        .book_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    // Hash and read the source on a blocking thread before the DB lock: the
    // digest and the data come from the same file state, and a slow source
    // book cannot stall other commands.
    let read_path = path.clone();
    let (sha256, src) = db::run_io(move || {
        utils::validate_gnucash_path(&read_path)?;
        let sha = file_sha256(&read_path)?;
        let source = connect_source_db(&std::path::PathBuf::from(&read_path))
            .map_err(|e| AppError::Io(format!("Failed to open source DB: {}", e)))?;
        let src = read_source_data(&source)?;
        Ok((sha, src))
    })
    .await?;
    let generation_handle = state.book_generation.clone();
    db::run_blocking(state.db.clone(), move |conn| {
        db::ensure_same_book(&generation_handle, generation)?;
        preview_source_inner(conn, src, &path, &filename, &sha256)
    })
    .await
}

/// Path-based preview for tests; the command reads the source before taking
/// the DB lock and calls [`preview_source_inner`].
#[cfg(test)]
fn preview_gnucash_inner(
    dest_conn: &Connection,
    source_path: &str,
    filename: &str,
    sha256: &str,
) -> Result<GnuCashPreview, AppError> {
    let source = connect_source_db(&std::path::PathBuf::from(source_path))
        .map_err(|e| AppError::Io(format!("Failed to open source DB: {}", e)))?;
    let src = read_source_data(&source)?;
    preview_source_inner(dest_conn, src, source_path, filename, sha256)
}

fn preview_source_inner(
    dest_conn: &Connection,
    src: SourceData,
    source_path: &str,
    filename: &str,
    sha256: &str,
) -> Result<GnuCashPreview, AppError> {
    if src.accounts.is_empty() {
        return Err(AppError::Validation(
            "Source book has no accounts. Nothing to import".to_string(),
        ));
    }
    let protected = protected_accounts(dest_conn, &src)?;
    let existing = ExistingData::load(dest_conn)?;
    let (counts, skipped_schedules) = plan_import(dest_conn, &src, &existing, &protected)?;
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
        schedules_added: counts.schedules_added,
        schedules_updated: counts.schedules_updated,
        orphan_account_names: orphan_account_names(&src, &existing, &protected, 20),
        skipped_schedules,
    })
}

#[tauri::command]
pub async fn import_gnucash(
    state: State<'_, AppState>,
    input: GnuCashImportInput,
) -> Result<ImportResult, String> {
    // Capture the active book before any await: the merge must never land in a
    // book the user switched to while the file was being hashed.
    let generation = state
        .book_generation
        .load(std::sync::atomic::Ordering::SeqCst);
    // Validate, hash and read the source on a blocking thread, before the DB
    // lock is touched. Hashing and reading in one closure also closes the
    // window where the file could change between the two.
    let read_path = input.path.clone();
    let (current_sha, src) = db::run_io(move || {
        utils::validate_gnucash_path(&read_path)?;
        let sha = file_sha256(&read_path)?;
        let source = connect_source_db(&std::path::PathBuf::from(&read_path))
            .map_err(|e| AppError::Io(format!("Failed to open source DB: {}", e)))?;
        let src = read_source_data(&source)?;
        Ok((sha, src))
    })
    .await?;
    if current_sha != input.sha256 {
        return Err(
            "Source file changed since the preview. Please run the import again".to_string(),
        );
    }
    let generation_handle = state.book_generation.clone();
    let GnuCashImportInput {
        filename,
        delete_orphans,
        ..
    } = input;
    let mut result = db::run_blocking(state.db.clone(), move |conn| {
        db::ensure_same_book(&generation_handle, generation)?;
        import_source_inner(conn, src, &filename, delete_orphans)
    })
    .await?;
    result.prices =
        fetch_prices_best_effort(state.db.clone(), state.book_generation.clone(), generation).await;
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
        assert_eq!(normalize_post_date("20240115103000"), "2024-01-15 10:30:00");
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
    /// rows; the merge adds source rows, sweeps orphans, and an identical
    /// reimport is a no-op.
    #[test]
    fn import_merges_and_sweeps_orphans() {
        let dir = std::env::temp_dir().join(format!("spendo-import-test-{}", uuid::Uuid::new_v4()));
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

        let result =
            import_gnucash_inner(&dest, &src_str, "book.gnucash", "deadbeef", true).unwrap();
        assert_eq!(result.commodities_added, 1);
        assert_eq!(result.commodities_orphaned, 1);
        assert_eq!(result.accounts_added, 2);
        assert_eq!(result.accounts_orphaned, 1);
        assert_eq!(result.transactions_added, 1);
        assert_eq!(result.transactions_orphaned, 1);
        assert_eq!(result.splits_added, 1);
        assert_eq!(result.splits_orphaned, 1);

        // Orphans are gone from the data.
        let count = |table: &str| -> i64 {
            dest.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
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

        // Identical reimport is a no-op.
        let again =
            import_gnucash_inner(&dest, &src_str, "book.gnucash", "deadbeef", true).unwrap();
        assert_eq!(again.transactions_added, 0);
        assert_eq!(again.transactions_orphaned, 0);

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
        let dir =
            std::env::temp_dir().join(format!("spendo-import-{tag}-{}", uuid::Uuid::new_v4()));
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

        let err =
            import_gnucash_inner(&dest, &src_str, "book.gnucash", "deadbeef", true).unwrap_err();
        assert!(err.to_string().contains("no accounts"), "unexpected: {err}");

        let accounts_after: i64 = dest
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(accounts_after, accounts_before);

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
            .query_row(
                "SELECT account_guid FROM splits WHERE guid = 's1'",
                [],
                |r| r.get(0),
            )
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

    /// Regression: sweeping a budgeted account cascades the budget rows away,
    /// so the sweep must skip them.
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
            dest.query_row(
                "SELECT description FROM transactions WHERE guid = 't-local'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "Local"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// GnuCash source with the scheduled-transaction tables: `sx-1` is a
    /// monthly SX whose template is the two splits of `tpl-tx` on the template
    /// account, with the real accounts and amounts in `sched-xaction/*` KVP
    /// slots and one generated instance (`gen-tx`). `sx-2` has two recurrence
    /// rules and must be reported as skipped rather than imported.
    fn write_sx_source_db(path: &std::path::Path) {
        let src = Connection::open(path).unwrap();
        src.execute_batch(
            "CREATE TABLE books (guid TEXT PRIMARY KEY, root_account_guid TEXT, root_template_guid TEXT);
             CREATE TABLE commodities (guid TEXT PRIMARY KEY, mnemonic TEXT, namespace TEXT, fullname TEXT);
             CREATE TABLE accounts (guid TEXT PRIMARY KEY, name TEXT, account_type TEXT, parent_guid TEXT, code TEXT, description TEXT, placeholder INTEGER, commodity_guid TEXT);
             CREATE TABLE transactions (guid TEXT PRIMARY KEY, post_date TEXT, description TEXT, num TEXT);
             CREATE TABLE splits (guid TEXT PRIMARY KEY, tx_guid TEXT, account_guid TEXT, value_num INTEGER, value_denom INTEGER, quantity_num INTEGER, quantity_denom INTEGER, reconcile_state TEXT, memo TEXT);
             CREATE TABLE slots (obj_guid TEXT, name TEXT, slot_type INTEGER, int64_val INTEGER, string_val TEXT, double_val REAL, timespec_val TEXT, guid_val TEXT, numeric_val_num INTEGER, numeric_val_denom INTEGER, gdate_val TEXT);
             CREATE TABLE schedxactions (guid TEXT PRIMARY KEY, name TEXT, enabled INTEGER NOT NULL, start_date TEXT, end_date TEXT, last_occur TEXT, num_occur INTEGER NOT NULL, rem_occur INTEGER NOT NULL, auto_create INTEGER NOT NULL, auto_notify INTEGER NOT NULL, adv_creation INTEGER NOT NULL, adv_notify INTEGER NOT NULL, instance_count INTEGER NOT NULL, template_act_guid TEXT NOT NULL);
             CREATE TABLE recurrences (id INTEGER PRIMARY KEY, obj_guid TEXT, recurrence_mult INTEGER, recurrence_period_type TEXT, recurrence_period_start TEXT, recurrence_weekend_adjust TEXT);

             INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               VALUES ('src-root', 'Root Account', 'ROOT', NULL, 0, 'c-eur'),
                      ('checking', 'Checking', 'BANK', 'src-root', 0, 'c-eur'),
                      ('expenses', 'Expenses', 'EXPENSE', 'src-root', 0, 'c-eur'),
                      ('tpl-root', 'Template Root', 'ROOT', NULL, 0, 'c-eur'),
                      ('tpl-acct', 'sx-1', 'BANK', 'tpl-root', 0, 'c-eur'),
                      ('tpl-acct-2', 'sx-2', 'BANK', 'tpl-root', 0, 'c-eur');
             INSERT INTO books (guid, root_account_guid, root_template_guid)
               VALUES ('book-1', 'src-root', 'tpl-root');
             INSERT INTO schedxactions (guid, name, enabled, start_date, end_date, last_occur,
                                        num_occur, rem_occur, auto_create, auto_notify,
                                        adv_creation, adv_notify, instance_count, template_act_guid)
               VALUES ('sx-1', 'Rent', 1, '20260101', NULL, '20260301', 0, 0, 0, 0, 0, 0, 3, 'tpl-acct'),
                      ('sx-2', 'Twice a month', 1, '20260101', NULL, NULL, 0, 0, 0, 0, 0, 0, 0, 'tpl-acct-2');
             INSERT INTO recurrences (obj_guid, recurrence_mult, recurrence_period_type,
                                      recurrence_period_start, recurrence_weekend_adjust)
               VALUES ('sx-1', 1, 'month', '20260105', 'none'),
                      ('sx-2', 1, 'month', '20260105', 'none'),
                      ('sx-2', 1, 'month', '20260120', 'none');
             INSERT INTO transactions (guid, post_date, description, num)
               VALUES ('tpl-tx', '2026-01-04 10:00:00', 'Monthly rent', 'R1'),
                      ('gen-tx', '2026-03-05 10:00:00', 'Monthly rent', NULL),
                      ('t1', '2026-01-15', 'Groceries', NULL);
             INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom,
                                 quantity_num, quantity_denom, reconcile_state, memo)
               VALUES ('tpl-s1', 'tpl-tx', 'tpl-acct', 0, 100, 0, 1, 'n', 'rent'),
                      ('tpl-s2', 'tpl-tx', 'tpl-acct', 0, 100, 0, 1, 'n', NULL),
                      ('gen-s1', 'gen-tx', 'expenses', 1000, 100, 1000, 100, 'n', NULL),
                      ('gen-s2', 'gen-tx', 'checking', -1000, 100, -1000, 100, 'n', NULL),
                      ('s1', 't1', 'expenses', 500, 100, 500, 100, 'n', NULL),
                      ('s2', 't1', 'checking', -500, 100, -500, 100, 'n', NULL);
             INSERT INTO slots (obj_guid, name, slot_type, string_val, guid_val,
                                numeric_val_num, numeric_val_denom)
               VALUES ('tpl-s1', 'sched-xaction', 9, NULL, 'frame-1', NULL, NULL),
                      ('frame-1', 'sched-xaction/account', 5, NULL, 'expenses', NULL, NULL),
                      ('frame-1', 'sched-xaction/debit-formula', 4, '1000', NULL, NULL, NULL),
                      ('frame-1', 'sched-xaction/debit-numeric', 3, NULL, NULL, 1000, 1),
                      ('frame-1', 'sched-xaction/credit-formula', 4, '', NULL, NULL, NULL),
                      ('frame-1', 'sched-xaction/credit-numeric', 3, NULL, NULL, 0, 1),
                      ('tpl-s2', 'sched-xaction', 9, NULL, 'frame-2', NULL, NULL),
                      ('frame-2', 'sched-xaction/account', 5, NULL, 'checking', NULL, NULL),
                      ('frame-2', 'sched-xaction/credit-formula', 4, '1000', NULL, NULL, NULL),
                      ('frame-2', 'sched-xaction/credit-numeric', 3, NULL, NULL, 1000, 1),
                      ('frame-2', 'sched-xaction/debit-formula', 4, '', NULL, NULL, NULL),
                      ('frame-2', 'sched-xaction/debit-numeric', 3, NULL, NULL, 0, 1),
                      ('tpl-tx', 'notes', 4, 'Rent note', NULL, NULL, NULL),
                      ('gen-tx', 'from-sched-xaction', 5, NULL, 'sx-1', NULL, NULL);",
        )
        .unwrap();
    }

    /// End-to-end SX conversion: source templates become schedules, template
    /// plumbing is excluded from the entity merge, the pointer skips generated
    /// instances, and skipped schedules are reported.
    #[test]
    fn import_converts_scheduled_transactions_and_excludes_templates() {
        let dir = std::env::temp_dir().join(format!("spendo-import-sx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("book.gnucash");
        write_sx_source_db(&src_path);
        let src_str = src_path.to_string_lossy().to_string();

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();

        // The preview reports the same schedule counts the merge applies.
        let preview = preview_gnucash_inner(&dest, &src_str, "book.gnucash", "hash").unwrap();
        assert_eq!(preview.schedules_added, 1);
        assert_eq!(preview.schedules_updated, 0);
        assert_eq!(preview.skipped_schedules.len(), 1);
        assert_eq!(preview.skipped_schedules[0].name, "Twice a month");
        assert!(
            preview.skipped_schedules[0]
                .reason
                .contains("more than one recurrence"),
            "unexpected: {}",
            preview.skipped_schedules[0].reason
        );

        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "hash", true).unwrap();
        assert_eq!(result.schedules_added, preview.schedules_added);
        assert_eq!(result.schedules_updated, preview.schedules_updated);
        assert_eq!(result.schedules_skipped, 1);
        assert_eq!(result.transactions_added, 2); // t1 + gen-tx, no template tx
        assert_eq!(result.accounts_added, 3); // no template root/accounts

        // Template plumbing never lands in the book.
        for junk in ["tpl-root", "tpl-acct", "tpl-acct-2"] {
            let n: i64 = dest
                .query_row(
                    "SELECT COUNT(*) FROM accounts WHERE guid = ?1",
                    [junk],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 0, "template account {junk} leaked");
        }
        assert_eq!(count(&dest, "recurring_schedules"), 1);

        // The schedule mirrors the SX definition, with the pointer after the
        // already-generated instance (2026-03-05) rather than at the start.
        let (name, freq, mult, start, max_occ, posted, next, active, template): (
            String,
            String,
            i64,
            String,
            Option<i64>,
            i64,
            Option<String>,
            i64,
            String,
        ) = dest
            .query_row(
                "SELECT name, frequency, interval_mult, start_date, max_occurrences,
                        occurrences_posted, next_due_date, active, template_json
                 FROM recurring_schedules WHERE guid = 'sx-1'",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(name, "Rent");
        assert_eq!(freq, "monthly");
        assert_eq!(mult, 1);
        assert_eq!(start, "2026-01-01");
        assert_eq!(max_occ, None);
        assert_eq!(posted, 3);
        assert_eq!(next.as_deref(), Some("2026-04-05"));
        assert_eq!(active, 1);
        let template: serde_json::Value = serde_json::from_str(&template).unwrap();
        assert_eq!(template["description"], "Monthly rent");
        assert_eq!(template["num"], "R1");
        assert_eq!(template["notes"], "Rent note");
        let splits = template["splits"].as_array().unwrap();
        assert_eq!(splits.len(), 2);
        assert!(splits
            .iter()
            .any(|s| s["accountGuid"] == "expenses" && s["amount"] == 1000.0));
        assert!(splits
            .iter()
            .any(|s| s["accountGuid"] == "checking" && s["amount"] == -1000.0));
        assert!(splits.iter().any(|s| s["memo"] == "rent"));

        // A re-import is a no-op.
        let again = import_gnucash_inner(&dest, &src_str, "book.gnucash", "hash", true).unwrap();
        assert_eq!(again.schedules_added, 0);
        assert_eq!(again.schedules_updated, 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Re-import refreshes the definition but keeps local state: a pause and
    /// the posts counter survive, and the pointer never moves backwards.
    #[test]
    fn reimport_refreshes_definition_but_keeps_local_state() {
        let dir = std::env::temp_dir().join(format!("spendo-import-sx2-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("book.gnucash");
        write_sx_source_db(&src_path);
        let src_str = src_path.to_string_lossy().to_string();

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        import_gnucash_inner(&dest, &src_str, "book.gnucash", "hash", true).unwrap();

        // Local decisions: paused and ahead on posts.
        dest.execute(
            "UPDATE recurring_schedules
             SET active = 0, occurrences_posted = 5 WHERE guid = 'sx-1'",
            [],
        )
        .unwrap();
        // Source edit: renamed.
        {
            let src = Connection::open(&src_path).unwrap();
            src.execute("UPDATE schedxactions SET name = 'Rent v2'", [])
                .unwrap();
        }

        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "hash2", true).unwrap();
        assert_eq!(result.schedules_added, 0);
        assert_eq!(result.schedules_updated, 1);
        let (name, active, posted, next): (String, i64, i64, Option<String>) = dest
            .query_row(
                "SELECT name, active, occurrences_posted, next_due_date
                 FROM recurring_schedules WHERE guid = 'sx-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(name, "Rent v2");
        assert_eq!(active, 0);
        assert_eq!(posted, 5);
        assert_eq!(next.as_deref(), Some("2026-04-05"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The sweep keeps accounts a schedule still references, like budgets and
    /// import rules (config tables with FK/dangling-reference semantics).
    #[test]
    fn import_preserves_accounts_referenced_by_schedules() {
        let dir = std::env::temp_dir().join(format!("spendo-import-sx3-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src_path = dir.join("book.gnucash");
        write_sx_source_db(&src_path);
        let src_str = src_path.to_string_lossy().to_string();

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        dest.execute_batch(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
               VALUES ('local-a', 'Local Savings', 'ASSET', NULL, 0, NULL),
                      ('local-b', 'Local Fees', 'EXPENSE', NULL, 0, NULL);",
        )
        .unwrap();
        schedules::apply_imported_schedule(
            &dest,
            &ImportedSchedule {
                guid: "local-sx".to_string(),
                input: ScheduleInput {
                    name: "Local".to_string(),
                    frequency: "monthly".to_string(),
                    interval_mult: Some(1),
                    start_date: "2026-01-01".to_string(),
                    end_date: None,
                    max_occurrences: None,
                    description: None,
                    num: None,
                    notes: None,
                    splits: vec![
                        SplitInput {
                            account_guid: "local-a".to_string(),
                            amount: -25.0,
                            quantity: None,
                            memo: None,
                        },
                        SplitInput {
                            account_guid: "local-b".to_string(),
                            amount: 25.0,
                            quantity: None,
                            memo: None,
                        },
                    ],
                },
                active: true,
                occurrences_posted: 0,
                next_due_date: Some("2026-01-01".to_string()),
            },
        )
        .unwrap();

        let result = import_gnucash_inner(&dest, &src_str, "book.gnucash", "hash", true).unwrap();
        // Local-only accounts survive; only the imported schedule is new.
        assert_eq!(result.schedules_added, 1);
        for guid in ["local-a", "local-b"] {
            let n: i64 = dest
                .query_row(
                    "SELECT COUNT(*) FROM accounts WHERE guid = ?1",
                    [guid],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "schedule account {guid} was swept");
        }
        assert_eq!(count(&dest, "recurring_schedules"), 2);

        std::fs::remove_dir_all(&dir).ok();
    }
}
