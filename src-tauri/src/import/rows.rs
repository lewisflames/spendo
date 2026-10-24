use super::*;

// ── Row structs + Importable impls ──────────────────────────────────────────

#[derive(Clone)]
pub(super) struct Commodity {
    pub(super) guid: String,
    pub(super) mnemonic: String,
    pub(super) namespace: String,
    pub(super) fullname: Option<String>,
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

    fn upsert(&self, conn: &Connection) -> Result<(), AppError> {
        conn.prepare_cached(
			"INSERT INTO commodities (guid, mnemonic, namespace, fullname) VALUES (?1, ?2, ?3, ?4)
			 ON CONFLICT(guid) DO UPDATE SET mnemonic=excluded.mnemonic, namespace=excluded.namespace, fullname=excluded.fullname",
		)?
		.execute(rusqlite::params![self.guid, self.mnemonic, self.namespace, self.fullname])?;
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct Account {
    pub(super) guid: String,
    pub(super) name: String,
    pub(super) account_type: String,
    pub(super) parent_guid: Option<String>,
    pub(super) code: Option<String>,
    pub(super) description: Option<String>,
    pub(super) placeholder: i64,
    pub(super) commodity_guid: Option<String>,
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

    fn upsert(&self, conn: &Connection) -> Result<(), AppError> {
        conn.prepare_cached(
			"INSERT INTO accounts (guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
			 ON CONFLICT(guid) DO UPDATE SET name=excluded.name, account_type=excluded.account_type, parent_guid=excluded.parent_guid, code=excluded.code, description=excluded.description, placeholder=excluded.placeholder, commodity_guid=excluded.commodity_guid",
		)?
		.execute(rusqlite::params![self.guid, self.name, self.account_type, self.parent_guid, self.code, self.description, self.placeholder, self.commodity_guid])?;
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct Transaction {
    pub(super) guid: String,
    pub(super) post_date: String,
    pub(super) description: Option<String>,
    pub(super) num: Option<String>,
    /// Free-form note from the source book's `slots` table (GnuCash stores
    /// transaction notes as a 'notes' string slot on the tx GUID).
    pub(super) notes: Option<String>,
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

    fn upsert(&self, conn: &Connection) -> Result<(), AppError> {
        conn.prepare_cached(
            "INSERT INTO transactions (guid, post_date, description, num, notes) VALUES (?1, ?2, ?3, ?4, ?5)
			 ON CONFLICT(guid) DO UPDATE SET post_date=excluded.post_date, description=excluded.description, num=excluded.num, notes=excluded.notes",
        )?
        .execute(rusqlite::params![
            self.guid,
            self.post_date,
            self.description,
            self.num,
            self.notes
        ])?;
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct Split {
    pub(super) guid: String,
    pub(super) tx_guid: String,
    pub(super) account_guid: String,
    pub(super) value_num: i64,
    pub(super) value_denom: i64,
    pub(super) quantity_num: i64,
    pub(super) quantity_denom: i64,
    pub(super) reconcile_state: Option<String>,
    pub(super) memo: Option<String>,
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

    fn upsert(&self, conn: &Connection) -> Result<(), AppError> {
        let reconciled = self
            .reconcile_state
            .as_deref()
            .is_some_and(|s| s == "c" || s == "y");
        conn.prepare_cached(
			"INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconciled, reconcile_state, memo) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
			 ON CONFLICT(guid) DO UPDATE SET tx_guid=excluded.tx_guid, account_guid=excluded.account_guid, value_num=excluded.value_num, value_denom=excluded.value_denom, quantity_num=excluded.quantity_num, quantity_denom=excluded.quantity_denom, reconciled=excluded.reconciled, reconcile_state=excluded.reconcile_state, memo=excluded.memo",
		)?
		.execute(rusqlite::params![self.guid, self.tx_guid, self.account_guid, self.value_num, self.value_denom, self.quantity_num, self.quantity_denom, reconciled as i64, self.reconcile_state, self.memo])?;
        Ok(())
    }
}

// ── UUID detection ──────────────────────────────────────────────────────────

pub(super) fn is_uuid(s: &str) -> bool {
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

pub(super) fn read_commodity(conn: &Connection) -> Result<Vec<Commodity>, AppError> {
    let mut stmt = conn.prepare("SELECT guid, mnemonic, namespace, fullname FROM commodities")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Commodity {
                guid: row.get(0)?,
                mnemonic: row.get(1)?,
                namespace: row.get(2)?,
                fullname: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub(super) fn read_account(conn: &Connection) -> Result<Vec<Account>, AppError> {
    let mut stmt = conn
		.prepare(
			"SELECT guid, name, account_type, parent_guid, code, description, placeholder, commodity_guid FROM accounts",
		)?;
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
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// App-DB loader (also used for change detection during merge).
pub(super) fn read_transaction(conn: &Connection) -> Result<Vec<Transaction>, AppError> {
    let mut stmt =
        conn.prepare("SELECT guid, post_date, description, num, notes FROM transactions")?;
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
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Normalize a source-book `post_date` to the date-only `YYYY-MM-DD` form
/// Spendo writes itself, so imported and hand-entered rows sort and compare
/// alike. GnuCash stores a UTC timestamp (`YYYY-MM-DD HH:MM:SS`, or the
/// compact `YYYYMMDDHHMMSS` some producers write): modern books use 10:59 UTC
/// so the day survives any timezone, older ones store local midnight in UTC
/// (`2024-01-14 23:00:00` for the 15th in CET), so the instant is converted to
/// the local calendar day rather than truncated. Anything not composed of
/// date/time characters is passed through unchanged.
pub(super) fn normalize_post_date(raw: &str) -> String {
    normalize_post_date_in(raw, &chrono::Local)
}

pub(super) fn normalize_post_date_in<Tz: chrono::TimeZone>(raw: &str, tz: &Tz) -> String {
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
    // Remaining pairs are HH, MM, SS; short tails (hour or hour+minute) imply
    // `:00`. Odd-length, over-long or out-of-range tails pass through for the
    // caller to surface.
    let tail = &digits[8..];
    if !tail.len().is_multiple_of(2) || !(2..=6).contains(&tail.len()) {
        return clean.to_string();
    }
    let mut time = tail.to_string();
    while time.len() < 6 {
        time.push_str("00");
    }
    match chrono::NaiveDateTime::parse_from_str(&format!("{date}{time}"), "%Y-%m-%d%H%M%S") {
        Ok(utc) => utc
            .and_utc()
            .with_timezone(tz)
            .date_naive()
            .format("%Y-%m-%d")
            .to_string(),
        Err(_) => clean.to_string(),
    }
}

/// Source-book loader: joins GnuCash's `slots` table, where transaction notes
/// live ('notes' string slot keyed by the tx GUID).
pub(super) fn read_source_transaction(source: &Connection) -> Result<Vec<Transaction>, AppError> {
    let mut stmt = source.prepare(
        "SELECT t.guid, t.post_date, t.description, t.num,
                    (SELECT s.string_val FROM slots s
                     WHERE s.obj_guid = t.guid AND s.name = 'notes' LIMIT 1) as notes
             FROM transactions t",
    )?;
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
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub(super) fn read_split(conn: &Connection) -> Result<Vec<Split>, AppError> {
    let mut stmt = conn
		.prepare(
			"SELECT guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom, reconcile_state, memo FROM splits",
		)?;
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
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub(super) fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|v| !v.is_empty())
}
