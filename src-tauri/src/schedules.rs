use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::history;
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
    pub notes: Option<String>,
    pub splits: Vec<SplitInput>,
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
    pub next_due_date: Option<String>,
    pub active: bool,
    pub description: Option<String>,
    pub num: Option<String>,
    pub notes: Option<String>,
    pub splits: Vec<ScheduleSplit>,
    /// Sum of the positive template split amounts: the transfer size behind a
    /// balanced template. Rendered in list rows (a raw sum is always 0).
    pub preview_amount: f64,
    pub created_at: String,
}

// ── Stored template ──────────────────────────────────────────────────────────

/// `template_json` shape: a `TxInput` without `postDate` (camelCase, mirroring
/// the IPC shapes so the editor can round-trip it).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSplit {
    account_guid: String,
    amount: f64,
    quantity: Option<f64>,
    memo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTemplate {
    description: Option<String>,
    num: Option<String>,
    notes: Option<String>,
    splits: Vec<StoredSplit>,
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
    next_due_date: Option<String>,
    active: bool,
    template_json: String,
    created_at: String,
}

const SELECT_COLS: &str = "guid, name, frequency, interval_mult, start_date, end_date, \
    max_occurrences, occurrences_posted, next_due_date, active, template_json, created_at";

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
        next_due_date: row.get(8)?,
        active: row.get::<_, i64>(9)? != 0,
        template_json: row.get(10)?,
        created_at: row.get(11)?,
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

// ── GnuCash recurrence (import support) ──────────────────────────────────────

/// First occurrence of a GnuCash-style recurrence on/after `from`, used by the
/// GnuCash import to seed `next_due_date`. Day/week recurrences are fixed
/// strides from `anchor`; month/year keep the anchor's day-of-month clamped to
/// short months (GnuCash's own day alignment).
pub(crate) fn first_occurrence_on_or_after(
    from: &str,
    anchor: &str,
    mult: i64,
    period_type: &str,
) -> Option<String> {
    let from = NaiveDate::parse_from_str(from, "%Y-%m-%d").ok()?;
    let anchor = NaiveDate::parse_from_str(anchor, "%Y-%m-%d").ok()?;
    let mult = mult.max(1);
    let occurrence = match period_type {
        "day" | "week" => {
            let step = if period_type == "day" { mult } else { 7 * mult };
            let diff = (from - anchor).num_days();
            let mut k = diff.div_euclid(step);
            let mut occurrence = anchor.checked_add_signed(chrono::Duration::days(k * step))?;
            while occurrence < from {
                k += 1;
                occurrence = anchor.checked_add_signed(chrono::Duration::days(k * step))?;
            }
            occurrence
        }
        "month" | "year" => {
            let step = if period_type == "month" { mult } else { 12 * mult };
            let month_diff = (from.year() as i64 * 12 + from.month0() as i64)
                - (anchor.year() as i64 * 12 + anchor.month0() as i64);
            let mut k = month_diff.div_euclid(step).max(0);
            let mut occurrence = add_months_anchored(anchor, k * step)?;
            while occurrence < from {
                k += 1;
                occurrence = add_months_anchored(anchor, k * step)?;
            }
            occurrence
        }
        _ => return None,
    };
    Some(occurrence.format("%Y-%m-%d").to_string())
}

/// Month arithmetic that keeps `anchor`'s day-of-month, clamped to the target
/// month's length (Jan 31 → Feb 28 → Mar 31). Unlike [`add_months_clamped`],
/// which clamps from the current date, the day phase survives short months.
fn add_months_anchored(anchor: NaiveDate, months: i64) -> Option<NaiveDate> {
    let total = anchor.year() as i64 * 12 + anchor.month0() as i64 + months;
    let year = total.div_euclid(12);
    let month0 = total.rem_euclid(12);
    if !(1..=9999).contains(&year) {
        return None;
    }
    let month = (month0 + 1) as u32;
    let day = anchor.day().min(last_day_of_month(year as i32, month));
    NaiveDate::from_ymd_opt(year as i32, month, day)
}

// ── Validation ───────────────────────────────────────────────────────────────

fn validate_frequency(raw: &str) -> Result<String, AppError> {
    let v = raw.trim().to_lowercase();
    if FREQUENCIES.contains(&v.as_str()) {
        Ok(v)
    } else {
        Err(AppError::Validation(format!(
            "Invalid frequency: {raw} — expected one of daily, weekly, biweekly, monthly, quarterly or yearly"
        )))
    }
}

