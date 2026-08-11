//! The audio-capture seam.
//!
//! [`AudioCapture`] is everything callers need from an audio backend: open a
//! device, start/stop a capture session, close. The cpal-based
//! [`AudioRecorder`](super::AudioRecorder) is the production adapter; tests
//! can provide fakes without touching real audio hardware.

use super::vad::VadPolicy;
use super::AudioRecorder;

/// Trait for audio capture — allows swapping backend implementations.
///
/// The session lifecycle is `open` → (`start` → `stop`)* → `close`.
/// `stop` returns the captured samples as 16 kHz mono f32.
pub trait AudioCapture: Send {
    /// Open the default input device and spin up the capture stream.
    fn open(&mut self) -> anyhow::Result<()>;

    /// Begin capturing with the given VAD policy.
    fn start(&self, policy: VadPolicy) -> anyhow::Result<()>;

    /// Stop capturing and return the samples recorded since `start`.
    fn stop(&self) -> anyhow::Result<Vec<f32>>;

    /// Tear down the stream and release the device.
    fn close(&mut self) -> anyhow::Result<()>;
}

impl AudioCapture for AudioRecorder {
    fn open(&mut self) -> anyhow::Result<()> {
        AudioRecorder::open(self, None)
    }

    fn start(&self, policy: VadPolicy) -> anyhow::Result<()> {
        AudioRecorder::start(self, policy)
    }

    fn stop(&self) -> anyhow::Result<Vec<f32>> {
        AudioRecorder::stop(self)
    }

    fn close(&mut self) -> anyhow::Result<()> {
        AudioRecorder::close(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake adapter: proves the seam is real (second adapter besides the
    /// cpal recorder) and that callers can be tested without audio hardware.
    struct FakeCapture {
        open: bool,
        samples: Vec<f32>,
    }

    impl AudioCapture for FakeCapture {
        fn open(&mut self) -> anyhow::Result<()> {
            self.open = true;
            Ok(())
        }
        fn start(&self, _policy: VadPolicy) -> anyhow::Result<()> {
            anyhow::ensure!(self.open, "not open");
            Ok(())
        }
        fn stop(&self) -> anyhow::Result<Vec<f32>> {
            Ok(self.samples.clone())
        }
        fn close(&mut self) -> anyhow::Result<()> {
            self.open = false;
            Ok(())
        }
    }

    #[test]
    fn capture_session_through_the_seam() {
        let mut capture: Box<dyn AudioCapture> = Box::new(FakeCapture {
            open: false,
            samples: vec![0.1, 0.2, 0.3],
        });
        capture.open().unwrap();
        capture.start(VadPolicy::Disabled).unwrap();
        let samples = capture.stop().unwrap();
        assert_eq!(samples.len(), 3);
        capture.close().unwrap();
    }
}
