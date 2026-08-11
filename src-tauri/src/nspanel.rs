//! Minimal NSPanel support for the recording overlay (macOS only).
//!
//! A plain always-on-top Tauri window either fails to order front or steals
//! keyboard focus, which breaks paste-into-the-focused-app. macOS solves this
//! with an `NSPanel` configured as a non-activating floating panel.
//!
//! PIE owns this code directly instead of depending on an external plugin. It
//! is the specific subset of the NSWindow→NSPanel subclassing pattern we need:
//! define a custom `NSPanel` subclass, reclass Tauri's `NSWindow` into it with
//! `object_setClass`, then apply the panel-level style. All of the `objc2`
//! calls mirror what the recording overlay used before, so runtime behavior is
//! unchanged.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, NSObjectProtocol};
use objc2::{class, define_class, msg_send, ClassType};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSAutoresizingMaskOptions, NSPanel, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject};
use tauri::{AppHandle, Position, Runtime, Size, WebviewUrl, WebviewWindowBuilder};

/// Zero-sized instance variables for [`OverlayPanel`]. Kept empty so the class
/// can be applied to an already-allocated `NSWindow` via `object_setClass`
/// (which has no room for real ivars).
struct OverlayPanelIvars;

define_class!(
    // A borderless, non-activating floating panel used for the recording
    // overlay. `canBecomeKeyWindow` returns false so the panel never steals
    // keyboard focus; `isFloatingPanel` returns true so it floats above normal
    // windows across spaces.
    #[unsafe(super = NSPanel)]
    #[name = "PieOverlayPanel"]
    #[ivars = OverlayPanelIvars]
    struct OverlayPanel;

    unsafe impl NSObjectProtocol for OverlayPanel {}

    impl OverlayPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            false
        }

        #[unsafe(method(isFloatingPanel))]
        fn is_floating_panel(&self) -> bool {
            true
        }
    }
);

// C runtime hook for swapping an object's class in place. Declared here exactly
// as the original NSPanel plugin did, rather than pulling in a bindings crate.
unsafe extern "C" {
    fn object_setClass(obj: *mut NSObject, cls: *const AnyClass) -> *const AnyClass;
}

/// Everything needed to build the floating overlay panel.
pub struct FloatingPanelConfig<'a> {
    pub label: &'a str,
    pub url: WebviewUrl,
    pub title: &'a str,
    pub position: Position,
    pub size: Size,
}

/// Create a non-activating, always-on-top, transparent floating panel and leave
/// it hidden. The panel is a reclassed Tauri window, so callers keep driving it
/// through the normal `WebviewWindow` API (`get_webview_window`, `show`, …).
///
/// Must be called on the main thread.
pub fn create_floating_panel<R: Runtime>(
    app: &AppHandle<R>,
    config: FloatingPanelConfig,
) -> tauri::Result<()> {
    // A non-activating panel must not pull the app to the foreground while it is
    // created. Flip the activation policy to Prohibited for the duration of the
    // build, then restore it — the caller re-shows the main window afterwards.
    let original_policy = MainThreadMarker::new().map(|mtm| {
        let ns_app = NSApplication::sharedApplication(mtm);
        let current = ns_app.activationPolicy();
        ns_app.setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
        current
    });

    let mut window_builder = WebviewWindowBuilder::new(app, config.label, config.url)
        .title(config.title)
        .decorations(false)
        .transparent(true)
        .focusable(false);

    match config.position {
        Position::Physical(p) => window_builder = window_builder.position(p.x as f64, p.y as f64),
        Position::Logical(p) => window_builder = window_builder.position(p.x, p.y),
    }
    match config.size {
        Size::Physical(s) => {
            window_builder = window_builder.inner_size(s.width as f64, s.height as f64)
        }
        Size::Logical(s) => window_builder = window_builder.inner_size(s.width, s.height),
    }

    let window = window_builder.build()?;

    // Reclass the underlying NSWindow into our NSPanel subclass, then apply the
    // panel style. Errors here only leave the activation policy to restore.
    let result = (|| -> tauri::Result<()> {
        let ns_window = window.ns_window()?;

        unsafe {
            object_setClass(ns_window as *mut NSObject, OverlayPanel::class());
            let panel_ptr = ns_window as *mut OverlayPanel;
            let panel = Retained::retain(panel_ptr).ok_or_else(|| {
                tauri::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "failed to retain overlay panel",
                ))
            })?;

            // is_floating_panel config: NSPanel has a setter for this.
            let _: () = msg_send![&*panel, setFloatingPanel: true];

            // Status-level float that joins all spaces and coexists with
            // fullscreen apps — the overlay must show over everything.
            let _: () = msg_send![&*panel, setLevel: 25_i64]; // NSStatusWindowLevel
            let behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary;
            let _: () = msg_send![&*panel, setCollectionBehavior: behavior];

            // Borderless + non-activating so it never becomes key.
            let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
            let _: () = msg_send![&*panel, setStyleMask: style];

            let _: () = msg_send![&*panel, setHasShadow: false];

            // Transparent background.
            let clear_color: Retained<NSObject> = msg_send![class!(NSColor), clearColor];
            let _: () = msg_send![&*panel, setBackgroundColor: &*clear_color];
            let _: () = msg_send![&*panel, setOpaque: false];

            // Let the webview content view resize with the panel.
            let content_view: Retained<NSView> = msg_send![&*panel, contentView];
            let subviews: Retained<NSArray<NSView>> = msg_send![&content_view, subviews];
            let count: usize = msg_send![&subviews, count];
            let resize_mask = NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable;
            for i in 0..count {
                let view: Retained<NSView> = msg_send![&subviews, objectAtIndex: i];
                let _: () = msg_send![&view, setAutoresizingMask: resize_mask];
            }

            // Start hidden; the caller shows it per recording state.
            let _: () = msg_send![&*panel, orderOut: objc2::ffi::nil];
        }
        Ok(())
    })();

    // Restore the activation policy regardless of how conversion went.
    if let (Some(policy), Some(mtm)) = (original_policy, MainThreadMarker::new()) {
        let ns_app = NSApplication::sharedApplication(mtm);
        ns_app.setActivationPolicy(policy);
    }

    result
}
