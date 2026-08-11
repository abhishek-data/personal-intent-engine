//! End-to-end pipeline verification (Phase 6): text in → corrected transcript,
//! intent, mode selection, optimized prompt out. Uses the ephemeral engine so
//! no real user config/memory files are touched, and no network (LLM
//! unavailable in tests → extraction falls back to rules inside Enhanced mode).

use pie_engine::{OptimizationMode, PieEngine};
use std::sync::atomic::{AtomicU64, Ordering};

static IT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_pron_path() -> std::path::PathBuf {
    let n = IT_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("pie-e2e-pron-{}-{}.json", std::process::id(), n))
}

#[tokio::test]
async fn test_simple_command() {
    let mut engine = PieEngine::new_ephemeral(temp_pron_path());
    let result = engine.process("create a new file", "auto").await.unwrap();
    assert!(!result.optimized_prompt.is_empty());
    assert_eq!(result.mode, OptimizationMode::Direct);
}

#[tokio::test]
async fn test_technical_correction() {
    let mut engine = PieEngine::new_ephemeral(temp_pron_path());
    let result = engine
        .process("set up a next jazz app", "auto")
        .await
        .unwrap();
    assert!(
        result.corrected_transcript.contains("Next.js"),
        "got: {}",
        result.corrected_transcript
    );
}

#[tokio::test]
async fn test_rambling_speech_selects_enhanced() {
    let mut engine = PieEngine::new_ephemeral(temp_pron_path());
    let input = "so I was thinking about this thing right, like, I want to build something that listens to me and figures out what I mean, you know, like, not just dictation but actually understanding";
    let result = engine.process(input, "auto").await.unwrap();
    // Long input → Enhanced mode; without an LLM configured the rule-based
    // fallback still produces a non-empty objective.
    assert_eq!(result.mode, OptimizationMode::Enhanced);
    assert!(!result.intent.objective.is_empty());
    assert!(!result.optimized_prompt.is_empty());
}
