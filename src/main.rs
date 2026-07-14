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

    let input = resolve_input(&args)?;

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

/// Resolve the input source: WAV file, microphone, CLI text, or stdin.
fn resolve_input(args: &Args) -> anyhow::Result<Input> {
    if let Some(path) = &args.audio_file {
        return Ok(Input::Audio(load_audio_file(path)?));
    }

    if args.voice {
        return Ok(Input::Audio(record_from_mic()?));
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
fn build_stt_engine(args: &Args) -> anyhow::Result<Box<dyn pie_engine::stt::SttEngine>> {
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
    let engine = pie_engine::stt::WhisperEngine::load(&model_path, &args.language)?;
    Ok(Box::new(engine))
}

#[cfg(feature = "whisper")]
fn load_audio_file(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    pie_engine::stt::load_wav_as_16k_mono(path)
}

#[cfg(feature = "whisper")]
fn record_from_mic() -> anyhow::Result<Vec<f32>> {
    use pie_engine::audio::{AudioRecorder, VadPolicy};

    let mut recorder = AudioRecorder::new()?;
    recorder.open(None)?;
    recorder.start(VadPolicy::Disabled)?;
    eprintln!("Recording... press Enter to stop.");
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let samples = recorder.stop()?;
    recorder.close()?;
    eprintln!("Captured {:.1}s of audio.", samples.len() as f32 / 16000.0);
    Ok(samples)
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
fn record_from_mic() -> anyhow::Result<Vec<f32>> {
    anyhow::bail!("--voice requires the 'whisper' feature: cargo run --features whisper")
}
