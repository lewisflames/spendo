use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::sql;
use crate::transactions::{SplitInput, TxInput};
use crate::utils;
use crate::utils::opt_text;

/// Frequencies supported by recurring schedules (v1: simple presets only —
/// no RRULE/cron, no weekday rules).
pub const FREQUENCIES: [&str; 6] = [
    "daily",
    "weekly",
    "biweekly",
    "monthly",
    "quarterly",
    "yearly",
];

// ── Wire structs ─────────────────────────────────────────────────────────────

/// Create/update payload: recurrence rule + a transaction template. One flat
/// `Input` struct per backend conventions (template fields mirror `TxInput`,
/// minus `postDate` — the due date supplies it at posting time).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInput {
    pub name: String,
    pub frequency: String,
    pub interval_mult: Option<i64>,
    pub start_date: String,
    pub end_date: Option<String>,
    pub max_occurrences: Option<i64>,
    pub description: Option<String>,
    pub num: Option<String>,
    /// `None` keeps an edited schedule's template notes (the form no longer
    /// shows them); a blank string clears them.
    #[serde(default)]
    pub notes: Option<String>,
    /// Payee name for every posted occurrence; created when missing.
    #[serde(default)]
    pub payee: Option<String>,
    pub splits: Vec<SplitInput>,
    /// Post due occurrences automatically when the book opens.
    #[serde(default)]
    pub auto_post: bool,
    /// Treat the split amounts as a total spread across `max_occurrences`.
    #[serde(default)]
    pub spread_total: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSplit {
    pub account_guid: String,
    pub account_name: Option<String>,
    pub amount: f64,
    pub quantity: Option<f64>,
    pub memo: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub guid: String,
    pub name: String,
    pub frequency: String,
    pub interval_mult: i64,
    pub start_date: String,
    pub end_date: Option<String>,
    pub max_occurrences: Option<i64>,
    pub occurrences_posted: i64,
    /// Occurrences skipped (kept in a side table; part of the cap count).
    pub skipped_count: i64,
    pub next_due_date: Option<String>,
    pub active: bool,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    pub payee: Option<String>,
    /// Template splits as entered (the totals, for a spread schedule).
    pub splits: Vec<ScheduleSplit>,
    /// Sum of the positive split amounts the next occurrence posts: the
    /// transfer size behind a balanced template, or one slice of a spread
    /// total. Rendered in list rows (a raw sum is always 0).
    pub preview_amount: f64,
    pub auto_post: bool,
    pub spread_total: bool,
    pub created_at: String,
}

/// Outcome of `post_due_schedules`: occurrences posted, plus the schedules
/// that stopped on a validation error (e.g. an account became a placeholder)
/// so the user can fix them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPostResult {
    pub posted: i64,
    pub failed: Vec<AutoPostFailure>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPostFailure {
    pub guid: String,
    pub name: String,
    pub message: String,
}

// ── Stored template ──────────────────────────────────────────────────────────

/// `template_json` shape: a `TxInput` without `postDate` (camelCase, mirroring
/// the IPC shapes so the editor can round-trip it).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredSplit {
    pub(crate) account_guid: String,
    pub(crate) amount: f64,
    pub(crate) quantity: Option<f64>,
    pub(crate) memo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredTemplate {
    pub(crate) description: Option<String>,
    pub(crate) num: Option<String>,
    pub(crate) notes: Option<String>,
    /// Absent in templates saved before payees existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) payee: Option<String>,
    pub(crate) splits: Vec<StoredSplit>,
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
    skipped_count: i64,
    next_due_date: Option<String>,
    active: bool,
    template_json: String,
    created_at: String,
    auto_post: bool,
    spread_total: bool,
}

const SELECT_COLS: &str = "guid, name, frequency, interval_mult, start_date, end_date, \
    max_occurrences, occurrences_posted, \
    COALESCE((SELECT skipped_count FROM schedule_skips k WHERE k.schedule_guid = recurring_schedules.guid), 0), \
    next_due_date, active, template_json, created_at, \
    COALESCE((SELECT auto_post FROM schedule_options o WHERE o.schedule_guid = recurring_schedules.guid), 0), \
    COALESCE((SELECT spread_total FROM schedule_options o WHERE o.schedule_guid = recurring_schedules.guid), 0)";

fn read_row(row: &rusqlite::Row) -> rusqlite::Result<ScheduleRow> {
    Ok(ScheduleRow {
        guid: row.get(0)?,
        name: row.get(1)?,
        frequency: row.get(2)?,
        interval_mult: row.get(3)?,
        start_date: row.get(4)?,
        end_date: row.get(5)?,
        max_occurrences: row.get(6)?,
        occurrences_posted: row.get(7)?,
        skipped_count: row.get(8)?,
        next_due_date: row.get(9)?,
        active: row.get::<_, i64>(10)? != 0,
        template_json: row.get(11)?,
        created_at: row.get(12)?,
        auto_post: row.get::<_, i64>(13)? != 0,
        spread_total: row.get::<_, i64>(14)? != 0,
    })
}

// ── Recurrence math (pure, unit-tested) ──────────────────────────────────────

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let first_next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    match first_next {
        Some(d) => (d - chrono::Duration::days(1)).day(),
        None => 28,
    }
}

/// Month arithmetic with end-of-month clamping (Jan 31 + 1mo → Feb 28/29),
/// implemented explicitly instead of relying on chrono's month behavior.
fn add_months_clamped(date: NaiveDate, months: i64) -> Option<NaiveDate> {
    let total = date.year() as i64 * 12 + date.month0() as i64 + months;
    let year = total.div_euclid(12);
    let month0 = total.rem_euclid(12);
    if !(1..=9999).contains(&year) {
        return None;
    }
    let month = (month0 + 1) as u32;
    let day = date.day().min(last_day_of_month(year as i32, month));
    NaiveDate::from_ymd_opt(year as i32, month, day)
}

/// Occurrence following `from` for `(frequency, interval_mult)`.
/// `biweekly` is 14 days × mult; quarterly/yearly are month-based (clamped).
pub(crate) fn next_occurrence(
    from: NaiveDate,
    frequency: &str,
    interval_mult: i64,
) -> Result<NaiveDate, AppError> {
    let mult = interval_mult.max(1);
    let next = match frequency {
        "daily" => from.checked_add_days(chrono::Days::new(mult as u64)),
        "weekly" => from.checked_add_days(chrono::Days::new((7 * mult) as u64)),
        "biweekly" => from.checked_add_days(chrono::Days::new((14 * mult) as u64)),
        "monthly" => add_months_clamped(from, mult),
        "quarterly" => add_months_clamped(from, 3 * mult),
        "yearly" => add_months_clamped(from, 12 * mult),
        _ => {
            return Err(AppError::Validation(format!(
                "Invalid frequency: {frequency}"
            )));
        }
    };
    next.ok_or_else(|| {
        AppError::Validation("Schedule date overflowed the supported range".to_string())
    })
}

