use std::path::Path;
use std::sync::{mpsc, Mutex, Once};

use transcribe_cpp::{Backend, Model, ModelOptions, RunOptions, Session, StreamOptions, Task};

use super::stream::StreamCmd;
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

    /// Whether the loaded model supports incremental streaming decode.
    pub fn supports_streaming(&self) -> bool {
        self.session
            .lock()
            .expect("whisper session poisoned")
            .model()
            .capabilities()
            .supports_streaming
    }

    /// Run a streaming transcription worker: drains `rx`, feeding frames to
    /// the incremental decoder and calling `on_partial(committed, tentative)`
    /// whenever the text changes.
    ///
    /// Blocks until `Finalize` (returns `Some(final_text)`, also sent on the
    /// finalize reply channel) or `Cancel`. Returns `Ok(None)` when the model
    /// doesn't support streaming or the stream failed to begin — the finalize
    /// handshake still completes so the caller can fall back to batch
    /// transcription.
    pub fn run_stream(
        &self,
        rx: mpsc::Receiver<StreamCmd>,
        mut on_partial: impl FnMut(&str, &str),
    ) -> anyhow::Result<Option<String>> {
        let mut session = self.session.lock().expect("whisper session poisoned");

        if !session.model().capabilities().supports_streaming {
            log::info!("Model does not support streaming; deferring to batch transcription");
            drain_until_finalize(rx);
            return Ok(None);
        }

        let run_options = self.run_options();
        let mut stream = match session.stream(&run_options, &StreamOptions::default()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to begin stream: {e}");
                drain_until_finalize(rx);
                return Ok(None);
            }
        };

        while let Ok(cmd) = rx.recv() {
            match cmd {
                StreamCmd::Feed(pcm) => match stream.feed(&pcm) {
                    Ok(update) => {
                        if update.committed_changed || update.tentative_changed {
                            let text = stream.text();
                            on_partial(&text.committed, &text.tentative);
                        }
                    }
                    Err(e) => log::warn!("Stream feed failed: {e}"),
                },
                StreamCmd::Finalize(reply) => {
                    // After finalize the committed prefix holds the full text;
                    // display() = committed + tentative is the safe read.
                    let result = match stream.finalize() {
                        Ok(_) => Some(stream.text().display()),
                        Err(e) => {
                            log::error!("Stream finalize failed: {e}; caller should batch");
                            None
                        }
                    };
                    let _ = reply.send(result.clone());
                    return Ok(result);
                }
                StreamCmd::Cancel => {
                    stream.reset();
                    return Ok(None);
                }
            }
        }

        // Channel dropped without a finalize/cancel handshake.
        Ok(None)
    }

    /// Build run options with the language passed only when the loaded model
    /// advertises it; otherwise auto-detect rather than failing with
    /// UNSUPPORTED_LANGUAGE. Language-agnostic models report an empty list,
    /// so they always stay on auto.
    fn run_options(&self) -> RunOptions {
        let language = match self.language.as_str() {
            "auto" => None,
            other => Some(other.to_string()).filter(|l| self.model_languages.contains(l)),
        };
        RunOptions {
            task: Task::Transcribe,
            language,
            ..Default::default()
        }
    }
}

/// Consume commands until the finalize/cancel handshake so the caller's
/// `StreamRouter::finalize` never hangs when no stream could run.
fn drain_until_finalize(rx: mpsc::Receiver<StreamCmd>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            StreamCmd::Feed(_) => {}
            StreamCmd::Finalize(reply) => {
                let _ = reply.send(None);
                break;
            }
            StreamCmd::Cancel => break,
        }
    }
}

impl SttEngine for WhisperEngine {
    fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
        let run_options = self.run_options();
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
