mod account_detail;
mod accounts;
mod accounts_mut;
mod attachments;
mod books;
mod budgets;
mod csv_import;
mod db;
mod export;
mod fx;
mod history;
mod home;
mod import;
mod init;
mod nav;
mod portfolio;
mod prices;
mod reconcile;
mod reports;
mod schedules;
mod search;
mod settings;
mod sql;
mod transactions;
mod tree;
mod utils;
mod yahoo;

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

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

            // Document model: reopen the last active book, else start book-less
            // on an in-memory placeholder (welcome screen) — startup never
            // crashes on a missing or corrupt file.
            let remembered = db::read_last_book(&app_data_dir).filter(|p| p.is_file());
            let (conn, active_path) = match remembered {
                Some(path) => match db::connect_db(&path)
                    .ok()
                    .and_then(|c| db::init_db(&c).ok().map(|_| c))
                {
                    Some(c) => {
                        db::stamp_app_id(&c);
                        if let Err(e) = crate::init::ensure_default_roots(&c) {
                            eprintln!("spendo: could not seed default accounts: {e}");
                        }
                        db::write_last_book(&app_data_dir, &path).ok();
                        (c, Some(path))
                    }
                    None => {
                        eprintln!("spendo: remembered book unreadable, starting book-less");
                        db::clear_last_book(&app_data_dir);
                        let c = db::open_placeholder()
                            .expect("Failed to initialize placeholder database");
                        (c, None)
                    }
                },
                None => {
                    let c =
                        db::open_placeholder().expect("Failed to initialize placeholder database");
                    (c, None)
                }
            };

            app.manage(AppState {
                db: Arc::new(Mutex::new(conn)),
                book_generation: Arc::new(AtomicU64::new(0)),
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
            books::close_book,
            init::set_base_currency,
            settings::clear_app_db,
            settings::write_text_file,
            settings::get_book_settings,
            settings::set_book_setting,
            import::import_gnucash,
            import::reimport_last,
            csv_import::csv_detect,
            csv_import::csv_preview,
            csv_import::csv_commit,
            history::fetch_change_history,
            history::undo_history_entry,
            history::redo_history_entry,
            accounts::fetch_accounts,
            accounts::fetch_nav_roots,
            accounts::fetch_book_currencies,
            nav::fetch_nav_badges,
            accounts_mut::create_account,
            accounts_mut::update_account,
            accounts_mut::delete_account,
            transactions::fetch_transactions,
            transactions::fetch_transaction,
            transactions::fetch_transaction_heatmap,
            transactions::create_transaction,
            transactions::update_transaction,
            transactions::delete_transaction,
            attachments::fetch_attachments,
            attachments::add_attachments,
            attachments::delete_attachment,
            attachments::fetch_attachment_data,
            attachments::export_attachment,
            attachments::open_attachment,
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
            schedules::fetch_schedule,
            schedules::post_schedule_occurrence,
            schedules::skip_schedule_occurrence,
            portfolio::fetch_portfolio_data,
            portfolio::fetch_holding_detail,
            search::search_global,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
