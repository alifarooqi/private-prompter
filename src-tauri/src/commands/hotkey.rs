//! Tauri commands for the hotkey UI.
//!
//! The frontend calls `get_hotkey` on Settings → General mount to render
//! the current shortcut, and `set_hotkey` when the user finishes recording
//! a new one.

use tauri::AppHandle;

use crate::hotkey;
use crate::hotkey_config::{self, HotkeyConfig};
use crate::SharedActiveRewrite;
use crate::SharedUndoState;

#[derive(serde::Serialize)]
pub struct HotkeyInfo {
    pub config: HotkeyConfig,
    pub display: String,
}

#[tauri::command]
pub fn get_hotkey() -> HotkeyInfo {
    let config = hotkey_config::cached().clone();
    let display = config.display();
    HotkeyInfo { config, display }
}

#[tauri::command]
pub async fn set_hotkey(
    app: AppHandle,
    undo_state: tauri::State<'_, SharedUndoState>,
    active: tauri::State<'_, SharedActiveRewrite>,
    config: HotkeyConfig,
) -> Result<HotkeyInfo, String> {
    let active = active.inner().clone();
    let undo_state = undo_state.inner().clone();
    let display = config.display();
    hotkey::reregister(&app, config, undo_state, active)?;
    Ok(HotkeyInfo {
        config: hotkey_config::cached().clone(),
        display,
    })
}

#[tauri::command]
pub fn reset_hotkey() -> HotkeyInfo {
    let cfg = HotkeyConfig::default_for_macos();
    let _ = hotkey_config::save(&cfg);
    hotkey_config::invalidate();
    HotkeyInfo {
        display: cfg.display(),
        config: cfg,
    }
}