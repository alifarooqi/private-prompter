//! Model registry, downloader, and storage.
//!
//! Split into submodules so each concern has its own test surface:
//!
//!   * `ram`      — total-RAM detection and the tier classification.
//!   * `registry` — parses `assets/models.json` and exposes lookups.
//!   * `downloader` — streams a GGUF from HuggingFace into our data dir,
//!                    emits progress events the frontend can listen to.
//!   * `store`    — knows where the downloaded model lives on disk.

pub mod downloader;
pub mod ram;
pub mod registry;
pub mod store;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("network: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("model {0} not found in registry")]
    UnknownModel(String),
    #[error("download cancelled")]
    Cancelled,
}

impl serde::Serialize for ModelError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// Snapshot of a model download in flight — emitted as a Tauri event so the
/// Settings → Model tab can render a progress bar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model_id: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub state: DownloadState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Started,
    Progress,
    Verifying,
    Completed,
    Failed,
}