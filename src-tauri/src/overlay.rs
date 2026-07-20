//! Floating recording indicator, shown while capturing/transcribing so the
//! user gets feedback even when the main window is hidden in the tray.
//!
//! macOS needs an NSPanel: a plain always-on-top window either fails to order
//! front or steals focus (which breaks paste-into-the-focused-app). The panel
//! is non-activating and floats over other spaces. Other platforms use a
//! regular transparent, non-focusable, always-on-top window.
//!
//! The overlay's own webview listens to `pie://state` for its color/label.

use tauri::{AppHandle, Manager};

pub const OVERLAY_LABEL: &str = "overlay";

// Logical size; kept in sync with the pill in Overlay.svelte (plus its 8px
// margin) so the transparent window hugs the visible content.
const OVERLAY_WIDTH: f64 = 200.0;
const OVERLAY_HEIGHT: f64 = 64.0;
/// Gap between the overlay and the bottom of the screen, in logical points.
const BOTTOM_MARGIN: f64 = 90.0;

/// Bottom-center of the primary monitor, in logical points.
fn overlay_position(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let origin = monitor.position();
    let size = monitor.size();
    let sw = size.width as f64 / scale;
    let sh = size.height as f64 / scale;
    let ox = origin.x as f64 / scale;
    let oy = origin.y as f64 / scale;
    let x = ox + (sw - OVERLAY_WIDTH) / 2.0;
    let y = oy + sh - OVERLAY_HEIGHT - BOTTOM_MARGIN;
    Some((x, y))
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use crate::nspanel::{create_floating_panel, FloatingPanelConfig};
    use tauri::{LogicalPosition, LogicalSize, Position, Size, WebviewUrl};

    pub fn create_overlay(app: &AppHandle) {
        if app.get_webview_window(OVERLAY_LABEL).is_some() {
            return;
        }
        let (x, y) = overlay_position(app).unwrap_or((100.0, 100.0));
        let result = create_floating_panel(
            app,
            FloatingPanelConfig {
                label: OVERLAY_LABEL,
                url: WebviewUrl::App("overlay.html".into()),
                title: "PIE Recording",
                position: Position::Logical(LogicalPosition { x, y }),
                size: Size::Logical(LogicalSize {
                    width: OVERLAY_WIDTH,
                    height: OVERLAY_HEIGHT,
                }),
            },
        );
        match result {
            Ok(()) => log::info!("Recording overlay panel created (hidden)"),
            Err(e) => log::error!("Failed to create overlay panel: {e}"),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    pub fn create_overlay(app: &AppHandle) {
        if app.get_webview_window(OVERLAY_LABEL).is_some() {
            return;
        }
        let result =
            WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
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
}

/// Create the overlay hidden. Safe to call once at startup.
pub fn create_overlay(app: &AppHandle) {
    platform::create_overlay(app);
}

/// Reposition bottom-center and show the overlay.
pub fn show_overlay(app: &AppHandle) {
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        return;
    };
    if let Some((x, y)) = overlay_position(app) {
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
    }
    let _ = window.show();
    let _ = window.set_always_on_top(true);
}

pub fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.hide();
    }
}