fn parse_iso_date(raw: &str, field: &str) -> Result<String, AppError> {
    utils::validate_iso_date(raw)
        .map_err(|_| AppError::Validation(format!("Invalid {field} (expected YYYY-MM-DD): {raw}")))
}

fn parse_stored_date(raw: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| AppError::Validation(format!("Corrupt schedule date: {raw}")))
}

struct ValidatedSchedule {
    name: String,
    frequency: String,
    interval_mult: i64,
    start_date: String,
    end_date: Option<String>,
    max_occurrences: Option<i64>,
    template: StoredTemplate,
}

fn validate_schedule_input(
    conn: &Connection,
    input: &ScheduleInput,
) -> Result<ValidatedSchedule, AppError> {
    validate_schedule_input_with(input, &|guid| {
        crate::transactions::account_info_lookup(conn, guid)
    })
}

/// [`validate_schedule_input`] with the template's account lookup injected, so
/// the GnuCash import preview can validate against the accounts the merge will
/// create (source ∪ destination) without writing anything.
fn validate_schedule_input_with(
    input: &ScheduleInput,
    lookup: &dyn Fn(&str) -> Result<Option<crate::transactions::SplitAccountInfo>, AppError>,
) -> Result<ValidatedSchedule, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation(
            "Schedule name is required".to_string(),
        ));
    }
    if name.len() > 200 {
        return Err(AppError::Validation(
            "Schedule name is too long (max 200)".to_string(),
        ));
    }
    let frequency = validate_frequency(&input.frequency)?;
    let interval_mult = input.interval_mult.unwrap_or(1);
    if !(1..=100).contains(&interval_mult) {
        return Err(AppError::Validation(
            "Repeat interval must be between 1 and 100".to_string(),
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
                "End date cannot be before the start date".to_string(),
            ));
        }
    }
    if let Some(m) = input.max_occurrences {
        if !(1..=1200).contains(&m) {
            return Err(AppError::Validation(
                "Occurrence limit must be between 1 and 1200".to_string(),
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
    };
    crate::transactions::validate_tx_body_with(&probe, lookup)?;
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
    })
}

// ── Pointer helpers ──────────────────────────────────────────────────────────

