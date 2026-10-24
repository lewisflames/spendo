//! Dividends attributed to holdings. A transaction with INCOME splits pays a
//! dividend to a commodity when either
//!
//! - it also touches that commodity's investment accounts with a zero-amount
//!   split (GnuCash's convention) or a share purchase (reinvestment), and with
//!   no sale or value-only move (those are realized-gain bookings), or
//! - it has no investment splits and posts to an income account linked to
//!   the commodity in `dividend_links`.
//!
//! Amounts are net of the transaction's EXPENSE splits (withholding tax), in
//! the transaction's currency; callers convert.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tauri::State;

use crate::db::{run_blocking, with_tx_err, AppError, AppState};
use crate::utils;

/// Share amounts below this are treated as zero.
const SHARE_EPS: f64 = 1e-9;

/// Money amounts below this count as zero (link splits, dust dividends).
const VALUE_EPS: f64 = 0.005;

/// One transaction's net effect on one commodity's investment accounts.
pub struct TradeLeg {
    pub tx_guid: String,
    pub commodity_guid: String,
    pub shares: f64,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DividendEvent {
    pub commodity_guid: String,
    pub tx_guid: String,
    pub post_date: String,
    pub description: Option<String>,
    pub amount: f64,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DividendAccount {
    pub account_guid: String,
    pub account_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DividendLinksInput {
    pub commodity_guid: String,
    pub account_guids: Vec<String>,
}

/// Income postings of one transaction, per income account.
struct IncomeTx {
    post_date: String,
    description: Option<String>,
    /// Income account → amount received (credits read positive).
    by_account: Vec<(String, f64)>,
}

/// Every dividend in the book, oldest first. `legs` are the investment
/// events netted per (transaction, commodity), as the portfolio loads them.
pub fn detect(conn: &Connection, legs: &[TradeLeg]) -> Result<Vec<DividendEvent>, AppError> {
    let income = load_income_txs(conn)?;
    let expenses = load_income_tx_expenses(conn)?;
    let links = load_links(conn)?;

    let mut legs_by_tx: HashMap<&str, Vec<&TradeLeg>> = HashMap::new();
    for leg in legs {
        legs_by_tx.entry(&leg.tx_guid).or_default().push(leg);
    }

    let mut out = Vec::new();
    for (tx_guid, tx) in &income {
        let total_income: f64 = tx.by_account.iter().map(|(_, a)| a).sum();
        let expense = expenses.get(tx_guid).copied().unwrap_or(0.0);
        // Commodity → share of the transaction's income it receives.
        let shares: Vec<(String, f64)> = match legs_by_tx.get(tx_guid.as_str()) {
            Some(tx_legs) => {
                let is_link =
                    |l: &TradeLeg| l.shares.abs() < SHARE_EPS && l.value.abs() < VALUE_EPS;
                let is_reinvest = |l: &TradeLeg| l.shares >= SHARE_EPS;
                if !tx_legs.iter().all(|l| is_link(l) || is_reinvest(l)) {
                    continue;
                }
                let mut commodities: Vec<&str> =
                    tx_legs.iter().map(|l| l.commodity_guid.as_str()).collect();
                commodities.sort_unstable();
                commodities.dedup();
                let n = commodities.len() as f64;
                commodities
                    .into_iter()
                    .map(|c| (c.to_string(), total_income / n))
                    .collect()
            }
            None => {
                let mut by_commodity: HashMap<&str, f64> = HashMap::new();
                for (account, amount) in &tx.by_account {
                    if let Some(commodity) = links.get(account) {
                        *by_commodity.entry(commodity).or_default() += amount;
                    }
                }
                by_commodity
                    .into_iter()
                    .map(|(c, a)| (c.to_string(), a))
                    .collect()
            }
        };
        for (commodity_guid, gross) in shares {
            // Withholding shared in proportion to the income it applies to.
            let withheld = if total_income.abs() >= VALUE_EPS {
                expense * gross / total_income
            } else {
                0.0
            };
            let amount = gross - withheld;
            if amount.abs() < VALUE_EPS {
                continue;
            }
            out.push(DividendEvent {
                commodity_guid,
                tx_guid: tx_guid.clone(),
                post_date: tx.post_date.clone(),
                description: tx.description.clone(),
                amount,
            });
        }
    }
    out.sort_by(|a, b| {
        a.post_date
            .cmp(&b.post_date)
            .then_with(|| a.tx_guid.cmp(&b.tx_guid))
            .then_with(|| a.commodity_guid.cmp(&b.commodity_guid))
    });
    Ok(out)
}

fn load_income_txs(conn: &Connection) -> Result<HashMap<String, IncomeTx>, AppError> {
    let sql = format!(
        "SELECT s.tx_guid, t.post_date, t.description, s.account_guid, -SUM({})
		 FROM splits s
		 JOIN accounts a ON a.guid = s.account_guid
		 JOIN transactions t ON t.guid = s.tx_guid
		 WHERE a.account_type = 'INCOME'
		   AND {}
		 GROUP BY s.tx_guid, s.account_guid",
        utils::RAW_AMOUNT,
        utils::VISIBLE_TX_FILTER
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
        ))
    })?;
    let mut out: HashMap<String, IncomeTx> = HashMap::new();
    for row in rows {
        let (tx_guid, post_date, description, account_guid, amount) = row?;
        out.entry(tx_guid)
            .or_insert_with(|| IncomeTx {
                post_date: post_date.get(..10).unwrap_or(&post_date).to_string(),
                description,
                by_account: vec![],
            })
            .by_account
            .push((account_guid, amount));
    }
    Ok(out)
}

