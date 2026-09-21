//! Filesystem layout for downloaded models and templates.
//!
//! Centralized so the rest of the app doesn't hard-code paths. The data dir
//! is resolved at app boot via Tauri's `Manager::path().app_data_dir()`,
//! which honors the bundle ID directly. On macOS that resolves to
//! `~/Library/Application Support/<bundle-identifier>/`.
//!
//! We can't compute that at module load (it requires an `AppHandle`), so
//! `set_data_dir()` is called from `setup` in lib.rs and the rest of the
//! code reads the resolved path through `data_dir()`. Tests that don't go
//! through lib.rs get a fallback to `$TMP/<bundle-id>`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const BUNDLE_ID: &str = "com.alifarooqi.privateprompter";

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Set the resolved data dir. Called once from `lib.rs` setup.
pub fn set_data_dir(path: PathBuf) {
    DATA_DIR
        .set(path)
        .expect("set_data_dir called more than once");
}

/// Returns the resolved data dir, or a temp fallback for unit tests that
/// don't go through lib.rs.
pub fn data_dir() -> PathBuf {
    DATA_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::temp_dir().join(BUNDLE_ID))
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

pub fn templates_dir() -> PathBuf {
    data_dir().join("templates")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn model_path(model_id: &str, file_name: &str) -> PathBuf {
    models_dir().join(model_id).join(file_name)
}

pub fn ensure_models_dir(model_id: &str) -> std::io::Result<PathBuf> {
    let dir = models_dir().join(model_id);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn ensure_app_data_dir() -> std::io::Result<&'static Path> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(Box::leak(dir.into_boxed_path()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_under_app_data_dir() {
        assert!(models_dir().ends_with("models"));
        assert!(templates_dir().ends_with("templates"));
        assert!(logs_dir().ends_with("logs"));
    }

    #[test]
    fn model_path_isolates_models_into_subdirs() {
        let p = model_path("foo", "bar.gguf");
        assert!(p.ends_with("models/foo/bar.gguf"));
    }
}
