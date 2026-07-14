use super::{OptimizationMode, OptimizedPrompt, PromptSection};
use crate::intent::Intent;
use crate::memory::store::MemoryStore;

/// Enhanced mode: Maximize reasoning quality.
/// Enriches the prompt with missing context, structured sections, and assumptions.
pub fn optimize(intent: &Intent, memory: &MemoryStore) -> OptimizedPrompt {
    let mut sections = Vec::new();

    // Full user context
    sections.push(PromptSection {
        label: "User Context".to_string(),
        content: format!(
            "Role: {}\nTechnologies: {}\nPreferred style: {:?}\nDetail level: {:?}",
            memory.profile.role.as_deref().unwrap_or("Not specified"),
            memory.profile.technologies.join(", "),
            memory.profile.preferred_style,
            memory.profile.detail_level,
        ),
    });

    // Objective with full context
    sections.push(PromptSection {
        label: "Objective".to_string(),
        content: intent.objective.clone(),
    });

    // Full context
    if !intent.context.is_empty() {
        sections.push(PromptSection {
            label: "Background Context".to_string(),
            content: intent.context.join("\n"),
        });
    }

    // Constraints as structured list
    if !intent.constraints.is_empty() {
        sections.push(PromptSection {
            label: "Constraints & Requirements".to_string(),
            content: intent
                .constraints
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{}. {}", i + 1, c))
                .collect::<Vec<_>>()
                .join("\n"),
        });
    }

    // Questions
    if !intent.questions.is_empty() {
        sections.push(PromptSection {
            label: "Specific Questions".to_string(),
            content: intent
                .questions
                .iter()
                .enumerate()
                .map(|(i, q)| format!("{}. {}", i + 1, q))
                .collect::<Vec<_>>()
                .join("\n"),
        });
    }

    // Topics for domain context
    if !intent.topics.is_empty() {
        sections.push(PromptSection {
            label: "Relevant Technologies".to_string(),
            content: intent.topics.join(", "),
        });
    }

    // Confidence note
    sections.push(PromptSection {
        label: "Confidence".to_string(),
        content: format!(
            "{:?} — {}",
            intent.confidence,
            match intent.confidence {
                crate::intent::IntentConfidence::High => "Clear request, proceed directly",
                crate::intent::IntentConfidence::Medium =>
                    "Some ambiguity, consider asking if unclear",
                crate::intent::IntentConfidence::Low =>
                    "Uncertain input, clarify before proceeding",
            }
        ),
    });

    // Expected output format
    sections.push(PromptSection {
        label: "Expected Output".to_string(),
        content: format!(
            "Respond in {:?} style with {:?} detail level.",
            memory.profile.preferred_style, memory.profile.detail_level,
        ),
    });

    let text = sections
        .iter()
        .map(|s| format!("## {}\n{}", s.label, s.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    let estimated_tokens = text.len() / 4;

    OptimizedPrompt {
        text,
        mode: OptimizationMode::Enhanced,
        estimated_tokens,
        sections,
    }
}
