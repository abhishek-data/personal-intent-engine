use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, SizedSample};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use super::{vad, FrameResampler, SmoothedVad, VadPolicy, WHISPER_SAMPLE_RATE};

/// Commands for the audio worker thread
enum Cmd {
    Start(VadPolicy, Instant),
    Stop(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

/// Audio chunk from cpal callback
enum AudioChunk {
    Samples(Vec<f32>),
    EndOfStream,
}

/// Callback for real-time 16kHz frames (used for streaming transcription)
pub type AudioFrameCallback = Arc<dyn Fn(&[f32]) + Send + Sync + 'static>;

/// Cross-platform audio recorder using cpal.
///
/// Captures audio from the default input device, resamples to 16kHz mono,
/// applies VAD filtering, and provides both buffered and streaming output.
///
/// Architecture based on Handy's recorder pattern:
/// - Dedicated worker thread for audio processing
/// - Channel-based communication (no shared mutable state)
/// - Cached device config to avoid HAL round-trips
pub struct AudioRecorder {
    device: Option<Device>,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    vad: Option<Arc<Mutex<SmoothedVad>>>,
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
    audio_cb: Option<AudioFrameCallback>,
    config_cache: Arc<Mutex<Option<(String, cpal::SupportedStreamConfig)>>>,
}

impl AudioRecorder {
    pub fn new() -> anyhow::Result<Self> {
        Ok(AudioRecorder {
            device: None,
            cmd_tx: None,
            worker_handle: None,
            vad: None,
            level_cb: None,
            audio_cb: None,
            config_cache: Arc::new(Mutex::new(None)),
        })
    }

    /// Attach a VAD engine
    pub fn with_vad(mut self, vad: SmoothedVad) -> Self {
        self.vad = Some(Arc::new(Mutex::new(vad)));
        self
    }

    /// Register a callback for audio level visualization
    pub fn with_level_callback<F: Fn(Vec<f32>) + Send + Sync + 'static>(
        mut self,
        cb: F,
    ) -> Self {
        self.level_cb = Some(Arc::new(cb));
        self
    }

    /// Register a callback for real-time 16kHz frames (streaming transcription)
    pub fn with_audio_callback<F: Fn(&[f32]) + Send + Sync + 'static>(
        mut self,
        cb: F,
    ) -> Self {
        self.audio_cb = Some(Arc::new(cb));
        self
    }

    /// Open the audio device and start the worker thread
    pub fn open(&mut self, device: Option<Device>) -> anyhow::Result<()> {
        if self.worker_handle.is_some() {
            return Ok(());
        }

        let (sample_tx, sample_rx) = mpsc::channel::<AudioChunk>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), String>>(1);

