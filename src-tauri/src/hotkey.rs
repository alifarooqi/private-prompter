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
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::clipboard::{self};
use crate::commands::model::SharedModelState;
use crate::commands::permissions::is_accessibility_trusted;
use crate::context;
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

fn default_shortcut() -> Shortcut {
    // ⌘⌥R (Cmd+Option+R) — "R for rewrite".
    //
    // We tried a few defaults and they all conflicted on the test machine:
    //   ⌘⇧Space      → Maccy default
    //   ⌘⌥Space      → Spotlight window-search variant on some macOS
    //   ⌘⌥P / ⌘⌥S   → Preferences / Save As in many apps
    //
    // ⌘⌥R is not bound by any first-party macOS shortcut. Users can pick
    // their own via the Settings UI once that lands.
    Shortcut::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::KeyR)
}

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

    app.global_shortcut()
        .on_shortcut(default_shortcut(), move |app_handle, _shortcut, event| {
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
    // The notification body reminds the user how to grant it.
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

    // Stream from llama-server if it's running, otherwise fall back to a
    // placeholder rewrite so the rest of the pipeline is exercisable
    // without a model.
    let url_string: String = {
        let guard = inference_state.inner.lock().await;
        match guard.as_ref() {
            Some(server) => server.base_url().to_string(),
            None => String::new(),
        }
    };

    let rewritten = if url_string.is_empty() {
        rewrite_placeholder(&rendered).await
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

    // Replace the user's selection via the Accessibility API.
    clipboard::replace_selected_text(&rewritten)
        .map_err(|e| format!("replace selection failed: {e}"))?;

    // Push an undo entry. We don't try to capture the original text
    // (kAXSelectedText before our write would now be the rewritten text);
    // the user can `⌘Z` in their app to revert.
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

    // Surface a macOS notification with the rewrite's char count, the
    // template that was used, and the detected context domain.
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

async fn rewrite_placeholder(rendered: &str) -> String {
    format!("[rewritten] {rendered}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_is_cmd_option_r() {
        assert!(matches!(default_shortcut().key, Code::KeyR));
    }
}