/// Whole cents of an amount (spread schedules work at cent precision).
fn to_cents(amount: f64) -> i64 {
    (amount * 100.0).round() as i64
}

/// `a / n` rounded half away from zero, so a split and its negated
/// counterpart round to exact opposites.
fn div_round(a: i64, n: i64) -> i64 {
    a.signum() * ((2 * a.abs() + n) / (2 * n))
}

/// The amounts occurrence `index` (0-based, posted + skipped before it) of a
/// spread schedule posts: each split's slice is `round(total·(k+1)/n) −
/// round(total·k/n)`, so the `n` slices sum exactly to the total and differ by
/// at most a cent. The largest split absorbs any cross-split rounding so every
/// occurrence balances on its own.
fn spread_occurrence_amounts(totals: &[f64], occurrences: i64, index: i64) -> Vec<f64> {
    let n = occurrences.max(1);
    let k = index.clamp(0, n - 1);
    let mut cents: Vec<i64> = totals
        .iter()
        .map(|t| {
            let t = to_cents(*t);
            div_round(t * (k + 1), n) - div_round(t * k, n)
        })
        .collect();
    if let Some(anchor) = (0..totals.len()).max_by_key(|&i| to_cents(totals[i]).abs()) {
        let others: i64 = cents
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != anchor)
            .map(|(_, c)| c)
            .sum();
        cents[anchor] = -others;
    }
    cents.into_iter().map(|c| c as f64 / 100.0).collect()
}

/// The split amounts the occurrence at `index` posts: the template as-is, or
/// its slice of the total for a spread schedule.
fn occurrence_amounts(template: &StoredTemplate, spread: Option<i64>, index: i64) -> Vec<f64> {
    let totals: Vec<f64> = template.splits.iter().map(|s| s.amount).collect();
    match spread {
        Some(n) => spread_occurrence_amounts(&totals, n, index),
        None => totals,
    }
}

/// Occurrence count a spread schedule divides by (`None` when not spread).
fn spread_count(spread_total: bool, max_occurrences: Option<i64>) -> Option<i64> {
    if spread_total {
        max_occurrences
    } else {
        None
    }
}

// ── GnuCash recurrence (import support) ──────────────────────────────────────

// ── Validation ───────────────────────────────────────────────────────────────

fn validate_frequency(raw: &str) -> Result<String, AppError> {
    let v = raw.trim().to_lowercase();
    if FREQUENCIES.contains(&v.as_str()) {
        Ok(v)
    } else {
        Err(AppError::Validation(format!(
            "Invalid frequency: {raw}. Expected one of daily, weekly, biweekly, monthly, quarterly or yearly"
        )))
    }
}

fn parse_iso_date(raw: &str, field: &str) -> Result<String, AppError> {
    utils::validate_iso_date(raw).map_err(|_| {
        AppError::Validation(format!(
            "That {field} doesn't look right (use YYYY-MM-DD): {raw}"
        ))
    })
}

fn parse_stored_date(raw: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| AppError::Validation(format!("Corrupt schedule date: {raw}")))
}

/// Advance a date by `steps` occurrences (the pointer position after `steps`
/// posted/skipped occurrences). Used to resume a completed schedule at the
/// first occurrence it never passed, instead of replaying from the start.
fn advance_occurrences(
    from: NaiveDate,
    frequency: &str,
    interval_mult: i64,
    steps: i64,
) -> Result<NaiveDate, AppError> {
    let mut date = from;
    for _ in 0..steps.max(0) {
        date = next_occurrence(date, frequency, interval_mult)?;
    }
    Ok(date)
}

struct ValidatedSchedule {
    name: String,
    frequency: String,
    interval_mult: i64,
    start_date: String,
    end_date: Option<String>,
    max_occurrences: Option<i64>,
    template: StoredTemplate,
    auto_post: bool,
    spread_total: bool,
}

fn validate_schedule_input(
    conn: &Connection,
    input: &ScheduleInput,
) -> Result<ValidatedSchedule, AppError> {
    let lookup = |guid: &str| crate::transactions::account_info_lookup(conn, guid);
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation(
            "Give this schedule a name".to_string(),
        ));
    }
    if name.len() > 200 {
        return Err(AppError::Validation(
            "Keep the schedule name under 200 characters".to_string(),
        ));
    }
    let frequency = validate_frequency(&input.frequency)?;
    let interval_mult = input.interval_mult.unwrap_or(1);
    if !(1..=100).contains(&interval_mult) {
        return Err(AppError::Validation(
            "Repeat every 1 to 100 times. Pick something in there".to_string(),
        ));
    }
    let start_date = parse_iso_date(&input.start_date, "start date")?;
    let end_date = match &input.end_date {
        Some(e) if !e.trim().is_empty() => Some(parse_iso_date(e, "end date")?),
        _ => None,
    };
    if let Some(ref e) = end_date {
        if e < &start_date {
            return Err(AppError::Validation(
                "The end date is before the start date".to_string(),
            ));
        }
    }
    if let Some(m) = input.max_occurrences {
        if !(1..=1200).contains(&m) {
            return Err(AppError::Validation(
                "Stop after 1 to 1200 times. Pick something in there".to_string(),
            ));
        }
    }
    // Template reuses the manual body validation (accounts, balance, limits)
    // without the post-date gate: a template is future intent, not a posting.
    // The full date-aware check runs again at posting time.
    let probe = TxInput {
        post_date: start_date.clone(),
        description: input.description.clone(),
        num: input.num.clone(),
        notes: input.notes.clone(),
        splits: input.splits.clone(),
        unreconcile: false,
        tags: None,
        payee: None,
    };
    crate::transactions::validate_tx_body_with(&probe, &lookup)?;
    let payee = input
        .payee
        .as_deref()
        .map(crate::payees::normalize_payee_name)
        .transpose()?
        .flatten();
    if input.spread_total {
        validate_spread(input)?;
    }
    Ok(ValidatedSchedule {
        name,
        frequency,
        interval_mult,
        start_date,
        end_date,
        max_occurrences: input.max_occurrences,
        template: StoredTemplate {
            description: opt_text(&input.description),
            num: opt_text(&input.num),
            notes: opt_text(&input.notes),
            payee,
            splits: input
                .splits
                .iter()
                .map(|s| StoredSplit {
                    account_guid: s.account_guid.trim().to_string(),
                    amount: s.amount,
                    quantity: s.quantity,
                    memo: opt_text(&s.memo),
                })
                .collect(),
        },
        auto_post: input.auto_post,
        spread_total: input.spread_total,
    })
}

