#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
#[cfg(target_os = "macos")]
mod nspanel;
mod overlay;
mod paste;
mod settings;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use paste::EnigoState;
use pie_engine::audio::{
    AudioRecorder, SileroVad, VadPipeline, VadPolicy, PIE_VAD_THRESHOLD,
    VAD_HANGOVER_FRAMES, VAD_SPEECH_THRESHOLD_FRAMES, VAD_CONTEXT_FRAMES,
    VAD_STREAM_HANGOVER_FRAMES,
};
use pie_engine::history::{HistoryStore, NewEntry};
use pie_engine::stt::{SttEngine, WhisperEngine};
use pie_engine::PieEngine;
use settings::Settings;

/// Everything the commands share. Locks are scoped and never held across
/// an await; heavy work (model load, transcription) runs on blocking threads.
struct AppState {
    settings: Mutex<Settings>,
    /// Open while a recording session is active.
    recorder: Mutex<Option<AudioRecorder>>,
    /// True from stop until processing finishes; the hotkey ignores presses
    /// while set so a double-tap can't start a session mid-decode.
    busy: AtomicBool,
    /// Loaded whisper engine, cached with the (path, language) it was built
    /// from so a settings change reloads it.
    whisper: Mutex<Option<(PathBuf, String, Arc<WhisperEngine>)>>,
    /// Text pipeline: intent -> memory -> optimizer -> LLM router.
    engine: tokio::sync::Mutex<PieEngine>,
    /// Local SQLite history of recordings.
    history: Mutex<HistoryStore>,
}

/// Result payload for the frontend after a recording is processed.
#[derive(Clone, Serialize)]
struct Outcome {
    transcript: String,
    objective: String,
    conversation_type: String,
    confidence: String,
    optimized_prompt: String,
    estimated_tokens: usize,
    mode: String,
}

/// Emit an event to the frontend, logging (rather than silently dropping) any
/// failure so a broken event channel is visible in the logs.
fn emit_event<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    if let Err(e) = app.emit(event, payload) {
        log::warn!("Failed to emit {event}: {e}");
    }
}

fn emit_state(app: &AppHandle, state: &str) {
    emit_event(app, "pie://state", state);
    // The floating overlay is only visible while a session is in flight.
    match state {
        "recording" | "decoding" => overlay::show_overlay(app),
        _ => overlay::hide_overlay(app),
    }
}

/// Bring the main window to the front (tray click / menu).
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/* ─── recording session (shared by UI commands and the global hotkey) ─── */

fn do_start_recording(app: &AppHandle) -> Result<(), String> {
    let state = app.try_state::<AppState>().ok_or("App state unavailable")?;
    let mut recorder_slot = state.recorder.lock().unwrap_or_else(|e| e.into_inner());
    if recorder_slot.is_some() {
        return Err("Already recording".to_string());
    }
    if state.busy.load(Ordering::Acquire) {
        return Err("Still processing the previous recording".to_string());
    }

    let settings = state
        .settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let (mut recorder, vad_active) = build_recorder(&settings).map_err(|e| e.to_string())?;
    recorder.open(None).map_err(|e| e.to_string())?;
    recorder
        .start(if vad_active {
            VadPolicy::Offline
        } else {
            VadPolicy::Disabled
        })
        .map_err(|e| e.to_string())?;

    *recorder_slot = Some(recorder);
    emit_state(app, "recording");
    Ok(())
}

async fn do_stop_recording(app: AppHandle) -> Result<Outcome, String> {
    let state = app.try_state::<AppState>().ok_or("App state unavailable")?;

    // 1. Capture: take the recorder out and collect the session's samples.
    let samples = {
        let mut recorder_slot = state.recorder.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut recorder) = recorder_slot.take() else {
            return Err("Not recording".to_string());
        };
        let samples = recorder.stop().map_err(|e| e.to_string())?;
        recorder.close().map_err(|e| e.to_string())?;
        samples
    };
    state.busy.store(true, Ordering::Release);
    emit_state(&app, "decoding");

    let result = transcribe_and_process(&app, samples).await;

    state.busy.store(false, Ordering::Release);
    emit_state(&app, "idle");
    result
}

