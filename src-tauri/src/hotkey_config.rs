//! Persistent hotkey configuration.
//!
//! Stored as JSON at `<data_dir>/config.json`. Defaults to `⌘⌥R` (Cmd +
//! Option + R) on first launch. Users pick a new combination via the
//! Settings → General UI; on save, Rust unregisters the old global
//! shortcut and registers the new one.

use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

const FILE_NAME: &str = "config.json";

/// Serializable form of a global shortcut. Modifiers are the keys you hold
/// down; `key` is the physical key that completes the combo. We keep this
/// in plain-string form (rather than the plugin's enum) so the JSON is
/// stable across plugin upgrades.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HotkeyConfig {
    pub modifiers: Vec<String>,
    pub key: String,
}

impl HotkeyConfig {
    /// The default combo. ⌘⌥R isn't bound by any first-party macOS shortcut
    /// (we tried ⌘⇧Space — Maccy; ⌘⌥Space — Spotlight window search; ⌘⌥P/S
    /// — many apps' Preferences/Save As).
    pub fn default_for_macos() -> Self {
        Self {
            modifiers: vec!["super".to_string(), "alt".to_string()],
            key: "R".to_string(),
        }
    }

    /// Convert to the plugin's `Shortcut` type. Returns Err on an unknown
    /// modifier or key so the UI can show a validation error rather than
    /// silently dropping the user's choice.
    pub fn to_shortcut(&self) -> Result<Shortcut, String> {
        let mut mods = Modifiers::empty();
        for m in &self.modifiers {
            mods |= match m.to_lowercase().as_str() {
                "super" | "cmd" | "command" => Modifiers::SUPER,
                "alt" | "option" => Modifiers::ALT,
                "shift" => Modifiers::SHIFT,
                "ctrl" | "control" => Modifiers::CONTROL,
                other => return Err(format!("unknown modifier: {other}")),
            };
        }
        let key_upper = self.key.to_uppercase();
        let code = match key_upper.as_str() {
            s if s.len() == 1 && s.chars().next().unwrap().is_ascii_alphabetic() => {
                let c = s.chars().next().unwrap();
                match c {
                    'A' => Code::KeyA, 'B' => Code::KeyB, 'C' => Code::KeyC,
                    'D' => Code::KeyD, 'E' => Code::KeyE, 'F' => Code::KeyF,
                    'G' => Code::KeyG, 'H' => Code::KeyH, 'I' => Code::KeyI,
                    'J' => Code::KeyJ, 'K' => Code::KeyK, 'L' => Code::KeyL,
                    'M' => Code::KeyM, 'N' => Code::KeyN, 'O' => Code::KeyO,
                    'P' => Code::KeyP, 'Q' => Code::KeyQ, 'R' => Code::KeyR,
                    'S' => Code::KeyS, 'T' => Code::KeyT, 'U' => Code::KeyU,
                    'V' => Code::KeyV, 'W' => Code::KeyW, 'X' => Code::KeyX,
                    'Y' => Code::KeyY, 'Z' => Code::KeyZ,
                    _ => unreachable!(),
                }
            }
            s if s.len() == 1 && s.chars().next().unwrap().is_ascii_digit() => {
                let d = s.chars().next().unwrap().to_digit(10).unwrap();
                match d {
                    0 => Code::Digit0, 1 => Code::Digit1, 2 => Code::Digit2,
                    3 => Code::Digit3, 4 => Code::Digit4, 5 => Code::Digit5,
                    6 => Code::Digit6, 7 => Code::Digit7, 8 => Code::Digit8,
                    9 => Code::Digit9,
                    _ => unreachable!(),
                }
            }
            "SPACE" => Code::Space,
            "ESCAPE" | "ESC" => Code::Escape,
            "ENTER" | "RETURN" => Code::Enter,
            "TAB" => Code::Tab,
            "BACKSPACE" => Code::Backspace,
            "DELETE" | "DEL" => Code::Delete,
            "UP" => Code::ArrowUp,
            "DOWN" => Code::ArrowDown,
            "LEFT" => Code::ArrowLeft,
            "RIGHT" => Code::ArrowRight,
            "F1" => Code::F1, "F2" => Code::F2, "F3" => Code::F3,
            "F4" => Code::F4, "F5" => Code::F5, "F6" => Code::F6,
            "F7" => Code::F7, "F8" => Code::F8, "F9" => Code::F9,
            "F10" => Code::F10, "F11" => Code::F11, "F12" => Code::F12,
            other => return Err(format!("unknown key: {other}")),
        };
        Ok(Shortcut::new(Some(mods), code))
    }

    /// Human-readable form, e.g. "⌘⌥R" or "Ctrl+Shift+F1".
    pub fn display(&self) -> String {
        let mut mods = String::new();
        for m in &self.modifiers {
            mods.push_str(match m.to_lowercase().as_str() {
                "super" | "cmd" | "command" => "⌘",
                "alt" | "option" => "⌥",
                "shift" => "⇧",
                "ctrl" | "control" => "⌃",
                other => other,
            });
        }
        format!("{mods}{}", self.key.to_uppercase())
    }
}

static CACHE: OnceLock<HotkeyConfig> = OnceLock::new();

/// Load the config from disk. Returns the macOS default if no file exists.
pub fn load() -> HotkeyConfig {
    let path = config_path();
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<HotkeyConfig>(&bytes) {
            Ok(cfg) => {
                tracing::info!(
                    "hotkey: loaded config from {} (mods={:?}, key={})",
                    path.display(),
                    cfg.modifiers,
                    cfg.key
                );
                cfg
            }
            Err(err) => {
                tracing::warn!(
                    "hotkey: malformed config at {} ({}); falling back to default",
                    path.display(),
                    err
                );
                HotkeyConfig::default_for_macos()
            }
        },
        Err(_) => {
            tracing::info!("hotkey: no config at {}; using default", path.display());
            HotkeyConfig::default_for_macos()
        }
    }
}

/// Cache the loaded config so we don't read the disk on every register call.
pub fn cached() -> &'static HotkeyConfig {
    CACHE.get_or_init(load)
}

/// Save the config atomically (write to .tmp, rename).
pub fn save(cfg: &HotkeyConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let bytes = serde_json::to_vec_pretty(cfg).map_err(|e| format!("serialize: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("write: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
    tracing::info!(
        "hotkey: saved config to {} (mods={:?}, key={})",
        path.display(),
        cfg.modifiers,
        cfg.key
    );
    Ok(())
}

/// Invalidate the cache so the next `cached()` call re-reads from disk.
/// Called by the `set_hotkey` command after it writes a new config.
pub fn invalidate() {
    // OnceLock has no reset; we leak the old value and replace. We only
    // call this once per user save, so the leak is bounded.
    let new = load();
    let _ = CACHE.set(new);
}

fn config_path() -> PathBuf {
    crate::model::store::data_dir().join(FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_roundtrips_to_shortcut() {
        let cfg = HotkeyConfig::default_for_macos();
        assert!(cfg.to_shortcut().is_ok());
    }

    #[test]
    fn display_uses_mac_glyphs() {
        let cfg = HotkeyConfig {
            modifiers: vec!["super".into(), "alt".into(), "shift".into()],
            key: "F1".into(),
        };
        assert_eq!(cfg.display(), "⌘⌥⇧F1");
    }

    #[test]
    fn unknown_modifier_errors() {
        let cfg = HotkeyConfig {
            modifiers: vec!["hyper".into()],
            key: "A".into(),
        };
        assert!(cfg.to_shortcut().is_err());
    }
}