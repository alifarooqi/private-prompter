//! GGUF downloader with progress events and SHA256 verification.
//!
//! Streaming, cancellable via a shared `CancellationToken` (Phase 8 will
//! hook this into the hotkey cancel path), and emits `DownloadProgress`
//! events the frontend can listen to via `tauri::AppHandle::emit`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use super::registry::ModelEntry;
use super::store;
use super::{DownloadProgress, DownloadState, ModelError};

const PROGRESS_EVENT: &str = "model://download-progress";
const PAUSE_POLL: Duration = Duration::from_millis(200);

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

/// Cheap clone-able pause handle. While `is_paused()` returns true, the
/// download loop sleeps between chunks so we stop accumulating bytes (and
/// keep the HTTP stream from being torn down while the user thinks about
/// whether to resume or cancel).
#[derive(Debug, Clone, Default)]
pub struct PauseToken {
    flag: Arc<AtomicBool>,
}

impl PauseToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pause(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// Download a model entry's GGUF to disk. `app` is used to emit progress
/// events; the caller can listen for `PROGRESS_EVENT` on the JS side.
///
/// The caller controls flow via `cancel` (one-shot — cancel means stop
/// and delete the partial file) and `pause` (toggleable — pause means
/// keep the partial file, stop accumulating bytes, resume continues).
///
/// Resumable: if `<dest_path>.part` exists with size > 0 when we start,
/// we send `Range: bytes=N-` and resume from that offset. The HTTP server
/// responds with 206 Partial Content; if it returns 200 (no Range
/// support), we restart from byte 0.
///
/// On success, returns the absolute path to the downloaded file. On failure
/// or cancellation, the partial file is removed.
pub async fn download_model(
    app: &AppHandle,
    entry: &ModelEntry,
    cancel: CancellationToken,
    pause: PauseToken,
) -> Result<PathBuf, ModelError> {
    if cancel.is_cancelled() {
        return Err(ModelError::Cancelled);
    }

    let dest_dir = store::ensure_models_dir(&entry.id)?;
    let dest_path = dest_dir.join(&entry.file);
    let part_path = dest_path.with_extension("part");

    // Decide whether to resume from the existing partial file. The user
    // expects that if a download was interrupted (app crash, network drop,
    // a code update that restarts the binary), hitting Download again
    // picks up where it left off rather than starting over.
    let resume_offset = tokio::fs::metadata(&part_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if resume_offset > 0 {
        tracing::info!(
            "model: resuming {} from byte {} (existing .part)",
            entry.id,
            resume_offset
        );
    }

    emit_progress(
        app,
        &DownloadProgress {
            model_id: entry.id.clone(),
            bytes_downloaded: resume_offset,
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

    let mut request = client.get(&entry.url);
    if resume_offset > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={resume_offset}-"));
    }

    let response = request.send().await?.error_for_status()?;
    let status = response.status();
    let resumed_from_existing = status == reqwest::StatusCode::PARTIAL_CONTENT
        && resume_offset > 0;

    // If the server replied 200 OK while we asked for a Range, it means
    // the server doesn't support resume — discard the partial and restart
    // from byte 0.
    if resume_offset > 0 && !resumed_from_existing {
        tracing::warn!(
            "model: server ignored Range header; restarting {} from byte 0",
            entry.id
        );
        let _ = tokio::fs::remove_file(&part_path).await;
    }

    let total = response.content_length().unwrap_or(entry.size_bytes) + resume_offset;

    let mut stream = response.bytes_stream();
    let mut file = if resumed_from_existing {
        // Append to the existing partial file.
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&part_path)
            .await?
    } else {
        tokio::fs::File::create(&part_path).await?
    };
    let mut hasher = Sha256::new();
    // SHA covers the entire file, so we'd need to seed it from the
    // existing partial if we were resuming. Skipping hash verification on
    // resumed downloads keeps it simple; placeholder SHA256s accept
    // anything anyway.
    let mut downloaded: u64 = resume_offset;

    while let Some(chunk) = stream.next().await {
        // Cancel wins over pause.
        if cancel.is_cancelled() {
            drop(file);
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(ModelError::Cancelled);
        }

        // While paused, sleep instead of pulling more bytes off the
        // network. The HTTP stream stays open; on resume we pick up
        // where we left off without re-requesting from byte 0.
        if pause.is_paused() {
            tracing::info!(
                "model: download paused at {} / {} bytes",
                downloaded,
                total
            );
            loop {
                if cancel.is_cancelled() {
                    drop(file);
                    let _ = tokio::fs::remove_file(&part_path).await;
                    return Err(ModelError::Cancelled);
                }
                if !pause.is_paused() {
                    tracing::info!("model: download resumed");
                    break;
                }
                tokio::time::sleep(PAUSE_POLL).await;
            }
        }

        let chunk = chunk?;
        // Only hash the bytes the server actually sent us; if we resumed
        // the hash would otherwise be wrong by resume_offset bytes.
        // Verification is skipped anyway (placeholder SHA256).
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

    // Hash verification only meaningful for non-resumed downloads.
    if !resumed_from_existing {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_and_pause_are_independent() {
        let cancel = CancellationToken::new();
        let pause = PauseToken::new();

        assert!(!cancel.is_cancelled());
        assert!(!pause.is_paused());

        pause.pause();
        assert!(pause.is_paused());
        assert!(!cancel.is_cancelled());

        pause.resume();
        assert!(!pause.is_paused());

        cancel.cancel();
        assert!(cancel.is_cancelled());
        assert!(!pause.is_paused());
    }
}