/// A spread total needs a fixed occurrence count to divide by, plain currency
/// splits (share quantities can't be sliced meaningfully) and totals that
/// balance in whole cents, so every slice balances too.
fn validate_spread(input: &ScheduleInput) -> Result<(), AppError> {
    if input.max_occurrences.is_none() {
        return Err(AppError::Validation(
            "Set how many times it repeats to spread the total".to_string(),
        ));
    }
    if input.splits.iter().any(|s| s.quantity.is_some()) {
        return Err(AppError::Validation(
            "A spread total can't carry share quantities. Remove them or stop spreading"
                .to_string(),
        ));
    }
    let cents: i64 = input.splits.iter().map(|s| to_cents(s.amount)).sum();
    if cents != 0 {
        return Err(AppError::Validation(
            "Spread totals must balance in whole cents".to_string(),
        ));
    }
    Ok(())
}

/// Upsert the side-table options. Rows exist only while something is on, so
/// the table stays empty for books that never use them.
fn save_options(
    conn: &Connection,
    guid: &str,
    auto_post: bool,
    spread_total: bool,
) -> Result<(), AppError> {
    if auto_post || spread_total {
        conn.execute(
            "INSERT INTO schedule_options (schedule_guid, auto_post, spread_total)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(schedule_guid) DO UPDATE
               SET auto_post = excluded.auto_post, spread_total = excluded.spread_total",
            params![guid, auto_post as i64, spread_total as i64],
        )?;
    } else {
        conn.execute(
            "DELETE FROM schedule_options WHERE schedule_guid = ?1",
            params![guid],
        )?;
    }
    Ok(())
}

// ── Pointer helpers ──────────────────────────────────────────────────────────

/// Re-apply the end-date / occurrence-count caps to a candidate pointer.
/// `passed_occurrences` is posted + skipped: a skipped occurrence consumes the
/// same capacity as a posted one. Returns `None` when the schedule is complete.
fn apply_caps(
    next: Option<String>,
    max_occurrences: Option<i64>,
    passed_occurrences: i64,
    end_date: Option<&str>,
) -> Option<String> {
    if max_occurrences
        .map(|m| passed_occurrences >= m)
        .unwrap_or(false)
    {
        return None;
    }
    match (next, end_date) {
        (Some(n), Some(e)) if n.as_str() > e => None,
        (n, _) => n,
    }
}

/// Whether a completed schedule (`next_due_date IS NULL`) has room again after
/// an edit that extended the count cap or end date.
fn has_room_again(
    max_occurrences: Option<i64>,
    passed_occurrences: i64,
    end_date: Option<&str>,
    start_date: &str,
) -> bool {
    if max_occurrences
        .map(|m| passed_occurrences >= m)
        .unwrap_or(false)
    {
        return false;
    }
    if let Some(e) = end_date {
        if start_date > e {
            return false;
        }
    }
    true
}

// ── Row → wire ───────────────────────────────────────────────────────────────

fn account_names(conn: &Connection, guids: &[String]) -> Result<HashMap<String, String>, AppError> {
    let mut uniq: Vec<String> = guids
        .iter()
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    uniq.sort();
    if uniq.is_empty() {
        return Ok(HashMap::new());
    }
    let sql = format!(
        "SELECT guid, name FROM accounts WHERE guid IN ({})",
        sql::in_clause(uniq.len())
    );
    let mut stmt = conn.prepare(&sql)?;
    let refs = sql::string_param_refs(&uniq);
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = HashMap::new();
    for r in rows {
        let (g, n) = r?;
        map.insert(g, n);
    }
    Ok(map)
}

pub(crate) fn parse_template(template_json: &str) -> Result<StoredTemplate, AppError> {
    serde_json::from_str(template_json)
        .map_err(|e| AppError::Validation(format!("Corrupt schedule data: {e}")))
}

fn preview_amount(amounts: &[f64]) -> f64 {
    amounts.iter().filter(|a| **a > 0.0).sum()
}

fn template_account_guids(template_json: &str) -> Result<Vec<String>, AppError> {
    Ok(parse_template(template_json)?
        .splits
        .iter()
        .map(|s| s.account_guid.clone())
        .collect())
}

fn to_schedule_with_names(
    row: ScheduleRow,
    names: &HashMap<String, String>,
) -> Result<Schedule, AppError> {
    let template = parse_template(&row.template_json)?;
    let next_amounts = occurrence_amounts(
        &template,
        spread_count(row.spread_total, row.max_occurrences),
        row.occurrences_posted + row.skipped_count,
    );
    Ok(Schedule {
        guid: row.guid,
        name: row.name,
        frequency: row.frequency,
        interval_mult: row.interval_mult,
        start_date: row.start_date,
        end_date: row.end_date,
        max_occurrences: row.max_occurrences,
        occurrences_posted: row.occurrences_posted,
        skipped_count: row.skipped_count,
        next_due_date: row.next_due_date,
        active: row.active,
        description: template.description,
        num: template.num,
        notes: template.notes,
        payee: template.payee,
        splits: template
            .splits
            .iter()
            .map(|s| ScheduleSplit {
                account_guid: s.account_guid.clone(),
                account_name: names.get(&s.account_guid).cloned(),
                amount: s.amount,
                quantity: s.quantity,
                memo: s.memo.clone(),
            })
            .collect(),
        preview_amount: preview_amount(&next_amounts),
        auto_post: row.auto_post,
        spread_total: row.spread_total,
        created_at: row.created_at,
    })
}

fn to_schedule(conn: &Connection, row: ScheduleRow) -> Result<Schedule, AppError> {
    let guids = template_account_guids(&row.template_json)?;
    let names = account_names(conn, &guids)?;
    to_schedule_with_names(row, &names)
}

fn fetch_row(conn: &Connection, guid: &str) -> Result<ScheduleRow, AppError> {
    let sql = format!("SELECT {SELECT_COLS} FROM recurring_schedules WHERE guid = ?1");
    conn.query_row(&sql, params![guid], read_row)
        .optional()?
        .ok_or_else(|| {
            AppError::NotFound("Schedule not found. It may have been deleted".to_string())
        })
}

fn fetch_schedule_inner(conn: &Connection, guid: &str) -> Result<Schedule, AppError> {
    to_schedule(conn, fetch_row(conn, guid)?)
}

