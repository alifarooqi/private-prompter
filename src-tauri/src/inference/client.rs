//! HTTP client for `llama-server`.
//!
//! Phase 8 wires up streaming SSE and a cancellation handle. Phase 5 used a
//! placeholder rewrite in the hotkey module; that placeholder is now a
//! call into `complete_streaming`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};

use super::InferenceError;

const STREAM_EVENT: &str = "inference://stream";

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct CompletionRequest {
    pub prompt: String,
    pub n_predict: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub stop: Vec<String>,
    pub stream: bool,
}

impl CompletionRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            n_predict: 1024,
            temperature: 0.7,
            top_p: 0.95,
            stop: vec!["</s>".to_string()],
            stream: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct CompletionChunk {
    pub content: String,
    pub stop: bool,
}

/// Cancel handle shared between the hotkey (cancels via second hotkey press)
/// and the inference client.
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

/// Non-streaming completion. Used by the inference tests.
pub async fn complete_blocking(
    base_url: &str,
    request: &CompletionRequest,
) -> Result<String, InferenceError> {
    let url = format!("{base_url}/completion");
    let client = reqwest::Client::new();
    let mut req = request.clone();
    req.stream = false;
    let resp = client.post(&url).json(&req).send().await?;
    let body: serde_json::Value = resp.json().await?;
    Ok(body["content"].as_str().unwrap_or_default().to_string())
}

/// Streaming completion. Yields chunks to the caller; emits `STREAM_EVENT` so
/// the frontend can update its UI. Stops on cancel or stop token.
pub async fn complete_streaming<R: tauri::Runtime, F>(
    app: &tauri::AppHandle<R>,
    base_url: &str,
    request: &CompletionRequest,
    cancel: CancellationToken,
    mut on_chunk: F,
) -> Result<String, InferenceError>
where
    F: FnMut(CompletionChunk) + Send,
{
    if cancel.is_cancelled() {
        return Err(InferenceError::NotRunning);
    }

    let url = format!("{base_url}/completion");
    let client = reqwest::Client::new();
    let mut req = request.clone();
    req.stream = true;

    let response = client.post(&url).json(&req).send().await?;
    if !response.status().is_success() {
        return Err(InferenceError::NotRunning);
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut full = String::new();

    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(InferenceError::NotRunning);
        }
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // llama-server's /completion stream emits one JSON object per line,
        // terminated by a blank line for end-of-response. We split on '\n' and
        // emit completed lines as chunks.
        let mut pieces: Vec<String> = buffer.split('\n').map(str::to_string).collect();
        // The last piece may be a partial line; keep it in the buffer.
        buffer = pieces.pop().unwrap_or_default();

        for line in pieces {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<LlamaStreamLine>(trimmed) {
                Ok(parsed) => {
                    let content = parsed.content;
                    let stop = parsed.stop;
                    if !content.is_empty() {
                        full.push_str(&content);
                        let cc = CompletionChunk {
                            content: content.clone(),
                            stop,
                        };
                        on_chunk(cc.clone());
                        let _ = app.emit(STREAM_EVENT, &cc);
                    }
                    if stop {
                        return Ok(full);
                    }
                }
                Err(err) => {
                    tracing::warn!("malformed stream line: {err} (line={trimmed:?})");
                }
            }
        }
    }

    Ok(full)
}

#[derive(Debug, Deserialize)]
struct LlamaStreamLine {
    content: String,
    stop: bool,
}