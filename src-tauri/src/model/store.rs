//! Filesystem layout for downloaded models and templates.
//!
//! Centralized so the rest of the app doesn't hard-code paths. On macOS this
//! resolves to `~/Library/Application Support/com.alifarooqi.privateprompter/`.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

const APP_DIR: &str = "PrivatePrompter";

pub fn app_data_dir() -> PathBuf {
    ProjectDirs::from("com", "alifarooqi", APP_DIR)
        .map(|p| p.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            // Fallback for headless/test environments where $HOME may not
            // point at a writable place. Use a temp dir so things still work.
            std::env::temp_dir().join(APP_DIR)
        })
}

pub fn models_dir() -> PathBuf {
    app_data_dir().join("models")
}

pub fn templates_dir() -> PathBuf {
    app_data_dir().join("templates")
}

pub fn logs_dir() -> PathBuf {
    app_data_dir().join("logs")
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
    // ProjectDirs is cheap to query repeatedly; just resolve each call.
    // For ergonomics we return a static Path by leaking the PathBuf; this is
    // fine because the dir never moves and we only call it on app start.
    let dir = app_data_dir();
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