// ── CRUD inners ──────────────────────────────────────────────────────────────

fn create_schedule_inner(conn: &Connection, input: ScheduleInput) -> Result<Schedule, AppError> {
    let v = validate_schedule_input(conn, &input)?;
    let guid = uuid::Uuid::new_v4().to_string();
    let created_at = utils::iso_now();
    let template_json = serde_json::to_string(&v.template)
        .map_err(|e| AppError::Validation(format!("We couldn't save that schedule: {e}")))?;
    // The pointer starts verbatim at the start date (no catch-up roll-forward):
    // a backdated start simply surfaces as overdue for the user to post/skip
    // (or, with auto-post on, catches up the next time the book opens).
    with_tx_err(conn, || -> Result<(), AppError> {
        conn.execute(
            "INSERT INTO recurring_schedules
          (guid, name, frequency, interval_mult, start_date, end_date,
           max_occurrences, occurrences_posted, next_due_date, active,
           template_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, 1, ?9, ?10)",
            params![
                guid,
                v.name,
                v.frequency,
                v.interval_mult,
                v.start_date,
                v.end_date,
                v.max_occurrences,
                v.start_date,
                template_json,
                created_at,
            ],
        )?;
        save_options(conn, &guid, v.auto_post, v.spread_total)
    })?;
    fetch_schedule_inner(conn, &guid)
}

fn update_schedule_inner(
    conn: &Connection,
    guid: &str,
    input: ScheduleInput,
) -> Result<Schedule, AppError> {
    let existing = fetch_row(conn, guid)?;
    let mut v = validate_schedule_input(conn, &input)?;
    if input.notes.is_none() {
        v.template.notes = parse_template(&existing.template_json)?.notes;
    }
    // Pointer rules: never-touched schedules (nothing posted, nothing skipped)
    // follow a start-date edit; otherwise the pointer is untouched (a start
    // edit must not rewrite history-in-progress). Completed schedules restart
    // at the new start when the edit makes room again (extended cap/end).
    // Caps always re-apply.
    let passed = existing.occurrences_posted + existing.skipped_count;
    let mut next = existing.next_due_date.clone();
    if passed == 0 {
        next = Some(v.start_date.clone());
    }
    if next.is_none()
        && has_room_again(
            v.max_occurrences,
            passed,
            v.end_date.as_deref(),
            &v.start_date,
        )
    {
        // Resume at the first occurrence never passed (posted or skipped);
        // replaying from `start_date` would duplicate history.
        next = Some(
            advance_occurrences(
                parse_stored_date(&v.start_date)?,
                &v.frequency,
                v.interval_mult,
                passed,
            )?
            .format("%Y-%m-%d")
            .to_string(),
        );
    }
    next = apply_caps(next, v.max_occurrences, passed, v.end_date.as_deref());
    let template_json = serde_json::to_string(&v.template)
        .map_err(|e| AppError::Validation(format!("We couldn't save that schedule: {e}")))?;
    with_tx_err(conn, || -> Result<(), AppError> {
        conn.execute(
            "UPDATE recurring_schedules
         SET name = ?1, frequency = ?2, interval_mult = ?3, start_date = ?4,
             end_date = ?5, max_occurrences = ?6, next_due_date = ?7,
             template_json = ?8
         WHERE guid = ?9",
            params![
                v.name,
                v.frequency,
                v.interval_mult,
                v.start_date,
                v.end_date,
                v.max_occurrences,
                next,
                template_json,
                guid,
            ],
        )?;
        save_options(conn, guid, v.auto_post, v.spread_total)
    })?;
    fetch_schedule_inner(conn, guid)
}

fn delete_schedule_inner(conn: &Connection, guid: &str) -> Result<(), AppError> {
    fetch_row(conn, guid)?;
    conn.execute(
        "DELETE FROM recurring_schedules WHERE guid = ?1",
        params![guid],
    )?;
    Ok(())
}

fn set_schedule_active_inner(
    conn: &Connection,
    guid: &str,
    active: bool,
) -> Result<Schedule, AppError> {
    fetch_row(conn, guid)?;
    conn.execute(
        "UPDATE recurring_schedules SET active = ?1 WHERE guid = ?2",
        params![if active { 1 } else { 0 }, guid],
    )?;
    fetch_schedule_inner(conn, guid)
}

fn fetch_schedules_inner(conn: &Connection) -> Result<Vec<Schedule>, AppError> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM recurring_schedules
         ORDER BY active DESC, next_due_date IS NULL, next_due_date, name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], read_row)?
        .collect::<Result<Vec<_>, _>>()?;
    // One account-name lookup for the whole list instead of per schedule.
    let mut guids: Vec<String> = Vec::new();
    for row in &rows {
        guids.extend(template_account_guids(&row.template_json)?);
    }
    let names = account_names(conn, &guids)?;
    rows.into_iter()
        .map(|r| to_schedule_with_names(r, &names))
        .collect()
}

/// Account GUIDs referenced by any schedule template. The GnuCash orphan sweep
/// uses this to keep accounts a schedule still needs (deleting them would leave
/// dangling template references).
pub(crate) fn schedule_account_guids(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT template_json FROM recurring_schedules")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut guids = HashSet::new();
    for row in rows {
        guids.extend(template_account_guids(&row?)?);
    }
    Ok(guids.into_iter().collect())
}

// ── Post / skip ──────────────────────────────────────────────────────────────

/// Materialize the due occurrence as a real transaction and advance the
/// pointer atomically. Due dates are always ≤ today (manual Post and
/// auto-post alike), so the manual future-date block is never bypassed.
fn post_schedule_occurrence_inner(conn: &Connection, guid: &str) -> Result<String, AppError> {
    // Local "today": the frontend enables Post from its local date, so the
    // backend must use the same clock.
    post_occurrence_at(
        conn,
        guid,
        &chrono::Local::now().format("%Y-%m-%d").to_string(),
    )
}

