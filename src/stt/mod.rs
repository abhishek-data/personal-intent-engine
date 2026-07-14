pub mod whisper;

/// Trait for speech-to-text engines
pub trait SttEngine: Send + Sync {
    /// Transcribe audio samples (16kHz mono f32) to text
    fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String>;

    /// Check if the engine is ready (model loaded)
    fn is_ready(&self) -> bool;
}
