//! Tauri command handlers exposed to the frontend.
//!
//! Each submodule owns one area (permissions, model management). The frontend
//! calls them with `invoke("name", args)`.

pub mod active_model;
pub mod hotkey;
pub mod model;
pub mod permissions;
