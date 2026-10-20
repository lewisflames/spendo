use super::*;

// ── Source scheduled transactions (GnuCash SX → Spendo schedules) ───────────
//
// GnuCash keeps a scheduled transaction's template in the regular account and
// transaction tables: the SX points at a template account (a child of the
// book's template root, named after the SX GUID), whose splits are template
// rows carrying the real account/amount in `sched-xaction/*` KVP slots. The
// rows below are read for conversion and then excluded from the ordinary
// entity merge.

/// One row of the source `schedxactions` table.
pub(super) struct SourceSchedule {
    pub(super) guid: String,
    pub(super) name: String,
    pub(super) enabled: bool,
    pub(super) start_date: Option<String>,
    pub(super) end_date: Option<String>,
    pub(super) last_occur: Option<String>,
    pub(super) num_occur: i64,
    pub(super) rem_occur: i64,
    pub(super) instance_count: i64,
    pub(super) template_act_guid: String,
    pub(super) recurrences: Vec<SourceRecurrence>,
}

/// One row of the source `recurrences` table belonging to an SX.
pub(super) struct SourceRecurrence {
    pub(super) mult: i64,
    pub(super) period_type: String,
    pub(super) period_start: Option<String>,
}

/// `sched-xaction/*` KVP data of one template split.
#[derive(Default, Clone)]
pub(super) struct TemplateSlot {
    pub(super) account_guid: Option<String>,
    pub(super) debit_numeric: Option<f64>,
    pub(super) credit_numeric: Option<f64>,
    pub(super) debit_formula: Option<String>,
    pub(super) credit_formula: Option<String>,
    pub(super) shares: Option<f64>,
}

impl TemplateSlot {
    /// GnuCash's template amount is `debit − credit`; numeric slots are the
    /// evaluated values and formula strings cover books written by versions
    /// that only stored expressions. `Err` when only an unevaluable expression
    /// is available.
    fn amount(&self) -> Result<f64, ()> {
        Ok(
            slot_value(self.debit_numeric, self.debit_formula.as_deref())?
                - slot_value(self.credit_numeric, self.credit_formula.as_deref())?,
        )
    }
}

pub(super) fn slot_value(numeric: Option<f64>, formula: Option<&str>) -> Result<f64, ()> {
    if let Some(n) = numeric {
        return Ok(n);
    }
    match formula.map(str::trim) {
        None | Some("") => Ok(0.0),
        Some(f) => f.parse::<f64>().map_err(|_| ()),
    }
}

pub(super) fn table_exists(conn: &Connection, name: &str) -> Result<bool, AppError> {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .map_err(AppError::from)
}

pub(super) fn table_has_column(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, AppError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names.iter().any(|n| n == column))
}

/// GnuCash gdates are `YYYYMMDD` text (ISO is tolerated); NULL/empty means unset.
pub(super) fn gdate_to_iso(raw: Option<String>) -> Option<String> {
    let digits: String = raw?.trim().chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 8 {
        return None;
    }
    let iso = format!("{}-{}-{}", &digits[..4], &digits[4..6], &digits[6..8]);
    utils::validate_iso_date(&iso).ok().map(|_| iso)
}

