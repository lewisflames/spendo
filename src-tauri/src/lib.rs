mod account_detail;
mod accounts;
mod accounts_mut;
mod books;
mod budgets;
mod db;
mod export;
mod fx;
mod history;
mod home;
mod import;
mod init;
mod portfolio;
mod prices;
mod reconcile;
mod reports;
mod schedules;
mod settings;
mod sql;
mod transactions;
mod tree;
mod utils;
mod yahoo;

use std::sync::Mutex;

use db::AppState;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).ok();

            // Document model: reopen the last active book, else adopt the
            // legacy single `spendo.db`, else start book-less on an in-memory
            // placeholder (welcome screen) — startup never crashes on a
            // missing or corrupt file.
            let remembered = db::read_last_book(&app_data_dir)
                .filter(|p| p.is_file())
                .or_else(|| {
                    let legacy = db::get_db_path(&app_data_dir);
                    legacy.is_file().then_some(legacy)
                });
            let (conn, active_path) = match remembered {
                Some(path) => match db::connect_db(&path)
                    .ok()
                    .and_then(|c| db::init_db(&c).ok().map(|_| c))
                {
                    Some(c) => {
                        db::stamp_app_id(&c);
                        db::write_last_book(&app_data_dir, &path).ok();
                        (c, Some(path))
                    }
                    None => {
                        eprintln!("spendo: remembered book unreadable, starting book-less");
                        db::clear_last_book(&app_data_dir);
                        let c = rusqlite::Connection::open_in_memory()
                            .expect("Failed to open placeholder database");
                        db::init_db(&c).expect("Failed to initialize placeholder database");
                        (c, None)
                    }
                },
                None => {
                    let c = rusqlite::Connection::open_in_memory()
                        .expect("Failed to open placeholder database");
                    db::init_db(&c).expect("Failed to initialize placeholder database");
                    (c, None)
                }
            };

            app.manage(AppState {
                db: std::sync::Arc::new(Mutex::new(conn)),
                db_path: Mutex::new(active_path),
                app_data_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            home::fetch_home_data,
            books::fetch_active_book,
            books::open_book,
            books::create_book_at,
            books::save_book_as,
            init::set_base_currency,
            settings::clear_app_db,
            settings::fetch_data_info,
            settings::write_text_file,
            settings::get_book_settings,
            settings::set_book_setting,
            import::import_gnucash,
            import::reimport_last,
            history::fetch_change_history,
            history::revert_history_entry,
            accounts::fetch_accounts,
            accounts::fetch_book_currencies,
            accounts_mut::create_account,
            accounts_mut::update_account,
            accounts_mut::move_account,
            accounts_mut::delete_account,
            transactions::fetch_transactions,
            transactions::fetch_transaction,
            transactions::fetch_transaction_heatmap,
            transactions::fetch_account_names,
            transactions::create_transaction,
            transactions::update_transaction,
            transactions::delete_transaction,
            reconcile::fetch_reconcile_info,
            reconcile::mark_reconcile_splits,
            reconcile::finish_reconciliation,
            export::fetch_export_transactions,
            export::fetch_export_accounts,
            prices::refresh_prices,
            account_detail::fetch_account_detail,
            account_detail::fetch_account_balance_trend,
            reports::fetch_pnl_report,
            reports::fetch_pnl_trend,
            prices::fetch_symbol_overrides,
            prices::set_symbol_override,
            prices::delete_symbol_override,
            budgets::set_budget_year_amount,
            budgets::fetch_budget_years,
            budgets::delete_budget_year,
            budgets::fetch_budget_vs_actual,
            budgets::fetch_budget_detail,
            budgets::fetch_budget_date_range,
            schedules::create_schedule,
            schedules::update_schedule,
            schedules::delete_schedule,
            schedules::set_schedule_active,
            schedules::fetch_schedules,
            schedules::fetch_upcoming_reminders,
            schedules::post_schedule_occurrence,
            schedules::skip_schedule_occurrence,
            portfolio::fetch_portfolio_data,
            portfolio::fetch_holding_detail,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
