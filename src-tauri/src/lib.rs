// Prevents additional console window on Windows in release; harmless on macOS.
// We keep it because Tauri's standard scaffold uses it and Tauri v2 cross-platform
// builds include Windows as a future target.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod tray;

use commands::permissions::{
    check_accessibility_permission, open_accessibility_settings,
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .invoke_handler(tauri::generate_handler![
            check_accessibility_permission,
            open_accessibility_settings,
        ])
        .setup(|app| {
            tray::install(app.handle())?;

            // The settings window starts hidden. Clicking the tray menu's
            // "Open Settings" item (or the tray icon itself) shows it.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}