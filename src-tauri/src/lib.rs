mod account_detail;
mod accounts;
mod budgets;
mod db;
mod fx;
mod home;
mod import;
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
            let db_path = db::get_db_path(&app_data_dir);
            let conn = db::connect_db(&db_path).expect("Failed to open database");
            db::init_db(&conn).expect("Failed to initialize database");

            app.manage(AppState {
                db: std::sync::Arc::new(Mutex::new(conn)),
                db_path,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            home::fetch_home_data,
            settings::clear_app_db,
            settings::get_data_info,
            import::fetch_import_history,
            import::import_gnucash,
            import::reimport_last,
            accounts::fetch_accounts,
            transactions::fetch_transactions,
            transactions::fetch_transaction,
            transactions::fetch_transaction_heatmap,
            transactions::fetch_account_names,
            prices::refresh_prices,
            account_detail::fetch_account_detail,
            account_detail::fetch_account_balance_trend,
            reports::generate_pnl_report,
            budgets::set_budget_year_amount,
            budgets::set_budget_year_amounts,
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
