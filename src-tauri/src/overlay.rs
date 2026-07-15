//! Floating recording indicator, shown while capturing/transcribing so the
//! user gets feedback even when the main window is hidden in the tray.
//!
//! One transparent, always-on-top, non-focusable window is created hidden at
//! startup and shown/hidden as recording state changes (OpenSuperWhisper's
//! indicator pattern). The overlay's own webview listens to `pie://state` to
//! pick its color and label.

use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub const OVERLAY_LABEL: &str = "overlay";

// Logical size; kept in sync with the pill in Overlay.svelte (plus its 8px
// margin) so the transparent window hugs the visible content.
const OVERLAY_WIDTH: f64 = 200.0;
const OVERLAY_HEIGHT: f64 = 64.0;
/// Gap between the overlay and the bottom of the screen, in logical points.
const BOTTOM_MARGIN: f64 = 90.0;

/// Create the overlay window hidden. Safe to call once at startup; a second
/// call is a no-op if the window already exists.
pub fn create_overlay(app: &AppHandle) {
    if app.get_webview_window(OVERLAY_LABEL).is_some() {
        return;
    }

    let result = WebviewWindowBuilder::new(
        app,
        OVERLAY_LABEL,
        WebviewUrl::App("overlay.html".into()),
    )
    .title("PIE Recording")
    .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .decorations(false)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focusable(false)
    .visible(false)
    .build();

    match result {
        Ok(_) => log::info!("Recording overlay created (hidden)"),
        Err(e) => log::error!("Failed to create overlay window: {e}"),
    }
}

/// Position bottom-center of the primary monitor and show the overlay.
pub fn show_overlay(app: &AppHandle) {
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        return;
    };
    position_bottom_center(&window);
    let _ = window.show();
    // Re-assert topmost: some compositors drop it after show().
    let _ = window.set_always_on_top(true);
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.hide();
    }
}

fn position_bottom_center(window: &WebviewWindow) {
    let monitor = match window.primary_monitor() {
        Ok(Some(m)) => m,
        _ => return, // no monitor info (headless / detection failed) — leave as-is
    };
    let scale = monitor.scale_factor();
    let size = monitor.size(); // physical pixels
    let origin = monitor.position(); // physical, for multi-monitor offsets

    let overlay_w = OVERLAY_WIDTH * scale;
    let overlay_h = OVERLAY_HEIGHT * scale;
    let x = origin.x as f64 + (size.width as f64 - overlay_w) / 2.0;
    let y = origin.y as f64 + size.height as f64 - overlay_h - BOTTOM_MARGIN * scale;

    let _ = window.set_position(PhysicalPosition::new(x, y));
}
