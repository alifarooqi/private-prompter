// Prevents additional console window on Windows in release; harmless on macOS.
// We keep it because Tauri's standard scaffold uses it and Tauri v2 cross-platform
// builds include Windows as a future target.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod commands;
mod context;
mod hotkey;
mod inference;
mod model;
mod prompt;
mod tray;
mod undo;

use commands::model::SharedModelState;
use commands::permissions::{
    check_accessibility_permission, open_accessibility_settings,
};
use hotkey::{ActiveRewrite, SharedActiveRewrite};
use inference::SharedInferenceState;
use tauri::Manager;
use undo::SharedUndoState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing once. Logs land at ~/Library/Logs/PrivatePrompter/ via
    // RUST_LOG env var (default = "info").
    init_logging();

    let model_state: SharedModelState = std::sync::Arc::new(commands::model::ModelState::default());
    let inference_state: SharedInferenceState =
        std::sync::Arc::new(inference::InferenceState::default());
    let undo_state: SharedUndoState = std::sync::Arc::new(undo::UndoState::default());
    let active_rewrite: SharedActiveRewrite = std::sync::Arc::new(ActiveRewrite::default());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .manage(model_state)
        .manage(inference_state)
        .manage(undo_state.clone())
        .manage(active_rewrite.clone())
        .invoke_handler(tauri::generate_handler![
            check_accessibility_permission,
            open_accessibility_settings,
            commands::model::list_models,
            commands::model::detect_ram,
            commands::model::recommended_model_id,
            commands::model::is_model_downloaded,
            commands::model::start_model_download,
            commands::model::cancel_model_download,
            commands::model::active_model_id,
            inference::commands::start_inference,
            inference::commands::stop_inference,
            inference::commands::inference_status,
            inference::commands::inference_health,
        ])
        .setup(move |app| {
            // Resolve the canonical data dir via Tauri (which uses the
            // bundle ID on macOS: ~/Library/Application Support/<bundle-id>/)
            // and stash it in a OnceLock so the rest of the app can read it
            // without needing an AppHandle.
            match app.path().app_data_dir() {
                Ok(path) => {
                    if let Err(err) = std::fs::create_dir_all(&path) {
                        tracing::warn!("failed to create app data dir: {err}");
                    }
                    model::store::set_data_dir(path);
                }
                Err(err) => {
                    tracing::warn!("app_data_dir unavailable, using temp fallback: {err}");
                    model::store::set_data_dir(std::env::temp_dir().join("com.alifarooqi.privateprompter"));
                }
            }

            // Re-populate the in-memory "downloaded" map from disk so we
            // don't lose track of models the user pulled on previous runs.
            {
                let state: tauri::State<SharedModelState> = app.state();
                commands::model::scan_downloaded(&state);
            }

            tray::install(app.handle())?;

            // Hydrate the undo stack from disk. Best-effort.
            {
                let state: tauri::State<SharedUndoState> = app.state();
                let app_handle = app.handle().clone();
                let state_clone: SharedUndoState = std::sync::Arc::clone(&state);
                tauri::async_runtime::spawn(async move {
                    let entries = undo::load(&app_handle).await;
                    for entry in entries {
                        state_clone.push(entry).await;
                    }
                });
            }

            // Register the global hotkey.
            if let Err(err) = hotkey::register(app.handle(), undo_state.clone(), active_rewrite.clone()) {
                tracing::warn!("failed to register global hotkey: {err}");
            }

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Initialize `tracing` with a stderr writer. Phase 9 (Polish) wires this up
/// to a log file at `~/Library/Logs/PrivatePrompter/`. For now we keep it
/// simple so the framework is in place.
fn init_logging() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}