/// Source SX rows with their recurrence rules. Sources without the SX tables
/// (hand-made or older fixtures) simply have no schedules.
pub(super) fn read_source_schedules(source: &Connection) -> Result<Vec<SourceSchedule>, AppError> {
    if !table_exists(source, "schedxactions")? || !table_exists(source, "recurrences")? {
        return Ok(Vec::new());
    }
    let mut stmt = source.prepare(
        "SELECT guid, name, enabled, start_date, end_date, last_occur,
                    num_occur, rem_occur, instance_count, template_act_guid
             FROM schedxactions",
    )?;
    let mut schedules = stmt
        .query_map([], |row| {
            Ok(SourceSchedule {
                guid: row.get(0)?,
                name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                enabled: row.get::<_, i64>(2)? != 0,
                start_date: gdate_to_iso(row.get(3)?),
                end_date: gdate_to_iso(row.get(4)?),
                last_occur: gdate_to_iso(row.get(5)?),
                num_occur: row.get(6)?,
                rem_occur: row.get(7)?,
                instance_count: row.get(8)?,
                template_act_guid: row.get(9)?,
                recurrences: Vec::new(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if schedules.is_empty() {
        return Ok(schedules);
    }

    let index: HashMap<String, usize> = schedules
        .iter()
        .enumerate()
        .map(|(i, s)| (s.guid.clone(), i))
        .collect();
    let mut stmt = source.prepare(
        "SELECT obj_guid, recurrence_mult, recurrence_period_type, recurrence_period_start
             FROM recurrences",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            gdate_to_iso(row.get(3)?),
        ))
    })?;
    for row in rows {
        let (obj_guid, mult, period_type, period_start) = row?;
        if let Some(&i) = index.get(&obj_guid) {
            schedules[i].recurrences.push(SourceRecurrence {
                mult,
                period_type,
                period_start,
            });
        }
    }
    Ok(schedules)
}

/// Template-split KVP slots, keyed by split GUID. GnuCash stores a KVP frame
/// as a row `(obj_guid = split, name = 'sched-xaction', slot_type = FRAME,
/// guid_val = frame GUID)` whose children reference the synthetic frame GUID;
/// this walks that indirection. Books whose `slots` table lacks the KVP
/// columns (fixtures) yield no slots.
pub(super) fn read_template_slots(
    source: &Connection,
) -> Result<HashMap<String, TemplateSlot>, AppError> {
    let mut slots: HashMap<String, TemplateSlot> = HashMap::new();
    if !table_exists(source, "slots")?
        || !table_has_column(source, "slots", "guid_val")?
        || !table_has_column(source, "slots", "numeric_val_num")?
    {
        return Ok(slots);
    }

    // FRAME rows map the synthetic frame GUID back to the owning split.
    // `slot_type = 9` is KvpValue::Type::FRAME.
    let mut frame_to_split: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = source.prepare(
            "SELECT obj_guid, guid_val FROM slots WHERE name = 'sched-xaction' AND slot_type = 9",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        })?;
        for row in rows {
            let (split, frame) = row?;
            if let (Some(split), Some(frame)) = (split, frame) {
                frame_to_split.insert(frame, split);
            }
        }
    }
    if frame_to_split.is_empty() {
        return Ok(slots);
    }

    let mut stmt = source.prepare(
        "SELECT obj_guid, name, guid_val, string_val, numeric_val_num, numeric_val_denom
             FROM slots
             WHERE name IN ('sched-xaction/account', 'sched-xaction/credit-formula',
                            'sched-xaction/debit-formula', 'sched-xaction/credit-numeric',
                            'sched-xaction/debit-numeric', 'sched-xaction/shares')",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
        ))
    })?;
    for row in rows {
        let (obj_guid, name, guid_val, string_val, num, denom) = row?;
        let split_guid = frame_to_split.get(&obj_guid).cloned().unwrap_or(obj_guid);
        let slot = slots.entry(split_guid).or_default();
        let numeric = match (num, denom) {
            (Some(n), Some(d)) if d != 0 => Some(n as f64 / d as f64),
            _ => None,
        };
        match name.as_str() {
            "sched-xaction/account" => slot.account_guid = guid_val,
            "sched-xaction/credit-formula" => slot.credit_formula = string_val,
            "sched-xaction/debit-formula" => slot.debit_formula = string_val,
            "sched-xaction/credit-numeric" => slot.credit_numeric = numeric,
            "sched-xaction/debit-numeric" => slot.debit_numeric = numeric,
            "sched-xaction/shares" => {
                slot.shares = string_val
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .or(numeric);
            }
            _ => {}
        }
    }
    Ok(slots)
}

