use super::*;

// ── Write ────────────────────────────────────────────────────────────────────

/// Target file lives next to the export so the final rename is atomic and
/// stays on one filesystem; a failed write leaves no partial target behind.
pub(super) fn temp_path_for(path: &Path) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(".spendo-export");
    PathBuf::from(raw)
}

pub(super) fn write_gnucash_file(path: &Path, data: &ExportData) -> Result<(), AppError> {
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
    std::fs::remove_file(path)
        .map_err(|e| AppError::Io(format!("Failed to replace export file: {e}")))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| AppError::Io(format!("Failed to write export file: {e}")))
}

pub(super) fn write_gnucash_db(path: &Path, data: &ExportData) -> Result<(), AppError> {
    let conn = Connection::open(path)
        .map_err(|e| AppError::Io(format!("Failed to create export file: {e}")))?;
    conn.execute_batch(GNUCASH_SCHEMA)
        .map_err(|e| AppError::Io(format!("Failed to write GnuCash schema: {e}")))?;

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
    })?;

    Ok(())
}

pub(super) fn write_commodities(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
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

pub(super) fn write_accounts(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
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

pub(super) fn write_transactions(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
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

pub(super) fn write_prices(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
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
pub(super) fn write_budgets(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
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
pub(super) fn write_budget_feature(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
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

pub(super) fn write_schedules(conn: &Connection, data: &ExportData) -> Result<(), AppError> {
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
