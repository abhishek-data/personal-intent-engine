#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
    AudioRecorder, SileroVad, SmoothedVad, VadPolicy, SILERO_DEFAULT_THRESHOLD,
    VAD_OFFLINE_HANGOVER_FRAMES, VAD_ONSET_FRAMES, VAD_PREFILL_FRAMES,
    VAD_STREAMING_HANGOVER_FRAMES,
};
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

fn emit_state(app: &AppHandle, state: &str) {
    let _ = app.emit("pie://state", state);
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
    let state = app.state::<AppState>();
    let mut recorder_slot = state.recorder.lock().expect("recorder poisoned");
    if recorder_slot.is_some() {
        return Err("Already recording".to_string());
    }
    if state.busy.load(Ordering::Acquire) {
        return Err("Still processing the previous recording".to_string());
    }

    let settings = state.settings.lock().expect("settings poisoned").clone();
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
    let state = app.state::<AppState>();

    // 1. Capture: take the recorder out and collect the session's samples.
    let samples = {
        let mut recorder_slot = state.recorder.lock().expect("recorder poisoned");
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

async fn transcribe_and_process(app: &AppHandle, samples: Vec<f32>) -> Result<Outcome, String> {
    let state = app.state::<AppState>();
    if samples.len() < 1600 {
        return Err("Recording too short (or VAD detected no speech)".to_string());
    }

    let settings = state.settings.lock().expect("settings poisoned").clone();

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

    Ok(Outcome {
        transcript,
        objective: result.intent.objective,
        conversation_type: format!("{:?}", result.intent.conversation_type),
        confidence: format!("{:?}", result.intent.confidence),
        optimized_prompt: result.optimized_prompt,
        estimated_tokens: result.estimated_tokens,
        mode: format!("{:?}", result.mode),
    })
}

/* ─── global hotkey ─── */

/// Toggle handler: first press starts recording, second press stops,
/// transcribes, and pastes the result into the app that has focus.
fn on_hotkey(app: &AppHandle) {
    let state = app.state::<AppState>();
    let recording = state
        .recorder
        .lock()
        .expect("recorder poisoned")
        .is_some();

    if !recording {
        if let Err(e) = do_start_recording(app) {
            log::warn!("Hotkey start failed: {e}");
            let _ = app.emit("pie://error", e);
        }
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match do_stop_recording(app.clone()).await {
            Ok(outcome) => {
                // Show the result in the window (if it's open) ...
                let _ = app.emit("pie://outcome", outcome.clone());

                // ... and paste into whichever app has focus.
                let settings = {
                    let state = app.state::<AppState>();
                    let s = state.settings.lock().expect("settings poisoned");
                    s.clone()
                };
                let text = if settings.paste_output == "prompt" {
                    outcome.optimized_prompt
                } else {
                    outcome.transcript
                };
                let result = tauri::async_runtime::spawn_blocking(move || {
                    let enigo = app.state::<EnigoState>();
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
                let _ = app.emit("pie://error", e);
            }
        }
    });
}

/// (Re-)register the global shortcut from settings. Returns a description of
/// what's active so callers can surface registration problems.
fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    shortcuts.unregister_all().map_err(|e| e.to_string())?;
    if hotkey.trim().is_empty() {
        return Ok(()); // hotkey disabled
    }
    let shortcut: Shortcut = hotkey
        .parse()
        .map_err(|e| format!("Invalid hotkey '{hotkey}': {e}"))?;
    shortcuts
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
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
    state.settings.lock().expect("settings poisoned").clone()
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    settings.save().map_err(|e| e.to_string())?;
    let hotkey_changed = {
        let mut current = state.settings.lock().expect("settings poisoned");
        let changed = current.hotkey != settings.hotkey;
        *current = settings.clone();
        changed
    };
    // The whisper cache checks (path, language) on next use, so a model or
    // language change reloads naturally — only the hotkey needs re-wiring.
    if hotkey_changed {
        register_hotkey(&app, &settings.hotkey)?;
    }
    Ok(())
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
fn cancel_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut recorder_slot = state.recorder.lock().expect("recorder poisoned");
    if let Some(mut recorder) = recorder_slot.take() {
        let _ = recorder.stop();
        let _ = recorder.close();
    }
    emit_state(&app, "idle");
    Ok(())
}

#[tauri::command]
async fn send_to_llm(state: State<'_, AppState>, prompt: String) -> Result<String, String> {
    let (provider, model) = {
        let settings = state.settings.lock().expect("settings poisoned");
        (settings.provider.clone(), settings.llm_model.clone())
    };
    let model = (!model.is_empty()).then_some(model);

    let engine = state.engine.lock().await;
    engine
        .send_to_llm(&prompt, &provider, model.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/* ─── helpers ─── */

/// Load (or reuse) the whisper engine for the configured model + language.
fn get_or_load_whisper(
    state: &State<'_, AppState>,
    settings: &Settings,
) -> Result<Arc<WhisperEngine>, String> {
    if settings.whisper_model.is_empty() {
        return Err(
            "No whisper model configured. Set one in Settings (e.g. \
             ~/.cache/pie/models/ggml-tiny.en.bin)."
                .to_string(),
        );
    }
    let path = Settings::expand(&settings.whisper_model);

    let mut cache = state.whisper.lock().expect("whisper cache poisoned");
    if let Some((cached_path, cached_lang, engine)) = cache.as_ref() {
        if *cached_path == path && *cached_lang == settings.language {
            return Ok(Arc::clone(engine));
        }
    }

    let engine = Arc::new(
        WhisperEngine::load(&path, &settings.language).map_err(|e| e.to_string())?,
    );
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
        SILERO_DEFAULT_THRESHOLD,
    )?;
    let smoothed = SmoothedVad::new(
        Box::new(silero),
        VAD_PREFILL_FRAMES,
        VAD_OFFLINE_HANGOVER_FRAMES,
        VAD_ONSET_FRAMES,
    );
    let recorder = AudioRecorder::new()?.with_vad(
        Box::new(smoothed),
        VAD_OFFLINE_HANGOVER_FRAMES,
        VAD_STREAMING_HANGOVER_FRAMES,
    );
    Ok((recorder, true))
}

fn main() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let engine = tauri::async_runtime::block_on(PieEngine::new())?;
            let settings = Settings::load();
            let hotkey = settings.hotkey.clone();
            app.manage(AppState {
                settings: Mutex::new(settings),
                recorder: Mutex::new(None),
                busy: AtomicBool::new(false),
                whisper: Mutex::new(None),
                engine: tokio::sync::Mutex::new(engine),
            });
            app.manage(EnigoState::new());

            if let Err(e) = register_hotkey(app.handle(), &hotkey) {
                // A bad hotkey must not prevent the app from starting.
                log::error!("{e}");
            }

            // Floating indicator, created hidden and shown per recording state.
            overlay::create_overlay(app.handle());

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
            start_recording,
            stop_recording,
            cancel_recording,
            send_to_llm,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PIE");
}
