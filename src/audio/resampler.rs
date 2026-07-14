use rubato::{Resampler, SincFixedIn, SincInterpolationParameters};

/// Resamples audio from source sample rate to target (typically 16kHz).
/// Handles mono downmixing from multi-channel input.
pub struct FrameResampler {
    resampler: SincFixedIn<f32>,
    source_channels: usize,
    source_rate: usize,
    target_rate: usize,
    chunk_size: usize,
}

impl FrameResampler {
    pub fn new(source_rate: usize, target_rate: usize, source_channels: usize) -> Self {
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: rubato::SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: rubato::WindowFunction::BlackmanHarris2,
        };

        let chunk_size = 1024;
        let resampler = SincFixedIn::<f32>::new(
            target_rate as f64 / source_rate as f64,
            2.0,
            params,
            chunk_size,
            source_channels,
        )
        .expect("Failed to create resampler");

        Self {
            resampler,
            source_channels,
            source_rate,
            target_rate,
            chunk_size,
        }
    }

    /// Resample interleaved samples to 16kHz mono
    pub fn resample(&mut self, samples: &[f32]) -> Vec<f32> {
        if self.source_rate == super::WHISPER_SAMPLE_RATE && self.source_channels == 1 {
            return samples.to_vec();
        }

        // De-interleave channels
        let n_frames = samples.len() / self.source_channels;
        let mut channels: Vec<Vec<f32>> = vec![Vec::with_capacity(n_frames); self.source_channels];
        for (i, sample) in samples.iter().enumerate() {
            channels[i % self.source_channels].push(*sample);
        }

        // Process in chunks
        let mut output = Vec::new();
        for chunk_start in (0..n_frames).step_by(self.chunk_size) {
            let chunk_end = (chunk_start + self.chunk_size).min(n_frames);
            let chunk: Vec<Vec<f32>> = channels
                .iter()
                .map(|ch| ch[chunk_start..chunk_end].to_vec())
                .collect();

            if chunk[0].len() == self.chunk_size {
                if let Ok(resampled) = self.resampler.process(&chunk, None) {
                    // Downmix to mono (average channels)
                    if resampled.len() == 1 {
                        output.extend_from_slice(&resampled[0]);
                    } else {
                        for i in 0..resampled[0].len() {
                            let sum: f32 = resampled.iter().map(|ch| ch[i]).sum();
                            output.push(sum / resampled.len() as f32);
                        }
                    }
                }
            }
        }

        output
    }
}