/// Stop and discard an in-flight recording without transcribing, and reset the
/// UI. Shared by the Cancel button and the in-window Escape key. Escape is
/// handled in the webview only (never grabbed globally) so it can't interfere
/// with Escape in other apps.
fn do_cancel_recording(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        let mut slot = state.recorder.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut recorder) = slot.take() {
            let _ = recorder.stop();
            let _ = recorder.close();
        }
    }
    emit_state(app, "idle");
}

async fn transcribe_and_process(app: &AppHandle, samples: Vec<f32>) -> Result<Outcome, String> {
    let state = app.try_state::<AppState>().ok_or("App state unavailable")?;
    if samples.len() < 1600 {
        return Err("Recording too short (or VAD detected no speech)".to_string());
    }

    let settings = state
        .settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();

    // 2. Transcribe on a blocking thread (Metal/CPU inference).
    let whisper = get_or_load_whisper(&state, &settings)?;
    let transcript = tauri::async_runtime::spawn_blocking(move || {
        whisper.transcribe(&samples).map(|t| t.trim().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    if transcript.is_empty() {
        return Err("Transcription produced no text (silence?)".to_string());
    }

    // 3. Intent + optimization through the shared pipeline.
    let mut engine = state.engine.lock().await;
    let result = engine
        .process(&transcript, &settings.mode)
        .await
        .map_err(|e| e.to_string())?;
    drop(engine);

    let outcome = Outcome {
        transcript,
        objective: result.intent.objective,
        conversation_type: format!("{:?}", result.intent.conversation_type),
        confidence: format!("{:?}", result.intent.confidence),
        optimized_prompt: result.optimized_prompt,
        estimated_tokens: result.estimated_tokens,
        mode: format!("{:?}", result.mode),
    };

    // Best-effort history capture — never fail the recording over it.
    {
        let entry = NewEntry {
            transcript: outcome.transcript.clone(),
            objective: opt(&outcome.objective),
            conversation_type: opt(&outcome.conversation_type),
            confidence: opt(&outcome.confidence),
            optimized_prompt: opt(&outcome.optimized_prompt),
            estimated_tokens: Some(outcome.estimated_tokens as i64),
            mode: opt(&outcome.mode),
            language: opt(&settings.language),
        };
        let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
        match history.add(entry, settings.history_limit) {
            Ok(_) => emit_event(app, "pie://history-changed", ()),
            Err(e) => log::warn!("Failed to record history: {e}"),
        }
    }

    Ok(outcome)
}

/* ─── global hotkey ─── */

/// Toggle handler: first press starts recording, second press stops,
/// transcribes, and pastes the result into the app that has focus.
fn on_hotkey(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        log::error!("on_hotkey: app state unavailable");
        return;
    };
    let recording = state
        .recorder
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some();
    log::debug!("on_hotkey: recording={recording}");

    if !recording {
        if let Err(e) = do_start_recording(app) {
            log::warn!("Hotkey start failed: {e}");
            emit_event(app, "pie://error", e);
        }
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match do_stop_recording(app.clone()).await {
            Ok(outcome) => {
                // Show the result in the window (if it's open) ...
                emit_event(&app, "pie://outcome", outcome.clone());

                // ... and paste into whichever app has focus.
                let Some(state) = app.try_state::<AppState>() else {
                    log::error!("hotkey paste: app state unavailable");
                    return;
                };
                let settings = {
                    let s = state.settings.lock().unwrap_or_else(|e| e.into_inner());
                    s.clone()
                };
                let text = if settings.paste_output == "prompt" {
                    outcome.optimized_prompt
                } else {
                    outcome.transcript
                };
                let result = tauri::async_runtime::spawn_blocking(move || {
                    let enigo = app
                        .try_state::<EnigoState>()
                        .ok_or("Keystroke engine unavailable")?;
                    paste::paste_text(&app, &enigo, &text)
                })
                .await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => log::error!("Paste failed: {e}"),
                    Err(e) => log::error!("Paste task failed: {e}"),
                }
            }
            Err(e) => {
                log::warn!("Hotkey stop failed: {e}");
                emit_event(&app, "pie://error", e);
            }
        }
    });
}