/// Re-apply the end-date / occurrence-count caps to a candidate pointer.
/// Returns `None` when the schedule is completed.
fn apply_caps(
    next: Option<String>,
    max_occurrences: Option<i64>,
    occurrences_posted: i64,
    end_date: Option<&str>,
) -> Option<String> {
    if max_occurrences
        .map(|m| occurrences_posted >= m)
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
    occurrences_posted: i64,
    end_date: Option<&str>,
    start_date: &str,
) -> bool {
    if max_occurrences
        .map(|m| occurrences_posted >= m)
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

fn account_names(
    conn: &Connection,
    guids: &[String],
) -> Result<HashMap<String, String>, AppError> {
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

fn parse_template(template_json: &str) -> Result<StoredTemplate, AppError> {
    serde_json::from_str(template_json)
        .map_err(|e| AppError::Validation(format!("Corrupt schedule data: {e}")))
}

fn preview_amount(splits: &[StoredSplit]) -> f64 {
    splits
        .iter()
        .filter(|s| s.amount > 0.0)
        .map(|s| s.amount)
        .sum()
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
    Ok(Schedule {
        guid: row.guid,
        name: row.name,
        frequency: row.frequency,
        interval_mult: row.interval_mult,
        start_date: row.start_date,
        end_date: row.end_date,
        max_occurrences: row.max_occurrences,
        occurrences_posted: row.occurrences_posted,
        next_due_date: row.next_due_date,
        active: row.active,
        description: template.description,
        num: template.num,
        notes: template.notes,
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
        preview_amount: preview_amount(&template.splits),
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
            AppError::NotFound("Schedule not found — it may have been removed".to_string())
        })
}

fn fetch_schedule_inner(conn: &Connection, guid: &str) -> Result<Schedule, AppError> {
    to_schedule(conn, fetch_row(conn, guid)?)
}

// ── CRUD inners ──────────────────────────────────────────────────────────────

fn create_schedule_inner(
    conn: &Connection,
    input: ScheduleInput,
) -> Result<Schedule, AppError> {
    let v = validate_schedule_input(conn, &input)?;
    let guid = uuid::Uuid::new_v4().to_string();
    let created_at = utils::iso_now();
    let template_json = serde_json::to_string(&v.template)
        .map_err(|e| AppError::Validation(format!("Failed to encode schedule: {e}")))?;
    // The pointer starts verbatim at the start date (no catch-up roll-forward):
    // a backdated start simply surfaces as overdue for the user to post/skip.
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
    fetch_schedule_inner(conn, &guid)
}

fn update_schedule_inner(
    conn: &Connection,
    guid: &str,
    input: ScheduleInput,
) -> Result<Schedule, AppError> {
    let existing = fetch_row(conn, guid)?;
    let v = validate_schedule_input(conn, &input)?;
    // Pointer rules: never-posted schedules follow a start-date edit;
    // otherwise the pointer is untouched (a start edit must not rewrite
    // history-in-progress). Completed schedules restart at the new start when
    // the edit makes room again (extended cap/end). Caps always re-apply.
    let mut next = existing.next_due_date.clone();
    if existing.occurrences_posted == 0 {
        next = Some(v.start_date.clone());
    }
    if next.is_none()
        && has_room_again(
            v.max_occurrences,
            existing.occurrences_posted,
            v.end_date.as_deref(),
            &v.start_date,
        ) {
        next = Some(v.start_date.clone());
    }
    next = apply_caps(
        next,
        v.max_occurrences,
        existing.occurrences_posted,
        v.end_date.as_deref(),
    );
    let template_json = serde_json::to_string(&v.template)
        .map_err(|e| AppError::Validation(format!("Failed to encode schedule: {e}")))?;
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

// ── GnuCash import ───────────────────────────────────────────────────────────

/// One scheduled transaction read from a GnuCash source, ready to merge. The SX
/// GUID doubles as the Spendo schedule GUID so re-imports update in place.
pub(crate) struct ImportedSchedule {
    pub guid: String,
    pub input: ScheduleInput,
    pub active: bool,
    /// Occurrences the source book already generated: seeded on insert and used
    /// by the occurrence caps.
    pub occurrences_posted: i64,
    /// First pending occurrence in the source (`None` = source exhausted).
    pub next_due_date: Option<String>,
}

/// What an import merge did (or would do) to one schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScheduleMerge {
    Added,
    Updated,
    Unchanged,
}

fn fetch_row_optional(conn: &Connection, guid: &str) -> Result<Option<ScheduleRow>, AppError> {
    let sql = format!("SELECT {SELECT_COLS} FROM recurring_schedules WHERE guid = ?1");
    conn.query_row(&sql, params![guid], read_row)
        .optional()
        .map_err(AppError::from)
}

fn template_eq(a: &StoredTemplate, b: &StoredTemplate) -> bool {
    a.description == b.description
        && a.num == b.num
        && a.notes == b.notes
        && a.splits.len() == b.splits.len()
        && a.splits.iter().zip(&b.splits).all(|(x, y)| {
            x.account_guid == y.account_guid
                && (x.amount - y.amount).abs() <= f64::EPSILON
                && x.quantity == y.quantity
                && x.memo == y.memo
        })
}

/// Definition comparison for re-imports: `active` is deliberately local (a
/// pause in Spendo survives a re-import) and pointer/counters are handled by
/// [`import_next_due`].
fn definition_differs(row: &ScheduleRow, v: &ValidatedSchedule) -> Result<bool, AppError> {
    if row.name != v.name
        || row.frequency != v.frequency
        || row.interval_mult != v.interval_mult
        || row.start_date != v.start_date
        || row.end_date != v.end_date
        || row.max_occurrences != v.max_occurrences
    {
        return Ok(true);
    }
    Ok(!template_eq(&parse_template(&row.template_json)?, &v.template))
}

/// Pointer rule for re-imports: forward-only, never revive a locally completed
/// schedule, complete when the source is exhausted, and always re-apply caps.
fn import_next_due(
    existing_next: Option<&str>,
    imported_next: Option<&str>,
    max_occurrences: Option<i64>,
    occurrences_posted: i64,
    end_date: Option<&str>,
) -> Option<String> {
    let candidate = match (existing_next, imported_next) {
        (None, _) | (Some(_), None) => return None,
        (Some(e), Some(i)) => {
            if i > e {
                i.to_string()
            } else {
                e.to_string()
            }
        }
    };
    apply_caps(Some(candidate), max_occurrences, occurrences_posted, end_date)
}

fn schedule_merge_state(
    existing: Option<&ScheduleRow>,
    v: &ValidatedSchedule,
    imported: &ImportedSchedule,
) -> Result<ScheduleMerge, AppError> {
    let Some(row) = existing else {
        return Ok(ScheduleMerge::Added);
    };
    let next = import_next_due(
        row.next_due_date.as_deref(),
        imported.next_due_date.as_deref(),
        v.max_occurrences,
        row.occurrences_posted,
        v.end_date.as_deref(),
    );
    let changed = definition_differs(row, v)? || next != row.next_due_date;
    Ok(if changed {
        ScheduleMerge::Updated
    } else {
        ScheduleMerge::Unchanged
    })
}

/// Read-only classification of one imported schedule against the destination
/// (or, for the import preview, against a look-ahead account set). Mirrors
/// [`apply_imported_schedule`] exactly so preview counts match the merge.
pub(crate) fn plan_imported_schedule(
    conn: &Connection,
    imported: &ImportedSchedule,
    lookup: &dyn Fn(&str) -> Result<Option<crate::transactions::SplitAccountInfo>, AppError>,
) -> Result<ScheduleMerge, AppError> {
    let v = validate_schedule_input_with(&imported.input, lookup)?;
    let existing = fetch_row_optional(conn, &imported.guid)?;
    schedule_merge_state(existing.as_ref(), &v, imported)
}

/// Insert a source schedule or refresh the definition of the existing row with
/// the same GUID. Manual fields are preserved: `active` (local pause wins),
/// `occurrences_posted`, and a pointer that only moves forward.
pub(crate) fn apply_imported_schedule(
    conn: &Connection,
    imported: &ImportedSchedule,
) -> Result<ScheduleMerge, AppError> {
    let v = validate_schedule_input_with(&imported.input, &|guid| {
        crate::transactions::account_info_lookup(conn, guid)
    })?;
    let existing = fetch_row_optional(conn, &imported.guid)?;
    let state = schedule_merge_state(existing.as_ref(), &v, imported)?;
    if state == ScheduleMerge::Unchanged {
        return Ok(state);
    }
    let template_json = serde_json::to_string(&v.template)
        .map_err(|e| AppError::Validation(format!("Failed to encode schedule: {e}")))?;
    match existing {
        Some(row) => {
            let next = import_next_due(
                row.next_due_date.as_deref(),
                imported.next_due_date.as_deref(),
                v.max_occurrences,
                row.occurrences_posted,
                v.end_date.as_deref(),
            );
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
                    imported.guid,
                ],
            )?;
        }
        None => {
            let next = apply_caps(
                imported.next_due_date.clone(),
                v.max_occurrences,
                imported.occurrences_posted,
                v.end_date.as_deref(),
            );
            conn.execute(
                "INSERT INTO recurring_schedules
                  (guid, name, frequency, interval_mult, start_date, end_date,
                   max_occurrences, occurrences_posted, next_due_date, active,
                   template_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    imported.guid,
                    v.name,
                    v.frequency,
                    v.interval_mult,
                    v.start_date,
                    v.end_date,
                    v.max_occurrences,
                    imported.occurrences_posted,
                    next,
                    if imported.active { 1 } else { 0 },
                    template_json,
                    utils::iso_now(),
                ],
            )?;
        }
    }
    Ok(state)
}

