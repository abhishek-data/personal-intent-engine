//! Refine mode: compress long, rambling voice input into a sharp prompt.
//! Pure and LLM-free — for long inputs it returns a `RefineRequest` the engine
//! runs against the configured LLM; short inputs delegate to balanced mode.

use super::{balanced, OptimizationMode, OptimizedPrompt};
use crate::intent::Intent;
use crate::memory::store::MemoryStore;

/// Inputs longer than this many words are refined; shorter ones pass through.
pub const REFINE_WORD_THRESHOLD: usize = 80;

/// The LLM instruction to compress a long input.
#[derive(Debug)]
pub struct RefineRequest {
    pub prompt: String,
}

/// Outcome of refine-mode optimization.
pub enum RefineResult {
    /// Short input — a ready balanced prompt, no LLM needed.
    Balanced(OptimizedPrompt),
    /// Long input — a deterministic fallback plus an LLM instruction.
    Refine {
        base: OptimizedPrompt,
        request: RefineRequest,
    },
}

/// Decide whether to refine. Counts words in `intent.raw_input`.
#[must_use]
pub fn optimize(intent: &Intent, memory: &MemoryStore) -> RefineResult {
    let word_count = intent.raw_input.split_whitespace().count();
    if word_count <= REFINE_WORD_THRESHOLD {
        return RefineResult::Balanced(balanced::optimize(intent, memory));
    }

    let role = memory.profile.role.as_deref().unwrap_or("developer");
    let tech = memory.profile.technologies.join(", ");
    let prompt = format!(
        "The user spoke this long voice request. Rewrite it as ONE clear, concise \
         prompt. Keep ALL technical terms, names, and constraints. Remove filler \
         (um, like, you know, so, basically) and deduplicate repeated ideas. \
         Output ONLY the refined prompt, nothing else.\n\n\
         User context: role={role}, tech={tech}.\n\n\
         User said:\n{text}",
        role = role,
        tech = tech,
        text = intent.raw_input,
    );

    let base = OptimizedPrompt {
        text: intent.raw_input.clone(),
        mode: OptimizationMode::Refine,
        estimated_tokens: intent.raw_input.len() / 4,
        sections: Vec::new(),
    };
    RefineResult::Refine {
        base,
        request: RefineRequest { prompt },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::IntentExtractor;
    use crate::memory::store::MemoryStore;

    fn intent_for(text: &str) -> crate::intent::Intent {
        IntentExtractor::new().extract(text)
    }

    #[test]
    fn short_input_delegates_to_balanced() {
        let intent = intent_for("build a rust cli that parses json");
        let mem = MemoryStore::default();
        match optimize(&intent, &mem) {
            RefineResult::Balanced(p) => assert_eq!(p.mode, OptimizationMode::Balanced),
            RefineResult::Refine { .. } => panic!("short input must not refine"),
        }
    }

    #[test]
    fn long_input_yields_refine_request_with_original_fallback() {
        // > 80 words
        let long = "so ".to_string() + &"word ".repeat(90);
        let intent = intent_for(&long);
        let mem = MemoryStore::default();
        match optimize(&intent, &mem) {
            RefineResult::Refine { base, request } => {
                assert_eq!(base.mode, OptimizationMode::Refine);
                assert_eq!(
                    base.text, intent.raw_input,
                    "fallback base is the original text"
                );
                assert!(
                    request.prompt.contains(&intent.raw_input),
                    "prompt includes the input to compress"
                );
                assert!(!request.prompt.is_empty());
            }
            RefineResult::Balanced(_) => panic!("long input must refine"),
        }
    }
}
