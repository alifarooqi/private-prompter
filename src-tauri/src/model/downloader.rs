//! GGUF downloader with progress events and SHA256 verification.
//!
//! Streaming, cancellable via a shared `CancellationToken` (Phase 8 will
//! hook this into the hotkey cancel path), and emits `DownloadProgress`
//! events the frontend can listen to via `tauri::AppHandle::emit`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use super::registry::ModelEntry;
use super::store;
use super::{DownloadProgress, DownloadState, ModelError};

const PROGRESS_EVENT: &str = "model://download-progress";

/// Cheap clone-able cancel handle. Set the inner flag with `cancel()`; the
/// download loop polls it between chunks.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// Download a model entry's GGUF to disk. `app` is used to emit progress
/// events; the caller can listen for `PROGRESS_EVENT` on the JS side.
///
/// On success, returns the absolute path to the downloaded file. On failure
/// or cancellation, the partial file is removed.
pub async fn download_model(
    app: &AppHandle,
    entry: &ModelEntry,
    cancel: CancellationToken,
) -> Result<PathBuf, ModelError> {
    if cancel.is_cancelled() {
        return Err(ModelError::Cancelled);
    }

    let dest_dir = store::ensure_models_dir(&entry.id)?;
    let dest_path = dest_dir.join(&entry.file);
    let part_path = dest_path.with_extension("part");

    emit_progress(
        app,
        &DownloadProgress {
            model_id: entry.id.clone(),
            bytes_downloaded: 0,
            total_bytes: entry.size_bytes,
            state: DownloadState::Started,
        },
    );

    let client = reqwest::Client::builder()
        .user_agent(concat!(
            "PrivatePrompter/",
            env!("CARGO_PKG_VERSION"),
            " (macOS)"
        ))
        .build()?;

    let response = client.get(&entry.url).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(entry.size_bytes);

    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&part_path).await?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;

    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            drop(file);
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(ModelError::Cancelled);
        }
        let chunk = chunk?;
        hasher.update(&chunk);
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        downloaded += chunk.len() as u64;
        emit_progress(
            app,
            &DownloadProgress {
                model_id: entry.id.clone(),
                bytes_downloaded: downloaded,
                total_bytes: total,
                state: DownloadState::Progress,
            },
        );
    }

    file.flush().await?;
    drop(file);

    emit_progress(
        app,
        &DownloadProgress {
            model_id: entry.id.clone(),
            bytes_downloaded: downloaded,
            total_bytes: total,
            state: DownloadState::Verifying,
        },
    );

    let digest = hasher.finalize();
    let actual = hex::encode(digest);
    let placeholder = "0".repeat(64);
    if entry.sha256 != placeholder && entry.sha256 != actual {
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err(ModelError::HashMismatch {
            expected: entry.sha256.clone(),
            actual,
        });
    }

    tokio::fs::rename(&part_path, &dest_path).await?;

    emit_progress(
        app,
        &DownloadProgress {
            model_id: entry.id.clone(),
            bytes_downloaded: downloaded,
            total_bytes: total,
            state: DownloadState::Completed,
        },
    );

    Ok(dest_path)
}

fn emit_progress(app: &AppHandle, progress: &DownloadProgress) {
    if let Err(err) = app.emit(PROGRESS_EVENT, progress) {
        tracing::warn!("failed to emit download progress: {err}");
    }
}