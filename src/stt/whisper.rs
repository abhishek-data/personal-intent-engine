use std::path::Path;
use std::sync::{Mutex, Once};

use transcribe_cpp::{Backend, Model, ModelOptions, RunOptions, Session, Task};

use super::SttEngine;

static BACKEND_INIT: Once = Once::new();

/// Register transcribe-cpp compute backends once, before the first model load.
/// In a static build (macOS Metal) `init_backends_default` is a harmless no-op;
/// in a `dynamic-backends` build it loads the per-ISA CPU / GPU modules.
fn init_backends() {
    BACKEND_INIT.call_once(|| {
        transcribe_cpp::init_logging();
        if let Err(e) = transcribe_cpp::init_backends_default() {
            log::warn!("Failed to initialize transcribe-cpp backends: {e}");
        }
    });
}

/// Whisper-family STT engine backed by whisper.cpp via transcribe-cpp.
///
/// Loads a GGML/GGUF model and transcribes 16 kHz mono f32 samples.
/// `Backend::Auto` picks the best available compute device (Metal on macOS)
/// with CPU fallback.
pub struct WhisperEngine {
    /// `Session::run` takes `&mut self`; the `SttEngine` trait is `&self` +
    /// `Sync`, so the session lives behind a mutex.
    session: Mutex<Session>,
    /// Languages the loaded model advertises (empty = language-agnostic).
    model_languages: Vec<String>,
    /// Requested language code, or "auto" for detection.
    language: String,
}

impl WhisperEngine {
    /// Load a whisper model from `model_path`. `language` is an ISO 639-1 code
    /// ("en", "de", ...) or "auto" for detection.
    pub fn load(model_path: &Path, language: &str) -> anyhow::Result<Self> {
        init_backends();

        let options = ModelOptions {
            backend: Backend::Auto,
            gpu_device: 0,
        };
        let model = Model::load_with(model_path, &options).map_err(|e| {
            anyhow::anyhow!(
                "Failed to load whisper model '{}': {e}",
                model_path.display()
            )
        })?;
        log::info!(
            "Loaded whisper model '{}' (backend '{}')",
            model_path.display(),
            model.backend()
        );

        let session = model
            .session()
            .map_err(|e| anyhow::anyhow!("Failed to create whisper session: {e}"))?;
        let model_languages = session.model().capabilities().languages;

        Ok(Self {
            session: Mutex::new(session),
            model_languages,
            language: language.to_string(),
        })
    }
}

impl SttEngine for WhisperEngine {
    fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
        // Only pass a language the loaded model actually advertises; otherwise
        // auto-detect rather than failing with UNSUPPORTED_LANGUAGE.
        // Language-agnostic models report an empty list, so they stay on auto.
        let language = match self.language.as_str() {
            "auto" => None,
            other => Some(other.to_string()).filter(|l| self.model_languages.contains(l)),
        };

        let run_options = RunOptions {
            task: Task::Transcribe,
            language,
            ..Default::default()
        };

        let mut session = self.session.lock().expect("whisper session poisoned");
        session
            .run(samples, &run_options)
            .map(|t| t.text)
            .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {e}"))
    }

    fn is_ready(&self) -> bool {
        true
    }
}
