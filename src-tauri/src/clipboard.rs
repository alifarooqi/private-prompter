//! Read and replace the user's selected text via the macOS Accessibility
//! API.
//!
//! Why we don't use the clipboard dance (⌘C → read → write → ⌘V):
//!
//!   * Reading via simulated ⌘C requires our process to have Input
//!     Monitoring permission; without it, enigo's `event.post()` SIGSEGVs
//!     the process. (We tested this — the OS kills us without a useful
//!     error message.)
//!   * Even with both Accessibility and IM granted, System Events'
//!     synthetic `key code 9 using {command down}` is silently ignored
//!     by some apps because they distinguish synthetic events from real
//!     hardware events via `CGEventSource`.
//!
//! The Accessibility API sidesteps both: `kAXSelectedTextAttribute` is
//! directly settable on editable text fields. We read the selection,
//! build the rewrite, then write it back as the new selection — no
//! clipboard, no synthetic keystrokes.
//!
//! Trade-off: rich-text formatting in the replacement is plain text only.
//! For our use case (replacing selection with a fresh rewritten string)
//! that's exactly right.

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};
use std::ffi::c_void;

#[derive(Debug, thiserror::Error)]
pub enum SelectedTextError {
    #[error("accessibility permission denied — grant it in System Settings → Privacy & Security → Accessibility")]
    AccessibilityDenied,
    #[error("no focused text element")]
    NoFocus,
    #[error("selected text attribute not readable on this element")]
    NotReadable,
    #[error("selected text attribute not settable on this element")]
    NotSettable,
    #[error("ax error: {0}")]
    Ax(String),
}

/// Captured at the start of a streaming replace. Holds onto the focused
/// AX element + the start of the original selection, and tracks how many
/// characters have been written so far so subsequent per-chunk writes
/// append after the previous chunk.
///
/// Strategy detection happens on the first write:
///   1. Try `kAXSelectedTextAttribute` (Strategy 1 — works on native
///      NSTextView and `<input>`/`<textarea>`). If the write succeeds
///      AND a verification readback matches what we wrote, the anchor
///      commits to Strategy 1 for the rest of the stream.
///   2. Otherwise, fall back to Strategy 2 (`kAXValueAttribute` with
///      manual range splice — works on browser `contenteditable` divs
///      like Gemini's composer or Notion's editor which reject
///      `AXSelectedText` writes).
///
/// If the user clicks elsewhere mid-stream, the next `reselect` may
/// fail but the write will still attempt at whatever the current
/// selection is — a minor misplacement, never a panic.
///
/// The anchor owns its `NSAutoreleasePool` so `CFString` instances
/// allocated inside the chunk loop stay alive for the duration of
/// the streaming replace. The pool is drained in `Drop`.
pub struct ReplaceAnchor {
    system: *mut AnyObject,
    focused: *mut AnyObject,
    /// Where the original selection started. First write lands here.
    start: i64,
    /// Length of the original selection — used for the first write so
    /// it cleanly replaces the original text instead of inserting.
    original_len: i64,
    /// Total chars we have written so far across all chunks (after the
    /// first write, this is the position the next chunk should append at).
    written_len: i64,
    /// Strategy chosen on the first write. `None` until we attempt the
    /// first write and discover what works on this element.
    strategy: Option<Strategy>,
    /// For Strategy 2: the original `AXValue` text outside the
    /// selection range (`value[..start]`). We cache this so we don't
    /// have to re-read the entire document on every chunk.
    before: Option<String>,
    /// For Strategy 2: the original `AXValue` text after the selection
    /// range (`value[end..]`). Same reason as `before`.
    after: Option<String>,
    /// End of the original selection. Strategy 2 splices
    /// `before + accumulated + after` on each write.
    end: i64,
    pool: *mut AnyObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    /// kAXSelectedTextAttribute — works on NSTextView / input / textarea.
    SelectedText,
    /// kAXValueAttribute splice — works on contenteditable divs.
    ValueSplice,
}

impl ReplaceAnchor {
    /// Position the caret at `target` (location, length) for the next
    /// `AXSelectedText` write. Only meaningful in Strategy 1.
    fn select(&self, location: i64, length: i64) -> bool {
        set_attr_range(
            self.focused,
            "AXSelectedTextRange",
            location as usize,
            length as usize,
            self.pool,
        )
    }

