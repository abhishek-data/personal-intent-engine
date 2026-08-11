//! Cache for the loaded VAD detector.
//!
//! Building a Silero detector calls `commit_from_file`, which constructs an
//! ONNX inference session from disk — too slow to redo on every recording. This
//! memoizes the detector by model path and hands out a shared handle so the
//! session persists. A path change rebuilds. The recorder resets the detector's
//! recurrent + smoothing state at each session start, so reuse is safe.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;

use super::vad::VoiceActivityDetector;

/// A VAD detector shared between the cache and a recorder, behind a mutex so the
/// recorder's worker thread can drive it.
pub type SharedVad = Arc<Mutex<Box<dyn VoiceActivityDetector>>>;

/// Memoizes the loaded VAD detector, keyed by model path.
#[derive(Default)]
pub struct VadCache {
    cached: Option<(PathBuf, SharedVad)>,
}

impl VadCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a shared detector for `model_path`, invoking `build` only on a
    /// cache miss (a different or absent path). On a hit, returns a clone of the
    /// cached handle without rebuilding. A `build` error propagates and leaves
    /// the cache unchanged.
    pub fn get_or_build<F>(&mut self, model_path: &Path, build: F) -> Result<SharedVad>
    where
        F: FnOnce() -> Result<Box<dyn VoiceActivityDetector>>,
    {
        if let Some((path, detector)) = &self.cached {
            if path == model_path {
                return Ok(Arc::clone(detector));
            }
        }
        let detector: SharedVad = Arc::new(Mutex::new(build()?));
        self.cached = Some((model_path.to_path_buf(), Arc::clone(&detector)));
        Ok(detector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::vad::PassthroughVad;
    use std::cell::Cell;

    fn passthrough() -> anyhow::Result<Box<dyn VoiceActivityDetector>> {
        Ok(Box::new(PassthroughVad))
    }

    #[test]
    fn same_path_builds_once_and_shares_the_handle() {
        let mut cache = VadCache::new();
        let calls = Cell::new(0);
        let p = Path::new("/models/silero.onnx");
        let a = cache
            .get_or_build(p, || {
                calls.set(calls.get() + 1);
                passthrough()
            })
            .unwrap();
        let b = cache
            .get_or_build(p, || {
                calls.set(calls.get() + 1);
                passthrough()
            })
            .unwrap();
        assert_eq!(
            calls.get(),
            1,
            "second call with same path must hit the cache"
        );
        assert!(
            Arc::ptr_eq(&a, &b),
            "same path must return the same shared handle"
        );
    }

    #[test]
    fn different_path_rebuilds() {
        let mut cache = VadCache::new();
        let calls = Cell::new(0);
        let a = cache
            .get_or_build(Path::new("/models/a.onnx"), || {
                calls.set(calls.get() + 1);
                passthrough()
            })
            .unwrap();
        let b = cache
            .get_or_build(Path::new("/models/b.onnx"), || {
                calls.set(calls.get() + 1);
                passthrough()
            })
            .unwrap();
        assert_eq!(calls.get(), 2, "a new path must rebuild");
        assert!(
            !Arc::ptr_eq(&a, &b),
            "different paths must not share a handle"
        );
    }

    #[test]
    fn build_error_propagates_and_leaves_cache_empty() {
        let mut cache = VadCache::new();
        let r = cache.get_or_build(Path::new("/models/a.onnx"), || {
            Err(anyhow::anyhow!("load failed"))
        });
        assert!(r.is_err(), "a build error must propagate");
        // A later successful build for the same path must still run (not cached).
        let ok = cache.get_or_build(Path::new("/models/a.onnx"), || {
            Ok(Box::new(PassthroughVad) as Box<dyn VoiceActivityDetector>)
        });
        assert!(ok.is_ok());
    }
}
