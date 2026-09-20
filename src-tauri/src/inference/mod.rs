//! Embedded `llama.cpp` sidecar management.
//!
//! Phase 3 owns:
//!   * Locating the sidecar binary and the GGUF model.
//!   * Spawning `llama-server` on `127.0.0.1:<random port>`.
//!   * Streaming completions via HTTP (Phase 8 plugs the streaming in).
//!   * Restarting on crash with backoff and tearing down on app quit.
//!
//! Phase 8 adds the streaming SSE client and the cancellation token that
//! Phase 5's hotkey hook will share.

pub mod client;
pub mod server;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

use super::model::store as store_paths;

pub use client::{CompletionChunk, CompletionRequest};

/// Bundle of the sidecar state we share with the frontend.
#[derive(Default)]
pub struct InferenceState {
    pub inner: Mutex<Option<server::RunningServer>>,
}

pub type SharedInferenceState = Arc<InferenceState>;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct ServerStatus {
    pub running: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub current_model_id: Option<String>,
    pub loading: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("server failed to become healthy within {0}s")]
    Unhealthy(u64),
    #[error("model not found in registry: {0}")]
    UnknownModel(String),
    #[error("gguf not downloaded yet for model {0}")]
    ModelNotDownloaded(String),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server not running — start it first")]
    NotRunning,
}

impl serde::Serialize for InferenceError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// Tauri commands for the inference layer. Wired up in lib.rs.
pub mod commands {
    use super::*;
    use crate::active_model;
    use crate::commands::model::SharedModelState;
    use crate::model::registry;

    /// Resolve the model_id the user wants to run inference with. Order:
    ///   1. The user's persisted active_model choice (if still downloaded).
    ///   2. The first model in the downloaded map (HashMap iteration order).
    fn pick_model_id(model_state: &SharedModelState) -> Option<String> {
        let downloaded = model_state.downloaded.blocking_lock();
        if let Some(picked) = active_model::cached().id() {
            if downloaded.contains_key(picked) {
                return Some(picked.to_string());
            }
            // The user picked a model they later deleted. Fall through to
            // any-downloaded so we don't leave the server unstartable.
        }
        downloaded.keys().next().cloned()
    }

    #[tauri::command]
    pub async fn start_inference(
        app: AppHandle,
        state: State<'_, SharedInferenceState>,
        model_state: State<'_, SharedModelState>,
    ) -> Result<ServerStatus, String> {
        let model_id_opt = pick_model_id(&model_state);

        let Some(model_id) = model_id_opt else {
            return Err("No model downloaded yet".to_string());
        };
        let entry = registry::get().find(&model_id).map_err(|e| e.to_string())?;
        let gguf = gguf_path(&entry.id, &entry.file).ok_or_else(|| {
            format!("GGUF not on disk for model {}", entry.id)
        })?;

        let mut guard = state.inner.lock().await;
        if let Some(existing) = guard.as_mut() {
            if existing.is_running() {
                let _ = existing.stop_in_place().await;
            }
        }

        let server = server::RunningServer::start(&app, &gguf, &entry.id)
            .await
            .map_err(|e| e.to_string())?;
        let status = server.status();
        *guard = Some(server);
        Ok(status)
    }

    #[tauri::command]
    pub async fn stop_inference(
        state: State<'_, SharedInferenceState>,
    ) -> Result<(), String> {
        let mut guard = state.inner.lock().await;
        if let Some(server) = guard.take() {
            let _ = server.stop().await;
        }
        Ok(())
    }

    #[tauri::command]
    pub async fn inference_status(
        state: State<'_, SharedInferenceState>,
    ) -> Result<ServerStatus, String> {
        let guard = state.inner.lock().await;
        Ok(match guard.as_ref() {
            Some(s) => s.status(),
            None => ServerStatus {
                running: false,
                host: None,
                port: None,
                current_model_id: None,
                loading: false,
            },
        })
    }

    /// Quick `/health` probe against the running sidecar. Returns null when
    /// the server isn't started yet (so the UI can show the "Start server"
    /// button instead of trying to poll a non-existent endpoint).
    #[tauri::command]
    pub async fn inference_health(
        state: State<'_, SharedInferenceState>,
    ) -> Result<Option<InferenceHealthPayload>, String> {
        let base_url = {
            let guard = state.inner.lock().await;
            guard.as_ref().map(|s| s.base_url().to_string())
        };
        let Some(url) = base_url else {
            return Ok(None);
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .get(format!("{url}/health"))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Ok(Some(InferenceHealthPayload {
                status: "loading",
                model_id: None,
            }));
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let status = body["status"].as_str().unwrap_or("unknown");
        Ok(Some(InferenceHealthPayload {
            status: match status {
                "ok" => "ok",
                "no slot loaded" | "no model loaded" => "no_model",
                _ => "loading",
            },
            model_id: None,
        }))
    }

    #[derive(serde::Serialize, Clone)]
    pub struct InferenceHealthPayload {
        pub status: &'static str,
        pub model_id: Option<String>,
    }

    fn gguf_path(model_id: &str, file: &str) -> Option<PathBuf> {
        let p = store_paths::model_path(model_id, file);
        if p.exists() { Some(p) } else { None }
    }
}