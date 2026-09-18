//! System tray icon for PrivatePrompter.
//!
//! PrivatePrompter is a menu-bar-only app (LSUIElement = true in Info.plist).
//! The tray icon is the only always-visible UI; clicking it opens the settings
//! window. We register a small menu in addition to the click handler so users
//! can quit without a window open.

use tauri::{
    image::Image,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

const TRAY_ID: &str = "private-prompter-tray";

/// Bundled tray icon. 32×32 PNG committed to the repo; embedded at compile
/// time so we get an `Image<'static>` without lifetime juggling.
///
/// Phase 9 (Polish & Error Surfaces) replaces this with the project icon
/// once branding is finalized.
const TRAY_ICON_BYTES: &[u8] =
    include_bytes!("../icons/32x32.png");

/// Build and attach the tray to the app. Called from `setup` in lib.rs.
pub fn install<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let icon = load_tray_icon();

    TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("PrivatePrompter")
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(handle_tray_event)
        .build(app)?;

    Ok(())
}

/// Parse the bundled PNG into a `'static` Image. Falls back to a 1×1
/// transparent PNG if the embedded file fails to decode (which would
/// indicate a build-time bug, not a runtime condition).
fn load_tray_icon() -> Image<'static> {
    Image::from_bytes(TRAY_ICON_BYTES)
        .or_else(|_| Ok::<Image<'static>, tauri::Error>(default_icon()))
        .expect("either the bundled icon or the fallback decoded")
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let open = MenuItem::with_id(app, "open_settings", "Open Settings", true, None::<&str>)?;
    let about = MenuItem::with_id(app, "about", "About PrivatePrompter", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    Menu::with_items(app, &[&open, &about, &separator, &quit])
}

fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id.as_ref() {
        "open_settings" => show_main_window(app),
        // "about" reuses the settings window for Phase 1; Phase 9 surfaces
        // a dedicated About panel.
        "about" => show_main_window(app),
        "quit" => app.exit(0),
        _ => {}
    }
}

fn handle_tray_event<R: Runtime>(tray: &tauri::tray::TrayIcon<R>, event: TrayIconEvent) {
    // Left-click the tray icon → open settings. Right-click shows the menu
    // because we set `show_menu_on_left_click(false)`.
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        show_main_window(tray.app_handle());
    }
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Fallback icon if the bundled icon fails to decode. Not pretty, but better
/// than a panic at startup.
fn default_icon() -> Image<'static> {
    const TRANSPARENT_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, //
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, //
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, //
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, // IEND
        0x42, 0x60, 0x82,
    ];
    Image::from_bytes(TRANSPARENT_1X1).expect("hardcoded 1x1 PNG is valid")
}