/// (Re-)register the global shortcut from settings. An empty string disables
/// the hotkey. The string is parsed *before* the current shortcut is
/// unregistered, so an invalid combo returns an error and leaves the working
/// hotkey intact.
fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    let trimmed = hotkey.trim();
    if trimmed.is_empty() {
        shortcuts.unregister_all().map_err(|e| e.to_string())?;
        log::info!("Global hotkey disabled");
        return Ok(());
    }
    let shortcut: Shortcut = trimmed
        .parse()
        .map_err(|e| format!("Invalid hotkey '{hotkey}': {e}"))?;
    shortcuts.unregister_all().map_err(|e| e.to_string())?;
    shortcuts
        .on_shortcut(shortcut, move |app, fired, event| {
            // Only the registered shortcut reaches here; log at debug for
            // diagnosing "did my hotkey fire?" without noise at info level.
            if event.state() == ShortcutState::Pressed {
                log::debug!("Hotkey fired: {fired:?}");
                on_hotkey(app);
            }
        })
        .map_err(|e| format!("Failed to register hotkey '{hotkey}': {e}"))?;
    log::info!("Global hotkey registered: {hotkey}");
    Ok(())
}

/* ─── commands ─── */

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state
        .settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    let hotkey_changed = {
        let current = state.settings.lock().unwrap_or_else(|e| e.into_inner());
        current.hotkey != settings.hotkey
    };
    // Register the new hotkey BEFORE persisting: if it's invalid, we return the
    // error without saving a broken binding (and the old one stays active).
    // The whisper cache checks (path, language) on next use, so model/language
    // changes reload naturally — only the hotkey needs re-wiring.
    if hotkey_changed {
        register_hotkey(&app, &settings.hotkey)?;
    }
    settings.save().map_err(|e| e.to_string())?;
    *state.settings.lock().unwrap_or_else(|e| e.into_inner()) = settings;
    Ok(())
}