    /// First-write path. Tries Strategy 1; on readback mismatch or
    /// failure, falls through to Strategy 2. Either way the anchor is
    /// locked into one strategy for the rest of the stream.
    fn write_first(&mut self, text: &str) -> bool {
        // Try Strategy 1 first.
        if !self.select(self.start, self.original_len) {
            tracing::warn!("ax: streaming anchor first-write reselect failed");
        }
        let s1_status = try_set_selected_text(self.focused, text, self.pool);
        if let Ok(true) = s1_status {
            // Verify readback. Some elements (notably contenteditable)
            // return success but ignore the write.
            let verify = read_attr_string(self.focused, "AXSelectedText");
            if let Ok(current) = verify {
                if current == text {
                    tracing::info!("ax: streaming chose Strategy 1 (AXSelectedText, verified)");
                    self.strategy = Some(Strategy::SelectedText);
                    self.written_len = text.chars().count() as i64;
                    return true;
                }
                tracing::warn!(
                    "ax: Strategy 1 reported success but readback differs (got {} chars, expected {})",
                    current.chars().count(),
                    text.chars().count()
                );
            }
        }
        // Fall through to Strategy 2.
        self.write_first_value_splice(text)
    }

    fn write_first_value_splice(&mut self, text: &str) -> bool {
        let value = match read_attr_string(self.focused, "AXValue") {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("ax: Strategy 2 readback of AXValue failed: {err}");
                return false;
            }
        };
        // Use our captured start/original_len rather than the live
        // range read — the user may have clicked elsewhere between
        // capture and first write, and we want to land where we said
        // we would.
        let start = (self.start as usize).min(value.len());
        let end = ((self.start + self.original_len) as usize).min(value.len());
        let before = value[..start].to_string();
        let after = value[end..].to_string();

        let mut new_value = String::with_capacity(before.len() + text.len() + after.len());
        new_value.push_str(&before);
        new_value.push_str(text);
        new_value.push_str(&after);

        if !set_attr_string(self.focused, "AXValue", &new_value, self.pool) {
            tracing::error!("ax: Strategy 2 setAttr(AXValue) returned non-zero");
            return false;
        }
        let caret = (start + text.chars().count()) as i64;
        let _ = set_attr_range(
            self.focused,
            "AXSelectedTextRange",
            caret as usize,
            0,
            self.pool,
        );

        self.strategy = Some(Strategy::ValueSplice);
        self.before = Some(before);
        self.after = Some(after);
        self.end = end as i64;
        self.written_len = text.chars().count() as i64;
        tracing::info!("ax: streaming chose Strategy 2 (AXValue splice)");
        true
    }

    /// Append a chunk after everything previously written. Routes to
    /// whichever strategy the first write committed us to.
    fn write_append(&mut self, text: &str) -> bool {
        match self.strategy {
            Some(Strategy::SelectedText) => self.write_append_selected_text(text),
            Some(Strategy::ValueSplice) => self.write_append_value_splice(text),
            None => {
                // Shouldn't happen — first write should have set the
                // strategy. Treat as a fresh first write.
                self.write_first(text)
            }
        }
    }

    fn write_append_selected_text(&mut self, text: &str) -> bool {
        // Strategy 1 contract: `text` is the FULL cumulative buffer.
        // We trim to the slice we haven't committed yet and insert
        // that — Strategy 1 inserts at the caret position, so writing
        // the full buffer would duplicate the prefix.
        let total_chars = text.chars().count() as i64;
        if total_chars <= self.written_len {
            // Caller passed nothing new since the last write.
            return true;
        }
        let skip_bytes = text
            .char_indices()
            .nth(self.written_len as usize)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        let slice = &text[skip_bytes..];
        let target = self.start + self.written_len;
        if !self.select(target, 0) {
            tracing::warn!("ax: streaming anchor append reselect failed");
        }
        let ok = try_set_selected_text(self.focused, slice, self.pool).unwrap_or(false);
        if ok {
            self.written_len = total_chars;
        }
        ok
    }

    fn write_append_value_splice(&mut self, text: &str) -> bool {
        // Strategy 2 contract: `text` is the FULL cumulative accumulated
        // buffer (not a delta). We rebuild the document as
        //   before + text + after
        // and write the whole thing back. This is correct for
        // contenteditable but slightly wasteful — every chunk costs one
        // AXValue read (implicit, via set) + one write. The 120 ms
        // throttle keeps the cost manageable.
        let Some(before) = self.before.as_ref() else {
            return false;
        };
        let Some(after) = self.after.as_ref() else {
            return false;
        };
        let mut new_value = String::with_capacity(before.len() + text.len() + after.len());
        new_value.push_str(before);
        new_value.push_str(text);
        new_value.push_str(after);

        if !set_attr_string(self.focused, "AXValue", &new_value, self.pool) {
            tracing::warn!("ax: Strategy 2 append: setAttr(AXValue) returned non-zero");
            return false;
        }
        let caret = ((self.start as usize) + text.chars().count()) as i64;
        let _ = set_attr_range(
            self.focused,
            "AXSelectedTextRange",
            caret as usize,
            0,
            self.pool,
        );
        self.written_len = text.chars().count() as i64;
        true
    }
}

