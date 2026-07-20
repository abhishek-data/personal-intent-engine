//! Paste text into the currently focused application.
//!
//! Clipboard-paste flow: save clipboard, write text, send platform paste
//! keystroke, restore. The keystroke is sent via enigo, and macOS requires the
//! Accessibility permission the first time keystrokes are simulated.

use std::sync::Mutex;
use std::time::Duration;

use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

/// Delay between writing the clipboard and sending the paste keystroke, so
/// the target app observes the new clipboard content.
const PASTE_DELAY_BEFORE: Duration = Duration::from_millis(80);
/// Delay before restoring the clipboard, so the paste has landed first.
const PASTE_DELAY_AFTER: Duration = Duration::from_millis(200);

/// Enigo held once per app: construction probes the platform input APIs, so
/// it shouldn't happen on every paste.
pub struct EnigoState(Mutex<Option<Enigo>>);

impl EnigoState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

pub fn paste_text(app: &AppHandle, state: &EnigoState, text: &str) -> Result<(), String> {
    let clipboard = app.clipboard();
    let previous = clipboard.read_text().unwrap_or_default();

    clipboard
        .write_text(text)
        .map_err(|e| format!("Failed to write clipboard: {e}"))?;
    std::thread::sleep(PASTE_DELAY_BEFORE);

    {
        let mut slot = state.0.lock().map_err(|_| "enigo poisoned".to_string())?;
        let enigo = match slot.as_mut() {
            Some(enigo) => enigo,
            None => {
                let enigo = Enigo::new(&EnigoSettings::default())
                    .map_err(|e| format!("Failed to initialize input simulation: {e}"))?;
                slot.insert(enigo)
            }
        };
        send_paste_keystroke(enigo)?;
    }

    std::thread::sleep(PASTE_DELAY_AFTER);
    let _ = clipboard.write_text(previous);
    Ok(())
}

fn send_paste_keystroke(enigo: &mut Enigo) -> Result<(), String> {
    // macOS pastes with Cmd+V; keycode 9 is 'v' on ANSI layouts.
    #[cfg(target_os = "macos")]
    let (modifier, v_key) = (Key::Meta, Key::Other(9));
    #[cfg(target_os = "windows")]
    let (modifier, v_key) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier, v_key) = (Key::Control, Key::Unicode('v'));

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| format!("Failed to press modifier: {e}"))?;
    enigo
        .key(v_key, Direction::Click)
        .map_err(|e| format!("Failed to click V: {e}"))?;
    std::thread::sleep(Duration::from_millis(100));
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| format!("Failed to release modifier: {e}"))?;
    Ok(())
}
