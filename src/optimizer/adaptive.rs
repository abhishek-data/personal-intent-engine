use crate::intent::{Intent, IntentConfidence};
use crate::memory::store::MemoryStore;
use super::{OptimizationMode, OptimizedPrompt};
use super::{compact, balanced, enhanced};

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
fn select_mode(intent: &Intent, memory: &MemoryStore) -> OptimizationMode {
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