impl Drop for ReplaceAnchor {
    fn drop(&mut self) {
        drain_pool(self.pool);
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> *mut AnyObject;
    fn AXUIElementCopyAttributeValue(
        element: *mut AnyObject,
        attribute: *const AnyObject,
        value: *mut *mut AnyObject,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: *mut AnyObject,
        attribute: *const AnyObject,
        value: *mut AnyObject,
    ) -> i32;
}

/// Read the currently selected text from the focused UI element.
pub fn read_selected_text() -> Result<String, SelectedTextError> {
    let pool = alloc_pool();
    let system = unsafe { AXUIElementCreateSystemWide() };
    if system.is_null() {
        drain_pool(pool);
        return Err(SelectedTextError::NoFocus);
    }

    let focused = copy_attr(system, attr_name("AXFocusedUIElement"))?;
    let raw = copy_attr(focused, attr_name("AXSelectedText"))?;
    let text = cf_string_to_string(raw);
    drain_pool(pool);

    if text.is_empty() {
        // An empty string can mean "no selection" or "no text element". We
        // can't distinguish, but for our caller the difference doesn't
        // matter — both should be reported as "no text selected".
        return Err(SelectedTextError::NotReadable);
    }
    Ok(text)
}

/// Capture the focused element + its current selection range so subsequent
/// per-chunk writes stay anchored to the user's original selection even
/// if the caret moves during streaming.
///
/// The returned anchor owns its own autorelease pool; drop it (or call
/// `replace_anchored` until done) to release the AX refs and any
/// allocated CFStrings.
pub fn capture_replace_anchor() -> Result<ReplaceAnchor, SelectedTextError> {
    let pool = alloc_pool();
    let system = unsafe { AXUIElementCreateSystemWide() };
    if system.is_null() {
        drain_pool(pool);
        return Err(SelectedTextError::NoFocus);
    }
    let focused = copy_attr(system, attr_name("AXFocusedUIElement"))?;
    let range = read_attr_range(focused, "AXSelectedTextRange").map_err(|err| {
        tracing::warn!("ax: capture_replace_anchor: no range: {err}");
        err
    })?;
    Ok(ReplaceAnchor {
        system,
        focused,
        start: range.location,
        original_len: range.length,
        written_len: 0,
        strategy: None,
        before: None,
        after: None,
        end: 0,
        pool,
    })
}

/// Write the cumulative `accumulated` text into the anchored selection.
///
/// `accumulated` must be the **full** buffer of all chunks received so
/// far — the anchor internally tracks how much has been committed via
/// `written_len` and only writes the new portion. This single contract
/// works for both strategies:
///   * Strategy 1 (AXSelectedText): trims `accumulated` to
///     `accumulated[written_len..]` and inserts at the caret position.
///   * Strategy 2 (AXValue splice): uses the full `accumulated` to
///     rebuild the document as `before + accumulated + after`.
///
/// `first = true` forces the first-write path (which performs strategy
/// detection). Subsequent calls pass `first = false`.
///
/// Returns `true` on success, `false` on failure. Callers should not
/// panic on `false` — partial replacement is acceptable during
/// streaming; the final write at end-of-stream is what matters.
pub fn replace_anchored(anchor: &mut ReplaceAnchor, accumulated: &str, first: bool) -> bool {
    if first {
        anchor.write_first(accumulated)
    } else {
        anchor.write_append(accumulated)
    }
}

/// Replace the currently selected text with `new_text`. The focused element
/// must be an editable text field; otherwise `NotSettable` is returned.
///
/// Two strategies, in order:
///   1. `kAXSelectedTextAttribute` — preferred for native text fields
///      (NSTextField, NSTextArea, browser `<input>` and `<textarea>`).
///      Some elements report success but don't actually update (notably
///      browser contenteditable), so we verify with a readback.
///   2. Fallback to `kAXValueAttribute` with manual range replacement —
///      works on browser `contenteditable` divs (Gemini's prompt, ChatGPT's
///      composer, Notion's editor) which expose value+range but don't
///      actually apply kAXSelectedText writes.
pub fn replace_selected_text(new_text: &str) -> Result<(), SelectedTextError> {
    let pool = alloc_pool();
    let system = unsafe { AXUIElementCreateSystemWide() };
    if system.is_null() {
        drain_pool(pool);
        return Err(SelectedTextError::NoFocus);
    }

    let focused = copy_attr(system, attr_name("AXFocusedUIElement"))?;

    // Strategy 1: settable AXSelectedText.
    let s1 = try_set_selected_text(focused, new_text, pool);
    tracing::info!("ax: strategy 1 (AXSelectedText) → {:?}", s1);
    if let Ok(true) = s1 {
        // Verify it actually took. Some browser elements return success
        // but ignore the write.
        let verify = read_attr_string(focused, "AXSelectedText");
        match &verify {
            Ok(current) if current == new_text => {
                tracing::info!("ax: strategy 1 verified, replacement OK");
                drain_pool(pool);
                return Ok(());
            }
            Ok(current) => {
                tracing::warn!(
                    "ax: strategy 1 reported success but readback differs (got {} chars, expected {})",
                    current.chars().count(),
                    new_text.chars().count()
                );
                // Fall through to Strategy 2.
            }
            Err(err) => {
                tracing::warn!("ax: strategy 1 readback failed: {err}");
            }
        }
    }

    // Strategy 2: read full value + selection range, splice new_text in,
    // set value back, restore caret.
    let full_value = read_attr_string(focused, "AXValue");
    let range = read_attr_range(focused, "AXSelectedTextRange");
    tracing::info!(
        "ax: strategy 2 reads: value len={:?}, range={:?}",
        full_value.as_ref().map(|s| s.chars().count()).unwrap_or(0),
        range.as_ref().ok().map(|r| (r.location, r.length))
    );

    if let (Ok(value), Ok(range)) = (&full_value, &range) {
        let mut new_value = String::with_capacity(value.len() + new_text.len());
        let start = (range.location as usize).min(value.len());
        let end = ((range.location + range.length) as usize).min(value.len());
        new_value.push_str(&value[..start]);
        new_value.push_str(new_text);
        new_value.push_str(&value[end..]);

        if set_attr_string(focused, "AXValue", &new_value, pool) {
            let caret = start + new_text.chars().count();
            set_attr_range(focused, "AXSelectedTextRange", caret, 0, pool);
            tracing::info!("ax: strategy 2 wrote {} chars", new_value.chars().count());
            drain_pool(pool);
            return Ok(());
        } else {
            tracing::warn!("ax: strategy 2 setAttr(AXValue) returned non-zero");
        }
    } else {
        tracing::warn!(
            "ax: strategy 2 missing preconditions: value_ok={}, range_ok={}",
            full_value.is_ok(),
            range.is_ok()
        );
    }

    drain_pool(pool);
    Err(SelectedTextError::NotSettable)
}

fn try_set_selected_text(
    focused: *mut AnyObject,
    new_text: &str,
    pool: *mut AnyObject,
) -> Result<bool, SelectedTextError> {
    let ns_string_class = AnyClass::get("NSString").ok_or_else(|| {
        drain_pool(pool);
        SelectedTextError::Ax("NSString class not found".to_string())
    })?;
    let new_ns: *mut AnyObject = unsafe {
        let c_string = std::ffi::CString::new(new_text).unwrap_or_default();
        msg_send![ns_string_class, stringWithUTF8String: c_string.as_ptr()]
    };

    let status =
        unsafe { AXUIElementSetAttributeValue(focused, attr_name("AXSelectedText"), new_ns) };
    Ok(status == 0)
}

fn read_attr_string(focused: *mut AnyObject, attr: &str) -> Result<String, SelectedTextError> {
    let raw = copy_attr(focused, attr_name(attr))?;
    Ok(cf_string_to_string(raw))
}

fn read_attr_range(focused: *mut AnyObject, attr: &str) -> Result<AxRange, SelectedTextError> {
    let raw = copy_attr(focused, attr_name(attr))?;
    Ok(ax_range_from_ns_value(raw))
}

fn set_attr_string(focused: *mut AnyObject, attr: &str, value: &str, pool: *mut AnyObject) -> bool {
    let Some(ns_string_class) = AnyClass::get("NSString") else {
        return false;
    };
    let new_ns: *mut AnyObject = unsafe {
        let c_string = std::ffi::CString::new(value).unwrap_or_default();
        msg_send![ns_string_class, stringWithUTF8String: c_string.as_ptr()]
    };
    let status = unsafe { AXUIElementSetAttributeValue(focused, attr_name(attr), new_ns) };
    let _ = pool;
    status == 0
}

fn set_attr_range(
    focused: *mut AnyObject,
    attr: &str,
    location: usize,
    length: usize,
    pool: *mut AnyObject,
) -> bool {
    let Some(ns_value_class) = AnyClass::get("NSValue") else {
        return false;
    };
    // AXValueRef range is encoded as two i64s in a single NSValue. The
    // objCType encoding matches `_NSRange` (the same struct NSTextView
    // produces when you ask it for its selection): a struct of two
    // unsigned long longs. Using `Q` (unsigned long long) rather than
    // `q` (signed long long) avoids sign-extension surprises when the
    // range location is large.
    let bytes: [u8; 16] = {
        let loc = (location as u64).to_ne_bytes();
        let len = (length as u64).to_ne_bytes();
        let mut b = [0u8; 16];
        b[..8].copy_from_slice(&loc);
        b[8..].copy_from_slice(&len);
        b
    };
    let range_value: *mut AnyObject = unsafe {
        let ptr = bytes.as_ptr() as *const c_void;
        msg_send![ns_value_class, valueWithBytes: ptr objCType: c"{_NSRange=QQ}".as_ptr()]
    };
    if range_value.is_null() {
        return false;
    }
    let status = unsafe { AXUIElementSetAttributeValue(focused, attr_name(attr), range_value) };
    let _ = pool;
    status == 0
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct AxRange {
    location: i64,
    length: i64,
}

/// Read an AX range (kAXSelectedTextRangeAttribute, kAXVisibleCharacterRangeAttribute,
/// kAXInsertionPointLineNumberAttribute) from a returned NSValue.
///
/// The on-the-wire encoding matches `_NSRange`: two unsigned long longs
/// (16 bytes). We read the first 16 bytes of the NSValue's internal
/// storage; the encoding must match what we produce in `set_attr_range`
/// (signed/unsigned mismatches corrupt the readback).
fn ax_range_from_ns_value(raw: *mut AnyObject) -> AxRange {
    unsafe {
        let bytes = std::slice::from_raw_parts(raw as *const u8, 16);
        let mut loc_bytes = [0u8; 8];
        let mut len_bytes = [0u8; 8];
        loc_bytes.copy_from_slice(&bytes[..8]);
        len_bytes.copy_from_slice(&bytes[8..]);
        AxRange {
            location: u64::from_ne_bytes(loc_bytes) as i64,
            length: u64::from_ne_bytes(len_bytes) as i64,
        }
    }
}

fn alloc_pool() -> *mut AnyObject {
    let Some(pool_class) = AnyClass::get("NSAutoreleasePool") else {
        return std::ptr::null_mut();
    };
    unsafe { msg_send![pool_class, new] }
}

fn drain_pool(pool: *mut AnyObject) {
    if pool.is_null() {
        return;
    }
    unsafe {
        let _: () = msg_send![pool, drain];
    }
}

fn attr_name(name: &str) -> *const AnyObject {
    let Some(ns_string_class) = AnyClass::get("NSString") else {
        return std::ptr::null();
    };
    unsafe {
        let c_string = std::ffi::CString::new(name).unwrap_or_default();
        let result: *mut AnyObject =
            msg_send![ns_string_class, stringWithUTF8String: c_string.as_ptr()];
        result as *const AnyObject
    }
}

fn copy_attr(
    element: *mut AnyObject,
    attribute: *const AnyObject,
) -> Result<*mut AnyObject, SelectedTextError> {
    let mut out: *mut AnyObject = std::ptr::null_mut();
    let status = unsafe { AXUIElementCopyAttributeValue(element, attribute, &mut out) };
    if status != 0 {
        // -25205 = attribute unsupported on this element (e.g. window has no
        // selected-text attr because it's not a text field).
        return Err(SelectedTextError::NotReadable);
    }
    if out.is_null() {
        return Err(SelectedTextError::NoFocus);
    }
    Ok(out)
}

fn cf_string_to_string(cf: *mut AnyObject) -> String {
    if cf.is_null() {
        return String::new();
    }
    unsafe {
        let utf8_ptr: *const i8 = msg_send![cf, UTF8String];
        if utf8_ptr.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(utf8_ptr)
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn module_compiles_without_ax_runtime() {
        // Sanity: the AX-dependent branches can't be exercised from cargo
        // test (no AX without an app context), but the module needs to
        // compile cleanly so we know we haven't broken the FFI signatures.
        let _: &str = "";
    }
}
