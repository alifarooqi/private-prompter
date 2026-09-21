//! Undo stack for hotkey rewrites.
//!
//! Each successful rewrite pushes an entry onto the stack; the user can
//! "undo" the most recent action via `Cmd+Shift+Z` or a notification button
//! (Phase 8 wires the notification). Entries auto-expire after `TTL_SECS`
//! and the stack is capped at `MAX_ENTRIES`.
//!
//! Persistence is best-effort: on app start we re-read the on-disk stack
//! from `data_dir()/undo.json`. Writes are atomic (write+rename).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

use crate::model::store as store_paths;

const TTL_SECS: u64 = 10;
const MAX_ENTRIES: usize = 20;
const FILE_NAME: &str = "undo.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoEntry {
    /// What the clipboard held *before* the rewrite. Pasting this restores
    /// the user's prior state.
    pub original_clipboard: String,
    /// What we wrote during the rewrite.
    pub rewritten: String,
    /// Unix seconds when the entry was created.
    pub created_at_unix: u64,
    pub model_id: String,
    pub template_id: String,
    /// Optional free-form context to show in the notification (frontmost app
    /// name, etc.). Truncated to keep the payload small.
    pub context_label: Option<String>,
}

impl UndoEntry {
    fn is_fresh(&self, now_unix: u64) -> bool {
        now_unix.saturating_sub(self.created_at_unix) < TTL_SECS
    }
}

#[derive(Default)]
pub struct UndoState {
    inner: Mutex<Vec<UndoEntry>>,
}

pub type SharedUndoState = Arc<UndoState>;

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl UndoState {
    pub async fn push(&self, entry: UndoEntry) {
        let mut guard = self.inner.lock().await;
        guard.push(entry);
        if guard.len() > MAX_ENTRIES {
            let excess = guard.len() - MAX_ENTRIES;
            guard.drain(..excess);
        }
    }

    pub async fn pop_fresh(&self) -> Option<UndoEntry> {
        let now = now_unix();
        let mut guard = self.inner.lock().await;
        while let Some(entry) = guard.pop() {
            if entry.is_fresh(now) {
                return Some(entry);
            }
        }
        None
    }

    pub async fn clear(&self) {
        self.inner.lock().await.clear();
    }

    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }
}

/// Read the persisted stack from disk. Returns an empty stack on a missing
/// or malformed file — losing undo history is preferable to crashing.
pub async fn load(app: &AppHandle) -> Vec<UndoEntry> {
    let path = undo_path(app);
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub async fn save(app: &AppHandle, entries: &[UndoEntry]) {
    let path = undo_path(app);
    let Ok(bytes) = serde_json::to_vec_pretty(entries) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if tokio::fs::write(&tmp, &bytes).await.is_ok() {
        let _ = tokio::fs::rename(&tmp, &path).await;
    }
}

fn undo_path(_app: &AppHandle) -> PathBuf {
    store_paths::data_dir().join(FILE_NAME)
}

/// Helper used by the notification flow to format the "Undo available for
/// 10s" label.
pub fn ttl_remaining(entry: &UndoEntry) -> Duration {
    let now = now_unix();
    let elapsed = now.saturating_sub(entry.created_at_unix);
    Duration::from_secs(TTL_SECS.saturating_sub(elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_at(unix: u64) -> UndoEntry {
        UndoEntry {
            original_clipboard: "orig".into(),
            rewritten: "new".into(),
            created_at_unix: unix,
            model_id: "m".into(),
            template_id: "t".into(),
            context_label: None,
        }
    }

    #[test]
    fn freshness_uses_ttl() {
        let e = entry_at(1000);
        assert!(e.is_fresh(1009));
        assert!(!e.is_fresh(1011));
    }

    #[tokio::test]
    async fn push_caps_stack() {
        let s = UndoState::default();
        for i in 0..(MAX_ENTRIES + 5) {
            s.push(entry_at(i as u64)).await;
        }
        assert_eq!(s.len().await, MAX_ENTRIES);
    }
}
