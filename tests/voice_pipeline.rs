//! End-to-end voice pipeline test: synthesized speech -> WAV -> whisper ->
//! intent extraction.
//!
//! Requires the `whisper` feature, macOS (`say` / `afconvert` for local speech
//! synthesis), and a whisper model at `PIE_WHISPER_MODEL` or the default cache
//! path. Skips (passes with a note) when those aren't available, so plain
//! `cargo test` stays green everywhere.
//!
//! Run with: cargo test --features whisper --test voice_pipeline

#![cfg(all(feature = "whisper", target_os = "macos"))]

use std::path::PathBuf;
use std::process::Command;

fn model_path() -> Option<PathBuf> {
    let path = std::env::var_os("PIE_WHISPER_MODEL")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                PathBuf::from(home)
                    .join(".cache")
                    .join("pie")
                    .join("models")
                    .join("ggml-tiny.en.bin")
            })
        })?;
    path.exists().then_some(path)
}

#[test]
fn transcribes_synthesized_speech_through_pipeline() {
    let Some(model) = model_path() else {
        eprintln!("skipping: no whisper model (set PIE_WHISPER_MODEL)");
        return;
    };

    let dir = std::env::temp_dir().join("pie-voice-test");
    std::fs::create_dir_all(&dir).unwrap();
    let aiff = dir.join("input.aiff");
    let wav = dir.join("input.wav");

    // Common dictionary words only: the tiny model renders rarer proper nouns
    // inconsistently ("postgres" -> "post-gurs"), which isn't what this test
    // is guarding.
    let phrase = "set up a docker container for my rust project";
    let say = Command::new("say")
        .arg("-o")
        .arg(&aiff)
        .arg(phrase)
        .status();
    let Ok(status) = say else {
        eprintln!("skipping: `say` unavailable");
        return;
    };
    assert!(status.success(), "say failed");

    // 22.05 kHz mono to exercise the resampling path, not just passthrough
    let status = Command::new("afconvert")
        .args(["-f", "WAVE", "-d", "LEI16@22050", "-c", "1"])
        .arg(&aiff)
        .arg(&wav)
        .status()
        .expect("afconvert unavailable");
    assert!(status.success(), "afconvert failed");

    let samples = pie_engine::stt::load_wav_as_16k_mono(&wav).expect("wav load failed");
    assert!(
        samples.len() > 16000,
        "expected >1s of audio, got {} samples",
        samples.len()
    );

    let engine = pie_engine::stt::WhisperEngine::load(&model, "en").expect("model load failed");
    let text = {
        use pie_engine::stt::SttEngine;
        engine.transcribe(&samples).expect("transcription failed")
    };
    let lower = text.to_lowercase();
    for word in ["docker", "rust", "project"] {
        assert!(
            lower.contains(word),
            "transcript missing '{word}': {text:?}"
        );
    }

    // Transcript flows through intent extraction
    let intent = pie_engine::intent::IntentExtractor::new().extract(&text);
    assert!(intent.topics.contains(&"docker".to_string()));
    assert!(intent.topics.contains(&"rust".to_string()));
}

/// Exercises the full streaming session exactly as the CLI voice path runs
/// it: feed frames through the StreamRouter to a run_stream worker, finalize,
/// and fall back to batch transcription when streaming isn't available.
///
/// transcribe-cpp 0.1.3 never advertises streaming for the whisper
/// architecture, so with whisper models the fallback branch IS the production
/// path — this test pins the handshake (finalize must return None promptly,
/// never hang) and the fallback transcript. If a streaming-capable model is
/// ever supplied, the streaming branch assertions run instead.
#[test]
fn streaming_session_finalizes_or_falls_back_to_batch() {
    use pie_engine::stt::{StreamRouter, SttEngine};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    let Some(model) = model_path() else {
        eprintln!("skipping: no whisper model (set PIE_WHISPER_MODEL)");
        return;
    };

    let engine = pie_engine::stt::WhisperEngine::load(&model, "en").expect("model load failed");
    let supports_streaming = engine.supports_streaming();

    let dir = std::env::temp_dir().join("pie-voice-test");
    std::fs::create_dir_all(&dir).unwrap();
    let aiff = dir.join("stream.aiff");
    let wav = dir.join("stream.wav");

    let Ok(status) = Command::new("say")
        .arg("-o")
        .arg(&aiff)
        .arg("build a docker container for my rust project")
        .status()
    else {
        eprintln!("skipping: `say` unavailable");
        return;
    };
    assert!(status.success(), "say failed");
    let status = Command::new("afconvert")
        .args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"])
        .arg(&aiff)
        .arg(&wav)
        .status()
        .expect("afconvert unavailable");
    assert!(status.success(), "afconvert failed");

    let samples = pie_engine::stt::load_wav_as_16k_mono(&wav).expect("wav load failed");

    let router = Arc::new(StreamRouter::new());
    let rx = router.open();
    let partials = Arc::new(AtomicUsize::new(0));

    let (streamed, finalize_elapsed) = std::thread::scope(|scope| {
        let worker = {
            let partials = Arc::clone(&partials);
            let engine = &engine;
            scope.spawn(move || {
                engine.run_stream(rx, |_committed, _tentative| {
                    partials.fetch_add(1, Ordering::Relaxed);
                })
            })
        };

        // Feed in 30ms frames like the recorder's audio callback does
        for frame in samples.chunks(480) {
            router.feed(frame);
        }
        let started = Instant::now();
        let text = router.finalize().expect("finalize failed");
        let elapsed = started.elapsed();
        worker
            .join()
            .expect("stream worker panicked")
            .expect("stream worker errored");
        (text, elapsed)
    });

    let transcript = match streamed {
        Some(text) => {
            assert!(supports_streaming, "streamed without advertised support?");
            assert!(
                partials.load(Ordering::Relaxed) > 0,
                "expected partial updates while streaming"
            );
            text
        }
        None => {
            // Fallback handshake: finalize must reply promptly, not hang
            // until the 30s timeout, and batch transcription takes over —
            // exactly what the CLI voice session does.
            assert!(
                finalize_elapsed.as_secs() < 5,
                "finalize handshake took {finalize_elapsed:?}; drain_until_finalize is broken"
            );
            engine.transcribe(&samples).expect("batch fallback failed")
        }
    };

    let lower = transcript.to_lowercase();
    for word in ["docker", "rust", "project"] {
        assert!(
            lower.contains(word),
            "transcript missing '{word}': {transcript:?}"
        );
    }
}
