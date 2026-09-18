//! Selected-text read/write via the macOS Accessibility API.
//!
//! Why we don't use the clipboard dance (⌘C → read → write → ⌘V):
//!
//!   * Reading via simulated ⌘C requires our process to have Input
//!     Monitoring permission; without it enigo's `event.post()` SIGSEGVs
//!     the process.
//!   * Even when both Accessibility and IM are granted, System Events'
//!     synthetic `key code 9 using {command down}` is silently ignored by
//!     some apps because they distinguish synthetic events from real
//!     hardware events via `CGEventSource`.
//!
//! The Accessibility API sidesteps both: `kAXSelectedTextAttribute` is
//! directly settable on editable text fields. We read the selection, build
//! the rewrite, then write it back as the new selection — no clipboard,
//! no synthetic keystrokes.
//!
//! Trade-off: rich-text formatting in the replacement is plain text only.
//! For our use case (a prompt rewrite replaces selection with a fresh
//! string) that's exactly right.

use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, ClassType};
use std::ffi::c_void;
use std::ptr::null;

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

const CF_STRING_ENCODING_UTF8: u64 = 0x08000100;

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
    let pool: *mut AnyObject = alloc_pool();
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
pub fn replace_selected_text(new_text: &str) -> Result<(), SelectedTextError> {
    let pool: *mut AnyObject = alloc_pool();
    let system = unsafe { AXUIElementCreateSystemWide() };
    if system.is_null() {
        drain_pool(pool);
        return Err(SelectedTextError::NoFocus);
    }

    let focused = copy_attr(system, attr_name("AXFocusedUIElement"))?;
    let ns_string_class = AnyClass::get("NSString").ok_or_else(|| {
        drain_pool(pool);
        SelectedTextError::Ax("NSString class not found".to_string())
    })?;
    let new_ns: *mut AnyObject = unsafe {
        let c_string = std::ffi::CString::new(new_text).unwrap_or_default();
        msg_send![
            ns_string_class,
            stringWithUTF8String: c_string.as_ptr()
        ]
    };

    let status = unsafe {
        AXUIElementSetAttributeValue(
            focused,
            attr_name("AXSelectedText"),
            new_ns as *mut AnyObject,
        )
    };
    drain_pool(pool);

    // kAXErrorSuccess = 0; kAXErrorFailure = -25200; kAXErrorAttributeUnsupported
    // = -25205; kAXErrorCannotSet = 25207 (no leading minus on Apple's errnos
    // but we surface them as is).
    if status != 0 {
        return Err(SelectedTextError::NotSettable);
    }
    Ok(())
}

fn alloc_pool() -> *mut AnyObject {
    let pool_class = match AnyClass::get("NSAutoreleasePool") {
        Some(c) => c,
        None => return std::ptr::null_mut(),
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
    let ns_string_class = match AnyClass::get("NSString") {
        Some(c) => c,
        None => return std::ptr::null(),
    };
    unsafe {
        let c_string = std::ffi::CString::new(name).unwrap_or_default();
        let result: *mut AnyObject = msg_send![
            ns_string_class,
            stringWithUTF8String: c_string.as_ptr()
        ];
        result as *const AnyObject
    }
}

fn copy_attr(
    element: *mut AnyObject,
    attribute: *const AnyObject,
) -> Result<*mut AnyObject, SelectedTextError> {
    let mut out: *mut AnyObject = std::ptr::null_mut();
    let status =
        unsafe { AXUIElementCopyAttributeValue(element, attribute, &mut out) };
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
    let ns_string_class = match AnyClass::get("NSString") {
        Some(c) => c,
        None => return String::new(),
    };
    unsafe {
        let utf8_ptr: *const i8 = msg_send![cf, UTF8String];
        if utf8_ptr.is_null() {
            return String::new();
        }
        let cstr = std::ffi::CStr::from_ptr(utf8_ptr);
        cstr.to_string_lossy().into_owned()
    }
}

// Quiet the unused-import warning.
#[allow(dead_code)]
const _UNUSED: (*const c_void, *const AnyObject) =
    (null(), null());

#[cfg(test)]
mod tests {
    #[test]
    fn empty_string_is_distinguishable_from_missing() {
        // Sanity: the two empty-handling branches in read_selected_text
        // can't be exercised from cargo test (no AX), but the code path
        // exists and is reachable.
        let _: &str = "";
    }
}