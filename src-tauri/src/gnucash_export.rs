//! Full-book export to a GnuCash SQLite book (`.gnucash`).
//!
//! GnuCash has no import API for third-party writers and no embeddable library
//! here, but its DBI backend only needs well-formed tables plus the `versions`
//! rows: the schema below is the exact DDL GnuCash 5.16 writes for its own
//! SQLite books (`libgnucash/backend/sql/*-sql.cpp`), verified by dumping a
//! book created by the installed GnuCash and by round-tripping a hand-built
//! file through the engine (open, re-save, re-open).
//!
//! Rules of that format this module must respect:
//! - The DDL and the per-table version numbers move together. GnuCash snaps
//!   mismatched versions by rebuilding a table from `SELECT *` of the old one,
//!   so a column/version disagreement corrupts or aborts the upgrade. Bump both
//!   together when re-targeting a GnuCash release, and re-verify against a book
//!   that release wrote.
//! - `versions` must contain a non-zero `Gnucash` row or GnuCash refuses the
//!   file (`ERR_SQL_DB_TOO_OLD`). `Gnucash-Resave` is a fixed constant.
//! - `books` must point at a ROOT account and at a template ROOT account.
//! - KVP frames (scheduled-transaction templates) are two rows: a frame slot
//!   (`slot_type = 9`) whose `guid_val` is a synthetic frame GUID, plus one row
//!   per child keyed by that frame GUID. Child `name`s carry the full path.
//! - Slot type codes: numeric = 3, string = 4, guid = 5, frame = 9.
//!
//! This file is a GnuCash artifact, not a Spendo book: it carries no
//! `application_id` stamp and none of the Spendo-only tables, so Spendo's own
//! Open rejects it exactly like any other foreign SQLite file.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::State;

use crate::db::{run_blocking, run_io, with_tx_err, AppError, AppState};
use crate::schedules;

/// GnuCash release this export targets (`Gnucash` version row = major·1e6 +
/// minor, as GnuCash writes it). Newer GnuCash upgrades the file on first
/// open; the DDL above must match whatever number is stamped here.
const GNUCASH_VERSION: i64 = 5_000_016; // GnuCash 5.16
const GNUCASH_RESAVE_VERSION: i64 = 19_920;

/// Table versions GnuCash 5.16 stamps for its own books, in the order its
/// backend writes them. Must stay in lockstep with [`GNUCASH_SCHEMA`].
const GNUCASH_TABLE_VERSIONS: [(&str, i64); 22] = [
    ("books", 1),
    ("commodities", 1),
    ("accounts", 1),
    ("budgets", 1),
    ("budget_amounts", 1),
    ("prices", 3),
    ("transactions", 4),
    ("splits", 5),
    ("slots", 4),
    ("recurrences", 2),
    ("schedxactions", 1),
    ("lots", 2),
    ("billterms", 2),
    ("customers", 2),
    ("employees", 2),
    ("entries", 4),
    ("invoices", 4),
    ("jobs", 1),
    ("orders", 1),
    ("taxtables", 2),
    ("taxtable_entries", 3),
    ("vendors", 1),
];

/// GnuCash 5.16 schema, transcribed from the SQL backend's table definitions.
/// Business tables we never populate are created empty so the file is complete
/// for every reader without waiting for GnuCash to fill the gaps on save.
const GNUCASH_SCHEMA: &str = "
CREATE TABLE gnclock ( Hostname varchar(255), PID int );
CREATE TABLE versions(table_name text(50) PRIMARY KEY NOT NULL, table_version integer NOT NULL);
CREATE TABLE books(guid text(32) PRIMARY KEY NOT NULL, root_account_guid text(32) NOT NULL, root_template_guid text(32) NOT NULL);
CREATE TABLE commodities(guid text(32) PRIMARY KEY NOT NULL, namespace text(2048) NOT NULL, mnemonic text(2048) NOT NULL, fullname text(2048), cusip text(2048), fraction integer NOT NULL, quote_flag integer NOT NULL, quote_source text(2048), quote_tz text(2048));
CREATE TABLE accounts(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, account_type text(2048) NOT NULL, commodity_guid text(32), commodity_scu integer NOT NULL, non_std_scu integer NOT NULL, parent_guid text(32), code text(2048), description text(2048), hidden integer, placeholder integer);
CREATE TABLE budgets(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, description text(2048), num_periods integer NOT NULL);
CREATE TABLE budget_amounts(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, budget_guid text(32) NOT NULL, account_guid text(32) NOT NULL, period_num integer NOT NULL, amount_num bigint NOT NULL, amount_denom bigint NOT NULL);
CREATE TABLE prices(guid text(32) PRIMARY KEY NOT NULL, commodity_guid text(32) NOT NULL, currency_guid text(32) NOT NULL, date text(19) NOT NULL, source text(2048), type text(2048), value_num bigint NOT NULL, value_denom bigint NOT NULL);
CREATE TABLE transactions(guid text(32) PRIMARY KEY NOT NULL, currency_guid text(32) NOT NULL, num text(2048) NOT NULL, post_date text(19), enter_date text(19), description text(2048));
CREATE TABLE splits(guid text(32) PRIMARY KEY NOT NULL, tx_guid text(32) NOT NULL, account_guid text(32) NOT NULL, memo text(2048) NOT NULL, action text(2048) NOT NULL, reconcile_state text(1) NOT NULL, reconcile_date text(19), value_num bigint NOT NULL, value_denom bigint NOT NULL, quantity_num bigint NOT NULL, quantity_denom bigint NOT NULL, lot_guid text(32));
CREATE TABLE slots(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, obj_guid text(32) NOT NULL, name text(4096) NOT NULL, slot_type integer NOT NULL, int64_val bigint, string_val text(4096), double_val float8, timespec_val text(19), guid_val text(32), numeric_val_num bigint, numeric_val_denom bigint, gdate_val text(8));
CREATE TABLE recurrences(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, obj_guid text(32) NOT NULL, recurrence_mult integer NOT NULL, recurrence_period_type text(2048) NOT NULL, recurrence_period_start text(8) NOT NULL, recurrence_weekend_adjust text(2048) NOT NULL);
CREATE TABLE schedxactions(guid text(32) PRIMARY KEY NOT NULL, name text(2048), enabled integer NOT NULL, start_date text(8), end_date text(8), last_occur text(8), num_occur integer NOT NULL, rem_occur integer NOT NULL, auto_create integer NOT NULL, auto_notify integer NOT NULL, adv_creation integer NOT NULL, adv_notify integer NOT NULL, instance_count integer NOT NULL, template_act_guid text(32) NOT NULL);
CREATE TABLE lots(guid text(32) PRIMARY KEY NOT NULL, account_guid text(32), is_closed integer NOT NULL);
CREATE TABLE billterms(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, description text(2048) NOT NULL, refcount integer NOT NULL, invisible integer NOT NULL, parent text(32), type text(2048) NOT NULL, duedays integer, discountdays integer, discount_num bigint, discount_denom bigint, cutoff integer);
CREATE TABLE customers(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, id text(2048) NOT NULL, notes text(2048) NOT NULL, active integer NOT NULL, discount_num bigint NOT NULL, discount_denom bigint NOT NULL, credit_num bigint NOT NULL, credit_denom bigint NOT NULL, currency text(32) NOT NULL, tax_override integer NOT NULL, addr_name text(1024), addr_addr1 text(1024), addr_addr2 text(1024), addr_addr3 text(1024), addr_addr4 text(1024), addr_phone text(128), addr_fax text(128), addr_email text(256), shipaddr_name text(1024), shipaddr_addr1 text(1024), shipaddr_addr2 text(1024), shipaddr_addr3 text(1024), shipaddr_addr4 text(1024), shipaddr_phone text(128), shipaddr_fax text(128), shipaddr_email text(256), terms text(32), tax_included integer, taxtable text(32));
CREATE TABLE employees(guid text(32) PRIMARY KEY NOT NULL, username text(2048) NOT NULL, id text(2048) NOT NULL, language text(2048) NOT NULL, acl text(2048) NOT NULL, active integer NOT NULL, currency text(32) NOT NULL, ccard_guid text(32), workday_num bigint NOT NULL, workday_denom bigint NOT NULL, rate_num bigint NOT NULL, rate_denom bigint NOT NULL, addr_name text(1024), addr_addr1 text(1024), addr_addr2 text(1024), addr_addr3 text(1024), addr_addr4 text(1024), addr_phone text(128), addr_fax text(128), addr_email text(256));
CREATE TABLE entries(guid text(32) PRIMARY KEY NOT NULL, date text(19) NOT NULL, date_entered text(19), description text(2048), action text(2048), notes text(2048), quantity_num bigint, quantity_denom bigint, i_acct text(32), i_price_num bigint, i_price_denom bigint, i_discount_num bigint, i_discount_denom bigint, invoice text(32), i_disc_type text(2048), i_disc_how text(2048), i_taxable integer, i_taxincluded integer, i_taxtable text(32), b_acct text(32), b_price_num bigint, b_price_denom bigint, bill text(32), b_taxable integer, b_taxincluded integer, b_taxtable text(32), b_paytype integer, billable integer, billto_type integer, billto_guid text(32), order_guid text(32));
CREATE TABLE invoices(guid text(32) PRIMARY KEY NOT NULL, id text(2048) NOT NULL, date_opened text(19), date_posted text(19), notes text(2048) NOT NULL, active integer NOT NULL, currency text(32) NOT NULL, owner_type integer, owner_guid text(32), terms text(32), billing_id text(2048), post_txn text(32), post_lot text(32), post_acc text(32), billto_type integer, billto_guid text(32), charge_amt_num bigint, charge_amt_denom bigint);
CREATE TABLE jobs(guid text(32) PRIMARY KEY NOT NULL, id text(2048) NOT NULL, name text(2048) NOT NULL, reference text(2048) NOT NULL, active integer NOT NULL, owner_type integer, owner_guid text(32));
CREATE TABLE orders(guid text(32) PRIMARY KEY NOT NULL, id text(2048) NOT NULL, notes text(2048) NOT NULL, reference text(2048) NOT NULL, active integer NOT NULL, date_opened text(19) NOT NULL, date_closed text(19) NOT NULL, owner_type integer NOT NULL, owner_guid text(32) NOT NULL);
CREATE TABLE taxtables(guid text(32) PRIMARY KEY NOT NULL, name text(50) NOT NULL, refcount bigint NOT NULL, invisible integer NOT NULL, parent text(32));
CREATE TABLE taxtable_entries(id integer PRIMARY KEY AUTOINCREMENT NOT NULL, taxtable text(32) NOT NULL, account text(32) NOT NULL, amount_num bigint NOT NULL, amount_denom bigint NOT NULL, type integer NOT NULL);
CREATE TABLE vendors(guid text(32) PRIMARY KEY NOT NULL, name text(2048) NOT NULL, id text(2048) NOT NULL, notes text(2048) NOT NULL, currency text(32) NOT NULL, active integer NOT NULL, tax_override integer NOT NULL, addr_name text(1024), addr_addr1 text(1024), addr_addr2 text(1024), addr_addr3 text(1024), addr_addr4 text(1024), addr_phone text(128), addr_fax text(128), addr_email text(256), terms text(32), tax_inc text(2048), tax_table text(32));
CREATE INDEX slots_guid_index ON slots(obj_guid);
CREATE INDEX splits_account_guid_index ON splits(account_guid);
CREATE INDEX splits_tx_guid_index ON splits(tx_guid);
CREATE INDEX tx_post_date_index ON transactions(post_date);
";

