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

    let status = unsafe {
        AXUIElementSetAttributeValue(
            focused,
            attr_name("AXSelectedText"),
            new_ns as *mut AnyObject,
        )
    };
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
    let status =
        unsafe { AXUIElementSetAttributeValue(focused, attr_name(attr), new_ns as *mut AnyObject) };
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
    // AXValueRef range is encoded as two i64s in a single NSValue. We
    // create the value with the bytes layout directly.
    let bytes: [u8; 16] = {
        let loc = (location as i64).to_ne_bytes();
        let len = (length as i64).to_ne_bytes();
        let mut b = [0u8; 16];
        b[..8].copy_from_slice(&loc);
        b[8..].copy_from_slice(&len);
        b
    };
    let range_value: *mut AnyObject = unsafe {
        let ptr = bytes.as_ptr() as *const c_void;
        msg_send![ns_value_class, valueWithBytes: ptr objCType: b"{?=q}{?=q}\0".as_ptr() as *const i8]
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
/// The exact on-the-wire encoding is two i64s. We cheat by reading the
/// first 16 bytes of the NSValue's internal storage. NSValue's encoding
/// matches what `valueWithBytes:objCType:` produces, which we use when
/// writing ranges back.
fn ax_range_from_ns_value(raw: *mut AnyObject) -> AxRange {
    unsafe {
        // NSValue exposes valueWithBytes: and a private ivar layout.
        // For values created with value:valueWithRange: the layout is
        // two i64s back-to-back. We pull 16 bytes from the start of the
        // object — this is the standard encoding for AXValueRef-backed
        // ranges and matches the bytes we'd produce via valueWithBytes:.
        let bytes = std::slice::from_raw_parts(raw as *const u8, 16);
        let mut loc_bytes = [0u8; 8];
        let mut len_bytes = [0u8; 8];
        loc_bytes.copy_from_slice(&bytes[..8]);
        len_bytes.copy_from_slice(&bytes[8..]);
        AxRange {
            location: i64::from_ne_bytes(loc_bytes),
            length: i64::from_ne_bytes(len_bytes),
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
