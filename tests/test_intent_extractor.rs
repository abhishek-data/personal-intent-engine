//! Phase 1 audit: run the current rule-based extractor against the corpus.
//!
//! Simple commands are asserted (they should work). Rambling-speech cases are
//! printed, not asserted — they document the failure mode that motivates the
//! LLM-backed extractor. Results: docs/INTENT_EXTRACTION_TEST_RESULTS.md.

use pie_engine::intent::IntentExtractor;
use serde_json::Value;
use std::fs;

#[test]
fn test_current_extractor_against_corpus() {
    let corpus: Value =
        serde_json::from_str(&fs::read_to_string("tests/fixtures/intent_corpus.json").unwrap())
            .unwrap();

    let extractor = IntentExtractor::new();

    // Simple commands: these should work reasonably well.
    for case in corpus["simple_commands"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let result = extractor.extract(input);

        println!("Input: {}", input);
        println!("Extracted objective: {:?}", result.objective);
        println!("Conversation type: {:?}", result.conversation_type);
        println!("Confidence: {:?}", result.confidence);
        println!("---");

        assert!(!result.objective.is_empty(), "Failed for: {}", input);
    }

    // Rambling speech: document the failure, don't assert.
    println!("\n=== RAMBLING SPEECH TESTS (expect poor results) ===");
    for case in corpus["rambling_speech"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let result = extractor.extract(input);

        println!("Input: {}", input);
        println!("Extracted objective: {:?}", result.objective);
        println!(
            "Expected intent: {}",
            case["expected_intent"].as_str().unwrap()
        );
        println!("Confidence: {:?}", result.confidence);
        println!("Topics: {:?}", result.topics);
        println!("---");
    }
}