/// Latest date an SX already generated a transaction for it (the
/// `from-sched-xaction` slot GnuCash writes on generated transactions). The
/// generated transactions themselves import as ordinary rows, so the schedule
/// pointer must start after them.
pub(super) fn read_last_instance_dates(
    source: &Connection,
) -> Result<HashMap<String, String>, AppError> {
    let mut dates: HashMap<String, String> = HashMap::new();
    if !table_exists(source, "slots")?
        || !table_has_column(source, "slots", "guid_val")?
        || !table_exists(source, "transactions")?
    {
        return Ok(dates);
    }
    let mut stmt = source.prepare(
        "SELECT sl.guid_val, t.post_date FROM slots sl
             JOIN transactions t ON t.guid = sl.obj_guid
             WHERE sl.name = 'from-sched-xaction'",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
        ))
    })?;
    for row in rows {
        let (guid, post_date) = row?;
        let (Some(guid), Some(post_date)) = (guid, post_date) else {
            continue;
        };
        let normalized = normalize_post_date(post_date.trim());
        let date: String = normalized.chars().take(10).collect();
        if date.len() != 10 || date.as_bytes()[4] != b'-' {
            continue;
        }
        dates
            .entry(guid)
            .and_modify(|d| {
                if date > *d {
                    *d = date.clone();
                }
            })
            .or_insert(date);
    }
    Ok(dates)
}

