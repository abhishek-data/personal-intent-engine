use super::SttEngine;

/// Whisper-based STT engine.
/// Currently a stub — will integrate transcribe-cpp/whisper-rs.
pub struct WhisperEngine {
    model_path: Option<String>,
}

impl WhisperEngine {
    pub fn new(model_path: Option<&str>) -> Self {
        Self {
            model_path: model_path.map(|s| s.to_string()),
        }
    }
}

impl SttEngine for WhisperEngine {
    fn transcribe(&self, _samples: &[f32]) -> anyhow::Result<String> {
        // TODO: Integrate transcribe-cpp or whisper-rs
        anyhow::bail!("Whisper STT not yet integrated. Use text input mode.")
    }

    fn is_ready(&self) -> bool {
        self.model_path.is_some()
    }
}
