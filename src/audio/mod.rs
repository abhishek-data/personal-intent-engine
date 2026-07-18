pub mod recorder;
pub mod resampler;
#[cfg(feature = "vad")]
pub mod silero;
#[cfg(feature = "vad")]
pub mod silero_vad_engine;
pub mod vad;

/// Audio capture configuration
pub const WHISPER_SAMPLE_RATE: usize = 16000;
pub const FRAME_DURATION_MS: usize = 30;
pub const FRAME_SAMPLES: usize = WHISPER_SAMPLE_RATE * FRAME_DURATION_MS / 1000; // 480

/// Re-exports
pub use recorder::{AudioFrameCallback, AudioRecorder};
pub use resampler::AudioResampler;
#[cfg(feature = "vad")]
pub use silero::{SileroVad, PIE_VAD_THRESHOLD};
pub use vad::{
    EnergyVad, VadPipeline, VadFrame, VadPolicy, VoiceActivityDetector,
    VAD_HANGOVER_FRAMES, VAD_SPEECH_THRESHOLD_FRAMES, VAD_CONTEXT_FRAMES,
    VAD_STREAM_HANGOVER_FRAMES,
};