/// Suspend (active=false) or restore (active=true) the global hotkey. The
/// Settings shortcut recorder suspends it while capturing so the current
/// binding doesn't fire on the keys being pressed to choose a new one.
#[tauri::command]
fn set_hotkey_active(
    app: AppHandle,
    state: State<'_, AppState>,
    active: bool,
) -> Result<(), String> {
    if active {
        let hotkey = state
            .settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .hotkey
            .clone();
        register_hotkey(&app, &hotkey)
    } else {
        app.global_shortcut()
            .unregister_all()
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn start_recording(app: AppHandle) -> Result<(), String> {
    do_start_recording(&app)
}

#[tauri::command]
async fn stop_recording(app: AppHandle) -> Result<Outcome, String> {
    do_stop_recording(app).await
}

#[tauri::command]
fn cancel_recording(app: AppHandle) -> Result<(), String> {
    do_cancel_recording(&app);
    Ok(())
}

#[tauri::command]
fn list_models(state: State<'_, AppState>) -> Vec<models::ModelInfo> {
    let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
    models::list_models(&settings)
}

/// Point the relevant setting at an already-downloaded catalog model.
#[tauri::command]
fn select_model(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let (kind, _url, path) = models::resolve(&id).ok_or("Unknown model")?;
    if !path.exists() {
        return Err("Model isn't downloaded yet".to_string());
    }
    let path_str = path.to_string_lossy().into_owned();
    let settings = {
        let mut settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
        match kind {
            models::ModelKind::Whisper => settings.whisper_model = path_str,
            models::ModelKind::Vad => settings.silero_model = path_str,
        }
        settings.clone()
    };
    settings.save().map_err(|e| e.to_string())?;
    emit_event(&app, "pie://models-changed", ());
    Ok(())
}

/// Delete a downloaded model file. Refuses if the model is currently selected.
#[tauri::command]
fn delete_model(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let (kind, _url, path) = models::resolve(&id).ok_or("Unknown model")?;
    if !path.exists() {
        return Err("Model isn't downloaded".to_string());
    }

    // Block deletion of the active model.
    let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
    let selected_path = match kind {
        models::ModelKind::Whisper => Settings::expand(&settings.whisper_model),
        models::ModelKind::Vad => Settings::expand(&settings.silero_model),
    };
    if selected_path == path {
        return Err("Can't delete the model currently in use".to_string());
    }
    drop(settings);

    std::fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete {}: {e}", path.display()))?;
    log::info!("Deleted model: {}", path.display());
    emit_event(&app, "pie://models-changed", ());
    Ok(())
}

/// Stream a catalog model to disk, emitting `pie://download` progress. On
/// success, auto-selects it so it's ready to use.
#[tauri::command]
async fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    use serde::Serialize;
    use std::time::{Duration, Instant};

    #[derive(Clone, Serialize)]
    struct Progress {
        id: String,
        received: u64,
        total: u64,
        done: bool,
        error: Option<String>,
    }

    let (_kind, url, dest) = models::resolve(&id).ok_or("Unknown model")?;

    // Throttle progress events to ~10/s so the event channel isn't flooded.
    let mut last_emit = Instant::now();
    let result = models::download_to(url, &dest, |received, total| {
        if last_emit.elapsed() >= Duration::from_millis(100) {
            last_emit = Instant::now();
            emit_event(
                &app,
                "pie://download",
                Progress {
                    id: id.clone(),
                    received,
                    total,
                    done: false,
                    error: None,
                },
            );
        }
    })
    .await;

    match result {
        Ok(received) => {
            emit_event(
                &app,
                "pie://download",
                Progress {
                    id: id.clone(),
                    received,
                    total: received,
                    done: true,
                    error: None,
                },
            );
            // Auto-select the freshly downloaded model (best-effort).
            if let Some(state) = app.try_state::<AppState>() {
                let _ = select_model(app.clone(), state, id);
            }
            Ok(())
        }
        Err(e) => {
            emit_event(
                &app,
                "pie://download",
                Progress {
                    id,
                    received: 0,
                    total: 0,
                    done: true,
                    error: Some(e.clone()),
                },
            );
            Err(e)
        }
    }
}

#[tauri::command]
async fn send_to_llm(state: State<'_, AppState>, prompt: String) -> Result<String, String> {
    let (provider, model) = {
        let settings = state.settings.lock().unwrap_or_else(|e| e.into_inner());
        (settings.provider.clone(), settings.llm_model.clone())
    };
    let model = (!model.is_empty()).then_some(model);

    let engine = state.engine.lock().await;
    engine
        .send_to_llm(&prompt, &provider, model.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_to_clipboard(app: AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("Failed to copy: {e}"))
}

#[tauri::command]
fn list_history(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<pie_engine::history::HistoryEntry>, String> {
    let limit = state
        .settings
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .history_limit;
    let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
    history
        .list(query.as_deref(), limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_history_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    {
        let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
        history.delete(id).map_err(|e| e.to_string())?;
    }
    emit_event(&app, "pie://history-changed", ());
    Ok(())
}

#[tauri::command]
fn clear_history(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
        history.clear().map_err(|e| e.to_string())?;
    }
    emit_event(&app, "pie://history-changed", ());
    Ok(())
}

// Synchronous: it blocks with sleeps + keystroke simulation. Tauri runs sync
// commands on its own thread pool, so this won't stall the async runtime.
#[tauri::command]
fn paste_history_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    enigo: State<'_, EnigoState>,
    id: i64,
) -> Result<(), String> {
    let text = {
        let history = state.history.lock().unwrap_or_else(|e| e.into_inner());
        history
            .get(id)
            .map_err(|e| e.to_string())?
            .map(|r| r.transcript)
            .ok_or_else(|| "History entry not found".to_string())?
    };

    // Hide the main window so focus returns to the previously active app,
    // then paste into it (same mechanism as the hotkey flow).
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    std::thread::sleep(std::time::Duration::from_millis(120));
    paste::paste_text(&app, &enigo, &text)
}

/* ─── helpers ─── */

/// Map an empty string to `None` so blank optimizer fields aren't stored.
fn opt(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// Load (or reuse) the whisper engine for the configured model + language.
fn get_or_load_whisper(
    state: &State<'_, AppState>,
    settings: &Settings,
) -> Result<Arc<WhisperEngine>, String> {
    if settings.whisper_model.is_empty() {
        return Err("No whisper model configured. Set one in Settings (e.g. \
             ~/.cache/pie/models/ggml-tiny.en.bin)."
            .to_string());
    }
    let path = Settings::expand(&settings.whisper_model);

    let mut cache = state.whisper.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached_path, cached_lang, engine)) = cache.as_ref() {
        if *cached_path == path && *cached_lang == settings.language {
            return Ok(Arc::clone(engine));
        }
    }

    let engine =
        Arc::new(WhisperEngine::load(&path, &settings.language).map_err(|e| e.to_string())?);
    *cache = Some((path, settings.language.clone(), Arc::clone(&engine)));
    Ok(engine)
}

/// Recorder with Silero VAD when configured; VAD-free otherwise.
fn build_recorder(settings: &Settings) -> anyhow::Result<(AudioRecorder, bool)> {
    if settings.silero_model.is_empty() {
        return Ok((AudioRecorder::new()?, false));
    }
    let silero = SileroVad::new(
        Settings::expand(&settings.silero_model),
        PIE_VAD_THRESHOLD,
    )?;
    let smoothed = VadPipeline::new(
        Box::new(silero),
        VAD_CONTEXT_FRAMES,
        VAD_HANGOVER_FRAMES,
        VAD_SPEECH_THRESHOLD_FRAMES,
    );
    let recorder = AudioRecorder::new()?.with_vad(
        Box::new(smoothed),
        VAD_HANGOVER_FRAMES,
        VAD_STREAM_HANGOVER_FRAMES,
    );
    Ok((recorder, true))
}

fn main() {
    env_logger::init();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());
    // macOS overlay is an NSPanel created directly in overlay.rs (see the
    // vendored nspanel module) — no external plugin needed.

    builder
        .setup(|app| {
            let engine = tauri::async_runtime::block_on(PieEngine::new())?;
            let settings = Settings::load();
            let hotkey = settings.hotkey.clone();
            let history_path = dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("pie")
                .join("history.db");
            let history = HistoryStore::open(&history_path).unwrap_or_else(|e| {
                log::error!("Failed to open history DB ({e}); using in-memory");
                HistoryStore::open_in_memory().expect("in-memory history must open")
            });
            app.manage(AppState {
                settings: Mutex::new(settings),
                recorder: Mutex::new(None),
                busy: AtomicBool::new(false),
                whisper: Mutex::new(None),
                engine: tokio::sync::Mutex::new(engine),
                history: Mutex::new(history),
            });
            app.manage(EnigoState::new());

            if let Err(e) = register_hotkey(app.handle(), &hotkey) {
                // A bad hotkey must not prevent the app from starting.
                log::error!("{e}");
            }

            // Floating indicator, created hidden and shown per recording state.
            // Creating the NSPanel overlay temporarily flips the app activation
            // policy to Prohibited (tauri-nspanel's no_activate), which orders
            // the main window out; restoring the policy doesn't bring it back,
            // so re-show the main window explicitly afterwards.
            overlay::create_overlay(app.handle());
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.show();
                let _ = main.set_focus();
            }

            // System tray: PIE runs in the background so the hotkey works from
            // any app; the tray is how you reopen or quit it.
            let show_item = MenuItemBuilder::with_id("show", "Show PIE").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit PIE").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("PIE — Personal Intent Engine")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window hides it to the tray instead of quitting,
            // so the global hotkey keeps working. Quit is via the tray menu.
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
            set_hotkey_active,
            start_recording,
            stop_recording,
            cancel_recording,
            send_to_llm,
            copy_to_clipboard,
            list_history,
            delete_history_entry,
            clear_history,
            paste_history_entry,
            list_models,
            select_model,
            delete_model,
            download_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PIE");
}

#[cfg(test)]
mod tests {
    use tauri_plugin_global_shortcut::Shortcut;

    /// The recorder builds accelerators from modifier names + `event.code`.
    /// Every shape it can produce must parse, or a captured hotkey would fail
    /// to register.
    #[test]
    fn recorder_accelerators_parse() {
        let cases = [
            "Command+Shift+Space",
            "Control+Alt+KeyK",
            "Command+KeyL",
            "Command+Digit1",
            "Shift+ArrowUp",
            "F5",
            "CmdOrCtrl+Shift+Space", // the default
        ];
        for a in cases {
            assert!(a.parse::<Shortcut>().is_ok(), "should parse: {a}");
        }
    }
}