/// KVP value type codes as stored in `slots.slot_type`.
const SLOT_NUMERIC: i64 = 3;
const SLOT_STRING: i64 = 4;
const SLOT_GUID: i64 = 5;
const SLOT_FRAME: i64 = 9;

/// GnuCash's own book-level plumbing commodity, used by the template root and
/// every scheduled-transaction template account.
const TEMPLATE_COMMODITY: (&str, &str) = ("template", "template");

/// Book feature marker GnuCash checks before running its one-time budget sign
/// migration (`gnc_maybe_scrub_all_budget_signs`): without it, a book that has
/// budgets gets scrubbed by heuristics and shows the "review the budgets and
/// amend signs" notice. Spendo budgets are expense targets, exported in
/// GnuCash's natural signs (debit positive), so declaring the feature is
/// truthful and skips the migration. The name/description pair must match
/// GnuCash's `features_table` verbatim — the SQL backend stores them as
/// `features/<name>` → description.
const BUDGET_FEATURE_NAME: &str = "Use natural signs in budget amounts";
const BUDGET_FEATURE_DESCRIPTION: &str =
    "Store budget amounts unreversed (i.e. natural) signs (requires at least Gnucash 3.8)";

/// Denominators used when a commodity has no data to derive one from.
const DEFAULT_CURRENCY_FRACTION: i64 = 100;
const DEFAULT_OTHER_FRACTION: i64 = 10_000;
/// GnuCash refuses fractions past 1e9 and displaying more than 9 decimals is
/// meaningless anyway.
const MAX_FRACTION_DIGITS: u32 = 9;
/// Spendo budget amounts arrive as `REAL`; six decimals cover every currency
/// subdivision we can express.
const AMOUNT_SCALE: i64 = 1_000_000;

// ── IPC shapes ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GnuCashExportInput {
    pub path: String,
}

/// Counts of what landed in the file, for the frontend toast. `futureDated`
/// and `attachmentsSkipped` are the caveats worth surfacing: this is a
/// complete dump (future rows included) and GnuCash has no receipt storage.
/// `roundedAmounts` counts splits carrying more decimals than GnuCash's
/// smallest unit for the commodity — GnuCash rounds those when loading (and
/// writes them back rounded on save).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GnuCashExportResult {
    pub accounts: i64,
    pub transactions: i64,
    pub splits: i64,
    pub prices: i64,
    pub budgets: i64,
    pub schedules: i64,
    pub future_dated: i64,
    pub attachments_skipped: i64,
    pub rounded_amounts: i64,
}

// ── Read model ───────────────────────────────────────────────────────────────

struct CommodityRow {
    guid: String,
    mnemonic: String,
    namespace: String,
    fullname: Option<String>,
}

struct AccountRow {
    guid: String,
    name: String,
    account_type: String,
    parent_guid: Option<String>,
    code: Option<String>,
    description: Option<String>,
    placeholder: bool,
    commodity_guid: Option<String>,
}

struct TransactionRow {
    guid: String,
    post_date: String,
    description: Option<String>,
    num: Option<String>,
    notes: Option<String>,
    currency_guid: String,
}