/// Account GUIDs referenced by any schedule template. The GnuCash orphan sweep
/// uses this to keep accounts a schedule still needs (deleting them would leave
/// dangling template references, and the sweep is outside undo snapshots for
/// config tables).
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

fn schedule_tx_label(template: &StoredTemplate, schedule_name: &str) -> String {
    let base = template
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(schedule_name);
    utils::truncate_chars(base, 80)
}

/// Materialize the due occurrence as a real transaction and advance the
/// pointer atomically. The new transaction is an ordinary `tx_create` with
/// `source = "schedule"` (plus `scheduleGuid` in the detail JSON), so linear
/// undo keeps working for the money movement; the pointer move itself is not
/// revertible. Posting is manual-only: due dates are always ≤ today, so the
/// manual future-date block is never bypassed.
fn post_schedule_occurrence_inner(
    conn: &Connection,
    guid: &str,
) -> Result<String, AppError> {
    let row = fetch_row(conn, guid)?;
    if !row.active {
        return Err(AppError::Validation(
            "Schedule is paused — resume it before posting".to_string(),
        ));
    }
    let due = row.next_due_date.clone().ok_or_else(|| {
        AppError::Validation("Schedule is completed — no occurrences remain".to_string())
    })?;
    // Local "today": the frontend enables Post from its local date, so the
    // backend must use the same clock.
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if due > today {
        return Err(AppError::Validation(format!(
            "Not due yet — the next occurrence is {due}"
        )));
    }
    let template = parse_template(&row.template_json)?;
    let input = TxInput {
        post_date: due.clone(),
        description: template.description.clone(),
        num: template.num.clone(),
        notes: template.notes.clone(),
        splits: template
            .splits
            .iter()
            .map(|s| SplitInput {
                account_guid: s.account_guid.clone(),
                amount: s.amount,
                quantity: s.quantity,
                memo: s.memo.clone(),
            })
            .collect(),
    };
    // Full manual validation at posting time: accounts may have been deleted
    // or turned into placeholders since the schedule was created.
    let post_date = crate::transactions::validate_tx_input(conn, &input)?;
    let tx_guid = uuid::Uuid::new_v4().to_string();
    let accounts_affected = input
        .splits
        .iter()
        .map(|s| s.account_guid.clone())
        .collect::<HashSet<_>>()
        .len() as i64;
    let label = schedule_tx_label(&template, &row.name);
    let detail = serde_json::json!({
        "postDate": post_date,
        "scheduleGuid": guid,
        "scheduleName": row.name,
    })
    .to_string();

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
        let split_guids =
            crate::transactions::insert_split_rows(conn, &tx_guid, &input.splits)?;
        let mut snapshots = vec![history::Snapshot {
            table_name: "transactions",
            row_guid: tx_guid.clone(),
            before_json: None,
        }];
        for g in &split_guids {
            snapshots.push(history::Snapshot {
                table_name: "splits",
                row_guid: g.clone(),
                before_json: None,
            });
        }
        history::insert_change_entry(
            conn,
            "tx_create",
            &label,
            "schedule",
            1,
            accounts_affected,
            Some(&detail),
            &snapshots,
        )?;
        let posted = row.occurrences_posted + 1;
        let advanced =
            next_occurrence(parse_stored_date(&due)?, &row.frequency, row.interval_mult)?
                .format("%Y-%m-%d")
                .to_string();
        let next = apply_caps(
            Some(advanced),
            row.max_occurrences,
            posted,
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
            "Schedule is paused — resume it before skipping".to_string(),
        ));
    }
    let due = row.next_due_date.clone().ok_or_else(|| {
        AppError::Validation("Schedule is completed — no occurrences remain".to_string())
    })?;
    let advanced =
        next_occurrence(parse_stored_date(&due)?, &row.frequency, row.interval_mult)?
            .format("%Y-%m-%d")
            .to_string();
    let next = apply_caps(
        Some(advanced),
        row.max_occurrences,
        row.occurrences_posted,
        row.end_date.as_deref(),
    );
    conn.execute(
        "UPDATE recurring_schedules SET next_due_date = ?1 WHERE guid = ?2",
        params![next, guid],
    )?;
    Ok(next)
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_schedule(
    state: State<'_, AppState>,
    input: ScheduleInput,
) -> Result<Schedule, String> {
    run_blocking(state.db.clone(), move |conn| {
        create_schedule_inner(conn, input).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn update_schedule(
    state: State<'_, AppState>,
    guid: String,
    input: ScheduleInput,
) -> Result<Schedule, String> {
    run_blocking(state.db.clone(), move |conn| {
        update_schedule_inner(conn, &guid, input).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn delete_schedule(state: State<'_, AppState>, guid: String) -> Result<(), String> {
    run_blocking(state.db.clone(), move |conn| {
        delete_schedule_inner(conn, &guid).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn set_schedule_active(
    state: State<'_, AppState>,
    guid: String,
    active: bool,
) -> Result<Schedule, String> {
    run_blocking(state.db.clone(), move |conn| {
        set_schedule_active_inner(conn, &guid, active).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn fetch_schedules(state: State<'_, AppState>) -> Result<Vec<Schedule>, String> {
    run_blocking(state.db.clone(), |conn| {
        fetch_schedules_inner(conn).map_err(String::from)
    })
    .await
}

#[tauri::command]
pub async fn fetch_schedule(state: State<'_, AppState>, guid: String) -> Result<Schedule, String> {
    run_blocking(state.db.clone(), move |conn| {
        fetch_schedule_inner(conn, &guid).map_err(String::from)
    })
    .await
}

/// Post the due occurrence now. Returns the new transaction GUID so the
/// frontend can navigate to its detail page.
#[tauri::command]
pub async fn post_schedule_occurrence(
    state: State<'_, AppState>,
    guid: String,
) -> Result<String, String> {
    run_blocking(state.db.clone(), move |conn| {
        post_schedule_occurrence_inner(conn, &guid).map_err(String::from)
    })
    .await
}

/// Skip the due occurrence without posting. Returns the new pointer (`None`
/// when the skip completed the schedule).
#[tauri::command]
pub async fn skip_schedule_occurrence(
    state: State<'_, AppState>,
    guid: String,
) -> Result<Option<String>, String> {
    run_blocking(state.db.clone(), move |conn| {
        skip_schedule_occurrence_inner(conn, &guid).map_err(String::from)
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
        }
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn next_occurrence_handles_all_frequencies() {
        assert_eq!(next_occurrence(d("2024-01-01"), "daily", 1).unwrap(), d("2024-01-02"));
        assert_eq!(next_occurrence(d("2024-01-01"), "daily", 3).unwrap(), d("2024-01-04"));
        assert_eq!(next_occurrence(d("2024-01-01"), "weekly", 1).unwrap(), d("2024-01-08"));
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
    fn first_occurrence_aligns_to_gnucash_recurrence() {
        // Fixed strides for day/week (weekly mult 2 = the biweekly stride).
        assert_eq!(
            first_occurrence_on_or_after("2026-01-10", "2026-01-01", 2, "day").as_deref(),
            Some("2026-01-11")
        );
        assert_eq!(
            first_occurrence_on_or_after("2026-01-08", "2026-01-01", 2, "week").as_deref(),
            Some("2026-01-15")
        );
        // Monthly keeps the anchor's day across short months (Jan 31 → Feb 28
        // → Mar 31), unlike `next_occurrence` which clamps from the pointer.
        assert_eq!(
            first_occurrence_on_or_after("2026-02-01", "2026-01-31", 1, "month").as_deref(),
            Some("2026-02-28")
        );
        assert_eq!(
            first_occurrence_on_or_after("2026-03-01", "2026-01-31", 1, "month").as_deref(),
            Some("2026-03-31")
        );
        // A `from` before the anchor snaps to the anchor.
        assert_eq!(
            first_occurrence_on_or_after("2025-12-01", "2026-01-02", 1, "month").as_deref(),
            Some("2026-01-02")
        );
        // Unsupported GnuCash period types have no occurrence.
        assert_eq!(
            first_occurrence_on_or_after("2026-01-01", "2026-01-01", 1, "end of month"),
            None
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

        // Logged as schedule-sourced so history shows where it came from.
        let (kind, source): (String, String) = conn
            .query_row(
                "SELECT kind, source FROM change_log ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((kind.as_str(), source.as_str()), ("tx_create", "schedule"));

        // Pointer advanced one month, posted count bumped.
        let after = fetch_schedule_inner(&conn, &s.guid).unwrap();
        assert_eq!(after.next_due_date.as_deref(), Some("2024-02-01"));
        assert_eq!(after.occurrences_posted, 1);
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
        assert_eq!(skip_schedule_occurrence_inner(&conn, &s.guid).unwrap(), None);
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
    fn crud_leaves_no_history_entries() {
        // Schedule definitions stay out of `change_log` so the linear-undo
        // head is never hijacked by a no-op entry (only posted money moves).
        let conn = setup();
        let s = create_schedule_inner(&conn, balanced_input()).unwrap();
        update_schedule_inner(&conn, &s.guid, balanced_input()).unwrap();
        set_schedule_active_inner(&conn, &s.guid, false).unwrap();
        skip_schedule_occurrence_inner(&conn, &s.guid).unwrap_err();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM change_log", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