fn post_occurrence_at(conn: &Connection, guid: &str, today: &str) -> Result<String, AppError> {
    let row = fetch_row(conn, guid)?;
    if !row.active {
        return Err(AppError::Validation(
            "Schedule is paused. Resume it before posting".to_string(),
        ));
    }
    let due = row.next_due_date.clone().ok_or_else(|| {
        AppError::Validation("This schedule is done. Nothing left to post".to_string())
    })?;
    if due.as_str() > today {
        return Err(AppError::Validation(format!(
            "Not due yet. The next occurrence is {due}"
        )));
    }
    let template = parse_template(&row.template_json)?;
    let amounts = occurrence_amounts(
        &template,
        spread_count(row.spread_total, row.max_occurrences),
        row.occurrences_posted + row.skipped_count,
    );
    let input = TxInput {
        post_date: due.clone(),
        description: template.description.clone(),
        num: template.num.clone(),
        notes: template.notes.clone(),
        unreconcile: false,
        tags: None,
        payee: None,
        splits: template
            .splits
            .iter()
            .zip(amounts)
            .map(|(s, amount)| SplitInput {
                account_guid: s.account_guid.clone(),
                amount,
                quantity: s.quantity,
                memo: s.memo.clone(),
            })
            .collect(),
    };
    // Full manual validation at posting time: accounts may have been deleted
    // or turned into placeholders since the schedule was created.
    let post_date = crate::transactions::validate_tx_input(conn, &input)?;
    let tx_guid = uuid::Uuid::new_v4().to_string();

    with_tx_err(conn, || {
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description, num, notes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                tx_guid,
                post_date,
                opt_text(&input.description),
                opt_text(&input.num),
                opt_text(&input.notes),
            ],
        )?;
        crate::transactions::insert_split_rows(conn, &tx_guid, &input.splits)?;
        crate::payees::set_transaction_payee(conn, &tx_guid, template.payee.as_deref())?;
        let posted = row.occurrences_posted + 1;
        let advanced =
            next_occurrence(parse_stored_date(&due)?, &row.frequency, row.interval_mult)?
                .format("%Y-%m-%d")
                .to_string();
        let next = apply_caps(
            Some(advanced),
            row.max_occurrences,
            posted + row.skipped_count,
            row.end_date.as_deref(),
        );
        conn.execute(
            "UPDATE recurring_schedules
             SET occurrences_posted = ?1, next_due_date = ?2 WHERE guid = ?3",
            params![posted, next, guid],
        )?;
        Ok(tx_guid.clone())
    })
}

/// Drop the due occurrence without posting (e.g. a skipped month) and advance
/// the pointer. Allowed for future occurrences too — skipping is pointer
/// movement, never money movement. Returns the new pointer (`None` when the
/// skip completed the schedule).
fn skip_schedule_occurrence_inner(
    conn: &Connection,
    guid: &str,
) -> Result<Option<String>, AppError> {
    let row = fetch_row(conn, guid)?;
    if !row.active {
        return Err(AppError::Validation(
            "Schedule is paused. Resume it before skipping".to_string(),
        ));
    }
    let due = row.next_due_date.clone().ok_or_else(|| {
        AppError::Validation("Schedule is completed. No occurrences remain".to_string())
    })?;
    let advanced = next_occurrence(parse_stored_date(&due)?, &row.frequency, row.interval_mult)?
        .format("%Y-%m-%d")
        .to_string();
    let skipped = row.skipped_count + 1;
    let next = apply_caps(
        Some(advanced),
        row.max_occurrences,
        row.occurrences_posted + skipped,
        row.end_date.as_deref(),
    );
    with_tx_err(conn, || -> Result<(), AppError> {
        conn.execute(
            "INSERT INTO schedule_skips (schedule_guid, skipped_count) VALUES (?1, 1)
             ON CONFLICT(schedule_guid) DO UPDATE SET skipped_count = skipped_count + 1",
            params![guid],
        )?;
        conn.execute(
            "UPDATE recurring_schedules SET next_due_date = ?1 WHERE guid = ?2",
            params![next, guid],
        )?;
        Ok(())
    })?;
    Ok(next)
}

/// Upper bound on occurrences one auto-post run creates, so a long-backdated
/// daily schedule can't stall the book open; the rest post on the next run.
const AUTO_POST_LIMIT: i64 = 500;

/// Post every due occurrence of active auto-post schedules, oldest first, up
/// to `today`. A schedule that fails validation stops (its pointer stays put)
/// and is reported; the others still post. DB failures abort the run.
fn post_due_schedules_inner(conn: &Connection, today: &str) -> Result<AutoPostResult, AppError> {
    let mut stmt = conn.prepare(
        "SELECT r.guid, r.name FROM recurring_schedules r
         JOIN schedule_options o ON o.schedule_guid = r.guid
         WHERE o.auto_post = 1 AND r.active = 1
           AND r.next_due_date IS NOT NULL AND r.next_due_date <= ?1
         ORDER BY r.next_due_date, r.name",
    )?;
    let due: Vec<(String, String)> = stmt
        .query_map(params![today], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut result = AutoPostResult {
        posted: 0,
        failed: Vec::new(),
    };
    for (guid, name) in due {
        loop {
            if result.posted >= AUTO_POST_LIMIT {
                return Ok(result);
            }
            let row = fetch_row(conn, &guid)?;
            match row.next_due_date {
                Some(next) if next.as_str() <= today => {}
                _ => break,
            }
            match post_occurrence_at(conn, &guid, today) {
                Ok(_) => result.posted += 1,
                Err(AppError::Validation(message)) => {
                    result.failed.push(AutoPostFailure {
                        guid: guid.clone(),
                        name: name.clone(),
                        message,
                    });
                    break;
                }
                Err(e) => return Err(e),
            }
        }
    }
    Ok(result)
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_schedule(
    state: State<'_, AppState>,
    input: ScheduleInput,
) -> Result<Schedule, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        create_schedule_inner(conn, input)
    })
    .await
}

#[tauri::command]
pub async fn update_schedule(
    state: State<'_, AppState>,
    guid: String,
    input: ScheduleInput,
) -> Result<Schedule, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        update_schedule_inner(conn, &guid, input)
    })
    .await
}

#[tauri::command]
pub async fn delete_schedule(state: State<'_, AppState>, guid: String) -> Result<(), AppError> {
    run_blocking(state.db.clone(), move |conn| {
        delete_schedule_inner(conn, &guid)
    })
    .await
}

#[tauri::command]
pub async fn set_schedule_active(
    state: State<'_, AppState>,
    guid: String,
    active: bool,
) -> Result<Schedule, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        set_schedule_active_inner(conn, &guid, active)
    })
    .await
}

#[tauri::command]
pub async fn fetch_schedules(state: State<'_, AppState>) -> Result<Vec<Schedule>, AppError> {
    run_blocking(state.db.clone(), fetch_schedules_inner).await
}

#[tauri::command]
pub async fn fetch_schedule(
    state: State<'_, AppState>,
    guid: String,
) -> Result<Schedule, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_schedule_inner(conn, &guid)
    })
    .await
}

/// Post the due occurrence now. Returns the new transaction GUID so the
/// frontend can navigate to its detail page.
#[tauri::command]
pub async fn post_schedule_occurrence(
    state: State<'_, AppState>,
    guid: String,
) -> Result<String, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        post_schedule_occurrence_inner(conn, &guid)
    })
    .await
}