struct SplitRow {
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

struct PriceRow {
    guid: String,
    commodity_guid: String,
    currency_guid: String,
    value_num: i64,
    value_denom: i64,
    date: String,
}

struct BudgetRow {
    account_guid: String,
    year: i64,
    amount: f64,
}

struct TemplateSplitRow {
    account_guid: String,
    /// Signed template amount (Spendo stores GnuCash signs: debit − credit).
    amount: f64,
    quantity: Option<f64>,
    memo: Option<String>,
}

struct ScheduleRow {
    guid: String,
    name: String,
    frequency: String,
    interval_mult: i64,
    start_date: String,
    end_date: Option<String>,
    max_occurrences: Option<i64>,
    occurrences_posted: i64,
    active: bool,
    description: Option<String>,
    num: Option<String>,
    notes: Option<String>,
    splits: Vec<TemplateSplitRow>,
}

/// Everything the writer needs, already normalized (currencies resolved,
/// fractions derived, orphan parents re-hung) so the disk phase is a pure
/// insert loop.
struct ExportData {
    book_guid: String,
    root_guid: String,
    template_root_guid: String,
    template_commodity_guid: String,
    base_currency_guid: String,
    commodities: Vec<CommodityRow>,
    fractions: HashMap<String, i64>,
    account_fractions: HashMap<String, i64>,
    accounts: Vec<AccountRow>,
    transactions: Vec<TransactionRow>,
    splits: Vec<SplitRow>,
    prices: Vec<PriceRow>,
    budgets: Vec<BudgetRow>,
    schedules: Vec<ScheduleRow>,
    future_dated: i64,
    attachments_skipped: i64,
    rounded_amounts: i64,
}

// ── Fetch ────────────────────────────────────────────────────────────────────

fn fetch_export_data_inner(conn: &Connection) -> Result<ExportData, AppError> {
    let mut accounts = read_accounts(conn)?;
    let mut commodities = read_commodities(conn)?;
    let transactions_raw = read_transactions(conn)?;
    let splits = read_splits(conn)?;
    let prices = read_prices(conn)?;
    let budgets = read_budgets(conn)?;
    let schedules = read_schedules(conn)?;

    if accounts.is_empty() {
        return Err(AppError::Validation(
            "Book has no accounts — nothing to export".to_string(),
        ));
    }

    // GnuCash's template root and template accounts need the plumbing
    // commodity; reuse an imported one when the book already carries it.
    if !commodities
        .iter()
        .any(|c| c.namespace == TEMPLATE_COMMODITY.0 && c.mnemonic == TEMPLATE_COMMODITY.1)
    {
        commodities.push(CommodityRow {
            guid: new_guid(),
            mnemonic: TEMPLATE_COMMODITY.1.to_string(),
            namespace: TEMPLATE_COMMODITY.0.to_string(),
            fullname: Some(TEMPLATE_COMMODITY.1.to_string()),
        });
    }
    let template_commodity_guid = commodities
        .iter()
        .find(|c| c.namespace == TEMPLATE_COMMODITY.0 && c.mnemonic == TEMPLATE_COMMODITY.1)
        .map(|c| c.guid.clone())
        .expect("template commodity was just ensured");

    let root_guid = choose_root(&accounts)?;
    rehang_orphans(&mut accounts, &root_guid);

    let commodity_by_guid: HashMap<&str, &CommodityRow> =
        commodities.iter().map(|c| (c.guid.as_str(), c)).collect();
    let account_by_guid: HashMap<&str, &AccountRow> =
        accounts.iter().map(|a| (a.guid.as_str(), a)).collect();

    let base_currency_guid = pick_base_currency(&accounts, &commodity_by_guid, &root_guid, conn)?
        .ok_or_else(|| {
        AppError::Validation("Book has no currency to use as the GnuCash base currency".to_string())
    })?;

    let fractions = derive_fractions(&commodities, &accounts, &splits);
    let account_fractions: HashMap<String, i64> = accounts
        .iter()
        .map(|account| {
            let fraction = account
                .commodity_guid
                .as_deref()
                .and_then(|guid| fractions.get(guid).copied())
                .or_else(|| fractions.get(&base_currency_guid).copied())
                .unwrap_or(DEFAULT_CURRENCY_FRACTION);
            (account.guid.clone(), fraction)
        })
        .collect();
    let transactions = resolve_transaction_currencies(
        transactions_raw,
        &splits,
        &account_by_guid,
        &commodity_by_guid,
        &base_currency_guid,
    );

    let future_dated = conn.query_row(
        "SELECT COUNT(*) FROM transactions WHERE post_date >= date('now', 'localtime', '+1 day')",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let attachments_skipped = conn.query_row("SELECT COUNT(*) FROM attachments", [], |row| {
        row.get::<_, i64>(0)
    })?;

    // GnuCash rounds every split to the commodity's smallest unit when it loads
    // the file (`xaccSplitSetValue`/`SetAmount`). Count the affected rows so the
    // frontend can report the rounding instead of losing precision silently.
    let currency_fraction_by_tx: HashMap<&str, i64> = transactions
        .iter()
        .map(|transaction| {
            (
                transaction.guid.as_str(),
                fractions
                    .get(&transaction.currency_guid)
                    .copied()
                    .unwrap_or(DEFAULT_CURRENCY_FRACTION),
            )
        })
        .collect();
    let rounded_amounts = splits
        .iter()
        .filter(|split| {
            let currency_fraction = currency_fraction_by_tx
                .get(split.tx_guid.as_str())
                .copied()
                .unwrap_or(DEFAULT_CURRENCY_FRACTION);
            let account_fraction = account_fractions
                .get(&split.account_guid)
                .copied()
                .unwrap_or(DEFAULT_CURRENCY_FRACTION);
            !fits_precision(split.value_num, split.value_denom, currency_fraction)
                || !fits_precision(split.quantity_num, split.quantity_denom, account_fraction)
        })
        .count() as i64;

    Ok(ExportData {
        book_guid: new_guid(),
        root_guid,
        template_root_guid: new_guid(),
        template_commodity_guid,
        base_currency_guid,
        commodities,
        fractions,
        account_fractions,
        accounts,
        transactions,
        splits,
        prices,
        budgets,
        schedules,
        future_dated,
        attachments_skipped,
        rounded_amounts,
    })
}

fn read_commodities(conn: &Connection) -> Result<Vec<CommodityRow>, AppError> {
    let mut stmt = conn.prepare("SELECT guid, mnemonic, namespace, fullname FROM commodities")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CommodityRow {
                guid: gnu_guid(&row.get::<_, String>(0)?),
                mnemonic: row.get(1)?,
                namespace: row.get(2)?,
                fullname: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_accounts(conn: &Connection) -> Result<Vec<AccountRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid
         FROM accounts",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(AccountRow {
                guid: gnu_guid(&row.get::<_, String>(0)?),
                name: row.get(1)?,
                account_type: row.get(2)?,
                parent_guid: row.get::<_, Option<String>>(3)?.map(|guid| gnu_guid(&guid)),
                code: row.get(4)?,
                description: row.get(5)?,
                placeholder: row.get::<_, Option<i64>>(6)?.unwrap_or(0) != 0,
                commodity_guid: row.get::<_, Option<String>>(7)?.map(|guid| gnu_guid(&guid)),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_transactions(conn: &Connection) -> Result<Vec<TransactionRow>, AppError> {
    let mut stmt =
        conn.prepare("SELECT guid, post_date, description, num, notes FROM transactions")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(TransactionRow {
                guid: gnu_guid(&row.get::<_, String>(0)?),
                post_date: row.get(1)?,
                description: row.get(2)?,
                num: row.get(3)?,
                notes: row.get(4)?,
                currency_guid: String::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_splits(conn: &Connection) -> Result<Vec<SplitRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom,
                reconcile_state, memo
         FROM splits",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SplitRow {
                guid: gnu_guid(&row.get::<_, String>(0)?),
                tx_guid: gnu_guid(&row.get::<_, String>(1)?),
                account_guid: gnu_guid(&row.get::<_, String>(2)?),
                value_num: row.get(3)?,
                value_denom: row.get(4)?,
                quantity_num: row.get(5)?,
                quantity_denom: row.get(6)?,
                reconcile_state: row.get(7)?,
                memo: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_prices(conn: &Connection) -> Result<Vec<PriceRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT guid, commodity_guid, currency_guid, value_num, value_denom, date FROM prices",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PriceRow {
                // Spendo keys price history by `{commodity}-{date}`, not a
                // GUID; prices are never referenced by GUID, so give each row
                // a canonical one like GnuCash's own writer does.
                guid: new_guid(),
                commodity_guid: gnu_guid(&row.get::<_, String>(1)?),
                currency_guid: gnu_guid(&row.get::<_, String>(2)?),
                value_num: row.get(3)?,
                value_denom: row.get(4)?,
                date: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_budgets(conn: &Connection) -> Result<Vec<BudgetRow>, AppError> {
    let mut stmt =
        conn.prepare("SELECT account_guid, year, amount FROM budgets ORDER BY year, account_guid")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(BudgetRow {
                account_guid: gnu_guid(&row.get::<_, String>(0)?),
                year: row.get(1)?,
                amount: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_schedules(conn: &Connection) -> Result<Vec<ScheduleRow>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT guid, name, frequency, interval_mult, start_date, end_date, max_occurrences,
                occurrences_posted, active, template_json
         FROM recurring_schedules",
    )?;
    let rows = stmt
        .query_map([], |row| {
            let template_json: String = row.get(9)?;
            Ok((
                gnu_guid(&row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)? != 0,
                template_json,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut schedules = Vec::with_capacity(rows.len());
    for (
        guid,
        name,
        frequency,
        interval_mult,
        start_date,
        end_date,
        max_occurrences,
        posted,
        active,
        template_json,
    ) in rows
    {
        let template = schedules::parse_template(&template_json)?;
        schedules.push(ScheduleRow {
            guid,
            name,
            frequency,
            interval_mult,
            start_date,
            end_date,
            max_occurrences,
            occurrences_posted: posted,
            active,
            description: template.description,
            num: template.num,
            notes: template.notes,
            splits: template
                .splits
                .into_iter()
                .map(|split| TemplateSplitRow {
                    account_guid: gnu_guid(&split.account_guid),
                    amount: split.amount,
                    quantity: split.quantity,
                    memo: split.memo,
                })
                .collect(),
        });
    }
    Ok(schedules)
}

// ── Derivations ──────────────────────────────────────────────────────────────

/// The GnuCash root: the ROOT account with the most descendants. Books can
/// carry a second ROOT (an imported source root next to the seeded one), and
/// only one of them can be `books.root_account_guid`.
fn choose_root(accounts: &[AccountRow]) -> Result<String, AppError> {
    let roots: Vec<&str> = accounts
        .iter()
        .filter(|a| a.account_type == "ROOT")
        .map(|a| a.guid.as_str())
        .collect();
    let Some(first) = roots.first() else {
        return Err(AppError::Validation(
            "Book has no ROOT account — nothing to export".to_string(),
        ));
    };

    let parent: HashMap<&str, Option<&str>> = accounts
        .iter()
        .map(|a| (a.guid.as_str(), a.parent_guid.as_deref()))
        .collect();
    let mut best = (*first, 0usize);
    for root in &roots {
        let mut count = 0usize;
        for account in accounts {
            let mut cursor = account.parent_guid.as_deref();
            // Depth cap: a corrupt parent cycle terminates instead of hanging.
            for _ in 0..accounts.len() {
                match cursor {
                    Some(guid) if guid != account.guid => {
                        if guid == *root {
                            count += 1;
                            break;
                        }
                        cursor = parent.get(guid).copied().flatten();
                    }
                    _ => break,
                }
            }
        }
        if count > best.1 || (count == best.1 && *root < best.0) {
            best = (root, count);
        }
    }
    Ok(best.0.to_string())
}

/// GnuCash only knows the tree reachable from its root: anything parentless (or
/// pointing at a nonexistent parent) is hung under it, so no account is lost.
fn rehang_orphans(accounts: &mut [AccountRow], root_guid: &str) {
    let known: HashSet<String> = accounts.iter().map(|a| a.guid.clone()).collect();
    for account in accounts.iter_mut() {
        if account.guid == root_guid {
            continue;
        }
        let dangling = account
            .parent_guid
            .as_deref()
            .is_none_or(|parent| !known.contains(parent));
        if dangling {
            account.parent_guid = Some(root_guid.to_string());
        }
    }
}

/// Base currency = the root account's commodity, else the most-used CURRENCY
/// among accounts, else the first CURRENCY commodity in the book.
fn pick_base_currency(
    accounts: &[AccountRow],
    commodity_by_guid: &HashMap<&str, &CommodityRow>,
    root_guid: &str,
    conn: &Connection,
) -> Result<Option<String>, AppError> {
    if let Some(root) = accounts.iter().find(|a| a.guid == root_guid) {
        if let Some(guid) = &root.commodity_guid {
            if commodity_by_guid
                .get(guid.as_str())
                .is_some_and(|c| c.namespace == "CURRENCY")
            {
                return Ok(Some(guid.clone()));
            }
        }
    }

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for account in accounts {
        if let Some(guid) = &account.commodity_guid {
            if commodity_by_guid
                .get(guid.as_str())
                .is_some_and(|c| c.namespace == "CURRENCY")
            {
                *counts.entry(guid.as_str()).or_default() += 1;
            }
        }
    }
    if let Some((guid, _)) = counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
    {
        return Ok(Some((*guid).to_string()));
    }

    Ok(conn
        .query_row(
            "SELECT guid FROM commodities WHERE namespace = 'CURRENCY' ORDER BY guid LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|guid| gnu_guid(&guid)))
}

/// GnuCash renders every amount to the commodity's `fraction` and rounds
/// loaded amounts to it (and to the account's SCU, which mirrors it), so a
/// currency carrying a share-sized denominator would show eight decimals on
/// every balance. Currencies therefore always get GnuCash's own 100 (two
/// decimals), exactly like books GnuCash writes: the rational columns keep the
/// full stored value, and anything beyond the smallest unit rounds the same way
/// GnuCash rounds amounts typed into its register.
///
/// Securities derive their fraction from the **share** (`quantity`)
/// denominators — that column is the account's own commodity. `value`
/// denominators are the transaction currency's and must not leak in.
fn derive_fractions(
    commodities: &[CommodityRow],
    accounts: &[AccountRow],
    splits: &[SplitRow],
) -> HashMap<String, i64> {
    let account_commodity: HashMap<&str, &str> = accounts
        .iter()
        .filter_map(|a| {
            a.commodity_guid
                .as_deref()
                .map(|commodity| (a.guid.as_str(), commodity))
        })
        .collect();

    let mut max_denominator: HashMap<&str, i64> = HashMap::new();
    for split in splits {
        let Some(commodity) = account_commodity.get(split.account_guid.as_str()) else {
            continue;
        };
        let entry = max_denominator.entry(*commodity).or_insert(1);
        *entry = (*entry).max(split.quantity_denom.abs().max(1));
    }

    commodities
        .iter()
        .map(|commodity| {
            let fraction = if commodity.namespace == TEMPLATE_COMMODITY.0 {
                // GnuCash's plumbing commodity, fraction 1 like its own books.
                1
            } else if commodity.namespace == "CURRENCY" {
                DEFAULT_CURRENCY_FRACTION
            } else {
                match max_denominator
                    .get(commodity.guid.as_str())
                    .copied()
                    .unwrap_or(0)
                {
                    0 => DEFAULT_OTHER_FRACTION,
                    denominator => 10i64.saturating_pow(fraction_digits(denominator)),
                }
            };
            (commodity.guid.clone(), fraction)
        })
        .collect()
}

/// Smallest `k <= 9` with `10^k % denominator == 0` (denominator 3 has no
/// finite decimal form; 9 digits is the display limit GnuCash accepts).
fn fraction_digits(denominator: i64) -> u32 {
    let denominator = denominator.abs();
    if denominator <= 1 {
        return 0;
    }
    for k in 0..=MAX_FRACTION_DIGITS {
        if 10i64.saturating_pow(k) % denominator == 0 {
            return k;
        }
    }
    MAX_FRACTION_DIGITS
}

/// Whether `num/denom` is exactly representable with `fraction` as the smallest
/// unit. GnuCash converts (rounds) split values to the transaction currency's
/// fraction and amounts to the account's SCU when it loads them.
fn fits_precision(num: i64, denom: i64, fraction: i64) -> bool {
    if denom == 0 {
        return false;
    }
    (num as i128 * fraction.max(1) as i128) % denom as i128 == 0
}

/// GnuCash stores a currency per transaction; Spendo dropped the column on
/// import. Prefer the book's base currency when one of the transaction's
/// accounts uses it, else the currency most represented among its accounts,
/// else the base currency (pure security trades).
fn resolve_transaction_currencies(
    mut transactions: Vec<TransactionRow>,
    splits: &[SplitRow],
    account_by_guid: &HashMap<&str, &AccountRow>,
    commodity_by_guid: &HashMap<&str, &CommodityRow>,
    base_currency_guid: &str,
) -> Vec<TransactionRow> {
    let mut per_tx: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
    for split in splits {
        let Some(account) = account_by_guid.get(split.account_guid.as_str()) else {
            continue;
        };
        let Some(commodity_guid) = account.commodity_guid.as_deref() else {
            continue;
        };
        if !commodity_by_guid
            .get(commodity_guid)
            .is_some_and(|c| c.namespace == "CURRENCY")
        {
            continue;
        }
        *per_tx
            .entry(split.tx_guid.as_str())
            .or_default()
            .entry(commodity_guid)
            .or_default() += 1;
    }

    for transaction in transactions.iter_mut() {
        transaction.currency_guid = match per_tx.get(transaction.guid.as_str()) {
            Some(counts) if counts.contains_key(base_currency_guid) => {
                base_currency_guid.to_string()
            }
            Some(counts) => counts
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.cmp(b.0)))
                .map(|(guid, _)| (*guid).to_string())
                .unwrap_or_else(|| base_currency_guid.to_string()),
            None => base_currency_guid.to_string(),
        };
    }
    transactions
}

// ── Small converters ─────────────────────────────────────────────────────────

/// GnuCash's SQL backend stores GUIDs as bare 32-char hex and looks several
/// objects up by exact string match (`slots.obj_guid`, subquery selectors for
/// splits/transactions, budget periods). Spendo-created rows carry hyphenated
/// UUIDs, so every GUID we write is normalized to the canonical form first —
/// otherwise those rows would exist in SQLite but be invisible to GnuCash.
fn gnu_guid(raw: &str) -> String {
    raw.chars()
        .filter(|c| *c != '-')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Fresh identifier already in GnuCash's canonical form.
fn new_guid() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Spendo stores `YYYY-MM-DD` (manual entry) or `YYYY-MM-DD HH:MM:SS`
/// (imported); GnuCash's timestamp columns are `text(19)`. Anything shorter
/// than a date is passed through unchanged rather than dropped.
fn to_gnucash_timestamp(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 19 {
        return trimmed.get(..19).unwrap_or(trimmed).to_string();
    }
    if trimmed.len() == 10 {
        return format!("{trimmed} 00:00:00");
    }
    trimmed.to_string()
}

/// GnuCash's `gdate` columns are compact `YYYYMMDD`; invalid input passes
/// through so a malformed date surfaces in GnuCash instead of becoming NULL.
fn to_gdate(raw: &str) -> String {
    let digits: String = raw.trim().chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 8 {
        digits[..8].to_string()
    } else {
        raw.trim().to_string()
    }
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

/// `REAL` amount → rational, reduced and, when the amount is exactly
/// representable in the target commodity's smallest unit, expressed in that
/// unit (GnuCash's own 99/100 style rather than 99000000/100000000).
fn amount_rational(amount: f64, target_fraction: i64) -> (i64, i64) {
    if !amount.is_finite() {
        return (0, target_fraction.max(1));
    }
    let scaled = (amount * AMOUNT_SCALE as f64).round();
    let clamped = scaled.clamp(i64::MIN as f64, i64::MAX as f64) as i64;
    let divisor = gcd(clamped, AMOUNT_SCALE);
    let (num, denom) = (clamped / divisor, AMOUNT_SCALE / divisor);
    let target_fraction = target_fraction.max(1);
    let scaled_num = num as i128 * target_fraction as i128;
    if denom != 0 && scaled_num % denom as i128 == 0 {
        let num = scaled_num / denom as i128;
        if (i64::MIN as i128..=i64::MAX as i128).contains(&num) {
            return (num as i64, target_fraction);
        }
    }
    (num, denom)
}

fn reduce_rational(num: i64, denom: i64) -> (i64, i64) {
    if denom == 0 {
        return (0, 1);
    }
    let divisor = gcd(num, denom);
    (num / divisor, denom / divisor)
}

/// Decimal string for a rational, for GnuCash's `*-formula` slots. GnuCash
/// writes both the formula and the evaluated numeric; we mirror it, and the
/// formula must be a plain decimal (`gnc_numeric_from_string` has no exponents).
fn rational_to_decimal(num: i64, denom: i64) -> String {
    if denom == 0 {
        return "0".to_string();
    }
    let sign = if (num < 0) != (denom < 0) { "-" } else { "" };
    let num = (num as i128).abs();
    let denom = (denom as i128).abs();
    let integer = num / denom;
    let remainder = num % denom;
    if remainder == 0 {
        return format!("{sign}{integer}");
    }

    let mut fraction = String::new();
    let mut value = remainder;
    for _ in 0..MAX_FRACTION_DIGITS {
        if value == 0 {
            break;
        }
        value *= 10;
        fraction.push(char::from(b'0' + (value / denom) as u8));
        value %= denom;
    }
    // Round instead of truncating when the expansion was cut off.
    if value * 10 / denom >= 5 {
        let mut digits: Vec<u8> = fraction.bytes().collect();
        let mut index = digits.len();
        let mut carry = true;
        while index > 0 && carry {
            index -= 1;
            if digits[index] < b'9' {
                digits[index] += 1;
                carry = false;
            } else {
                digits[index] = b'0';
            }
        }
        if carry {
            return format!("{sign}{}{}", integer + 1, "0".repeat(fraction.len()));
        }
        fraction = String::from_utf8(digits).unwrap_or(fraction);
    }
    let fraction = fraction.trim_end_matches('0');
    if fraction.is_empty() {
        format!("{sign}{integer}")
    } else {
        format!("{sign}{integer}.{fraction}")
    }
}

/// Last date the schedule actually posted an occurrence, for GnuCash's
/// `last_occur` marker. Spendo keeps a count, not dates; occurrences run
/// consecutively from the start date, so walking the same recurrence rule
/// reproduces it.
fn last_occurrence(
    start_date: &str,
    frequency: &str,
    interval_mult: i64,
    posted: i64,
) -> Option<String> {
    if posted <= 0 {
        return None;
    }
    let mut date = chrono::NaiveDate::parse_from_str(start_date, "%Y-%m-%d").ok()?;
    for _ in 1..posted {
        date = schedules::next_occurrence(date, frequency, interval_mult).ok()?;
    }
    Some(date.format("%Y-%m-%d").to_string())
}

/// Spendo frequency → GnuCash `(period_type, mult)`. `biweekly`/`quarterly`
/// exist as their own Spendo names but are week/month multiples to GnuCash.
fn to_recurrence(frequency: &str, interval_mult: i64) -> Option<(&'static str, i64)> {
    let mult = interval_mult.max(1);
    match frequency {
        "daily" => Some(("day", mult)),
        "weekly" => Some(("week", mult)),
        "biweekly" => Some(("week", 2 * mult)),
        "monthly" => Some(("month", mult)),
        "quarterly" => Some(("month", 3 * mult)),
        "yearly" => Some(("year", mult)),
        _ => None,
    }
}

// ── Write ────────────────────────────────────────────────────────────────────

/// Target file lives next to the export so the final rename is atomic and
/// stays on one filesystem; a failed write leaves no partial target behind.
fn temp_path_for(path: &Path) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(".spendo-export");
    PathBuf::from(raw)
}

fn write_gnucash_file(path: &Path, data: &ExportData) -> Result<(), String> {
    let tmp = temp_path_for(path);
    let _ = std::fs::remove_file(&tmp);
    if let Err(e) = write_gnucash_db(&tmp, data) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if std::fs::rename(&tmp, path).is_ok() {
        return Ok(());
    }
    // Windows refuses to replace an existing file with `rename`.
    std::fs::remove_file(path).map_err(|e| format!("Failed to replace export file: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Failed to write export file: {e}"))
}

fn write_gnucash_db(path: &Path, data: &ExportData) -> Result<(), String> {
    let conn = Connection::open(path).map_err(|e| format!("Failed to create export file: {e}"))?;
    conn.execute_batch(GNUCASH_SCHEMA)
        .map_err(|e| format!("Failed to write GnuCash schema: {e}"))?;

    with_tx_err(&conn, || -> Result<(), AppError> {
        conn.execute(
            "INSERT INTO books (guid, root_account_guid, root_template_guid) VALUES (?1, ?2, ?3)",
            params![data.book_guid, data.root_guid, data.template_root_guid],
        )?;
        for (table, version) in GNUCASH_TABLE_VERSIONS {
            conn.execute(
                "INSERT INTO versions (table_name, table_version) VALUES (?1, ?2)",
                params![table, version],
            )?;
        }
        conn.execute(
            "INSERT INTO versions (table_name, table_version) VALUES ('Gnucash', ?1)",
            params![GNUCASH_VERSION],
        )?;
        conn.execute(
            "INSERT INTO versions (table_name, table_version) VALUES ('Gnucash-Resave', ?1)",
            params![GNUCASH_RESAVE_VERSION],
        )?;

        write_commodities(&conn, data)?;
        write_accounts(&conn, data)?;
        write_transactions(&conn, data)?;
        write_prices(&conn, data)?;
        write_budgets(&conn, data)?;
        if !data.budgets.is_empty() {
            write_budget_feature(&conn, data)?;
        }
        write_schedules(&conn, data)?;
        Ok(())
    })
    .map_err(String::from)?;

    Ok(())
}

fn write_commodities(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
    let mut stmt = conn.prepare(
        "INSERT INTO commodities (guid, namespace, mnemonic, fullname, cusip, fraction,
                                  quote_flag, quote_source, quote_tz)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, NULL)",
    )?;
    for commodity in &data.commodities {
        let is_currency = commodity.namespace == "CURRENCY";
        let fraction = data
            .fractions
            .get(&commodity.guid)
            .copied()
            .unwrap_or(DEFAULT_OTHER_FRACTION);
        stmt.execute(params![
            commodity.guid,
            commodity.namespace,
            commodity.mnemonic,
            commodity.fullname,
            fraction,
            is_currency as i64,
            is_currency.then_some("currency")
        ])?;
    }
    Ok(())
}

fn write_accounts(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
    let mut stmt = conn.prepare(
        "INSERT INTO accounts (guid, name, account_type, commodity_guid, commodity_scu, non_std_scu,
                               parent_guid, code, description, hidden, placeholder)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, 0, ?9)",
    )?;

    // GnuCash's template root: parentless, hidden from reports, owner of the
    // scheduled-transaction template accounts.
    stmt.execute(params![
        data.template_root_guid,
        "Template Root",
        "ROOT",
        data.template_commodity_guid,
        1i64,
        None::<String>,
        None::<String>,
        None::<String>,
        0i64
    ])?;

    for account in &data.accounts {
        let commodity = account
            .commodity_guid
            .clone()
            .unwrap_or_else(|| data.base_currency_guid.clone());
        let scu = data
            .account_fractions
            .get(&account.guid)
            .copied()
            .unwrap_or(DEFAULT_CURRENCY_FRACTION);
        stmt.execute(params![
            account.guid,
            account.name,
            account.account_type,
            commodity,
            scu,
            account.parent_guid,
            account.code,
            account.description,
            account.placeholder as i64
        ])?;
    }
    Ok(())
}

fn write_transactions(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
    let mut tx_stmt = conn.prepare(
        "INSERT INTO transactions (guid, currency_guid, num, post_date, enter_date, description)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    let mut split_stmt = conn.prepare(
        "INSERT INTO splits (guid, tx_guid, account_guid, memo, action, reconcile_state,
                             reconcile_date, value_num, value_denom, quantity_num, quantity_denom,
                             lot_guid)
         VALUES (?1, ?2, ?3, ?4, '', ?5, NULL, ?6, ?7, ?8, ?9, NULL)",
    )?;
    let mut slot_stmt = conn.prepare(
        "INSERT INTO slots (obj_guid, name, slot_type, string_val) VALUES (?1, 'notes', ?2, ?3)",
    )?;

    for transaction in &data.transactions {
        let post_date = to_gnucash_timestamp(&transaction.post_date);
        tx_stmt.execute(params![
            transaction.guid,
            transaction.currency_guid,
            transaction.num.clone().unwrap_or_default(),
            post_date,
            post_date,
            transaction.description.clone().unwrap_or_default(),
        ])?;
        if let Some(notes) = transaction
            .notes
            .as_deref()
            .filter(|n| !n.trim().is_empty())
        {
            slot_stmt.execute(params![transaction.guid, SLOT_STRING, notes])?;
        }
    }

    for split in &data.splits {
        split_stmt.execute(params![
            split.guid,
            split.tx_guid,
            split.account_guid,
            split.memo.clone().unwrap_or_default(),
            split
                .reconcile_state
                .clone()
                .unwrap_or_else(|| "n".to_string()),
            split.value_num,
            split.value_denom,
            split.quantity_num,
            split.quantity_denom,
        ])?;
    }
    Ok(())
}

fn write_prices(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
    let mut stmt = conn.prepare(
        "INSERT INTO prices (guid, commodity_guid, currency_guid, date, source, type,
                             value_num, value_denom)
         VALUES (?1, ?2, ?3, ?4, 'Finance::Quote', 'last', ?5, ?6)",
    )?;
    for price in &data.prices {
        stmt.execute(params![
            price.guid,
            price.commodity_guid,
            price.currency_guid,
            to_gnucash_timestamp(&price.date),
            price.value_num,
            price.value_denom,
        ])?;
    }
    Ok(())
}

/// One GnuCash budget per Spendo budget year, with the yearly target spread
/// evenly over 12 monthly periods. Spendo's implicit carryover has no GnuCash
/// equivalent and is not exported.
fn write_budgets(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
    let mut budget_stmt = conn.prepare(
        "INSERT INTO budgets (guid, name, description, num_periods) VALUES (?1, ?2, '', 12)",
    )?;
    let mut amount_stmt = conn.prepare(
        "INSERT INTO budget_amounts (budget_guid, account_guid, period_num, amount_num, amount_denom)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;

    let mut years: Vec<i64> = data.budgets.iter().map(|b| b.year).collect();
    years.sort_unstable();
    years.dedup();
    for year in years {
        let guid = new_guid();
        budget_stmt.execute(params![guid, year.to_string()])?;
        for row in data.budgets.iter().filter(|b| b.year == year) {
            let fraction = data
                .account_fractions
                .get(&row.account_guid)
                .copied()
                .unwrap_or(DEFAULT_CURRENCY_FRACTION);
            let (num, denom) = amount_rational(row.amount / 12.0, fraction);
            for period in 0..12 {
                amount_stmt.execute(params![guid, row.account_guid, period, num, denom])?;
            }
        }
    }
    Ok(())
}

/// GnuCash's one-time budget-sign migration skips any book that declares the
/// natural-sign feature. Writing it also protects the export from the
/// heuristic scrub (which otherwise negates income/liability/equity budgets
/// based on a per-type tally of the amounts it finds).
fn write_budget_feature(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
    let mut stmt = conn.prepare(
        "INSERT INTO slots (obj_guid, name, slot_type, guid_val, string_val, int64_val)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    let frame_guid = new_guid();
    stmt.execute(params![
        data.book_guid,
        "features",
        SLOT_FRAME,
        frame_guid,
        None::<String>,
        0i64
    ])?;
    stmt.execute(params![
        frame_guid,
        format!("features/{BUDGET_FEATURE_NAME}"),
        SLOT_STRING,
        None::<String>,
        BUDGET_FEATURE_DESCRIPTION,
        None::<i64>
    ])?;
    Ok(())
}

fn write_schedules(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
    let mut sx_stmt = conn.prepare(
        "INSERT INTO schedxactions (guid, name, enabled, start_date, end_date, last_occur,
                                    num_occur, rem_occur, auto_create, auto_notify, adv_creation,
                                    adv_notify, instance_count, template_act_guid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 0, 0, 0, ?9, ?10)",
    )?;
    let mut rec_stmt = conn.prepare(
        "INSERT INTO recurrences (obj_guid, recurrence_mult, recurrence_period_type,
                                  recurrence_period_start, recurrence_weekend_adjust)
         VALUES (?1, ?2, ?3, ?4, 'none')",
    )?;
    let mut tx_stmt = conn.prepare(
        "INSERT INTO transactions (guid, currency_guid, num, post_date, enter_date, description)
         VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
    )?;
    let mut split_stmt = conn.prepare(
        "INSERT INTO splits (guid, tx_guid, account_guid, memo, action, reconcile_state,
                             reconcile_date, value_num, value_denom, quantity_num, quantity_denom,
                             lot_guid)
         VALUES (?1, ?2, ?3, ?4, '', 'n', NULL, 0, ?5, ?6, ?7, NULL)",
    )?;
    let mut slot_stmt = conn.prepare(
        "INSERT INTO slots (obj_guid, name, slot_type, guid_val, string_val,
                            numeric_val_num, numeric_val_denom)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;

    for schedule in &data.schedules {
        let Some((period_type, mult)) = to_recurrence(&schedule.frequency, schedule.interval_mult)
        else {
            // A frequency Spendo itself validates can't land here; skipping is
            // safer than writing an SX GnuCash would refuse to load.
            continue;
        };
        let template_act_guid = new_guid();

        // Template account: named after the SX GUID so the importers treat it
        // as plumbing rather than user data.
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, commodity_guid, commodity_scu,
                                   non_std_scu, parent_guid, code, description, hidden, placeholder)
             VALUES (?1, ?2, 'BANK', ?3, 1, 0, ?4, NULL, NULL, 0, 0)",
            params![
                template_act_guid,
                schedule.guid,
                data.template_commodity_guid,
                data.template_root_guid
            ],
        )?;

        let template_tx_guid = new_guid();
        tx_stmt.execute(params![
            template_tx_guid,
            data.base_currency_guid,
            schedule.num.clone().unwrap_or_default(),
            format!("{} 00:00:00", schedule.start_date),
            schedule.description.clone().unwrap_or_default(),
        ])?;
        if let Some(notes) = schedule.notes.as_deref().filter(|n| !n.trim().is_empty()) {
            slot_stmt.execute(params![
                template_tx_guid,
                "notes",
                SLOT_STRING,
                None::<String>,
                notes,
                None::<i64>,
                None::<i64>
            ])?;
        }

        for split in &schedule.splits {
            let target_fraction = data
                .account_fractions
                .get(&split.account_guid)
                .copied()
                .unwrap_or(DEFAULT_CURRENCY_FRACTION);
            let (amount_num, amount_denom) = amount_rational(split.amount, target_fraction);
            let (quantity_num, quantity_denom) = match split.quantity {
                Some(quantity) if quantity.is_finite() => {
                    let (num, denom) = amount_rational(quantity, AMOUNT_SCALE);
                    reduce_rational(num, denom)
                }
                _ => (0, 1),
            };
            let split_guid = new_guid();
            split_stmt.execute(params![
                split_guid,
                template_tx_guid,
                template_act_guid,
                split.memo.clone().unwrap_or_default(),
                amount_denom,
                quantity_num,
                quantity_denom
            ])?;

            // `sched-xaction` KVP frame: the split's target account and amount
            // live here, not in the split row (GnuCash's SX plumbing).
            let frame_guid = new_guid();
            slot_stmt.execute(params![
                split_guid,
                "sched-xaction",
                SLOT_FRAME,
                frame_guid,
                None::<String>,
                None::<i64>,
                None::<i64>
            ])?;
            slot_stmt.execute(params![
                frame_guid,
                "sched-xaction/account",
                SLOT_GUID,
                split.account_guid,
                None::<String>,
                None::<i64>,
                None::<i64>
            ])?;

            // GnuCash stores both sides on every template split: the used side
            // carries the signed numeric plus its decimal formula, the unused
            // side is zero with an empty formula string.
            let (debit_num, debit_denom, credit_num, credit_denom, debit_formula, credit_formula) =
                if amount_num >= 0 {
                    (
                        amount_num,
                        amount_denom,
                        0,
                        amount_denom,
                        rational_to_decimal(amount_num, amount_denom),
                        String::new(),
                    )
                } else {
                    (
                        0,
                        amount_denom,
                        -amount_num,
                        amount_denom,
                        String::new(),
                        rational_to_decimal(-amount_num, amount_denom),
                    )
                };
            for (name, formula) in [
                ("sched-xaction/debit-formula", debit_formula),
                ("sched-xaction/credit-formula", credit_formula),
            ] {
                slot_stmt.execute(params![
                    frame_guid,
                    name,
                    SLOT_STRING,
                    None::<String>,
                    formula,
                    None::<i64>,
                    None::<i64>
                ])?;
            }
            for (name, num, denom) in [
                ("sched-xaction/debit-numeric", debit_num, debit_denom),
                ("sched-xaction/credit-numeric", credit_num, credit_denom),
            ] {
                slot_stmt.execute(params![
                    frame_guid,
                    name,
                    SLOT_NUMERIC,
                    None::<String>,
                    None::<String>,
                    num,
                    denom
                ])?;
            }
            if split.quantity.is_some_and(|q| q.is_finite()) {
                slot_stmt.execute(params![
                    frame_guid,
                    "sched-xaction/shares",
                    SLOT_STRING,
                    None::<String>,
                    rational_to_decimal(quantity_num, quantity_denom),
                    None::<i64>,
                    None::<i64>
                ])?;
            }
        }

        sx_stmt.execute(params![
            schedule.guid,
            schedule.name,
            schedule.active as i64,
            to_gdate(&schedule.start_date),
            schedule.end_date.as_deref().map(to_gdate),
            last_occurrence(
                &schedule.start_date,
                &schedule.frequency,
                schedule.interval_mult,
                schedule.occurrences_posted
            )
            .map(|date| to_gdate(&date)),
            schedule.max_occurrences.unwrap_or(0),
            schedule
                .max_occurrences
                .map(|max| (max - schedule.occurrences_posted).max(0))
                .unwrap_or(0),
            schedule.occurrences_posted,
            template_act_guid
        ])?;
        rec_stmt.execute(params![
            schedule.guid,
            mult,
            period_type,
            to_gdate(&schedule.start_date)
        ])?;
    }
    Ok(())
}

// ── Command ──────────────────────────────────────────────────────────────────

/// Whether two paths resolve to the same file. Falls back to a literal
/// comparison for paths that don't exist yet (the common case for the target).
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Full-book GnuCash SQLite export written straight to the chosen path: the
/// read happens under the DB lock, the file write on `run_io` without it.
#[tauri::command]
pub async fn export_gnucash(
    state: State<'_, AppState>,
    input: GnuCashExportInput,
) -> Result<GnuCashExportResult, String> {
    let path = PathBuf::from(input.path.trim());
    if path.as_os_str().is_empty() {
        return Err("Export path is required".to_string());
    }

    let active = state.db_path.lock().map_err(|e| e.to_string())?.clone();
    let Some(active) = active else {
        return Err("No book is open".to_string());
    };
    if same_path(&active, &path) {
        return Err("Choose a file other than the open book".to_string());
    }

    let data = run_blocking(state.db.clone(), |conn| {
        fetch_export_data_inner(conn).map_err(String::from)
    })
    .await?;
    let result = GnuCashExportResult {
        accounts: data.accounts.len() as i64,
        transactions: data.transactions.len() as i64,
        splits: data.splits.len() as i64,
        prices: data.prices.len() as i64,
        budgets: data.budgets.len() as i64,
        schedules: data.schedules.len() as i64,
        future_dated: data.future_dated,
        attachments_skipped: data.attachments_skipped,
        rounded_amounts: data.rounded_amounts,
    };
    let target = path;
    run_io(move || write_gnucash_file(&target, &data)).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;
    use crate::import::import_gnucash_inner;

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("spendo-gnucash-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES
             ('ceur', 'EUR', 'CURRENCY', 'Euro'),
             ('cusd', 'USD', 'CURRENCY', 'US Dollar'),
             ('caapl', 'AAPL', 'NASDAQ', 'Apple Inc')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid, code, description) VALUES
             ('root', 'Root Account', 'ROOT', NULL, 0, 'ceur', NULL, NULL),
             ('assets', 'Assets', 'ASSET', 'root', 1, 'ceur', NULL, NULL),
             ('liabilities', 'Liabilities', 'LIABILITY', 'root', 1, 'ceur', NULL, NULL),
             ('equity', 'Equity', 'EQUITY', 'root', 1, 'ceur', NULL, NULL),
             ('income', 'Income', 'INCOME', 'root', 1, 'ceur', NULL, NULL),
             ('expenses', 'Expenses', 'EXPENSE', 'root', 1, 'ceur', NULL, NULL),
             ('checking', 'Checking', 'BANK', 'assets', 0, 'ceur', '1001', 'Everyday'),
             ('broker', 'Broker', 'STOCK', 'assets', 0, 'caapl', NULL, NULL),
             ('groceries', 'Groceries', 'EXPENSE', 'expenses', 0, 'ceur', NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes) VALUES
             ('t1', '2026-03-01', 'Groceries', '12', 'weekly shop'),
             ('t2', '2026-03-02 08:30:00', 'Buy shares', NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state, memo) VALUES
             ('s1', 't1', 'checking', -1234, 100, -1234, 100, 'c', 'card'),
             ('s2', 't1', 'groceries', 1234, 100, 1234, 100, 'n', NULL),
             ('s3', 't2', 'broker', 100000, 100, 100, 1000000, 'n', NULL),
             ('s4', 't2', 'checking', -100000, 100, -100000, 100, 'n', NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prices (guid, commodity_guid, currency_guid, value_num, value_denom, date) VALUES
             ('p1', 'caapl', 'ceur', 1053, 10, '2026-03-02')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO budgets (guid, account_guid, year, amount, created_at) VALUES
             ('b1', 'groceries', 2026, 1200.0, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO recurring_schedules (guid, name, frequency, interval_mult, start_date, end_date,
                                              max_occurrences, occurrences_posted, next_due_date, active,
                                              template_json, created_at)
             VALUES ('sched1', 'Rent', 'monthly', 1, '2026-01-15', NULL, NULL, 2, '2026-03-15', 1,
                     '{\"description\":\"Rent\",\"num\":\"7\",\"notes\":\"due on the 15th\",\"splits\":[{\"accountGuid\":\"groceries\",\"amount\":-900.0,\"quantity\":null,\"memo\":\"flat\"},{\"accountGuid\":\"checking\",\"amount\":900.0,\"quantity\":null,\"memo\":null}]}',
                     '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    fn export_to(conn: &Connection, dir: &Path) -> PathBuf {
        let data = fetch_export_data_inner(conn).unwrap();
        let path = dir.join("book.gnucash");
        write_gnucash_file(&path, &data).unwrap();
        path
    }

    #[test]
    fn export_writes_schema_metadata_and_counts() {
        let conn = setup();
        let dir = tmp_dir("schema");
        let path = export_to(&conn, &dir);

        let out = Connection::open(&path).unwrap();
        let tables: i64 = out
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name <> 'sqlite_sequence'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 24, "all GnuCash tables must exist");

        let version: i64 = out
            .query_row(
                "SELECT table_version FROM versions WHERE table_name = 'Gnucash'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, GNUCASH_VERSION);
        let splits_version: i64 = out
            .query_row(
                "SELECT table_version FROM versions WHERE table_name = 'splits'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(splits_version, 5);

        let books: (String, String) = out
            .query_row(
                "SELECT root_account_guid, root_template_guid FROM books",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(books.0, "root");
        let template_type: String = out
            .query_row(
                "SELECT account_type FROM accounts WHERE guid = ?1",
                params![books.1],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(template_type, "ROOT");

        // No sidecars and no leftover temp file: the export hands over one file.
        assert!(!temp_path_for(&path).exists());
        assert!(!dir.join("book.gnucash-journal").exists());

        // Every Spendo account survives, plus the template plumbing.
        let accounts: i64 = out
            .query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            accounts, 11,
            "9 book accounts + template root + 1 SX template"
        );
    }

    #[test]
    fn export_round_trips_through_the_gnucash_importer() {
        let conn = setup();
        let dir = tmp_dir("roundtrip");
        let path = export_to(&conn, &dir);

        let dest = Connection::open_in_memory().unwrap();
        init_db(&dest).unwrap();
        let path_str = path.to_string_lossy().to_string();
        let result = import_gnucash_inner(&dest, &path_str, "book.gnucash", "", true).unwrap();

        assert_eq!(result.accounts_added, 9);
        assert_eq!(result.transactions_added, 2);
        assert_eq!(result.splits_added, 4);
        assert_eq!(result.schedules_added, 1);

        let notes: String = dest
            .query_row(
                "SELECT notes FROM transactions WHERE guid = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(notes, "weekly shop");

        let (name, frequency, start, posted, next_due): (String, String, String, i64, String) =
            dest.query_row(
                "SELECT name, frequency, start_date, occurrences_posted, next_due_date
                 FROM recurring_schedules WHERE guid = 'sched1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            (
                name.as_str(),
                frequency.as_str(),
                start.as_str(),
                posted,
                next_due.as_str()
            ),
            ("Rent", "monthly", "2026-01-15", 2, "2026-03-15")
        );

        // Signature, description, notes and memo survive on the template.
        let template_json: String = dest
            .query_row(
                "SELECT template_json FROM recurring_schedules WHERE guid = 'sched1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let template: serde_json::Value = serde_json::from_str(&template_json).unwrap();
        assert_eq!(template["num"], "7");
        assert_eq!(template["description"], "Rent");
        assert_eq!(template["notes"], "due on the 15th");
        assert_eq!(template["splits"][0]["memo"], "flat");

        // Template accounts are plumbing: they must not come back as accounts.
        let template_accounts: i64 = dest
            .query_row(
                "SELECT COUNT(*) FROM accounts WHERE name = 'sched1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(template_accounts, 0);

        // Amounts round-trip with their sign (debit − credit in GnuCash).
        let amounts: Vec<f64> = dest
            .prepare("SELECT CAST(value_num AS REAL) / value_denom FROM splits ORDER BY value_num")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(amounts, vec![-1000.0, -12.34, 12.34, 1000.0]);
    }

    #[test]
    fn export_derives_currency_and_fractions() {
        let conn = setup();
        let dir = tmp_dir("derive");
        let path = export_to(&conn, &dir);
        let out = Connection::open(&path).unwrap();

        // Base currency is the transaction currency for the cash transaction.
        let currency: String = out
            .query_row(
                "SELECT currency_guid FROM transactions WHERE guid = 't1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(currency, "ceur");

        // 1/1000000 share denominator → fraction 1e6; EUR stays at 100.
        let fraction = |guid: &str| -> i64 {
            out.query_row(
                "SELECT fraction FROM commodities WHERE guid = ?1",
                params![guid],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_eq!(fraction("caapl"), 1_000_000);
        assert_eq!(fraction("ceur"), 100);

        let scu: i64 = out
            .query_row(
                "SELECT commodity_scu FROM accounts WHERE guid = 'broker'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(scu, 1_000_000);
    }

    #[test]
    fn export_keeps_currency_display_at_two_decimals() {
        let conn = setup();
        // Spendo stores every split's quantity over 1e8 (see
        // `transactions::QUANTITY_DENOM`), even for plain currency accounts.
        // That denominator must not become the currency's display precision,
        // or GnuCash renders eight decimals on every balance.
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes) VALUES
             ('tfrac', '2026-03-04', 'CSV import', NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state, memo) VALUES
             ('sfrac1', 'tfrac', 'checking', -1234, 10000, -1234, 100000000, 'n', NULL),
             ('sfrac2', 'tfrac', 'groceries', 1234, 10000, 1234, 100000000, 'n', NULL)",
            [],
        )
        .unwrap();

        let dir = tmp_dir("currency-precision");
        let path = export_to(&conn, &dir);
        let out = Connection::open(&path).unwrap();
        let fraction = |guid: &str| -> i64 {
            out.query_row(
                "SELECT fraction FROM commodities WHERE guid = ?1",
                params![guid],
                |row| row.get(0),
            )
            .unwrap()
        };
        let scu = |account: &str| -> i64 {
            out.query_row(
                "SELECT commodity_scu FROM accounts WHERE guid = ?1",
                params![account],
                |row| row.get(0),
            )
            .unwrap()
        };

        assert_eq!(fraction("ceur"), 100, "currencies display two decimals");
        assert_eq!(scu("checking"), 100);
        // Securities keep their share precision (only the quantity column
        // counts; the transaction-currency value column must not inflate it).
        assert_eq!(fraction("caapl"), 1_000_000);
        assert_eq!(scu("broker"), 1_000_000);

        // The 4-decimal amounts in this fixture don't fit a 2-decimal currency,
        // so the export reports them as rounding losses for the UI to surface.
        let data = fetch_export_data_inner(&conn).unwrap();
        assert_eq!(data.rounded_amounts, 2);
    }

    #[test]
    fn export_reparents_a_second_root_and_keeps_future_dates() {
        let conn = setup();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
             ('root2', 'Imported Root', 'ROOT', NULL, 0, 'cusd'),
             ('usdbank', 'USD Bank', 'BANK', 'root2', 0, 'cusd'),
             ('usdsavings', 'USD Savings', 'BANK', 'root2', 0, 'cusd')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes) VALUES
             ('tfuture', '2099-01-01', 'Future transfer', NULL, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state, memo) VALUES
             ('sfuture1', 'tfuture', 'usdbank', -100, 100, -100, 100, 'n', NULL),
             ('sfuture2', 'tfuture', 'usdsavings', 100, 100, 100, 100, 'n', NULL)",
            [],
        )
        .unwrap();
        let dir = tmp_dir("root");
        let path = export_to(&conn, &dir);
        let out = Connection::open(&path).unwrap();

        // The seeded root owns the canonical tree and stays the book root.
        let root: String = out
            .query_row("SELECT root_account_guid FROM books", [], |row| row.get(0))
            .unwrap();
        assert_eq!(root, "root");
        let root2_parent: Option<String> = out
            .query_row(
                "SELECT parent_guid FROM accounts WHERE guid = 'root2'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(root2_parent.as_deref(), Some("root"));

        // The transaction currency follows its accounts.
        let currency: String = out
            .query_row(
                "SELECT currency_guid FROM transactions WHERE guid = 'tfuture'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(currency, "cusd");

        let data = fetch_export_data_inner(&conn).unwrap();
        assert_eq!(data.future_dated, 1);
    }

    #[test]
    fn export_writes_notes_budget_and_price_rows() {
        let conn = setup();
        let dir = tmp_dir("rows");
        let path = export_to(&conn, &dir);
        let out = Connection::open(&path).unwrap();

        let note: (i64, String) = out
            .query_row(
                "SELECT slot_type, string_val FROM slots WHERE obj_guid = 't1' AND name = 'notes'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(note, (SLOT_STRING, "weekly shop".to_string()));

        let (name, periods, rows, total): (String, i64, i64, f64) = out
            .query_row(
                "SELECT b.name, b.num_periods, COUNT(a.id),
                        SUM(CAST(a.amount_num AS REAL) / a.amount_denom)
                 FROM budgets b JOIN budget_amounts a ON a.budget_guid = b.guid
                 GROUP BY b.guid",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!((name.as_str(), periods, rows), ("2026", 12, 12));
        assert!((total - 1200.0).abs() < 0.005, "budget total: {total}");
        let period_amount: f64 = out
            .query_row(
                "SELECT CAST(amount_num AS REAL) / amount_denom FROM budget_amounts LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!((period_amount - 100.0).abs() < 1e-9);

        let (source, kind, value): (String, String, f64) = out
            .query_row(
                "SELECT source, type, CAST(value_num AS REAL) / value_denom FROM prices",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((source.as_str(), kind.as_str()), ("Finance::Quote", "last"));
        assert!((value - 105.3).abs() < 1e-9);

        // GnuCash's natural-sign budget feature is declared, so opening the
        // export doesn't trigger its budget sign migration notice.
        let feature: (i64, String) = out
            .query_row(
                "SELECT f.slot_type, c.string_val
                 FROM slots f JOIN slots c ON c.obj_guid = f.guid_val
                 WHERE f.obj_guid = (SELECT guid FROM books)
                   AND f.name = 'features'
                   AND c.name = 'features/Use natural signs in budget amounts'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(feature.0, SLOT_FRAME);
        assert_eq!(
            feature.1,
            "Store budget amounts unreversed (i.e. natural) signs (requires at least Gnucash 3.8)"
        );
    }

    #[test]
    fn export_normalizes_uuids_to_gnucashs_canonical_form() {
        let conn = setup();
        // GnuCash's SQL backend matches several GUID columns by exact string
        // (`slots.obj_guid`, split subqueries), so hyphenated UUIDs would be
        // invisible to it even though SQLite stores them fine.
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes) VALUES
             ('9f8b1c2d-3e4f-4a5b-8c7d-6e5f4a3b2c1d', '2026-03-03', 'Hand-written', NULL, 'note')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state, memo) VALUES
             ('a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d', '9f8b1c2d-3e4f-4a5b-8c7d-6e5f4a3b2c1d', 'checking', -100, 100, -100, 100, 'n', NULL)",
            [],
        )
        .unwrap();
        let dir = tmp_dir("uuids");
        let path = export_to(&conn, &dir);
        let out = Connection::open(&path).unwrap();

        let tx = "9f8b1c2d3e4f4a5b8c7d6e5f4a3b2c1d";
        let split = "a1b2c3d4e5f64a7b8c9d0e1f2a3b4c5d";
        let transactions: i64 = out
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE guid = ?1",
                params![tx],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(transactions, 1);
        let slots: i64 = out
            .query_row(
                "SELECT COUNT(*) FROM slots WHERE obj_guid = ?1 AND name = 'notes'",
                params![tx],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(slots, 1);
        let splits: i64 = out
            .query_row(
                "SELECT COUNT(*) FROM splits WHERE guid = ?1 AND tx_guid = ?2",
                params![split, tx],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(splits, 1);
        let hyphenated: i64 = out
            .query_row(
                "SELECT COUNT(*) FROM slots WHERE obj_guid LIKE '%-%' OR guid_val LIKE '%-%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            hyphenated, 0,
            "no hyphenated GUID may reach the SQLite file"
        );
    }

    #[test]
    fn export_without_budgets_omits_the_budget_feature() {
        let conn = setup();
        conn.execute("DELETE FROM budgets", []).unwrap();
        let dir = tmp_dir("nobudget");
        let path = export_to(&conn, &dir);
        let out = Connection::open(&path).unwrap();
        let features: i64 = out
            .query_row(
                "SELECT COUNT(*) FROM slots WHERE obj_guid = (SELECT guid FROM books)
                 AND name = 'features'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(features, 0, "no budgets means no natural-sign claim");
    }

    #[test]
    fn export_writes_schedule_kvp_frames() {
        let conn = setup();
        let dir = tmp_dir("sx");
        let path = export_to(&conn, &dir);
        let out = Connection::open(&path).unwrap();

        let (enabled, num_occur, rem_occur, instance_count, last_occur, start): (
            i64,
            i64,
            i64,
            i64,
            String,
            String,
        ) = out
            .query_row(
                "SELECT enabled, num_occur, rem_occur, instance_count, last_occur, start_date
                 FROM schedxactions WHERE guid = 'sched1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(enabled, 1);
        assert_eq!((num_occur, rem_occur), (0, 0));
        assert_eq!(instance_count, 2);
        assert_eq!(start, "20260115");
        assert_eq!(last_occur, "20260215");

        let (mult, period, period_start): (i64, String, String) = out
            .query_row(
                "SELECT recurrence_mult, recurrence_period_type, recurrence_period_start
                 FROM recurrences WHERE obj_guid = 'sched1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (mult, period.as_str(), period_start.as_str()),
            (1, "month", "20260115")
        );

        // Each template split owns a frame whose children carry the target
        // account, the signed amount and the memo.
        let frames: i64 = out
            .query_row(
                "SELECT COUNT(*) FROM slots WHERE name = 'sched-xaction' AND slot_type = 9",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(frames, 2);

        let accounts: Vec<String> = out
            .prepare(
                "SELECT guid_val FROM slots WHERE name = 'sched-xaction/account' ORDER BY guid_val",
            )
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            accounts,
            vec!["checking".to_string(), "groceries".to_string()]
        );

        // The credit-side split (`flat`, −900) keeps its signed amount on the
        // credit side; the formula on that side is the readable decimal and the
        // unused debit side is empty.
        let frame: String = out
            .query_row(
                "SELECT guid_val FROM slots WHERE name = 'sched-xaction'
                 AND obj_guid IN (SELECT guid FROM splits WHERE memo = 'flat')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let (credit_num, credit_denom): (i64, i64) = out
            .query_row(
                "SELECT numeric_val_num, numeric_val_denom FROM slots
                 WHERE obj_guid = ?1 AND name = 'sched-xaction/credit-numeric'",
                params![frame],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((credit_num, credit_denom), (90000, 100));

        let formulas: Vec<(String, String)> = out
            .prepare(
                "SELECT name, string_val FROM slots
                 WHERE obj_guid = ?1
                   AND name IN ('sched-xaction/debit-formula', 'sched-xaction/credit-formula')
                 ORDER BY name",
            )
            .unwrap()
            .query_map(params![frame], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            formulas,
            vec![
                (
                    "sched-xaction/credit-formula".to_string(),
                    "900".to_string()
                ),
                ("sched-xaction/debit-formula".to_string(), String::new()),
            ]
        );
    }

    #[test]
    fn same_path_compares_canonical_forms() {
        let dir = tmp_dir("paths");
        let file = dir.join("a.gnucash");
        std::fs::write(&file, b"x").unwrap();
        let dotted = dir.join(".").join("a.gnucash");
        assert!(same_path(&file, &dotted));
        assert!(!same_path(&file, &dir.join("b.gnucash")));
    }

    #[test]
    fn rational_to_decimal_renders_readable_amounts() {
        assert_eq!(rational_to_decimal(99, 100), "0.99");
        assert_eq!(rational_to_decimal(-1234, 100), "-12.34");
        assert_eq!(rational_to_decimal(5, 1), "5");
        assert_eq!(rational_to_decimal(0, 1), "0");
        assert_eq!(rational_to_decimal(1, 8), "0.125");
        assert_eq!(rational_to_decimal(1, 3), "0.333333333");
        assert_eq!(rational_to_decimal(2, 3), "0.666666667");
    }
}
