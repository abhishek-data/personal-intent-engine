use anyhow::Result;

/// Trait for Voice Activity Detection engines.
pub trait VoiceActivityDetector: Send + Sync {
    /// Push a frame and get back whether it's speech or noise.
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>>;

    /// Check if a frame is voice (without buffering)
    fn is_voice(&mut self, frame: &[f32]) -> Result<bool>;

    /// Reset internal state (called between recording sessions)
    fn reset(&mut self);

    /// Update hangover frames
    fn set_hangover_frames(&mut self, frames: usize);
}

/// Result of VAD classification
pub enum VadFrame<'a> {
    /// Frame contains speech (may include prefill buffer)
    Speech(&'a [f32]),
    /// Frame is noise/silence
    Noise,
}

/// How VAD should filter frames for a recording session
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VadPolicy {
    /// Bypass VAD entirely
    Disabled,
    /// Standard offline profile
    Offline,
    /// Longer hangover for streaming models
    Streaming,
}

// VAD timing constants (in 30ms frames)
pub const VAD_PREFILL_FRAMES: usize = 15; // 450ms pre-speech context
pub const VAD_OFFLINE_HANGOVER_FRAMES: usize = 30; // 900ms post-speech
pub const VAD_STREAMING_HANGOVER_FRAMES: usize = 60; // 1.8s for streaming
pub const VAD_ONSET_FRAMES: usize = 3; // 90ms onset detection

/// Smoothed VAD wrapper with onset detection, hangover tail, and prefill buffering.
///
/// Based on Handy's SmoothedVad architecture:
/// - Buffers pre-speech frames for context
/// - Requires N consecutive voice frames before triggering speech
/// - Continues forwarding after speech ends (hangover tail)
/// - State machine: Silence -> Onset -> Speech -> Hangover -> Silence
pub struct SmoothedVad {
    inner: Box<dyn VoiceActivityDetector>,
    prefill_frames: usize,
    hangover_frames: usize,
    onset_frames: usize,

    frame_buffer: std::collections::VecDeque<Vec<f32>>,
    hangover_counter: usize,
    onset_counter: usize,
    in_speech: bool,

    temp_out: Vec<f32>,
}

impl SmoothedVad {
    pub fn new(
        inner: Box<dyn VoiceActivityDetector>,
        prefill_frames: usize,
        hangover_frames: usize,
        onset_frames: usize,
    ) -> Self {
        Self {
            inner,
            prefill_frames,
            hangover_frames,
            onset_frames,
            frame_buffer: std::collections::VecDeque::new(),
            hangover_counter: 0,
            onset_counter: 0,
            in_speech: false,
            temp_out: Vec::new(),
        }
    }
}

impl VoiceActivityDetector for SmoothedVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        // Buffer every frame for pre-roll
        self.frame_buffer.push_back(frame.to_vec());
        while self.frame_buffer.len() > self.prefill_frames + 1 {
            self.frame_buffer.pop_front();
        }

        let is_voice = self.inner.is_voice(frame)?;

        match (self.in_speech, is_voice) {
            // Potential start of speech
            (false, true) => {
                self.onset_counter += 1;
                if self.onset_counter >= self.onset_frames {
                    self.in_speech = true;
                    self.hangover_counter = self.hangover_frames;
                    self.onset_counter = 0;

                    // Collect prefill + current
                    self.temp_out.clear();
                    for buf in &self.frame_buffer {
                        self.temp_out.extend(buf);
                    }
                    Ok(VadFrame::Speech(&self.temp_out))
                } else {
                    Ok(VadFrame::Noise)
                }
            }
            // Ongoing speech
            (true, true) => {
                self.hangover_counter = self.hangover_frames;
                Ok(VadFrame::Speech(frame))
            }
            // End of speech (hangover)
            (true, false) => {
                if self.hangover_counter > 0 {
                    self.hangover_counter -= 1;
                    Ok(VadFrame::Speech(frame))
                } else {
                    self.in_speech = false;
                    Ok(VadFrame::Noise)
                }
            }
            // Silence
            (false, false) => {
                self.onset_counter = 0;
                Ok(VadFrame::Noise)
            }
        }
    }

    fn is_voice(&mut self, frame: &[f32]) -> Result<bool> {
        self.inner.is_voice(frame)
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.frame_buffer.clear();
        self.hangover_counter = 0;
        self.onset_counter = 0;
        self.in_speech = false;
    }

    fn set_hangover_frames(&mut self, frames: usize) {
        self.hangover_frames = frames;
    }
}

/// Placeholder VAD that always returns speech (for testing without Silero)
pub struct PassthroughVad;

impl VoiceActivityDetector for PassthroughVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        Ok(VadFrame::Speech(frame))
    }

    fn is_voice(&mut self, _frame: &[f32]) -> Result<bool> {
        Ok(true)
    }

    fn reset(&mut self) {}

    fn set_hangover_frames(&mut self, _frames: usize) {}
}

/// Energy-based VAD (simple threshold, no ML model required)
pub struct EnergyVad {
    threshold: f32,
}

impl EnergyVad {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }
}

impl VoiceActivityDetector for EnergyVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        let energy: f32 = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
        if energy.sqrt() > self.threshold {
            Ok(VadFrame::Speech(frame))
        } else {
            Ok(VadFrame::Noise)
        }
    }

    fn is_voice(&mut self, frame: &[f32]) -> Result<bool> {
        let energy: f32 = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
        Ok(energy.sqrt() > self.threshold)
    }

    fn reset(&mut self) {}

    fn set_hangover_frames(&mut self, _frames: usize) {}
}