/// Accounts that are template plumbing: the template root recorded in `books`
/// (or its fixed name for books without the `books` table) plus every
/// descendant, including the per-SX template accounts.
pub(super) fn excluded_template_accounts(
    source: &Connection,
    accounts: &[Account],
    schedules: &[SourceSchedule],
) -> Result<HashSet<String>, AppError> {
    let mut excluded: HashSet<String> = schedules
        .iter()
        .map(|s| s.template_act_guid.clone())
        .collect();

    let mut found_root = false;
    if table_exists(source, "books")? && table_has_column(source, "books", "root_template_guid")? {
        let root: Option<String> = source
            .query_row("SELECT root_template_guid FROM books LIMIT 1", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .optional()?
            .flatten();
        if let Some(root) = root {
            excluded.insert(root);
            found_root = true;
        }
    }
    if !found_root {
        // GnuCash's fixed template-root name; the fallback only fires for
        // sources without a `books` table (hand-made fixtures).
        for a in accounts {
            if a.account_type == "ROOT" && a.name == "Template Root" {
                excluded.insert(a.guid.clone());
            }
        }
    }

    // Close over descendants so orphaned template accounts come along too.
    loop {
        let before = excluded.len();
        for a in accounts {
            if let Some(parent) = &a.parent_guid {
                if excluded.contains(parent) {
                    excluded.insert(a.guid.clone());
                }
            }
        }
        if excluded.len() == before {
            break;
        }
    }
    Ok(excluded)
}

pub(super) fn build_import_schedules(
    source_schedules: &[SourceSchedule],
    splits: &[Split],
    transactions: &[Transaction],
    slots: &HashMap<String, TemplateSlot>,
    last_instance_dates: &HashMap<String, String>,
) -> (Vec<ImportedSchedule>, Vec<SkippedSchedule>) {
    let txs: HashMap<&str, &Transaction> =
        transactions.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for sx in source_schedules {
        match build_import_schedule(sx, splits, &txs, slots, last_instance_dates) {
            Ok(schedule) => imported.push(schedule),
            Err(reason) => skipped.push(SkippedSchedule {
                name: sx.name.trim().to_string(),
                reason,
            }),
        }
    }
    (imported, skipped)
}

pub(super) fn build_import_schedule(
    sx: &SourceSchedule,
    splits: &[Split],
    txs: &HashMap<&str, &Transaction>,
    slots: &HashMap<String, TemplateSlot>,
    last_instance_dates: &HashMap<String, String>,
) -> Result<ImportedSchedule, String> {
    let template_splits: Vec<&Split> = splits
        .iter()
        .filter(|s| s.account_guid == sx.template_act_guid)
        .collect();

    // The template transaction carries the posted transaction's description,
    // number and notes.
    let tx = template_splits
        .first()
        .and_then(|s| txs.get(s.tx_guid.as_str()));
    let description = tx.and_then(|t| t.description.clone());
    let name = {
        let raw = sx.name.trim();
        let base = if raw.is_empty() {
            description.as_deref().unwrap_or("")
        } else {
            raw
        };
        utils::truncate_chars(base, 200)
    };
    if name.is_empty() {
        return Err("Missing a name".to_string());
    }

    if sx.recurrences.len() != 1 {
        return Err(if sx.recurrences.is_empty() {
            "no recurrence rule".to_string()
        } else {
            "more than one recurrence rule".to_string()
        });
    }
    let rec = &sx.recurrences[0];
    let (frequency, interval_mult) = match rec.period_type.as_str() {
        "day" => ("daily", rec.mult),
        "week" if rec.mult == 2 => ("biweekly", 1),
        "week" => ("weekly", rec.mult),
        "month" if rec.mult == 3 => ("quarterly", 1),
        "month" => ("monthly", rec.mult),
        "year" => ("yearly", rec.mult),
        other => return Err(format!("That repeat pattern isn't supported yet ({other})")),
    };
    if !(1..=100).contains(&interval_mult) {
        return Err("Repeats every 1 to 100 times".to_string());
    }

    let start_date = sx
        .start_date
        .clone()
        .or_else(|| rec.period_start.clone())
        .ok_or_else(|| "missing start date".to_string())?;
    let end_date = sx.end_date.clone();
    let max_occurrences = if sx.num_occur > 0 {
        Some(sx.num_occur)
    } else {
        None
    };
    let occurrences_posted = if sx.num_occur > 0 {
        (sx.num_occur - sx.rem_occur).max(0)
    } else {
        sx.instance_count.max(0)
    };

    // First pending occurrence: strictly after the last instance the source
    // generated, its `last_occur` marker and its start date. GnuCash's own
    // recurrence alignment is used here; Spendo then advances with clamped
    // month math, so monthly schedules anchored on the 29th–31st can drift
    // after a short month (a known representation limit).
    let anchor = rec.period_start.as_deref().unwrap_or(&start_date);
    let mut from = start_date.clone();
    let latest = match (
        sx.last_occur.as_deref(),
        last_instance_dates.get(&sx.guid).map(String::as_str),
    ) {
        (Some(a), Some(b)) => Some(if a >= b { a } else { b }),
        (a, b) => a.or(b),
    };
    if let Some(latest) = latest {
        if latest >= from.as_str() {
            if let Some(day_after) = next_day(latest) {
                from = day_after;
            }
        }
    }
    let next_due_date =
        schedules::first_occurrence_on_or_after(&from, anchor, rec.mult, &rec.period_type);
    let next_due_date = next_due_date.filter(|d| {
        let exhausted = sx.num_occur > 0 && sx.rem_occur <= 0;
        let after_end = end_date.as_deref().is_some_and(|e| d.as_str() > e);
        !exhausted && !after_end
    });

    let mut template = Vec::with_capacity(template_splits.len());
    for s in &template_splits {
        let Some(slot) = slots.get(&s.guid) else {
            return Err("A split is missing its account".to_string());
        };
        let Some(account_guid) = slot.account_guid.clone() else {
            return Err("A split is missing its account".to_string());
        };
        let amount = slot
            .amount()
            .map_err(|_| "template uses formulas Spendo can't evaluate".to_string())?;
        template.push(SplitInput {
            account_guid,
            amount,
            quantity: slot.shares.filter(|q| q.is_finite()),
            memo: s.memo.as_deref().map(|m| utils::truncate_chars(m, 500)),
        });
    }

    Ok(ImportedSchedule {
        guid: sx.guid.clone(),
        input: ScheduleInput {
            name,
            frequency: frequency.to_string(),
            interval_mult: Some(interval_mult),
            start_date,
            end_date,
            max_occurrences,
            description: description.map(|d| utils::truncate_chars(&d, 500)),
            num: tx
                .and_then(|t| t.num.clone())
                .map(|n| utils::truncate_chars(&n, 100)),
            notes: tx
                .and_then(|t| t.notes.clone())
                .map(|n| utils::truncate_chars(&n, 2000)),
            splits: template,
        },
        active: sx.enabled,
        occurrences_posted,
        next_due_date,
    })
}

pub(super) fn next_day(iso: &str) -> Option<String> {
    chrono::NaiveDate::parse_from_str(iso, "%Y-%m-%d")
        .ok()?
        .checked_add_days(chrono::Days::new(1))
        .map(|d| d.format("%Y-%m-%d").to_string())
}
