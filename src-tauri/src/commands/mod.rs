//! Tauri command handlers exposed to the frontend.
//!
//! Each submodule owns one area (permissions today; hotkey, clipboard, etc.
//! arrive in later phases). The frontend calls them with `invoke("name", args)`.

pub mod permissions;