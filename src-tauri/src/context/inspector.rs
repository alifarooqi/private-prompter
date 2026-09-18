//! Tier 1 — fetch the frontmost app and, if it's a browser, its URL/title.
//!
//! Uses AppleScript/JXA through `osascript`. macOS will surface a permission
//! prompt the first time we run it; once Accessibility is granted subsequent
//! calls are silent. The accessibility check is in
//! `commands::permissions::check_accessibility_permission`.

use super::FrontmostApp;

/// Best-effort fetch. Each subsystem failure is silently swallowed and
/// replaced with `None`; the rest of the pipeline still works.
pub fn frontmost_app() -> Option<FrontmostApp> {
    let app_name = osascript("tell application \"System Events\" to return name of first application process whose frontmost is true")
        .ok()
        .filter(|s| !s.is_empty())?;

    // Chrome / Safari / Firefox / Arc / Brave expose their active tab URL
    // via a small AppleScript. Other apps just don't have one.
    let url_and_title = browser_url_and_title(&app_name);
    let (url, title) = match url_and_title {
        Some((u, t)) => (Some(u), Some(t)),
        None => (None, None),
    };

    Some(FrontmostApp {
        name: app_name,
        bundle_id: None, // AppleScript can't give us this; Phase 9 (Polish) can.
        url,
        title,
    })
}

fn browser_url_and_title(app: &str) -> Option<(String, String)> {
    let script = match app {
        "Google Chrome" | "Chromium" | "Brave Browser" | "Arc" | "Microsoft Edge" => {
            r#"
                tell application "System Events"
                    tell process "APP"
                        set theUrl to URL of front window
                        set theTitle to title of front window
                        return theUrl & "|||" & theTitle
                    end tell
                end tell
            "#
        }
        "Safari" => {
            r#"
                tell application "Safari"
                    set theUrl to URL of current tab of front window
                    set theTitle to name of current tab of front window
                    return theUrl & "|||" & theTitle
                end tell
            "#
        }
        "Firefox" => {
            // Firefox doesn't expose URL through AppleScript cleanly; we
            // skip it. Phase 9 (Polish) can wire a Firefox-specific bridge.
            return None;
        }
        _ => return None,
    };

    let script = script.replace("APP", app);
    let out = osascript(&script).ok()?;
    let mut parts = out.splitn(2, "|||");
    let url = parts.next()?.trim().to_string();
    let title = parts.next().unwrap_or("").trim().to_string();
    if url.is_empty() {
        return None;
    }
    Some((url, title))
}

fn osascript(script: &str) -> Result<String, std::io::Error> {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::other(format!(
            "osascript exited with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}