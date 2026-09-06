mod account_detail;
mod accounts;
mod books;
mod budgets;
mod db;
mod fx;
mod history;
mod home;
mod import;
mod init;
mod portfolio;
mod prices;
mod reports;
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
            books::get_active_book,
            books::open_book,
            books::create_book_at,
            books::save_book_as,
            init::create_empty_book,
            init::set_base_currency,
            settings::clear_app_db,
            settings::get_data_info,
            import::import_gnucash,
            import::reimport_last,
            history::fetch_change_history,
            history::revert_history_entry,
            accounts::fetch_accounts,
            transactions::fetch_transactions,
            transactions::fetch_transaction,
            transactions::fetch_transaction_heatmap,
            transactions::fetch_account_names,
            prices::refresh_prices,
            account_detail::fetch_account_detail,
            account_detail::fetch_account_balance_trend,
            reports::generate_pnl_report,
            reports::generate_pnl_trend,
            reports::generate_balance_sheet,
            prices::list_symbol_overrides,
            prices::set_symbol_override,
            prices::delete_symbol_override,
            budgets::set_budget_year_amount,
            budgets::get_budget_years,
            budgets::delete_budget_year,
            budgets::get_budget_vs_actual,
            budgets::get_budget_detail,
            budgets::get_budget_date_range,
            portfolio::fetch_portfolio_data,
            portfolio::fetch_holding_detail,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
