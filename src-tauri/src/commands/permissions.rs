//! macOS permission checks.
//!
//! `check_accessibility_permission` is called by the onboarding view (and the
//! "Recheck" button in Settings → Permissions). We probe Accessibility by
//! asking System Events for the name of the frontmost process — a non-trivial
//! Apple Event that returns an error if Accessibility hasn't been granted.
//!
//! `open_accessibility_settings` opens System Settings to the Accessibility
//! pane so the user can grant the permission without hunting through menus.

use serde::Serialize;

#[derive(Serialize)]
pub struct PermissionStatus {
    pub granted: bool,
    /// Best-effort explanation when `granted` is false. Useful for the UI.
    pub detail: Option<String>,
}

/// Returns whether macOS Accessibility permission is currently granted to us.
///
/// We shell out to `osascript` because `osascript` calls go through the same
/// permission gate the user is granting us; if Accessibility is denied the
/// process returns a non-zero status. We avoid linking `objc2` directly here
/// to keep the dependency surface small for Phase 1.
#[tauri::command]
pub fn check_accessibility_permission() -> PermissionStatus {
    let probe = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to return name of first application process whose frontmost is true",
        ])
        .output();

    match probe {
        Ok(out) if out.status.success() => PermissionStatus {
            granted: true,
            detail: None,
        },
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            PermissionStatus {
                granted: false,
                detail: Some(if stderr.is_empty() {
                    "Accessibility permission is required.".to_string()
                } else {
                    stderr
                }),
            }
        }
        Err(err) => PermissionStatus {
            granted: false,
            detail: Some(format!("Failed to probe Accessibility: {err}")),
        },
    }
}

/// Opens System Settings → Privacy & Security → Accessibility.
///
/// Uses the `x-apple.systempreferences:` URL scheme, which opens the right
/// pane directly on macOS 13+. On older macOS versions this falls through to
/// the generic System Settings window — still better than nothing.
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_with_grandred_field() {
        let s = PermissionStatus {
            granted: true,
            detail: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"granted\":true"));
        assert!(json.contains("\"detail\":null"));
    }
}