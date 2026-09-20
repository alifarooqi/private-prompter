//! Tauri commands for the Model tab.
//!
//! Exposes:
//!   * `list_models`              — registry contents (sanitized).
//!   * `detect_ram_tier`          — RAM bucket for the current machine.
//!   * `recommended_model`        — best model for the detected RAM tier.
//!   * `is_model_downloaded`      — does the GGUF already exist on disk?
//!   * `start_model_download`     — kick off a streaming download.
//!   * `pause_model_download`     — pause the in-flight download.
//!   * `resume_model_download`    — resume a paused download.
//!   * `cancel_model_download`    — abort the in-flight download.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};
use tokio::sync::Mutex;

use crate::model::downloader::{
    download_model as downloader_download, CancellationToken, PauseToken,
};
use crate::model::ram::{detect_ram_tier, RamTier};
use crate::model::registry::{self, ModelEntry};
use crate::model::store as store_paths;

/// Active-download state for one in-flight download. Cancelling, pausing,
/// and resuming all reach the same handle via `state.active`.
struct ActiveDownload {
    model_id: String,
    cancel: CancellationToken,
    pause: PauseToken,
}

#[derive(Default)]
pub struct ModelState {
    pub active: Mutex<Option<ActiveDownload>>,
    /// `model_id` -> downloaded file path.
    pub downloaded: Mutex<HashMap<String, PathBuf>>,
}

pub type SharedModelState = Arc<ModelState>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ModelSummary {
    pub id: String,
    pub display_name: String,
    pub publisher: String,
    pub size_bytes: u64,
    pub context_length: u32,
    pub min_ram_bytes: u64,
    pub downloaded: bool,
}

#[tauri::command]
pub fn list_models(state: State<'_, SharedModelState>) -> Vec<ModelSummary> {
    // Source of truth is the filesystem, not the in-memory HashMap —
    // the HashMap can drift if a previous run's file was renamed to .gguf
    // but the new run's HashMap is empty. Falling back to the file means
    // "Ready" can never disagree with "the file is on disk".
    let downloaded = state.downloaded.blocking_lock();
    registry::get()
        .list()
        .iter()
        .map(|m| {
            let (display_name, publisher, size_bytes, context_length, min_ram_bytes) =
                summary_fields(m);
            let file_path =
                crate::model::store::model_path(&m.id, &m.file);
            let file_exists = file_path.exists();
            ModelSummary {
                id: m.id.clone(),
                display_name,
                publisher,
                size_bytes,
                context_length,
                min_ram_bytes,
                downloaded: downloaded.contains_key(&m.id) || file_exists,
            }
        })
        .collect()
}

#[tauri::command]
pub fn detect_ram() -> RamTier {
    detect_ram_tier()
}

#[tauri::command]
pub fn recommended_model_id() -> String {
    let tier = detect_ram_tier();
    registry::get().recommend_for_tier(tier.as_key()).id.clone()
}

#[tauri::command]
pub fn is_model_downloaded(
    state: State<'_, SharedModelState>,
    model_id: String,
) -> bool {
    // Same: prefer the HashMap for speed but fall back to the file.
    if state.downloaded.blocking_lock().contains_key(&model_id) {
        return true;
    }
    let Some(entry) = registry::get().find(&model_id).ok() else {
        return false;
    };
    crate::model::store::model_path(&entry.id, &entry.file).exists()
}

#[tauri::command]
pub async fn start_model_download(
    app: AppHandle,
    state: State<'_, SharedModelState>,
    model_id: String,
) -> Result<(), String> {
    let entry = registry::get().find(&model_id).map_err(|e| e.to_string())?;

    // Cancel any in-flight download and install the new handle.
    let cancel = CancellationToken::new();
    let pause = PauseToken::new();
    {
        let mut guard = state.active.lock().await;
        if let Some(prev) = guard.as_ref() {
            prev.cancel.cancel();
        }
        *guard = Some(ActiveDownload {
            model_id: entry.id.clone(),
            cancel: cancel.clone(),
            pause: pause.clone(),
        });
    }

    let downloader_result =
        downloader_download(&app, entry, cancel.clone(), pause.clone()).await;

    // Clear the in-flight token regardless of outcome.
    {
        let mut guard = state.active.lock().await;
        *guard = None;
    }

    let path = downloader_result.map_err(|e| e.to_string())?;

    let mut downloaded = state.downloaded.lock().await;
    downloaded.insert(entry.id.clone(), path);

    Ok(())
}

#[tauri::command]
pub async fn pause_model_download(
    state: State<'_, SharedModelState>,
) -> Result<(), String> {
    let guard = state.active.lock().await;
    if let Some(active) = guard.as_ref() {
        active.pause.pause();
        tracing::info!("pausing download of {}", active.model_id);
    }
    Ok(())
}

#[tauri::command]
pub async fn resume_model_download(
    state: State<'_, SharedModelState>,
) -> Result<(), String> {
    let guard = state.active.lock().await;
    if let Some(active) = guard.as_ref() {
        active.pause.resume();
        tracing::info!("resuming download of {}", active.model_id);
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_model_download(
    state: State<'_, SharedModelState>,
) -> Result<(), String> {
    let mut guard = state.active.lock().await;
    if let Some(active) = guard.take() {
        active.cancel.cancel();
        tracing::info!("cancelling download of {}", active.model_id);
    }
    Ok(())
}

/// Snapshot what GGUF is currently selected as the inference model.
#[tauri::command]
pub fn active_model_id(state: State<'_, SharedModelState>) -> Option<String> {
    state
        .downloaded
        .blocking_lock()
        .keys()
        .next()
        .cloned()
}

/// Scan the models directory and populate the `downloaded` map. Called
/// once on app startup so we don't lose track of models the user has
/// already pulled (the in-memory state isn't persisted across launches).
pub fn scan_downloaded(state: &SharedModelState) {
    use crate::model::registry;
    use std::path::PathBuf;

    let manifest = registry::get();
    let Ok(mut guard) = state.downloaded.try_lock() else {
        tracing::warn!("scan: could not lock downloaded map (busy)");
        return;
    };
    for entry in manifest.list() {
        let path: PathBuf = crate::model::store::model_path(&entry.id, &entry.file);
        if path.exists() {
            tracing::info!("scan: found {} at {}", entry.id, path.display());
            guard.insert(entry.id.clone(), path);
        }
    }
}

fn summary_fields(m: &ModelEntry) -> (String, String, u64, u32, u64) {
    (
        m.display_name.clone(),
        m.publisher.clone(),
        m.size_bytes,
        m.context_length,
        m.min_ram_bytes,
    )
}

#[allow(dead_code)]
fn default_model_path(model_id: &str, file: &str) -> PathBuf {
    store_paths::model_path(model_id, file)
}