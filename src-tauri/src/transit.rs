use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::db::{run_blocking, AppError, AppState};
use crate::transactions::VALUE_DENOM;
use crate::utils;

/// Two legs of one transfer through the clearing account must land within
/// this many days of each other to pair up (bank transfers take 1–4 days).
const MATCH_WINDOW_DAYS: i64 = 14;

/// A clearing-account leg with no opposite leg within the window: money that
/// left one account and hasn't been recorded arriving (or vice versa).
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenLeg {
    pub tx_guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub amount: f64,
    /// The other side of this transaction (its largest other split).
    pub counter_account: Option<String>,
    pub days_open: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitCheck {
    /// The `transit_account` book setting, when it names an existing account.
    pub account_guid: Option<String>,
    pub account_name: Option<String>,
    /// Oldest first.
    pub open_legs: Vec<OpenLeg>,
    /// Sum of the open legs: the clearing balance, since pairs cancel out.
    pub open_total: f64,
}

struct Leg {
    leg: OpenLeg,
    date: NaiveDate,
    /// Value in `VALUE_DENOM` units, so pairs compare exactly.
    value: i64,
}

/// Pair legs oldest first: each leg cancels the earliest still-open leg of
/// the opposite value at most `window` days older. What's left is open.
fn unmatched(legs: Vec<Leg>, window: i64) -> Vec<Leg> {
    let mut open: Vec<Leg> = Vec::new();
    for leg in legs {
        let partner = open
            .iter()
            .position(|o| o.value == -leg.value && (leg.date - o.date).num_days() <= window);
        match partner {
            Some(i) => {
                open.remove(i);
            }
            None => open.push(leg),
        }
    }
    open
}

fn fetch_transit_check_inner(
    conn: &Connection,
    today: NaiveDate,
) -> Result<TransitCheck, AppError> {
    let setting: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'transit_account'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let account: Option<(String, String)> = match setting.filter(|g| !g.is_empty()) {
        Some(guid) => conn
            .query_row(
                "SELECT guid, name FROM accounts WHERE guid = ?1",
                [&guid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?,
        None => None,
    };
    let Some((guid, name)) = account else {
        return Ok(TransitCheck {
            account_guid: None,
            account_name: None,
            open_legs: Vec::new(),
            open_total: 0.0,
        });
    };

    let sql = format!(
        "SELECT t.guid, substr(t.post_date, 1, 10), t.description,
                CAST(ROUND(s.value_num * {VALUE_DENOM}.0 / s.value_denom) AS INTEGER),
                (SELECT a.name FROM splits o JOIN accounts a ON a.guid = o.account_guid
                  WHERE o.tx_guid = s.tx_guid AND o.account_guid <> s.account_guid
                  ORDER BY abs(o.value_num * 1.0 / o.value_denom) DESC, o.guid LIMIT 1)
         FROM splits s JOIN transactions t ON t.guid = s.tx_guid
         WHERE s.account_guid = ?1 AND {}
         ORDER BY t.post_date, t.guid, s.guid",
        utils::VISIBLE_TX_FILTER
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![guid], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut legs = Vec::new();
    for row in rows {
        let (tx_guid, post_date, description, value, counter_account) = row?;
        let date = NaiveDate::parse_from_str(&post_date, "%Y-%m-%d")
            .map_err(|_| AppError::Validation(format!("Corrupt transaction date: {post_date}")))?;
        legs.push(Leg {
            leg: OpenLeg {
                tx_guid,
                post_date,
                description,
                amount: value as f64 / VALUE_DENOM as f64,
                counter_account,
                days_open: (today - date).num_days(),
            },
            date,
            value,
        });
    }
    let open = unmatched(legs, MATCH_WINDOW_DAYS);
    let open_total = open.iter().map(|l| l.value).sum::<i64>() as f64 / VALUE_DENOM as f64;
    Ok(TransitCheck {
        account_guid: Some(guid),
        account_name: Some(name),
        open_legs: open.into_iter().map(|l| l.leg).collect(),
        open_total,
    })
}

/// Unpaired transfer legs in the book's clearing account (`transit_account`
/// setting); empty with no account when the setting is off.
#[tauri::command]
pub async fn fetch_transit_check(state: State<'_, AppState>) -> Result<TransitCheck, AppError> {
    run_blocking(state.db.clone(), |conn| {
        fetch_transit_check_inner(conn, chrono::Local::now().date_naive())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_db;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (guid, name, account_type, placeholder)
               VALUES ('transit', 'Transit', 'CASH', 0),
                      ('bank', 'Santander', 'BANK', 0),
                      ('broker', 'IBKR cash', 'BANK', 0);
             INSERT INTO app_settings (key, value) VALUES ('transit_account', 'transit');",
        )
        .unwrap();
        conn
    }

    /// One leg of a transfer: `amount` into the transit account from `other`.
    fn leg(conn: &Connection, tx: &str, date: &str, amount_cents: i64, other: &str) {
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description) VALUES (?1, ?2, ?1)",
            params![tx, date],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom,
                                 quantity_num, quantity_denom)
             VALUES (?1 || '-t', ?1, 'transit', ?2, 100, ?2, 100),
                    (?1 || '-o', ?1, ?3, -?2, 100, -?2, 100)",
            params![tx, amount_cents, other],
        )
        .unwrap();
    }

    #[test]
    fn pairs_legs_within_the_window() {
        let conn = setup();
        // Paired: out of the bank on the 31st, into the broker on the 4th.
        leg(&conn, "out-1", "2026-07-31", 145_430, "bank");
        leg(&conn, "in-1", "2026-08-04", -145_430, "broker");
        // Imported legs carry GnuCash timestamps; they pair the same way.
        leg(&conn, "out-2", "2026-08-31 10:59:00", 102_249, "bank");
        leg(&conn, "in-2", "2026-09-01 10:59:00", -102_249, "broker");
        // Open: sent, never recorded arriving.
        leg(&conn, "out-3", "2026-09-30", 113_041, "bank");
        // Too far apart to be one transfer.
        leg(&conn, "out-4", "2024-09-15", 100, "bank");
        leg(&conn, "in-4", "2025-02-01", -100, "broker");

        let check = fetch_transit_check_inner(&conn, d("2026-10-24")).unwrap();
        assert_eq!(check.account_name.as_deref(), Some("Transit"));
        let open: Vec<&str> = check.open_legs.iter().map(|l| l.tx_guid.as_str()).collect();
        assert_eq!(open, ["out-4", "in-4", "out-3"]);
        let out3 = &check.open_legs[2];
        assert_eq!(out3.amount, 1130.41);
        assert_eq!(out3.counter_account.as_deref(), Some("Santander"));
        assert_eq!(out3.days_open, 24);
        assert_eq!(check.open_total, 1130.41);
    }

    #[test]
    fn each_leg_pairs_only_once() {
        let conn = setup();
        leg(&conn, "a", "2026-09-01", 5_000, "bank");
        leg(&conn, "b", "2026-09-01", 5_000, "bank");
        leg(&conn, "c", "2026-09-02", -5_000, "broker");
        let check = fetch_transit_check_inner(&conn, d("2026-09-03")).unwrap();
        assert_eq!(check.open_legs.len(), 1);
        assert_eq!(check.open_total, 50.0);
    }

    #[test]
    fn future_legs_and_unset_setting_are_ignored() {
        let conn = setup();
        leg(&conn, "future", "2099-01-01", 5_000, "bank");
        let check = fetch_transit_check_inner(&conn, d("2026-09-03")).unwrap();
        assert!(check.open_legs.is_empty());

        conn.execute(
            "UPDATE app_settings SET value = '' WHERE key = 'transit_account'",
            [],
        )
        .unwrap();
        let off = fetch_transit_check_inner(&conn, d("2026-09-03")).unwrap();
        assert_eq!(off.account_guid, None);
    }
}
