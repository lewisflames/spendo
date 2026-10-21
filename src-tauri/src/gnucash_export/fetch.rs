use super::*;

// ── Fetch ────────────────────────────────────────────────────────────────────

pub(super) fn fetch_export_data_inner(conn: &Connection) -> Result<ExportData, AppError> {
    let mut accounts = read_accounts(conn)?;
    let mut commodities = read_commodities(conn)?;
    let transactions_raw = read_transactions(conn)?;
    let splits = read_splits(conn)?;
    let prices = read_prices(conn)?;
    let budgets = read_budgets(conn)?;
    let schedules = read_schedules(conn)?;

    if accounts.is_empty() {
        return Err(AppError::Validation(
            "Book has no accounts. Nothing to export".to_string(),
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

pub(super) fn read_commodities(conn: &Connection) -> Result<Vec<CommodityRow>, AppError> {
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

pub(super) fn read_accounts(conn: &Connection) -> Result<Vec<AccountRow>, AppError> {
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

pub(super) fn read_transactions(conn: &Connection) -> Result<Vec<TransactionRow>, AppError> {
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

pub(super) fn read_splits(conn: &Connection) -> Result<Vec<SplitRow>, AppError> {
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

pub(super) fn read_prices(conn: &Connection) -> Result<Vec<PriceRow>, AppError> {
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

pub(super) fn read_budgets(conn: &Connection) -> Result<Vec<BudgetRow>, AppError> {
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

pub(super) fn read_schedules(conn: &Connection) -> Result<Vec<ScheduleRow>, AppError> {
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
