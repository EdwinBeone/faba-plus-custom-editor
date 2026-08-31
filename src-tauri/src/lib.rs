mod commands;
mod diagnostics;
mod domain;
mod storage;

use commands::AppState;
use diagnostics::DiagnosticLogger;
use storage::LibraryDatabase;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data)?;
            let database = LibraryDatabase::new(app_data.join("library.sqlite3"));
            database.initialize()?;
            let diagnostics = DiagnosticLogger::new(app_data.join("diagnostics.log"));
            diagnostics.info(
                "app.start",
                format!(
                    "version={} os={} arch={}",
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::OS,
                    std::env::consts::ARCH
                ),
            );
            app.manage(AppState {
                database,
                diagnostics,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::detect_cards,
            commands::recent_cards,
            commands::scan_card,
            commands::get_diagnostics,
            commands::clear_diagnostics,
            commands::save_figure,
            commands::rename_figure,
            commands::delete_figure,
            commands::export_figure
        ])
        .run(tauri::generate_context!())
        .expect("error while running FABA+ Custom Editor");
}
