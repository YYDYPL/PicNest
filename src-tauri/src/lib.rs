mod ai;
mod commands;
mod db;
mod diagnostics;
mod error;
mod library;
mod models;
mod ocr;
mod organize;
mod sources;
mod watch;

use db::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let state = AppState::initialize(app.handle())?;
            app.manage(state);
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::list_assets,
            commands::scan_paths,
            commands::cancel_scan,
            commands::get_asset_thumbnail,
            commands::get_asset_preview,
            commands::preview_remove_source,
            commands::remove_source,
            commands::create_organize_plan,
            commands::apply_organize_plan,
            commands::undo_last_operation,
            commands::rollback_organize_plan,
            commands::set_favorite,
            commands::analyze_asset,
            commands::test_ai_connection,
            commands::clear_ai_results,
            commands::delete_api_key,
            commands::ocr_asset,
            commands::save_settings,
            commands::create_album,
            commands::assign_asset_to_album,
            commands::assign_assets_to_album,
            commands::set_asset_tags,
            commands::list_asset_locations,
            commands::move_duplicate_to_trash,
            commands::export_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PicNest");
}
