//! The corrector must sit inside the real pipeline: a garbled term in the input
//! becomes its canonical form in the optimized prompt.

use pie_engine::PieEngine;
use std::sync::atomic::{AtomicU64, Ordering};

static IT_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_pron_path() -> std::path::PathBuf {
    let n = IT_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("pie-it-pron-{}-{}.json", std::process::id(), n))
}

#[tokio::test]
async fn corrector_fixes_jargon_in_the_optimized_prompt() {
    let mut engine = PieEngine::new_ephemeral(temp_pron_path());
    let result = engine
        .process("build a next jazz app", "balanced")
        .await
        .expect("process");
    assert!(
        result.optimized_prompt.contains("Next.js"),
        "expected corrected term in prompt, got: {}",
        result.optimized_prompt
    );
    assert_eq!(result.corrected_transcript, "build a Next.js app");
    assert!(result.applied.iter().any(|f| f.to == "Next.js"));
}

#[tokio::test]
async fn clean_input_is_unchanged_by_the_corrector() {
    let mut engine = PieEngine::new_ephemeral(temp_pron_path());
    let result = engine
        .process("please summarize this document", "balanced")
        .await
        .expect("process");
    assert_eq!(
        result.corrected_transcript,
        "please summarize this document"
    );
    assert!(result.applied.is_empty());
}

#[tokio::test]
async fn deep_correct_runs_through_the_echo_provider() {
    let engine = PieEngine::new_ephemeral(temp_pron_path());
    let out = engine
        .deep_correct("deploy to coobernetes", "echo", None)
        .await
        .expect("deep");
    // echo returns the full prompt (prefixed with "[PIE Echo]\n"), which includes
    // the input transcript, so we can assert the call path works and the text
    // contains the embedded input.
    assert!(out.text.contains("coobernetes"));
}