/// Post what auto-post schedules have due; the layout runs this when a book
/// opens.
#[tauri::command]
pub async fn post_due_schedules(state: State<'_, AppState>) -> Result<AutoPostResult, AppError> {
    run_blocking(state.db.clone(), |conn| {
        post_due_schedules_inner(conn, &chrono::Local::now().format("%Y-%m-%d").to_string())
    })
    .await
}

/// Skip the due occurrence without posting. Returns the new pointer (`None`
/// when the skip completed the schedule).
#[tauri::command]
pub async fn skip_schedule_occurrence(
    state: State<'_, AppState>,
    guid: String,
) -> Result<Option<String>, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        skip_schedule_occurrence_inner(conn, &guid)
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
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES ('c-eur', 'EUR', 'CURRENCY')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('root', 'Root', 'ROOT', NULL, 0, 'c-eur'),
                    ('checking', 'Checking', 'BANK', 'root', 0, 'c-eur'),
                    ('rent-exp', 'Rent', 'EXPENSE', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn
    }

    fn balanced_input() -> ScheduleInput {
        ScheduleInput {
            name: "Rent".to_string(),
            frequency: "monthly".to_string(),
            interval_mult: None,
            start_date: "2024-01-01".to_string(),
            end_date: None,
            max_occurrences: None,
            description: Some("Monthly rent".to_string()),
            num: None,
            notes: None,
            payee: None,
            splits: vec![
                SplitInput {
                    account_guid: "rent-exp".to_string(),
                    amount: 1000.0,
                    quantity: None,
                    memo: None,
                },
                SplitInput {
                    account_guid: "checking".to_string(),
                    amount: -1000.0,
                    quantity: None,
                    memo: None,
                },
            ],
            auto_post: false,
            spread_total: false,
        }
    }

    /// A 1199.00 laptop depreciated over 96 months, like a device asset.
    fn spread_input() -> ScheduleInput {
        let mut input = balanced_input();
        input.name = "Laptop depreciation".to_string();
        input.max_occurrences = Some(96);
        input.spread_total = true;
        input.splits[0].amount = 1199.0;
        input.splits[1].amount = -1199.0;
        input
    }

    fn posted_amounts(conn: &Connection, account: &str) -> Vec<i64> {
        let mut stmt = conn
            .prepare(
                "SELECT s.value_num * 100 / s.value_denom FROM splits s
                 JOIN transactions t ON t.guid = s.tx_guid
                 WHERE s.account_guid = ?1 ORDER BY t.post_date",
            )
            .unwrap();
        stmt.query_map([account], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn spread_slices_sum_to_total_and_balance() {
        let totals = [1199.0, -1199.0];
        let slices: Vec<Vec<f64>> = (0..96)
            .map(|k| spread_occurrence_amounts(&totals, 96, k))
            .collect();
        let sum: i64 = slices.iter().map(|s| to_cents(s[0])).sum();
        assert_eq!(sum, 119_900);
        for s in &slices {
            assert_eq!(to_cents(s[0]) + to_cents(s[1]), 0);
            assert!((1248..=1249).contains(&to_cents(s[0])));
        }
        // Three legs that round independently still balance per occurrence.
        let totals = [10.0, -5.0, -5.0];
        let mut legs = [0i64; 3];
        for k in 0..3 {
            let s = spread_occurrence_amounts(&totals, 3, k);
            assert_eq!(s.iter().map(|a| to_cents(*a)).sum::<i64>(), 0);
            for (leg, a) in legs.iter_mut().zip(&s) {
                *leg += to_cents(*a);
            }
        }
        assert_eq!(legs, [1000, -500, -500]);
    }

    #[test]
    fn spread_requires_count_and_plain_amounts() {
        let conn = setup();
        let mut no_count = spread_input();
        no_count.max_occurrences = None;
        assert!(matches!(
            create_schedule_inner(&conn, no_count),
            Err(AppError::Validation(_))
        ));
        let mut with_qty = spread_input();
        with_qty.splits[0].quantity = Some(1.0);
        assert!(matches!(
            create_schedule_inner(&conn, with_qty),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn spread_schedule_posts_slices_and_previews_next() {
        let conn = setup();
        let s = create_schedule_inner(&conn, spread_input()).unwrap();
        assert!(s.spread_total);
        assert_eq!(s.splits[0].amount, 1199.0); // the editor sees the total
        assert_eq!(
            to_cents(s.preview_amount),
            to_cents(spread_occurrence_amounts(&[1199.0, -1199.0], 96, 0)[0])
        );
        post_occurrence_at(&conn, &s.guid, "2024-01-01").unwrap();
        let after = fetch_schedule_inner(&conn, &s.guid).unwrap();
        assert_eq!(
            to_cents(after.preview_amount),
            to_cents(spread_occurrence_amounts(&[1199.0, -1199.0], 96, 1)[0])
        );
        let posted = posted_amounts(&conn, "rent-exp");
        assert_eq!(
            posted,
            vec![to_cents(
                spread_occurrence_amounts(&[1199.0, -1199.0], 96, 0)[0]
            )]
        );
    }

    #[test]
    fn auto_post_catches_up_to_today_and_completes() {
        let conn = setup();
        let mut input = spread_input();
        input.auto_post = true;
        input.max_occurrences = Some(3);
        input.start_date = "2024-01-15".to_string();
        let s = create_schedule_inner(&conn, input).unwrap();
        // A manual schedule that's also due is left for the user.
        create_schedule_inner(&conn, balanced_input()).unwrap();

        let r = post_due_schedules_inner(&conn, "2024-02-20").unwrap();
        assert_eq!(r.posted, 2);
        assert!(r.failed.is_empty());
        let mid = fetch_schedule_inner(&conn, &s.guid).unwrap();
        assert_eq!(mid.next_due_date.as_deref(), Some("2024-03-15"));

        let r = post_due_schedules_inner(&conn, "2025-01-01").unwrap();
        assert_eq!(r.posted, 1);
        let done = fetch_schedule_inner(&conn, &s.guid).unwrap();
        assert_eq!(done.next_due_date, None);
        assert_eq!(
            posted_amounts(&conn, "rent-exp").iter().sum::<i64>(),
            119_900
        );
        assert_eq!(
            post_due_schedules_inner(&conn, "2025-01-01")
                .unwrap()
                .posted,
            0
        );
    }

    #[test]
    fn auto_post_reports_invalid_schedules_and_continues() {
        let conn = setup();
        let mut broken = balanced_input();
        broken.name = "Broken".to_string();
        broken.auto_post = true;
        let b = create_schedule_inner(&conn, broken).unwrap();
        let mut ok = balanced_input();
        ok.auto_post = true;
        ok.start_date = "2024-01-02".to_string();
        create_schedule_inner(&conn, ok).unwrap();
        conn.execute(
            "UPDATE accounts SET placeholder = 1 WHERE guid = 'rent-exp'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid)
             VALUES ('other-exp', 'Other', 'EXPENSE', 'root', 0, 'c-eur')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE recurring_schedules SET template_json = replace(template_json, 'rent-exp', 'other-exp')
             WHERE guid <> ?1",
            [&b.guid],
        )
        .unwrap();

        let r = post_due_schedules_inner(&conn, "2024-01-02").unwrap();
        assert_eq!(r.posted, 1);
        assert_eq!(r.failed.len(), 1);
        assert_eq!(r.failed[0].name, "Broken");
        let still = fetch_schedule_inner(&conn, &b.guid).unwrap();
        assert_eq!(still.next_due_date.as_deref(), Some("2024-01-01"));
    }

    #[test]
    fn options_round_trip_and_clear() {
        let conn = setup();
        let mut input = balanced_input();
        input.auto_post = true;
        let s = create_schedule_inner(&conn, input).unwrap();
        assert!(s.auto_post && !s.spread_total);
        let off = update_schedule_inner(&conn, &s.guid, balanced_input()).unwrap();
        assert!(!off.auto_post);
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schedule_options", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn next_occurrence_handles_all_frequencies() {
        assert_eq!(
            next_occurrence(d("2024-01-01"), "daily", 1).unwrap(),
            d("2024-01-02")
        );
        assert_eq!(
            next_occurrence(d("2024-01-01"), "daily", 3).unwrap(),
            d("2024-01-04")
        );
        assert_eq!(
            next_occurrence(d("2024-01-01"), "weekly", 1).unwrap(),
            d("2024-01-08")
        );
        assert_eq!(
            next_occurrence(d("2024-01-01"), "biweekly", 1).unwrap(),
            d("2024-01-15")
        );
        assert_eq!(
            next_occurrence(d("2024-01-01"), "monthly", 1).unwrap(),
            d("2024-02-01")
        );
        assert_eq!(
            next_occurrence(d("2024-01-01"), "quarterly", 1).unwrap(),
            d("2024-04-01")
        );
        assert_eq!(
            next_occurrence(d("2024-01-01"), "yearly", 1).unwrap(),
            d("2025-01-01")
        );
        assert_eq!(
            next_occurrence(d("2024-01-01"), "monthly", 2).unwrap(),
            d("2024-03-01")
        );
        assert!(next_occurrence(d("2024-01-01"), "hourly", 1).is_err());
    }

    #[test]
    fn next_occurrence_clamps_month_ends() {
        // Jan 31 → Feb 29 in a leap year, Feb 28 otherwise.
        assert_eq!(
            next_occurrence(d("2024-01-31"), "monthly", 1).unwrap(),
            d("2024-02-29")
        );
        assert_eq!(
            next_occurrence(d("2023-01-31"), "monthly", 1).unwrap(),
            d("2023-02-28")
        );
        // Feb 29 → Feb 28 in a common year.
        assert_eq!(
            next_occurrence(d("2024-02-29"), "yearly", 1).unwrap(),
            d("2025-02-28")
        );
        // Quarterly from Nov 30 → Feb 29.
        assert_eq!(
            next_occurrence(d("2023-11-30"), "quarterly", 1).unwrap(),
            d("2024-02-29")
        );
    }

    #[test]
    fn create_starts_pointer_at_start_date() {
        let conn = setup();
        let s = create_schedule_inner(&conn, balanced_input()).unwrap();
        assert_eq!(s.next_due_date.as_deref(), Some("2024-01-01"));
        assert_eq!(s.interval_mult, 1);
        assert_eq!(s.preview_amount, 1000.0);
        assert!(s.active);
        assert_eq!(s.splits.len(), 2);
        assert_eq!(s.splits[0].account_name.as_deref(), Some("Rent"));
    }

    #[test]
    fn validation_rejects_bad_rules_and_templates() {
        let conn = setup();
        let mut bad = balanced_input();
        bad.frequency = "hourly".to_string();
        assert!(create_schedule_inner(&conn, bad.clone()).is_err());

        let mut bad = balanced_input();
        bad.end_date = Some("2023-12-31".to_string());
        assert!(create_schedule_inner(&conn, bad.clone()).is_err());

        let mut bad = balanced_input();
        bad.name = "   ".to_string();
        assert!(create_schedule_inner(&conn, bad.clone()).is_err());

        let mut bad = balanced_input();
        bad.interval_mult = Some(0);
        assert!(create_schedule_inner(&conn, bad.clone()).is_err());

        // Unbalanced template is rejected by the shared body validation.
        let mut bad = balanced_input();
        bad.splits[0].amount = 999.0;
        assert!(create_schedule_inner(&conn, bad.clone()).is_err());

        // Unknown account is rejected too.
        let mut bad = balanced_input();
        bad.splits[0].account_guid = "nope".to_string();
        assert!(create_schedule_inner(&conn, bad.clone()).is_err());
    }

    #[test]
    fn post_creates_visible_transaction_and_advances() {
        let conn = setup();
        let s = create_schedule_inner(&conn, balanced_input()).unwrap();
        let tx_guid = post_schedule_occurrence_inner(&conn, &s.guid).unwrap();

        // Real, visible ledger row dated at the due date.
        let (post_date, desc): (String, Option<String>) = conn
            .query_row(
                "SELECT post_date, description FROM transactions WHERE guid = ?1",
                params![tx_guid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(post_date, "2024-01-01");
        assert_eq!(desc.as_deref(), Some("Monthly rent"));
        let splits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM splits WHERE tx_guid = ?1",
                params![tx_guid],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(splits, 2);

        // Pointer advanced one month, posted count bumped.
        let after = fetch_schedule_inner(&conn, &s.guid).unwrap();
        assert_eq!(after.next_due_date.as_deref(), Some("2024-02-01"));
        assert_eq!(after.occurrences_posted, 1);
    }

    #[test]
    fn skips_count_toward_caps_and_survive_edits() {
        let conn = setup();
        let mut input = balanced_input();
        input.max_occurrences = Some(2);
        let s = create_schedule_inner(&conn, input.clone()).unwrap();

        // First occurrence skipped: pointer advances, counter records it.
        let next = skip_schedule_occurrence_inner(&conn, &s.guid).unwrap();
        assert_eq!(next.as_deref(), Some("2024-02-01"));
        let after = fetch_schedule_inner(&conn, &s.guid).unwrap();
        assert_eq!(after.skipped_count, 1);
        assert_eq!(after.occurrences_posted, 0);

        // Second skip consumes the cap: the schedule completes.
        let next = skip_schedule_occurrence_inner(&conn, &s.guid).unwrap();
        assert_eq!(next, None);

        // Editing without extending the cap must not replay skipped occurrences.
        let updated = update_schedule_inner(&conn, &s.guid, input.clone()).unwrap();
        assert_eq!(updated.next_due_date, None);
        assert_eq!(updated.skipped_count, 2);

        // Raising the cap resumes at the first unpassed occurrence (March), not
        // at the original start date.
        input.max_occurrences = Some(3);
        let updated = update_schedule_inner(&conn, &s.guid, input).unwrap();
        assert_eq!(updated.next_due_date.as_deref(), Some("2024-03-01"));
        assert_eq!(updated.skipped_count, 2);
    }

    #[test]
    fn posted_and_skipped_occurrences_share_the_cap() {
        let conn = setup();
        let mut input = balanced_input();
        input.max_occurrences = Some(2);
        let s = create_schedule_inner(&conn, input).unwrap();

        post_schedule_occurrence_inner(&conn, &s.guid).unwrap();
        let next = skip_schedule_occurrence_inner(&conn, &s.guid).unwrap();
        assert_eq!(next, None);
        assert_eq!(
            fetch_schedule_inner(&conn, &s.guid).unwrap().skipped_count,
            1
        );
    }

    #[test]
    fn post_rejects_future_paused_and_completed() {
        let conn = setup();
        let mut future = balanced_input();
        future.start_date = "2099-01-01".to_string();
        let f = create_schedule_inner(&conn, future).unwrap();
        assert!(post_schedule_occurrence_inner(&conn, &f.guid).is_err());

        set_schedule_active_inner(&conn, &f.guid, false).unwrap();
        assert!(post_schedule_occurrence_inner(&conn, &f.guid).is_err());
        // Skipping a paused schedule is blocked too.
        assert!(skip_schedule_occurrence_inner(&conn, &f.guid).is_err());
    }

    #[test]
    fn skip_advances_without_posting() {
        let conn = setup();
        let s = create_schedule_inner(&conn, balanced_input()).unwrap();
        let next = skip_schedule_occurrence_inner(&conn, &s.guid).unwrap();
        assert_eq!(next.as_deref(), Some("2024-02-01"));
        let txs: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(txs, 0);
    }

    #[test]
    fn count_cap_completes_on_post() {
        let conn = setup();
        let mut one = balanced_input();
        one.max_occurrences = Some(1);
        let s = create_schedule_inner(&conn, one).unwrap();
        post_schedule_occurrence_inner(&conn, &s.guid).unwrap();
        let after = fetch_schedule_inner(&conn, &s.guid).unwrap();
        assert_eq!(after.next_due_date, None);
        assert_eq!(after.occurrences_posted, 1);
        // No occurrences remain.
        assert!(post_schedule_occurrence_inner(&conn, &s.guid).is_err());
    }

    #[test]
    fn end_date_cap_completes_on_skip() {
        let conn = setup();
        let mut input = balanced_input();
        input.start_date = "2024-01-01".to_string();
        input.end_date = Some("2024-01-01".to_string());
        let s = create_schedule_inner(&conn, input).unwrap();
        // Skipping the only in-range occurrence completes the schedule.
        assert_eq!(
            skip_schedule_occurrence_inner(&conn, &s.guid).unwrap(),
            None
        );
    }

    #[test]
    fn update_without_notes_keeps_them() {
        let conn = setup();
        let mut input = balanced_input();
        input.notes = Some("landlord".to_string());
        let s = create_schedule_inner(&conn, input).unwrap();

        // `None` keeps the template notes; a blank string clears them.
        let after = update_schedule_inner(&conn, &s.guid, balanced_input()).unwrap();
        assert_eq!(after.notes.as_deref(), Some("landlord"));
        let mut blank = balanced_input();
        blank.notes = Some(String::new());
        let after = update_schedule_inner(&conn, &s.guid, blank).unwrap();
        assert_eq!(after.notes, None);
    }

    #[test]
    fn update_moves_pointer_only_before_first_post() {
        let conn = setup();
        let s = create_schedule_inner(&conn, balanced_input()).unwrap();
        // Never posted: a start-date edit moves the pointer with it.
        let mut edited = balanced_input();
        edited.start_date = "2024-03-01".to_string();
        let after = update_schedule_inner(&conn, &s.guid, edited).unwrap();
        assert_eq!(after.next_due_date.as_deref(), Some("2024-03-01"));

        // After a post, editing the name keeps the pointer.
        post_schedule_occurrence_inner(&conn, &s.guid).unwrap();
        let mut renamed = balanced_input();
        renamed.name = "Rent v2".to_string();
        renamed.start_date = "2020-01-01".to_string();
        let after = update_schedule_inner(&conn, &s.guid, renamed).unwrap();
        assert_eq!(after.name, "Rent v2");
        assert_eq!(after.next_due_date.as_deref(), Some("2024-04-01"));
        assert_eq!(after.occurrences_posted, 1);
    }

    #[test]
    fn delete_removes_schedule() {
        let conn = setup();
        let s = create_schedule_inner(&conn, balanced_input()).unwrap();
        delete_schedule_inner(&conn, &s.guid).unwrap();
        assert!(fetch_schedule_inner(&conn, &s.guid).is_err());
        assert!(fetch_schedules_inner(&conn).unwrap().is_empty());
    }

    #[test]
    fn payee_rides_the_template_onto_posted_transactions() {
        let conn = setup();
        let s = create_schedule_inner(
            &conn,
            ScheduleInput {
                payee: Some("  O2 ".into()),
                ..balanced_input()
            },
        )
        .unwrap();
        assert_eq!(s.payee.as_deref(), Some("O2"));
        let tx = post_occurrence_at(&conn, &s.guid, "2024-01-01").unwrap();
        let payee = crate::payees::payees_for_txns(&conn, std::slice::from_ref(&tx))
            .unwrap()
            .remove(&tx)
            .map(|p| p.name);
        assert_eq!(payee.as_deref(), Some("O2"));

        // Templates saved before payees existed have no `payee` key.
        let legacy: StoredTemplate =
            serde_json::from_str(r#"{"description":"Rent","num":null,"notes":null,"splits":[]}"#)
                .unwrap();
        assert_eq!(legacy.payee, None);
    }
}
