//! Global hotkey registration and the rewrite pipeline.
//!
//! Flow when the hotkey fires:
//!
//!   1. Refuse to proceed unless AXIsProcessTrusted reports our process
//!      as trusted. (Without this guard, downstream calls into CoreGraphics
//!      would crash the process; we tested it.)
//!   2. Read the user's selected text via `kAXSelectedTextAttribute`.
//!   3. Detect context (frontmost app + URL, then URL/app heuristic).
//!   4. Render the active template (default = `prompt-master`).
//!   5. Stream inference from the local `llama-server`. If no model is
//!      downloaded yet, fall back to a placeholder rewrite so the rest
//!      of the pipeline is still exercisable end-to-end.
//!   6. Replace the user's selection via `kAXSelectedTextAttribute`.
//!   7. Push an undo entry and surface a notification.
//!
//! A second hotkey press while a rewrite is in flight cancels it (the
//! `ActiveRewrite` cancel flag is checked between streaming chunks).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::clipboard::{self};
use crate::commands::model::SharedModelState;
use crate::commands::permissions::is_accessibility_trusted;
use crate::context;
use crate::hotkey_config::{self, HotkeyConfig};
use crate::inference::client::CompletionRequest;
use crate::inference::SharedInferenceState;
use crate::prompt::{self, PromptInput};
use crate::undo::{self, SharedUndoState, UndoEntry};

/// Tracks the in-flight rewrite so a second hotkey press can cancel it.
#[derive(Default)]
pub struct ActiveRewrite {
    pub cancel: crate::inference::client::CancellationToken,
    pub in_flight: Arc<AtomicBool>,
}

pub type SharedActiveRewrite = Arc<ActiveRewrite>;

/// Register the global hotkey. Called once from lib.rs `setup`.
pub fn register<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    undo_state: SharedUndoState,
    active: SharedActiveRewrite,
) -> tauri::Result<()> {
    let model_state: tauri::State<SharedModelState> = app.state();
    let inference_state: tauri::State<SharedInferenceState> = app.state();

    let undo = undo_state.clone();
    let model = Arc::clone(&*model_state);
    let inference = Arc::clone(&*inference_state);
    let active_for_cb = active.clone();

    let shortcut = hotkey_config::cached().to_shortcut().map_err(|err| {
        tauri::Error::from(anyhow::anyhow!("hotkey config invalid: {err}"))
    })?;

    app.global_shortcut()
        .on_shortcut(shortcut, move |app_handle, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                // Cancellation path: if a rewrite is in flight, cancel and
                // restore the original selection.
                if active_for_cb.in_flight.load(Ordering::SeqCst) {
                    active_for_cb.cancel.cancel();
                    return;
                }

                let undo = undo.clone();
                let model = Arc::clone(&model);
                let inference = Arc::clone(&inference);
                let active = active_for_cb.clone();
                let app_handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = run_rewrite(&app_handle, undo, model, inference, active).await {
                        tracing::warn!("rewrite failed: {err}");
                    }
                });
            }
        })
        .map_err(|e| tauri::Error::from(anyhow::anyhow!("hotkey register: {e}")))?;

    tracing::info!("hotkey: registered {}", hotkey_config::cached().display());
    Ok(())
}

/// Re-register the global hotkey using `cfg`. Called by the
/// `set_hotkey` Tauri command after the user picks a new combination.
/// Returns Err if the registration fails (e.g. the combo is already
/// claimed by another app like Maccy or Spotlight).
pub fn reregister<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    cfg: HotkeyConfig,
    undo_state: SharedUndoState,
    active: SharedActiveRewrite,
) -> Result<(), String> {
    // Unregister the old shortcut (best-effort — if it wasn't registered
    // because the previous attempt failed, this is a no-op).
    let prev = hotkey_config::cached().clone();
    if let Ok(prev_shortcut) = prev.to_shortcut() {
        let _ = app.global_shortcut().unregister(prev_shortcut);
    }

    // Save the new config to disk before registering. If registration
    // fails, we leave the on-disk state pointing at the new (failed)
    // combo so the user sees the same state on relaunch and can fix it.
    hotkey_config::save(&cfg).map_err(|e| format!("save: {e}"))?;
    hotkey_config::invalidate();

    // Register the new combo with the rewrite pipeline.
    let model_state: tauri::State<SharedModelState> = app.state();
    let inference_state: tauri::State<SharedInferenceState> = app.state();

    let undo = undo_state.clone();
    let model = Arc::clone(&*model_state);
    let inference = Arc::clone(&*inference_state);
    let active_for_cb = active.clone();

    let new_shortcut = cfg.to_shortcut().map_err(|e| format!("invalid combo: {e}"))?;
    app.global_shortcut()
        .on_shortcut(new_shortcut, move |app_handle, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                if active_for_cb.in_flight.load(Ordering::SeqCst) {
                    active_for_cb.cancel.cancel();
                    return;
                }
                let undo = undo.clone();
                let model = Arc::clone(&model);
                let inference = Arc::clone(&inference);
                let active = active_for_cb.clone();
                let app_handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) =
                        run_rewrite(&app_handle, undo, model, inference, active).await
                    {
                        tracing::warn!("rewrite failed: {err}");
                    }
                });
            }
        })
        .map_err(|e| format!("register: {e}"))?;

    tracing::info!("hotkey: reregistered to {}", cfg.display());
    Ok(())
}

