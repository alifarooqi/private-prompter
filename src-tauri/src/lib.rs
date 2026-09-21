// Prevents additional console window on Windows in release; harmless on macOS.
// We keep it because Tauri's standard scaffold uses it and Tauri v2 cross-platform
// builds include Windows as a future target.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Many modules scaffold types and helpers ahead of the consumer wiring
// (e.g. the alternative `Failed` download state, the `complete_blocking`
// non-streaming variant, the `TemplateSummary` editor-preview helper).
// Treating those as errors would force us to either wire them in early
// or rip them out — both worse than letting the MVP flag them as
// intentionally scaffolded. Phase 9 will either consume them or delete.
#![allow(dead_code)]

mod active_model;
mod clipboard;
mod commands;
mod context;
mod hotkey;
mod hotkey_config;
mod inference;
mod model;
mod prompt;
mod tray;
mod undo;

use commands::model::SharedModelState;
use commands::permissions::{check_accessibility_permission, open_accessibility_settings};
use hotkey::{ActiveRewrite, SharedActiveRewrite};
use inference::SharedInferenceState;
use tauri::Manager;
use undo::SharedUndoState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
            commands::model::pause_model_download,
            commands::model::resume_model_download,
            commands::model::cancel_model_download,
            commands::model::active_model_id,
            inference::commands::start_inference,
            inference::commands::stop_inference,
            inference::commands::inference_status,
            inference::commands::inference_health,
            commands::hotkey::get_hotkey,
            commands::hotkey::set_hotkey,
            commands::hotkey::reset_hotkey,
            commands::active_model::get_active_model,
            commands::active_model::set_active_model,
        ])
        .setup(move |app| {
            // Resolve the canonical data dir via Tauri (which uses the
            // bundle ID on macOS: ~/Library/Application Support/<bundle-id>/)
            // and stash it in a OnceLock so the rest of the app can read it
            // without needing an AppHandle.
            match app.path().app_data_dir() {
                Ok(path) => {
                    if let Err(err) = std::fs::create_dir_all(&path) {
                        eprintln!("failed to create app data dir: {err}");
                    }
                    model::store::set_data_dir(path);
                }
                Err(err) => {
                    eprintln!("app_data_dir unavailable, using temp fallback: {err}");
                    model::store::set_data_dir(
                        std::env::temp_dir().join("com.alifarooqi.privateprompter"),
                    );
                }
            }

            // Initialize file logging now that we know where the data dir is.
            // Daily-rolling logs at <data_dir>/logs/private-prompter.YYYY-MM-DD.log.
            init_file_logging();

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
            if let Err(err) =
                hotkey::register(app.handle(), undo_state.clone(), active_rewrite.clone())
            {
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

/// Initialize file logging now that the data dir is known. Daily-rolling
/// files at `<data_dir>/logs/private-prompter.YYYY-MM-DD.log` plus a tee to
/// stderr so `cargo tauri dev` output is still useful. The worker guard
/// is leaked into a `Box` so the background writer lives for the full
/// process lifetime.
fn init_file_logging() {
    use std::io::Write;
    use tracing_appender::non_blocking;
    use tracing_appender::rolling;
    use tracing_subscriber::fmt::MakeWriter;
    use tracing_subscriber::EnvFilter;

    let log_dir = model::store::data_dir().join("logs");
    if let Err(err) = std::fs::create_dir_all(&log_dir) {
        eprintln!("failed to create logs dir {}: {err}", log_dir.display());
        return;
    }

    let appender = rolling::daily(&log_dir, "private-prompter.log");
    let (file_writer, guard) = non_blocking(appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // Tee writer: writes each line to both stderr (for `cargo tauri dev`
    // visibility) and the rolling log (for production debugging).
    struct Tee<A, B>(A, B);
    impl<A: Write, B: Write> Write for Tee<A, B> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let n = self.0.write(buf)?;
            let _ = self.1.write(buf);
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()?;
            self.1.flush()
        }
    }
    struct TeeWriter<A, B>(A, B);
    impl<'a, A: MakeWriter<'a>, B: MakeWriter<'a>> MakeWriter<'a> for TeeWriter<A, B> {
        type Writer = Tee<A::Writer, B::Writer>;
        fn make_writer(&'a self) -> Self::Writer {
            Tee(self.0.make_writer(), self.1.make_writer())
        }
    }

    if tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(TeeWriter(file_writer, std::io::stderr))
        .try_init()
        .is_ok()
    {
        // Keep the worker alive for the program's lifetime. The Box::leak
        // here is intentional and small (one Arc + a thread handle).
        Box::leak(Box::new(guard));
        tracing::info!("logging initialized: {}", log_dir.display());
    }
}
