//! LLM-backed intent extraction, exercised through the LlmClient seam with a
//! mock adapter — no network. Covers: LLM path for rambling speech, the
//! rule-based fast path for short commands, and fallback on LLM failure.

mod mock_llm;

use mock_llm::MockLlmClient;
use pie_engine::intent::{ConversationType, IntentExtractor};

#[tokio::test]
async fn test_llm_extraction_rambling_speech() {
    let extractor = IntentExtractor::new();

    let mock_response = r#"{
        "objective": "build intent extraction system",
        "type": "task",
        "topics": ["voice assistant", "intent extraction", "NLP"],
        "constraints": [],
        "questions": [],
        "confidence": "medium"
    }"#;

    let mock_llm = MockLlmClient::new(vec![mock_response.to_string()]);

    let input = "so I was thinking about this thing right, like, I want to build something that listens to me and figures out what I mean, you know, like, not just dictation but actually understanding, and then it should, like, make a prompt or something";

    let result = extractor.extract_with_llm(input, &mock_llm, None).await;

    assert_eq!(result.objective, "build intent extraction system");
    assert_eq!(result.conversation_type, ConversationType::Task);
    assert!(result.topics.contains(&"voice assistant".to_string()));
    assert_eq!(result.raw_input, input);
}

#[tokio::test]
async fn test_llm_extraction_handles_fenced_json() {
    let extractor = IntentExtractor::new();

    // Real LLMs often wrap JSON in markdown fences; the parser must cope.
    let mock_response = "```json\n{\"objective\": \"deploy the service to production\", \"type\": \"task\", \"topics\": [], \"constraints\": [], \"questions\": [], \"confidence\": \"high\"}\n```";

    let mock_llm = MockLlmClient::new(vec![mock_response.to_string()]);

    let input = "okay so um what I really want here, after all that back and forth, is basically just to get the service deployed out to production you know";
    let result = extractor.extract_with_llm(input, &mock_llm, None).await;

    assert_eq!(result.objective, "deploy the service to production");
}

#[tokio::test]
async fn test_llm_extraction_simple_command_uses_rules() {
    let extractor = IntentExtractor::new();

    // Mock with no responses: any LLM call would error. The short-input fast
    // path must never reach it.
    let mock_llm = MockLlmClient::new(vec![]);

    let input = "create a file";
    let result = extractor.extract_with_llm(input, &mock_llm, None).await;

    // Should use rules (≤15 words, no question mark).
    assert!(!result.objective.is_empty());
}

#[tokio::test]
async fn test_llm_extraction_fallback_on_error() {
    let extractor = IntentExtractor::new();

    // Mock that returns an error (no responses configured).
    let mock_llm = MockLlmClient::new(vec![]);

    let input = "so I was thinking about building this really complex system that does a lot of things and I'm not sure where to start";
    let result = extractor.extract_with_llm(input, &mock_llm, None).await;

    // Should fall back to rule-based extraction.
    assert!(!result.objective.is_empty());
}

#[tokio::test]
async fn test_llm_extraction_fallback_on_garbage_response() {
    let extractor = IntentExtractor::new();

    let mock_llm = MockLlmClient::new(vec!["Sure! Happy to help.".to_string()]);

    let input = "so basically what I want to do is refactor the whole parser module because it has grown way too complicated over time";
    let result = extractor.extract_with_llm(input, &mock_llm, None).await;

    // Non-JSON response → rule-based fallback, never an empty intent.
    assert!(!result.objective.is_empty());
}
