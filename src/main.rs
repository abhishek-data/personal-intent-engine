use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "pie",
    about = "Personal Intent Engine - Intelligent AI middleware"
)]
struct Args {
    /// Text input to process
    #[arg(trailing_var_arg = true)]
    input: Vec<String>,

    /// Optimization mode
    #[arg(short, long, default_value = "balanced")]
    mode: String,

    /// LLM provider
    #[arg(short, long, default_value = "openai")]
    provider: String,

    /// Model name
    #[arg(long)]
    model: Option<String>,

    /// Record from the microphone until Enter is pressed (requires the
    /// `whisper` feature and a whisper model)
    #[arg(long)]
    voice: bool,

    /// Transcribe a WAV file and use it as input (requires the `whisper`
    /// feature and a whisper model)
    #[arg(long)]
    audio_file: Option<std::path::PathBuf>,

    /// Path to a whisper GGML/GGUF model (or set PIE_WHISPER_MODEL)
    #[arg(long)]
    whisper_model: Option<std::path::PathBuf>,

    /// Path to a Silero VAD ONNX model for --voice (or set PIE_SILERO_MODEL;
    /// defaults to ~/.cache/pie/models/silero_vad_v4.onnx when present)
    #[arg(long)]
    silero_model: Option<std::path::PathBuf>,

    /// Spoken language code ("en", "de", ...) or "auto"
    #[arg(long, default_value = "auto")]
    language: String,

    /// Verbose output (show intent, optimized prompt, etc.)
    #[arg(short, long)]
    verbose: bool,
}

/// Resolved user input: either text to process directly, or 16 kHz mono
/// samples awaiting transcription.
enum Input {
    Text(String),
    Audio(Vec<f32>),
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();

    let input = if args.voice {
        // Voice transcribes during/after capture (streaming when the model
        // supports it), so it enters the pipeline as text.
        Input::Text(voice_session(&args)?)
    } else {
        resolve_input(&args)?
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut engine = pie_engine::PieEngine::new().await?;

        if let Input::Audio(_) = &input {
            engine = engine.with_stt(build_stt_engine(&args)?);
        }

        if args.verbose {
            println!("[PIE] Mode: {}", args.mode);
            println!("[PIE] Provider: {}", args.provider);
        }

        let result = match &input {
            Input::Text(text) => {
                if args.verbose {
                    println!("[PIE] Input: {text}\n");
                }
                engine.process(text, &args.mode).await?
            }
            Input::Audio(samples) => {
                let result = engine.process_audio(samples, &args.mode).await?;
                if args.verbose {
                    println!("[PIE] Transcript: {}\n", result.intent.raw_input);
                }
                result
            }
        };

        if args.verbose {
            println!("[PIE] Detected intent:");
            println!("  Objective: {}", result.intent.objective);
            println!("  Type: {:?}", result.intent.conversation_type);
            println!("  Confidence: {:?}", result.intent.confidence);
            println!("  Context: {:?}", result.intent.context);
            println!("  Constraints: {:?}", result.intent.constraints);
            println!();
            println!(
                "[PIE] Optimized prompt ({} chars):",
                result.optimized_prompt.len()
            );
            println!("{}", result.optimized_prompt);
            println!();
        }

        // Send to LLM
        let response = engine
            .send_to_llm(
                &result.optimized_prompt,
                &args.provider,
                args.model.as_deref(),
            )
            .await?;

        println!("{}", response);
        Ok(())
    })
}

/// Resolve a non-voice input source: WAV file, CLI text, or stdin.
fn resolve_input(args: &Args) -> anyhow::Result<Input> {
    if let Some(path) = &args.audio_file {
        return Ok(Input::Audio(load_audio_file(path)?));
    }

    let text = if args.input.is_empty() {
        let mut buffer = String::new();
        std::io::stdin().read_line(&mut buffer)?;
        buffer.trim().to_string()
    } else {
        args.input.join(" ")
    };

    if text.is_empty() {
        anyhow::bail!("No input. Pass text, pipe via stdin, or use --voice / --audio-file.");
    }
    Ok(Input::Text(text))
}

#[cfg(feature = "whisper")]
fn build_whisper_engine(args: &Args) -> anyhow::Result<pie_engine::stt::WhisperEngine> {
    let model_path = args
        .whisper_model
        .clone()
        .or_else(|| std::env::var_os("PIE_WHISPER_MODEL").map(Into::into))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No whisper model. Pass --whisper-model <path> or set PIE_WHISPER_MODEL.\n\
                 Download one, e.g.:\n  curl -L -o ggml-tiny.en.bin \
                 https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin"
            )
        })?;
    pie_engine::stt::WhisperEngine::load(&model_path, &args.language)
}

#[cfg(feature = "whisper")]
fn build_stt_engine(args: &Args) -> anyhow::Result<Box<dyn pie_engine::stt::SttEngine>> {
    Ok(Box::new(build_whisper_engine(args)?))
}

#[cfg(feature = "whisper")]
fn load_audio_file(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    pie_engine::stt::load_wav_as_16k_mono(path)
}

