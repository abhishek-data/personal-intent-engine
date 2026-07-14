#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

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
    /// Loaded whisper engine, cached with the (path, language) it was built
    /// from so a settings change reloads it.
    whisper: Mutex<Option<(PathBuf, String, Arc<WhisperEngine>)>>,
    /// Text pipeline: intent -> memory -> optimizer -> LLM router.
    engine: tokio::sync::Mutex<PieEngine>,
}

/// Result payload for the frontend after a recording is processed.
#[derive(Serialize)]
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
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().expect("settings poisoned").clone()
}

#[tauri::command]
fn update_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    settings.save().map_err(|e| e.to_string())?;
    *state.settings.lock().expect("settings poisoned") = settings;
    // The whisper cache checks (path, language) on next use, so a model or
    // language change reloads naturally — no invalidation needed here.
    Ok(())
}

#[tauri::command]
fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let mut recorder_slot = state.recorder.lock().expect("recorder poisoned");
    if recorder_slot.is_some() {
        return Err("Already recording".to_string());
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
    emit_state(&app, "recording");
    Ok(())
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
async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Outcome, String> {
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
    emit_state(&app, "decoding");

    let result = transcribe_and_process(&app, &state, samples).await;
    emit_state(&app, "idle");
    result
}

async fn transcribe_and_process(
    _app: &AppHandle,
    state: &State<'_, AppState>,
    samples: Vec<f32>,
) -> Result<Outcome, String> {
    if samples.len() < 1600 {
        return Err("Recording too short (or VAD detected no speech)".to_string());
    }

    let settings = state.settings.lock().expect("settings poisoned").clone();

    // 2. Transcribe on a blocking thread (Metal/CPU inference).
    let whisper = get_or_load_whisper(state, &settings)?;
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
        .setup(|app| {
            let engine = tauri::async_runtime::block_on(PieEngine::new())?;
            app.manage(AppState {
                settings: Mutex::new(Settings::load()),
                recorder: Mutex::new(None),
                whisper: Mutex::new(None),
                engine: tokio::sync::Mutex::new(engine),
            });
            Ok(())
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
