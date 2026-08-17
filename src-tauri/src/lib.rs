mod commands;
mod domain;
mod storage;

use commands::AppState;
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
            app.manage(AppState { database });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::detect_cards,
            commands::recent_cards,
            commands::scan_card,
            commands::save_figure,
            commands::rename_figure,
            commands::delete_figure,
            commands::export_figure
        ])
        .run(tauri::generate_context!())
        .expect("error while running FABA+ Custom Editor");
}