        let host = cpal::default_host();
        let device = match device {
            Some(dev) => dev,
            None => host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("No input device found"))?,
        };

        let thread_device = device.clone();
        let vad = self.vad.clone();
        let level_cb = self.level_cb.clone();
        let audio_cb = self.audio_cb.clone();
        let config_cache = Arc::clone(&self.config_cache);

        let worker = std::thread::spawn(move || {
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_for_stream = stop_flag.clone();

            // Get or create stream config
            let config = match Self::get_preferred_config(&thread_device) {
                Ok(cfg) => cfg,
                Err(e) => {
                    let _ = init_tx.send(Err(format!("Config failed: {e}")));
                    return;
                }
            };

            let sample_rate = config.sample_rate().0;
            let channels = config.channels() as usize;

            // Build cpal stream
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    Self::build_stream::<f32>(&thread_device, &config, sample_tx.clone(), stop_flag_for_stream.clone())
                }
                cpal::SampleFormat::I16 => {
                    Self::build_stream::<i16>(&thread_device, &config, sample_tx.clone(), stop_flag_for_stream.clone())
                }
                cpal::SampleFormat::U8 => {
                    Self::build_stream::<u8>(&thread_device, &config, sample_tx.clone(), stop_flag_for_stream.clone())
                }
                fmt => {
                    let _ = init_tx.send(Err(format!("Unsupported format: {fmt:?}")));
                    return;
                }
            };

            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    let _ = init_tx.send(Err(format!("Stream build failed: {e}")));
                    return;
                }
            };

            let _ = init_tx.send(Ok(()));

            // Consumer loop: read audio chunks, resample, apply VAD
            let mut resampler = FrameResampler::new(sample_rate as usize, WHISPER_SAMPLE_RATE, channels);
            let mut frame_buffer: Vec<f32> = Vec::new();

            loop {
                // Check for commands (non-blocking)
                match cmd_rx.try_recv() {
                    Ok(Cmd::Start(policy, _ts)) => {
                        stop_flag.store(false, Ordering::Relaxed);
                        if let Some(ref vad) = vad {
                            vad.lock().unwrap().reset();
                        }
                        frame_buffer.clear();
                        let _ = stream.play();
                    }
                    Ok(Cmd::Stop(sender)) => {
                        stop_flag.store(true, Ordering::Relaxed);
                        let _ = stream.pause();
                        let _ = sender.send(frame_buffer.clone());
                        frame_buffer.clear();
                    }
                    Ok(Cmd::Shutdown) => {
                        stop_flag.store(true, Ordering::Relaxed);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }

                // Read audio samples
                match sample_rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(AudioChunk::Samples(samples)) => {
                        // Resample to 16kHz mono
                        let resampled = resampler.resample(&samples);

                        // Apply VAD if configured
                        if let Some(ref vad) = vad {
                            for chunk in resampled.chunks(super::FRAME_SAMPLES) {
                                if chunk.len() == super::FRAME_SAMPLES {
                                    let mut vad_guard = vad.lock().unwrap();
                                    match vad_guard.push_frame(chunk) {
                                        Ok(vad::VadFrame::Speech(buf)) => {
                                            frame_buffer.extend_from_slice(buf);
                                            // Feed streaming callback
                                            if let Some(ref cb) = audio_cb {
                                                cb(buf);
                                            }
                                        }
                                        Ok(vad::VadFrame::Noise) => {
                                            // Silence — skip
                                        }
                                        Err(_) => {
                                            // VAD error — pass through
                                            frame_buffer.extend_from_slice(chunk);
                                        }
                                    }
                                }
                            }
                        } else {
                            frame_buffer.extend_from_slice(&resampled);
                            if let Some(ref cb) = audio_cb {
                                cb(&resampled);
                            }
                        }

                        // Level callback
                        if let Some(ref cb) = level_cb {
                            cb(resampled);
                        }
                    }
                    Ok(AudioChunk::EndOfStream) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        // Wait for init
        match init_rx.recv() {
            Ok(Ok(())) => {
                self.cmd_tx = Some(cmd_tx);
                self.worker_handle = Some(worker);
                self.device = Some(device);
            }
            Ok(Err(e)) => anyhow::bail!("Audio init failed: {e}"),
            Err(e) => anyhow::bail!("Audio init channel error: {e}"),
        }

        Ok(())
    }

    /// Start recording
    pub fn start(&self, policy: VadPolicy) -> anyhow::Result<()> {
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Start(policy, Instant::now()))
                .map_err(|_| anyhow::anyhow!("Audio worker disconnected"))?;
        }
        Ok(())
    }

    /// Stop recording and return accumulated audio
    pub fn stop(&self) -> anyhow::Result<Vec<f32>> {
        let (tx, rx) = mpsc::channel();
        if let Some(cmd_tx) = &self.cmd_tx {
            cmd_tx.send(Cmd::Stop(tx))
                .map_err(|_| anyhow::anyhow!("Audio worker disconnected"))?;
        }
        rx.recv().map_err(|_| anyhow::anyhow!("Audio worker dropped result"))
    }

    /// Build a cpal input stream for the given sample type
    fn build_stream<T: SizedSample + Sample>(
        device: &Device,
        config: &cpal::StreamConfig,
        tx: mpsc::Sender<AudioChunk>,
        stop_flag: Arc<AtomicBool>,
    ) -> anyhow::Result<cpal::Stream> {
        let channels = config.channels as usize;
        let stream = device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    if stop_flag.load(Ordering::Relaxed) {
                        return;
                    }
                    let samples: Vec<f32> = data.iter().map(|s| s.to_sample::<f32>()).collect();
                    let _ = tx.send(AudioChunk::Samples(samples));
                },
                |err| {
                    log::error!("Audio stream error: {err}");
                },
                None,
            )
            .map_err(|e| anyhow::anyhow!("Stream build failed: {e}"))?;

        Ok(stream)
    }

    /// Get preferred stream config for a device (with caching)
    fn get_preferred_config(device: &Device) -> anyhow::Result<cpal::SupportedStreamConfig> {
        device
            .default_input_config()
            .map_err(|e| anyhow::anyhow!("No input config: {e}"))
    }

    /// List available input devices
    pub fn list_devices() -> anyhow::Result<Vec<Device>> {
        let host = cpal::default_host();
        let devices: Vec<Device> = host
            .input_devices()
            .map_err(|e| anyhow::anyhow!("Device enumeration failed: {e}"))?
            .collect();
        Ok(devices)
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}