/// Record from the microphone and return the transcript. Streams the decode
/// live (partial text as you speak) when the model supports it; otherwise
/// transcribes the captured audio in one batch after Enter.
#[cfg(feature = "whisper")]
fn voice_session(args: &Args) -> anyhow::Result<String> {
    use pie_engine::audio::VadPolicy;
    use pie_engine::stt::{StreamRouter, SttEngine};
    use std::sync::Arc;

    let engine = build_whisper_engine(args)?;
    let use_streaming = engine.supports_streaming();
    let router = Arc::new(StreamRouter::new());

    let (recorder, vad_active) = build_recorder(args)?;
    let mut recorder = if use_streaming {
        let feed_router = Arc::clone(&router);
        recorder.with_audio_callback(move |frame| feed_router.feed(frame))
    } else {
        recorder
    };

    let policy = match (vad_active, use_streaming) {
        (false, _) => VadPolicy::Disabled,
        (true, true) => VadPolicy::Streaming,
        (true, false) => VadPolicy::Offline,
    };

    let transcript = std::thread::scope(|scope| -> anyhow::Result<String> {
        let worker = use_streaming.then(|| {
            let rx = router.open();
            let engine = &engine;
            scope.spawn(move || {
                engine.run_stream(rx, |committed, tentative| {
                    eprint!("\r\x1b[2K[live] {committed}{tentative}");
                })
            })
        });

        recorder.open(None)?;
        recorder.start(policy)?;
        eprintln!("Recording... press Enter to stop.");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let samples = recorder.stop()?;
        recorder.close()?;
        eprintln!(
            "\nCaptured {:.1}s of audio.",
            samples.len() as f32 / 16000.0
        );

        // All frames were fed on the recorder's consumer thread before stop()
        // returned, so FIFO ordering puts them ahead of this Finalize.
        let streamed = if let Some(worker) = worker {
            let text = router.finalize()?;
            if let Err(e) = worker.join().expect("stream worker panicked") {
                log::warn!("Stream worker error: {e}");
            }
            text
        } else {
            None
        };

        match streamed.filter(|t| !t.trim().is_empty()) {
            Some(text) => Ok(text.trim().to_string()),
            None => {
                if use_streaming {
                    eprintln!("Stream produced no text; falling back to batch transcription.");
                }
                Ok(engine.transcribe(&samples)?.trim().to_string())
            }
        }
    })?;

    if transcript.is_empty() {
        anyhow::bail!("Transcription produced no text (silence or unintelligible audio)");
    }
    Ok(transcript)
}

/// Build the recorder with Silero VAD when available: an explicit
/// --silero-model / PIE_SILERO_MODEL must load (errors propagate); the
/// default cache path is used opportunistically.
#[cfg(all(feature = "whisper", feature = "vad"))]
fn build_recorder(args: &Args) -> anyhow::Result<(pie_engine::audio::AudioRecorder, bool)> {
    use pie_engine::audio::{
        AudioRecorder, SileroVad, SmoothedVad, SILERO_DEFAULT_THRESHOLD,
        VAD_OFFLINE_HANGOVER_FRAMES, VAD_ONSET_FRAMES, VAD_PREFILL_FRAMES,
        VAD_STREAMING_HANGOVER_FRAMES,
    };

    let model_path = args
        .silero_model
        .clone()
        .or_else(|| std::env::var_os("PIE_SILERO_MODEL").map(Into::into))
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| {
                    std::path::PathBuf::from(home).join(".cache/pie/models/silero_vad_v4.onnx")
                })
                .filter(|p| p.exists())
        });

    match model_path {
        Some(path) => {
            let silero = SileroVad::new(&path, SILERO_DEFAULT_THRESHOLD)?;
            let smoothed = SmoothedVad::new(
                Box::new(silero),
                VAD_PREFILL_FRAMES,
                VAD_OFFLINE_HANGOVER_FRAMES,
                VAD_ONSET_FRAMES,
            );
            let recorder = AudioRecorder::new()?.with_vad(
                Box::new(smoothed),
                VAD_OFFLINE_HANGOVER_FRAMES,
                VAD_STREAMING_HANGOVER_FRAMES,
            );
            Ok((recorder, true))
        }
        None => {
            eprintln!(
                "No Silero VAD model found; recording without VAD. \
                 Download one:\n  curl -L -o ~/.cache/pie/models/silero_vad_v4.onnx \
                 https://github.com/snakers4/silero-vad/raw/v4.0/files/silero_vad.onnx"
            );
            Ok((pie_engine::audio::AudioRecorder::new()?, false))
        }
    }
}

#[cfg(all(feature = "whisper", not(feature = "vad")))]
fn build_recorder(_args: &Args) -> anyhow::Result<(pie_engine::audio::AudioRecorder, bool)> {
    Ok((pie_engine::audio::AudioRecorder::new()?, false))
}

#[cfg(not(feature = "whisper"))]
fn build_stt_engine(_args: &Args) -> anyhow::Result<Box<dyn pie_engine::stt::SttEngine>> {
    anyhow::bail!("Voice input requires the 'whisper' feature: cargo run --features whisper")
}

#[cfg(not(feature = "whisper"))]
fn load_audio_file(_path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    anyhow::bail!("--audio-file requires the 'whisper' feature: cargo run --features whisper")
}

#[cfg(not(feature = "whisper"))]
fn voice_session(_args: &Args) -> anyhow::Result<String> {
    anyhow::bail!("--voice requires the 'whisper' feature: cargo run --features whisper")
}
