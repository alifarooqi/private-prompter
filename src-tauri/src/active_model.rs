//! Persistent "which model should the inference server use" setting.
//!
//! Stored as JSON at `<data_dir>/active_model.json`. A `None` value means
//! the user hasn't picked one — the server start command then falls back
//! to whichever downloaded model comes first in the registry.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "active_model.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveModel(pub Option<String>);

impl ActiveModel {
    pub fn id(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

// Mutex (not OnceLock) so set_active_model can actually replace the value.
// OnceLock::set is one-shot and silently no-ops on the second call — the
// same bug we just fixed in hotkey_config.
static CACHE: Mutex<Option<ActiveModel>> = Mutex::new(None);

/// Read the persisted selection, falling back to None if no file exists
/// or the file is malformed.
pub fn load() -> ActiveModel {
    let path = path();
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<ActiveModel>(&bytes) {
            Ok(cfg) => {
                tracing::info!(
                    "active_model: loaded {} (id={:?})",
                    path.display(),
                    cfg.0
                );
                cfg
            }
            Err(err) => {
                tracing::warn!(
                    "active_model: malformed {} ({}); using default",
                    path.display(),
                    err
                );
                ActiveModel(None)
            }
        },
        Err(_) => {
            tracing::info!("active_model: no config at {}; using default", path.display());
            ActiveModel(None)
        }
    }
}

/// Read the cached value, loading from disk on first call.
pub fn cached() -> ActiveModel {
    let mut guard = CACHE.lock().expect("active_model mutex poisoned");
    if guard.is_none() {
        *guard = Some(load());
    }
    guard.as_ref().expect("just initialized").clone()
}

/// Persist a new selection and refresh the cache.
pub fn set(id: Option<String>) -> Result<(), String> {
    let path = path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let cfg = ActiveModel(id.clone());
    let bytes = serde_json::to_vec_pretty(&cfg).map_err(|e| format!("serialize: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("write: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
    *CACHE.lock().expect("active_model mutex poisoned") = Some(cfg);
    tracing::info!("active_model: saved id={:?} to {}", id, path.display());
    Ok(())
}

fn path() -> PathBuf {
    crate::model::store::data_dir().join(FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_replaces_or_clears() {
        // Both behaviors in one test because they share the same
        // on-disk file; running two separate tests in parallel races
        // the .tmp rename.
        *CACHE.lock().unwrap() = Some(ActiveModel(Some("model-a".into())));

        set(Some("model-b".into())).unwrap();
        assert_eq!(cached().id(), Some("model-b"));

        set(None).unwrap();
        assert_eq!(cached().id(), None);
    }
}