/// EXPENSE totals of transactions that carry income (withholding tax).
fn load_income_tx_expenses(conn: &Connection) -> Result<HashMap<String, f64>, AppError> {
    let sql = format!(
        "SELECT s.tx_guid, SUM({})
		 FROM splits s
		 JOIN accounts a ON a.guid = s.account_guid
		 WHERE a.account_type = 'EXPENSE'
		   AND s.tx_guid IN (
		     SELECT s2.tx_guid FROM splits s2
		     JOIN accounts a2 ON a2.guid = s2.account_guid
		     WHERE a2.account_type = 'INCOME'
		   )
		 GROUP BY s.tx_guid",
        utils::RAW_AMOUNT
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(rows)
}

/// Income account → linked commodity.
fn load_links(conn: &Connection) -> Result<HashMap<String, String>, AppError> {
    let mut stmt = conn.prepare("SELECT account_guid, commodity_guid FROM dividend_links")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(rows)
}

/// Accounts carrying a dividend link, which the orphan sweep and account
/// deletion must not drop through the FK cascade.
pub(crate) fn linked_account_guids(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT account_guid FROM dividend_links")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Income accounts linked to one commodity, by name.
pub(crate) fn linked_accounts(
    conn: &Connection,
    commodity_guid: &str,
) -> Result<Vec<DividendAccount>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT a.guid, a.name
		 FROM dividend_links l
		 JOIN accounts a ON a.guid = l.account_guid
		 WHERE l.commodity_guid = ?1
		 ORDER BY a.name, a.guid",
    )?;
    let rows = stmt
        .query_map([commodity_guid], |row| {
            Ok(DividendAccount {
                account_guid: row.get(0)?,
                account_name: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Replace the income accounts linked to one commodity.
fn set_dividend_links_inner(
    conn: &Connection,
    input: &DividendLinksInput,
) -> Result<Vec<DividendAccount>, AppError> {
    let commodity: Option<String> = conn
        .query_row(
            "SELECT guid FROM commodities WHERE guid = ?1",
            [&input.commodity_guid],
            |row| row.get(0),
        )
        .optional()?;
    if commodity.is_none() {
        return Err(AppError::NotFound(
            "That holding no longer exists".to_string(),
        ));
    }

    let mut seen = HashSet::new();
    let guids: Vec<&String> = input
        .account_guids
        .iter()
        .filter(|g| seen.insert(g.as_str()))
        .collect();
    for guid in &guids {
        let account: Option<(String, String, i64)> = conn
            .query_row(
                "SELECT name, account_type, COALESCE(placeholder, 0) FROM accounts WHERE guid = ?1",
                [guid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((name, account_type, placeholder)) = account else {
            return Err(AppError::NotFound(
                "One of those accounts no longer exists".to_string(),
            ));
        };
        if account_type != "INCOME" || placeholder != 0 {
            return Err(AppError::Validation(format!(
                "{name} isn't an income account that holds transactions"
            )));
        }
        let other: Option<String> = conn
            .query_row(
                "SELECT c.mnemonic FROM dividend_links l
				 JOIN commodities c ON c.guid = l.commodity_guid
				 WHERE l.account_guid = ?1 AND l.commodity_guid <> ?2",
                params![guid, input.commodity_guid],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(mnemonic) = other {
            return Err(AppError::Validation(format!(
                "{name} already pays dividends to {mnemonic}"
            )));
        }
    }

    with_tx_err(conn, || -> Result<(), AppError> {
        conn.execute(
            "DELETE FROM dividend_links WHERE commodity_guid = ?1",
            [&input.commodity_guid],
        )?;
        for guid in &guids {
            conn.execute(
                "INSERT INTO dividend_links (account_guid, commodity_guid) VALUES (?1, ?2)",
                params![guid, input.commodity_guid],
            )?;
        }
        Ok(())
    })?;
    linked_accounts(conn, &input.commodity_guid)
}

#[tauri::command]
pub async fn set_dividend_links(
    state: State<'_, AppState>,
    input: DividendLinksInput,
) -> Result<Vec<DividendAccount>, AppError> {
    run_blocking(state.db.clone(), move |conn| {
        set_dividend_links_inner(conn, &input)
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
        conn.execute_batch(
            "INSERT INTO commodities (guid, mnemonic, namespace) VALUES
             ('c-eur', 'EUR', 'CURRENCY'),
             ('c-aapl', 'AAPL', 'NASDAQ'),
             ('c-vwce', 'VWCE', 'XETRA');
             INSERT INTO accounts (guid, name, account_type, parent_guid, placeholder, commodity_guid) VALUES
             ('broker', 'Broker', 'STOCK', NULL, 0, 'c-aapl'),
             ('cash', 'Cash', 'BANK', NULL, 0, 'c-eur'),
             ('div', 'Dividends', 'INCOME', NULL, 0, 'c-eur'),
             ('div-vwce', 'VWCE dividends', 'INCOME', NULL, 0, 'c-eur'),
             ('salary', 'Salary', 'INCOME', NULL, 0, 'c-eur'),
             ('gains', 'Capital gains', 'INCOME', NULL, 0, 'c-eur'),
             ('tax', 'Withholding', 'EXPENSE', NULL, 0, 'c-eur'),
             ('inc-root', 'Income', 'INCOME', NULL, 1, 'c-eur');",
        )
        .unwrap();
        conn
    }

    fn add_tx(conn: &Connection, tx: &str, date: &str, legs: &[(&str, i64, i64)]) {
        conn.execute(
            "INSERT INTO transactions (guid, post_date, description) VALUES (?1, ?2, ?1)",
            params![tx, date],
        )
        .unwrap();
        for (i, (account, shares, value)) in legs.iter().enumerate() {
            conn.execute(
                "INSERT INTO splits (guid, tx_guid, account_guid, value_num, value_denom, quantity_num, quantity_denom)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, 1)",
                params![format!("{tx}-{i}"), tx, account, value, shares],
            )
            .unwrap();
        }
    }

    fn leg(tx: &str, commodity: &str, shares: f64, value: f64) -> TradeLeg {
        TradeLeg {
            tx_guid: tx.to_string(),
            commodity_guid: commodity.to_string(),
            shares,
            value,
        }
    }

    #[test]
    fn zero_split_and_reinvested_dividends_net_of_withholding() {
        let conn = setup();
        // Cash dividend 100, 15 withheld, flagged by a zero split.
        add_tx(
            &conn,
            "d1",
            "2024-03-01",
            &[
                ("broker", 0, 0),
                ("div", -100, -100),
                ("tax", 15, 15),
                ("cash", 85, 85),
            ],
        );
        // Reinvested: 50 of income buys one share.
        add_tx(
            &conn,
            "d2",
            "2024-06-01",
            &[("broker", 1, 50), ("div", -50, -50)],
        );
        let legs = [
            leg("d1", "c-aapl", 0.0, 0.0),
            leg("d2", "c-aapl", 1.0, 50.0),
        ];
        let divs = detect(&conn, &legs).unwrap();
        assert_eq!(divs.len(), 2);
        assert_eq!(divs[0].amount, 85.0);
        assert_eq!(divs[0].post_date, "2024-03-01");
        assert_eq!(divs[1].amount, 50.0);
        assert!(divs.iter().all(|d| d.commodity_guid == "c-aapl"));
    }

    #[test]
    fn gain_bookings_and_unlinked_income_are_not_dividends() {
        let conn = setup();
        // GnuCash realized-gain transaction: value-only stock split.
        add_tx(
            &conn,
            "g1",
            "2024-03-01",
            &[("broker", 0, 40), ("gains", -40, -40)],
        );
        // A sale with its gain booked in the same transaction.
        add_tx(
            &conn,
            "s1",
            "2024-03-02",
            &[
                ("broker", -2, -200),
                ("gains", -20, -20),
                ("cash", 220, 220),
            ],
        );
        add_tx(
            &conn,
            "p1",
            "2024-03-03",
            &[("salary", -900, -900), ("cash", 900, 900)],
        );
        let legs = [
            leg("g1", "c-aapl", 0.0, 40.0),
            leg("s1", "c-aapl", -2.0, -200.0),
        ];
        assert!(detect(&conn, &legs).unwrap().is_empty());
    }

    #[test]
    fn linked_income_accounts_pay_their_holding() {
        let conn = setup();
        add_tx(
            &conn,
            "v1",
            "2024-04-01",
            &[
                ("div-vwce", -30, -30),
                ("salary", -70, -70),
                ("tax", 3, 3),
                ("cash", 97, 97),
            ],
        );
        assert!(detect(&conn, &[]).unwrap().is_empty());

        let input = DividendLinksInput {
            commodity_guid: "c-vwce".to_string(),
            account_guids: vec!["div-vwce".to_string(), "div-vwce".to_string()],
        };
        let linked = set_dividend_links_inner(&conn, &input).unwrap();
        assert_eq!(linked.len(), 1);
        let divs = detect(&conn, &[]).unwrap();
        assert_eq!(divs.len(), 1);
        assert_eq!(divs[0].commodity_guid, "c-vwce");
        // Withholding shared by income: 3 × 30 / 100.
        assert!((divs[0].amount - 29.1).abs() < 1e-9);
    }

    #[test]
    fn links_validate_accounts_and_reject_double_links() {
        let conn = setup();
        let set = |commodity: &str, accounts: &[&str]| {
            set_dividend_links_inner(
                &conn,
                &DividendLinksInput {
                    commodity_guid: commodity.to_string(),
                    account_guids: accounts.iter().map(|a| a.to_string()).collect(),
                },
            )
        };
        assert!(matches!(set("nope", &[]), Err(AppError::NotFound(_))));
        assert!(matches!(
            set("c-aapl", &["gone"]),
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            set("c-aapl", &["cash"]),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            set("c-aapl", &["inc-root"]),
            Err(AppError::Validation(_))
        ));

        set("c-aapl", &["div"]).unwrap();
        let err = set("c-vwce", &["div"]).unwrap_err();
        assert!(err.to_string().contains("AAPL"));
        // Replacing a commodity's own links is fine, and empty clears them.
        assert_eq!(set("c-aapl", &["div", "div-vwce"]).unwrap().len(), 2);
        assert!(set("c-aapl", &[]).unwrap().is_empty());
        assert!(linked_account_guids(&conn).unwrap().is_empty());
    }
}