async fn run_rewrite<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    undo_state: SharedUndoState,
    model_state: SharedModelState,
    inference_state: SharedInferenceState,
    active: SharedActiveRewrite,
) -> Result<(), String> {
    active.in_flight.store(true, Ordering::SeqCst);

    // Bail with a notification if we don't actually have Accessibility.
    match is_accessibility_trusted(false) {
        Ok(true) => {}
        Ok(false) => {
            active.in_flight.store(false, Ordering::SeqCst);
            let body = "Accessibility permission is required. \
                        Open Settings → Permissions, grant it, then press the hotkey again.";
            if let Err(err) = app
                .notification()
                .builder()
                .title("PrivatePrompter")
                .body(body)
                .show()
            {
                tracing::debug!("notification show failed: {err}");
            }
            return Err("accessibility not granted".to_string());
        }
        Err(err) => {
            active.in_flight.store(false, Ordering::SeqCst);
            return Err(format!("accessibility probe failed: {err}"));
        }
    }

    let result = run_rewrite_inner(app, undo_state, model_state, inference_state, active.clone()).await;
    active.in_flight.store(false, Ordering::SeqCst);
    result
}

async fn run_rewrite_inner<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    undo_state: SharedUndoState,
    model_state: SharedModelState,
    inference_state: SharedInferenceState,
    active: SharedActiveRewrite,
) -> Result<(), String> {
    let raw = clipboard::read_selected_text()
        .map_err(|e| format!("read selection failed: {e}"))?;
    if raw.trim().is_empty() {
        return Err("no text selected".to_string());
    }

    let profile = context::detect();
    let (template_id, template_str) =
        prompt::load_template("prompt-master").map_err(|e| e.to_string())?;
    let input = PromptInput {
        input: &raw,
        context: &profile,
        profile_domain: &profile.domain,
        profile_tone: &profile.tone,
        profile_tools: profile.tools.clone(),
    };
    let rendered = prompt::render(&template_str, &input).map_err(|e| e.to_string())?;

    let url_string: String = {
        let guard = inference_state.inner.lock().await;
        match guard.as_ref() {
            Some(server) => server.base_url().to_string(),
            None => String::new(),
        }
    };

    let rewritten = if url_string.is_empty() {
        rewrite_placeholder(&rendered, &raw).await
    } else {
        let url = url_string;
        let mut accumulated = String::new();
        let cancel = active.cancel.clone();
        let request = CompletionRequest::new(rendered);
        let app_for_stream = app.clone();
        crate::inference::client::complete_streaming(
            &app_for_stream,
            &url,
            &request,
            cancel,
            |chunk| {
                accumulated.push_str(&chunk.content);
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        accumulated
    };

    clipboard::replace_selected_text(&rewritten)
        .map_err(|e| format!("replace selection failed: {e}"))?;

    let model_id = {
        let guard = model_state.downloaded.lock().await;
        guard.keys().next().cloned().unwrap_or_default()
    };
    undo_state
        .push(UndoEntry {
            original_clipboard: String::new(),
            rewritten: rewritten.clone(),
            created_at_unix: undo::now_unix(),
            model_id: model_id.clone(),
            template_id: template_id.clone(),
            context_label: Some(profile.domain.clone()),
        })
        .await;

    let body = format!(
        "Rewrote {} chars using {} / {}.",
        rewritten.chars().count(),
        template_id,
        profile.domain
    );
    if let Err(err) = app
        .notification()
        .builder()
        .title("PrivatePrompter")
        .body(body)
        .show()
    {
        tracing::debug!("notification show failed: {err}");
    }

    Ok(())
}

/// Stand-in rewrite used when no model is downloaded yet. The full meta-
/// prompt template is designed to *guide* a model — dumping it into the
/// user's selection is noise, not a rewrite. So we pass the user's
/// original text through unchanged with a marker so the user can verify
/// the hotkey pipeline ran, but doesn't see the template internals.
async fn rewrite_placeholder(_rendered: &str, raw: &str) -> String {
    format!("[no model loaded — pass-through] {raw}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shortcut_parses() {
        // The default config must always produce a valid Shortcut; if it
        // doesn't, the app won't register anything on first launch.
        let cfg = hotkey_config::HotkeyConfig::default_for_macos();
        assert!(cfg.to_shortcut().is_ok(), "{:?}", cfg);
    }
}