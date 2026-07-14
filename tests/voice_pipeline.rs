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
