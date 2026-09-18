//! Clipboard operations: read the highlighted text, write the rewrite back,
//! and simulate `Cmd+C` / `Cmd+V` to bridge our app and the focused window.
//!
//! Phase 5 hotkey flow:
//!   1. `simulate_copy()`         — enigo ⌘C
//!   2. `read_text()`             — arboard grab
//!   3. ... user code calls LLM ...
//!   4. `write_text()`            — arboard put
//!   5. `simulate_paste()`        — enigo ⌘V
//!
//! `enigo` requires Accessibility permission. We surface a clear error so the
//! onboarding view can prompt the user to grant it.

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("accessibility permission denied — cannot simulate keystrokes")]
    AccessibilityDenied,
    #[error("clipboard: {0}")]
    Arboard(String),
    #[error("simulate keystroke: {0}")]
    Enigo(String),
    #[error("clipboard was empty")]
    Empty,
}

pub fn read_text() -> Result<String, ClipboardError> {
    let mut cb = Clipboard::new().map_err(|e| ClipboardError::Arboard(e.to_string()))?;
    cb.get_text()
        .map_err(|e| ClipboardError::Arboard(e.to_string()))
}

pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    let mut cb = Clipboard::new().map_err(|e| ClipboardError::Arboard(e.to_string()))?;
    cb.set_text(text.to_string())
        .map_err(|e| ClipboardError::Arboard(e.to_string()))?;
    Ok(())
}

/// Press `Cmd+C` so the user's highlighted text lands on the clipboard. We
/// rely on the user's selection still being live; if they have nothing
/// highlighted, the clipboard becomes whatever was last there.
pub fn simulate_copy() -> Result<(), ClipboardError> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| ClipboardError::Enigo(e.to_string()))?;
    enigo
        .key(Key::Meta, Direction::Press)
        .map_err(|e| ClipboardError::Enigo(e.to_string()))?;
    enigo
        .key(Key::Unicode('c'), Direction::Click)
        .map_err(|e| ClipboardError::Enigo(e.to_string()))?;
    enigo
        .key(Key::Meta, Direction::Release)
        .map_err(|e| ClipboardError::Enigo(e.to_string()))?;
    Ok(())
}

/// Press `Cmd+V` so the rewritten text pastes into the focused field.
pub fn simulate_paste() -> Result<(), ClipboardError> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| ClipboardError::Enigo(e.to_string()))?;
    enigo
        .key(Key::Meta, Direction::Press)
        .map_err(|e| ClipboardError::Enigo(e.to_string()))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| ClipboardError::Enigo(e.to_string()))?;
    enigo
        .key(Key::Meta, Direction::Release)
        .map_err(|e| ClipboardError::Enigo(e.to_string()))?;
    Ok(())
}

/// Read text from the clipboard, but only if it was actually copied by us or
/// the user within the last few seconds. We don't have that signal here; the
/// caller compares against the original_clipboard_pre_save we stashed on the
/// undo stack to decide whether anything actually changed.
pub fn clipboard_changed(previous: &str, current: &str) -> bool {
    previous != current
}