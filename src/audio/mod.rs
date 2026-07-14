pub mod recorder;
pub mod resampler;
pub mod vad;

/// Audio capture configuration
pub const WHISPER_SAMPLE_RATE: usize = 16000;
pub const FRAME_DURATION_MS: usize = 30;
pub const FRAME_SAMPLES: usize = WHISPER_SAMPLE_RATE * FRAME_DURATION_MS / 1000; // 480

/// Re-exports
pub use recorder::AudioRecorder;
pub use resampler::FrameResampler;
pub use vad::{SmoothedVad, VadPolicy, VoiceActivityDetector};
