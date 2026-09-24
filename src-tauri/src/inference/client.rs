//! HTTP client for `llama-server`.
//!
//! Phase 8 wires up streaming SSE and a cancellation handle. Phase 5 used a
//! placeholder rewrite in the hotkey module; that placeholder is now a
//! call into `complete_streaming`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use super::InferenceError;

const STREAM_EVENT: &str = "inference://stream";

/// Shared reqwest client. Building one per call (the old behaviour) means
/// every hotkey press rebuilds the TLS stack and connection pool; the
/// reqwest 0.12 default builder does keep-alive but we still save the
/// per-call config evaluation, and it gives us a single place to tweak
/// defaults later.
pub(crate) fn shared_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct CompletionRequest {
    pub prompt: String,
    pub n_predict: u32,
    pub temperature: f32,
    pub top_p: f32,
    /// Top-K sampling. 0 disables it; llama-server default is 40. We set
    /// it explicitly so a degenerate repetition loop (where the same
    /// high-prob token keeps winning the top-p cut) gets clipped by the
    /// top-k cut.
    #[serde(default)]
    pub top_k: u32,
    /// Repetition penalty. llama-server default is 1.0 (no penalty).
    /// 1.1 nudges the model away from repeating recent tokens, which is
    /// the single biggest fix for the looped-output problem we saw on
    /// CPU inference with the 1.5B Qwen model.
    #[serde(default = "default_repeat_penalty")]
    pub repeat_penalty: f32,
    pub stop: Vec<String>,
    pub stream: bool,
    /// Reuse llama.cpp's KV cache across calls when the prefix matches.
    /// Pairs with `slot_id` so the cache lives in a stable slot.
    #[serde(default = "default_cache_prompt")]
    pub cache_prompt: bool,
    /// Pin to a single slot so KV cache hits are deterministic. Set
    /// to -1 to let llama-server allocate (safer when state can leak
    /// across requests).
    #[serde(default = "default_slot_id")]
    pub slot_id: i32,
}

fn default_cache_prompt() -> bool {
    true
}

fn default_repeat_penalty() -> f32 {
    1.1
}

fn default_slot_id() -> i32 {
    -1
}

impl CompletionRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            // Realistic prompt-master outputs are 80–200 tokens; cap at
            // 256 to leave headroom for multi-step rewrites without
            // inviting runaway generation on edge cases.
            n_predict: 256,
            temperature: 0.7,
            top_p: 0.95,
            top_k: 40,
            repeat_penalty: 1.1,
            // </s> is Qwen's true EOS; <|im_end|> is what it actually
            // emits at the end of an assistant turn in ChatML framing;
            // <|endoftext|> is the raw base-model stop. Cover all three
            // so we never get tail garbage after a clean answer.
            stop: vec![
                "</s>".to_string(),
                "<|im_end|>".to_string(),
                "<|endoftext|>".to_string(),
            ],
            stream: true,
            cache_prompt: true,
            // Don't pin a slot — slot 0 in particular risks retaining
            // stale generation state between requests, which combined
            // with cache_prompt can cause the model to loop on previous
            // output. Let llama-server pick.
            slot_id: -1,
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
    let client = shared_client();
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
        return Err(InferenceError::Cancelled);
    }

    let url = format!("{base_url}/completion");
    let client = shared_client();
    let mut req = request.clone();
    req.stream = true;

    let response = client.post(&url).json(&req).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        tracing::error!(
            "inference: /completion returned {} body={}",
            status,
            body.chars().take(500).collect::<String>()
        );
        return Err(InferenceError::RequestFailed {
            status: status.as_u16(),
            body: body.chars().take(200).collect::<String>(),
        });
    }
    tracing::info!(
        "inference: streaming started for prompt ({} chars)",
        req.prompt.len()
    );

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut full = String::new();
    let mut line_count: u32 = 0;
    let mut raw_bytes: u64 = 0;

    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(InferenceError::Cancelled);
        }
        let chunk = chunk?;
        raw_bytes += chunk.len() as u64;
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
            // llama-server uses SSE: each event line is "data: <json>",
            // terminated by "[DONE]". Strip the prefix and skip sentinel.
            let payload = trimmed
                .strip_prefix("data:")
                .map(str::trim_start)
                .unwrap_or(trimmed);
            if payload == "[DONE]" {
                tracing::info!(
                    "inference: stream done (SSE [DONE]), {} lines, {} bytes, {} chars",
                    line_count,
                    raw_bytes,
                    full.chars().count()
                );
                return Ok(full);
            }
            line_count += 1;
            match serde_json::from_str::<LlamaStreamLine>(payload) {
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
                        tracing::info!(
                            "inference: stream done, {} lines, {} bytes, {} chars",
                            line_count,
                            raw_bytes,
                            full.chars().count()
                        );
                        return Ok(full);
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        "inference: malformed stream line ({}): {} (line={:?})",
                        line_count,
                        err,
                        &payload[..payload.len().min(200)]
                    );
                }
            }
        }
    }

    tracing::warn!(
        "inference: stream ended without stop token ({} lines, {} bytes, {} chars)",
        line_count,
        raw_bytes,
        full.chars().count()
    );
    Ok(full)
}

#[derive(Debug, Deserialize)]
struct LlamaStreamLine {
    content: String,
    stop: bool,
}
