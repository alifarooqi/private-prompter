//! macOS permission checks.
//!
//! `check_accessibility_permission` calls `AXIsProcessTrustedWithOptions` —
//! the canonical way to ask whether *our own process* has the Accessibility
//! permission. We pass `.prompt = true` on the first call so macOS shows the
//! system permission dialog; subsequent calls are silent.
//!
//! The earlier osascript-based probe was wrong: it asked whether **System
//! Events** was trusted (always true) rather than whether we were. That led
//! to a Settings UI that said "Granted" while the underlying process had no
//! permission — and calling `enigo::key()` from that state crashed the app
//! via a CoreFoundation SIGSEGV. This implementation fixes both.

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use serde::Serialize;

#[derive(Serialize)]
pub struct PermissionStatus {
    pub granted: bool,
    pub detail: Option<String>,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const AnyObject) -> Bool;
}

/// Returns whether macOS Accessibility permission is currently granted to us.
///
/// If `prompt` is true and we're not yet trusted, the system shows the
/// permission dialog. The user can still deny — we surface that as
/// `granted = false`.
#[tauri::command]
pub fn check_accessibility_permission(prompt: bool) -> PermissionStatus {
    match is_process_trusted(prompt) {
        Ok(true) => PermissionStatus {
            granted: true,
            detail: None,
        },
        Ok(false) => PermissionStatus {
            granted: false,
            detail: Some(
                "PrivatePrompter is not in System Settings → Privacy & Security → Accessibility. \
                 Grant it and click Recheck."
                    .to_string(),
            ),
        },
        Err(err) => PermissionStatus {
            granted: false,
            detail: Some(format!("Accessibility probe failed: {err}")),
        },
    }
}

/// Opens System Settings → Privacy & Security → Accessibility.
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map_err(|e| format!("Failed to open System Settings: {e}"))?;
    Ok(())
}

/// Public re-export of `is_process_trusted` for use by the hotkey handler,
/// which needs to bail out before calling enigo when accessibility isn't
/// granted.
pub fn is_accessibility_trusted(prompt: bool) -> Result<bool, String> {
    is_process_trusted(prompt)
}

fn is_process_trusted(prompt: bool) -> Result<bool, String> {
    // Build an NSDictionary { "AXTrustedCheckOptionPrompt" = @YES } and pass
    // it to AXIsProcessTrustedWithOptions. We go through the Objective-C
    // runtime directly so we don't need to pull in `objc2-foundation`.
    unsafe {
        let ns_string_class = AnyClass::get("NSString").ok_or("NSString class not found")?;
        let ns_number_class = AnyClass::get("NSNumber").ok_or("NSNumber class not found")?;
        let ns_dict_class = AnyClass::get("NSDictionary").ok_or("NSDictionary class not found")?;
        let ns_pool_class =
            AnyClass::get("NSAutoreleasePool").ok_or("NSAutoreleasePool class not found")?;

        let pool: *mut AnyObject = msg_send![ns_pool_class, new];

        let dict: *mut AnyObject = if prompt {
            let key_nsstring: *mut AnyObject = msg_send![
                ns_string_class,
                stringWithUTF8String: c"AXTrustedCheckOptionPrompt".as_ptr()
            ];
            let true_number: *mut AnyObject = msg_send![ns_number_class, numberWithBool: true];
            let objects: [*const AnyObject; 2] =
                [true_number as *const AnyObject, std::ptr::null()];
            let keys: [*const AnyObject; 2] = [key_nsstring as *const AnyObject, std::ptr::null()];
            msg_send![
                ns_dict_class,
                dictionaryWithObjects: objects.as_ptr()
                forKeys: keys.as_ptr()
                count: 1usize
            ]
        } else {
            std::ptr::null_mut()
        };

        let trusted: Bool = AXIsProcessTrustedWithOptions(dict as *const AnyObject);

        let _: () = msg_send![pool, drain];

        Ok(trusted.as_bool())
    }
}

// (No FFI placeholder needed — every import is used above.)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_with_granted_field() {
        let s = PermissionStatus {
            granted: true,
            detail: None,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"granted\":true"));
        assert!(json.contains("\"detail\":null"));
    }
}
