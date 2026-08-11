//! Silero VAD ONNX inference engine.
//!
//! Runs the Silero VAD v4 ONNX model over 30 ms audio frames via ONNX Runtime
//! (`ort`), returning a per-frame speech probability. The v4 model is a small
//! recurrent (LSTM) network, so its hidden (`h`) and cell (`c`) states are fed
//! back in on every call and must be reset between recordings.
//!
//! PIE owns this code directly rather than depending on an external VAD crate:
//! it is thin ONNX glue, and the only runtime dependencies are the widely-used
//! `ort` and `ndarray` crates. See [`super::silero`] for the smoothing wrapper
//! that turns these raw probabilities into speech/noise decisions.

use std::path::Path;

use anyhow::{Context, Result};
use ndarray::{Array1, Array2, Array3};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;

/// Collapse a parameterized `ort::Error<R>` (which borrows the builder/session
/// and is therefore not `Send`/`Sync`) into the plain `ort::Error` that
/// `anyhow` can absorb.
fn plain(e: impl Into<ort::Error>) -> ort::Error {
    e.into()
}

/// Silero VAD v4 hidden/cell state shape: (2 layers, batch 1, 64 hidden units).
const STATE_SHAPE: (usize, usize, usize) = (2, 1, 64);

/// A loaded Silero VAD ONNX session plus the recurrent state carried between
/// frames.
pub struct SileroVadEngine {
    session: Session,
    /// LSTM hidden state, fed back on each `compute` call.
    h_tensor: Array3<f32>,
    /// LSTM cell state, fed back on each `compute` call.
    c_tensor: Array3<f32>,
    /// Sample rate tensor, constant for the lifetime of the engine.
    sample_rate_tensor: Array1<i64>,
}

impl SileroVadEngine {
    /// Load the Silero ONNX model at `model_path`. `sample_rate` must be 8000
    /// or 16000 (PIE always uses 16000).
    pub fn new<P: AsRef<Path>>(model_path: P, sample_rate: usize) -> Result<Self> {
        if ![8000_usize, 16000].contains(&sample_rate) {
            anyhow::bail!("unsupported sample rate {sample_rate}; use 8000 or 16000");
        }

        let session = Session::builder()
            .map_err(plain)
            .context("create ONNX session builder")?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(plain)?
            .with_intra_threads(1)
            .map_err(plain)?
            .with_inter_threads(1)
            .map_err(plain)?
            .commit_from_file(model_path.as_ref())
            .map_err(plain)
            .with_context(|| format!("load Silero model at {}", model_path.as_ref().display()))?;

        Ok(Self {
            session,
            h_tensor: Array3::<f32>::zeros(STATE_SHAPE),
            c_tensor: Array3::<f32>::zeros(STATE_SHAPE),
            sample_rate_tensor: Array1::from_vec(vec![sample_rate as i64]),
        })
    }

    /// Run inference on one frame of samples and return its speech probability
    /// in `0.0..=1.0`. Updates the recurrent state as a side effect.
    pub fn compute(&mut self, samples: &[f32]) -> Result<f32> {
        let samples_tensor = Array2::from_shape_vec((1, samples.len()), samples.to_vec())?;
        let samples_value = Value::from_array(samples_tensor)?;
        let sr_value = Value::from_array(self.sample_rate_tensor.clone())?;
        let h_value = Value::from_array(self.h_tensor.clone())?;
        let c_value = Value::from_array(self.c_tensor.clone())?;

        let result = self.session.run(ort::inputs![
            "input" => samples_value,
            "sr" => sr_value,
            "h" => h_value,
            "c" => c_value
        ])?;

        // Feed the updated hidden/cell state back in for the next frame.
        let h_output = result
            .get("hn")
            .context("model output missing 'hn'")?
            .try_extract_tensor::<f32>()?;
        self.h_tensor = Array3::from_shape_vec(STATE_SHAPE, h_output.1.to_vec())?;

        let c_output = result
            .get("cn")
            .context("model output missing 'cn'")?
            .try_extract_tensor::<f32>()?;
        self.c_tensor = Array3::from_shape_vec(STATE_SHAPE, c_output.1.to_vec())?;

        let output = result
            .get("output")
            .context("model output missing 'output'")?
            .try_extract_tensor::<f32>()?;
        output
            .1
            .first()
            .copied()
            .context("model produced an empty output tensor")
    }

    /// Clear the LSTM hidden/cell state so a new session doesn't inherit
    /// recurrent context from the previous recording.
    pub fn reset(&mut self) {
        self.h_tensor.fill(0.0);
        self.c_tensor.fill(0.0);
    }
}
