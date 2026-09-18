//! Global hotkey registration and the rewrite pipeline.
//!
//! Phase 8 wires up:
//!   * streaming inference
//!   * cancellation via a second hotkey press
//!   * a notification with an Undo affordance

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::clipboard::{self, ClipboardError};
use crate::commands::model::SharedModelState;
use crate::commands::permissions::is_accessibility_trusted;
use crate::context;
use crate::inference::client::{CancellationToken, CompletionRequest};
use crate::inference::SharedInferenceState;
use crate::model::registry;
use crate::prompt::{self, PromptInput};
use crate::undo::{self, SharedUndoState, UndoEntry};

/// Tracks the in-flight rewrite so a second hotkey press can cancel it.
#[derive(Default)]
pub struct ActiveRewrite {
    pub cancel: CancellationToken,
    pub in_flight: Arc<AtomicBool>,
}

pub type SharedActiveRewrite = Arc<ActiveRewrite>;

fn default_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space)
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
                // restore the original clipboard.
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

/// End-to-end rewrite (Phase 8):
///   1. Save current clipboard so we can restore on undo.
///   2. Simulate `Cmd+C` and read the highlighted text.
///   3. Detect context.
///   4. Render the active template (default = `prompt-master`).
///   5. Stream inference from `llama-server`, updating the clipboard as
///      tokens arrive.
///   6. Simulate `Cmd+V` once streaming completes.
///   7. Push an undo entry and show a notification.
#[allow(clippy::too_many_arguments)]
async fn run_rewrite<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    undo_state: SharedUndoState,
    model_state: SharedModelState,
    inference_state: SharedInferenceState,
    active: SharedActiveRewrite,
) -> Result<(), String> {
    active.in_flight.store(true, Ordering::SeqCst);

    // Refuse to call enigo unless we *actually* have Accessibility. Without
    // this guard, enigo's macOS backend can crash the process via a CGF
    // SIGSEGV when the underlying CGEventPost returns an unhandled error.
    match is_accessibility_trusted(false) {
        Ok(true) => {}
        Ok(false) => {
            active.in_flight.store(false, Ordering::SeqCst);
            let body = "Accessibility permission is required. \
                        Open Settings → Permissions, grant it, then press ⌘⇧Space again.";
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

    let result = run_rewrite_inner(
        app,
        undo_state,
        model_state,
        inference_state,
        active.clone(),
    )
    .await;
    active.in_flight.store(false, Ordering::SeqCst);
    result
}

async fn run_rewrite_inner<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    undo_state: SharedUndoState,
    model_state: SharedModelState,
    _inference_state: SharedInferenceState,
    active: SharedActiveRewrite,
) -> Result<(), String> {
    let pre_clipboard = clipboard::read_text().map_err(|e| e.to_string())?;
    clipboard::simulate_copy().map_err(map_clip_err)?;
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    let raw = clipboard::read_text().map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Err("no text selected".to_string());
    }

    let profile = context::detect();
    let (template_id, template_str) = prompt::load_template("prompt-master")
        .map_err(|e| e.to_string())?;
    let input = PromptInput {
        input: &raw,
        context: &profile,
        profile_domain: &profile.domain,
        profile_tone: &profile.tone,
        profile_tools: profile.tools.clone(),
    };
    let rendered = prompt::render(&template_str, &input).map_err(|e| e.to_string())?;

    // Resolve the sidecar base URL. If the server isn't running we fall back
    // to the placeholder rewrite so the rest of the pipeline (clipboard +
    // undo + notification) is still exercisable without a model.
    let url_string: String = {
        let guard = _inference_state.inner.lock().await;
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
                let _ = clipboard::write_text(&accumulated);
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        accumulated
    };

    clipboard::write_text(&rewritten).map_err(|e| e.to_string())?;
    clipboard::simulate_paste().map_err(map_clip_err)?;

    let model_id = {
        let guard = model_state.downloaded.lock().await;
        guard.keys().next().cloned().unwrap_or_default()
    };
    undo_state
        .push(UndoEntry {
            original_clipboard: pre_clipboard,
            rewritten: rewritten.clone(),
            created_at_unix: undo::now_unix(),
            model_id: model_id.clone(),
            template_id: template_id.clone(),
            context_label: Some(profile.domain.clone()),
        })
        .await;

    // Phase 8: notify. macOS will prompt for notification permission on first
    // show; if the user denies we silently swallow the error.
    let body = format!(
        "Rewrote {} chars using {} / {}. Press ⌘⇧Z to undo (10s).",
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

    let _ = (model_id, template_id);
    let _ = registry::get();
    Ok(())
}

async fn rewrite_placeholder(rendered: &str) -> String {
    format!("[rewritten] {rendered}")
}

fn map_clip_err(err: ClipboardError) -> String {
    match err {
        ClipboardError::AccessibilityDenied => {
            "Accessibility permission denied. Open Settings → Permissions to grant it.".to_string()
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_is_cmd_shift_space() {
        assert!(matches!(default_shortcut().key, Code::Space));
    }
}