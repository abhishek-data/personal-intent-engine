//! The corrector must sit inside the real pipeline: a garbled term in the input
//! becomes its canonical form in the optimized prompt.

use pie_engine::PieEngine;

#[tokio::test]
async fn corrector_fixes_jargon_in_the_optimized_prompt() {
    let mut engine = PieEngine::new().await.expect("engine");
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
    let mut engine = PieEngine::new().await.expect("engine");
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
