//! Tauri commands for the active-model selection.
//!
//! The frontend reads `get_active_model` on Settings → Model mount so the
//! radio button reflects the persisted choice. `set_active_model` saves
//! the user's pick to <data_dir>/active_model.json and the next
//! `start_inference` call will use it.

use tauri::State;

use crate::active_model::{self, ActiveModel};
use crate::commands::model::SharedModelState;

#[tauri::command]
pub fn get_active_model() -> ActiveModelInfo {
    let cfg = active_model::cached();
    ActiveModelInfo {
        id: cfg.id().map(str::to_string),
    }
}

#[derive(serde::Serialize)]
pub struct ActiveModelInfo {
    pub id: Option<String>,
}

#[tauri::command]
pub fn set_active_model(
    state: State<'_, SharedModelState>,
    id: Option<String>,
) -> Result<(), String> {
    if let Some(ref id) = id {
        // Validate that the model is in the registry.
        let _ = crate::model::registry::get()
            .find(id)
            .map_err(|e| format!("unknown model: {e}"))?;
        // And that the user has actually downloaded it (or that the file
        // is on disk — we rely on filesystem truth here).
        if !state.downloaded.blocking_lock().contains_key(id) {
            // Double-check the disk before declaring it a failure.
            let entry = crate::model::registry::get()
                .find(id)
                .map_err(|e| e.to_string())?;
            let path = crate::model::store::model_path(&entry.id, &entry.file);
            if !path.exists() {
                return Err(format!("{id} isn't downloaded yet"));
            }
        }
    }
    active_model::set(id)
}
