use super::{balanced, compact, enhanced};
use super::{OptimizationMode, OptimizedPrompt};
use crate::intent::{Intent, IntentConfidence};
use crate::memory::store::MemoryStore;

/// Adaptive mode: Automatically select the best optimization strategy.
pub fn optimize(intent: &Intent, memory: &MemoryStore) -> OptimizedPrompt {
    let mode = select_mode(intent, memory);
    match mode {
        OptimizationMode::Compact => compact::optimize(intent, memory),
        OptimizationMode::Balanced => balanced::optimize(intent, memory),
        OptimizationMode::Enhanced => enhanced::optimize(intent, memory),
        OptimizationMode::Adaptive => balanced::optimize(intent, memory), // fallback
    }
}

/// Select the best optimization mode based on context
fn select_mode(intent: &Intent, _memory: &MemoryStore) -> OptimizationMode {
    let word_count = intent.raw_input.split_whitespace().count();

    // Short, high-confidence inputs -> compact
    if intent.confidence == IntentConfidence::High && word_count < 15 {
        return OptimizationMode::Compact;
    }

    // Low confidence or complex input -> enhanced
    if intent.confidence == IntentConfidence::Low {
        return OptimizationMode::Enhanced;
    }

    // Many constraints/questions -> enhanced
    if intent.constraints.len() > 2 || intent.questions.len() > 1 {
        return OptimizationMode::Enhanced;
    }

    // Code tasks with good context -> balanced
    if intent.conversation_type == crate::intent::ConversationType::Code {
        return OptimizationMode::Balanced;
    }

    // Default
    OptimizationMode::Balanced
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::ConversationType;

    fn intent(raw: &str, confidence: IntentConfidence) -> Intent {
        Intent {
            raw_input: raw.to_string(),
            objective: raw.to_string(),
            confidence,
            ..Intent::default()
        }
    }

    #[test]
    fn short_high_confidence_selects_compact() {
        let out = optimize(
            &intent("deploy the app", IntentConfidence::High),
            &MemoryStore::default(),
        );
        assert_eq!(out.mode, OptimizationMode::Compact);
    }

    #[test]
    fn low_confidence_selects_enhanced() {
        let out = optimize(
            &intent("maybe fix the thing somehow", IntentConfidence::Low),
            &MemoryStore::default(),
        );
        assert_eq!(out.mode, OptimizationMode::Enhanced);
    }

    #[test]
    fn many_constraints_select_enhanced() {
        let mut i = intent(
            "build the service with several requirements attached to it and more words to avoid compact",
            IntentConfidence::Medium,
        );
        i.constraints = vec!["a".into(), "b".into(), "c".into()];
        let out = optimize(&i, &MemoryStore::default());
        assert_eq!(out.mode, OptimizationMode::Enhanced);
    }

    #[test]
    fn code_requests_select_balanced() {
        let mut i = intent(
            "implement the parser module for the configuration file format we discussed",
            IntentConfidence::Medium,
        );
        i.conversation_type = ConversationType::Code;
        let out = optimize(&i, &MemoryStore::default());
        assert_eq!(out.mode, OptimizationMode::Balanced);
    }

    #[test]
    fn default_is_balanced() {
        let out = optimize(
            &intent(
                "write up a summary of the meeting notes from yesterday afternoon session",
                IntentConfidence::Medium,
            ),
            &MemoryStore::default(),
        );
        assert_eq!(out.mode, OptimizationMode::Balanced);
    }
}
