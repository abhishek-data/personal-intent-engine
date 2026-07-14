use super::{OptimizationMode, OptimizedPrompt, PromptSection};
use crate::intent::Intent;
use crate::memory::store::MemoryStore;

/// Balanced mode: Keep relevant context, remove noise, organize prompts.
/// This is the default mode — good balance of tokens and quality.
pub fn optimize(intent: &Intent, memory: &MemoryStore) -> OptimizedPrompt {
    let mut sections = Vec::new();

    // User context
    if let Some(role) = &memory.profile.role {
        sections.push(PromptSection {
            label: "Context".to_string(),
            content: format!(
                "Role: {}. Technologies: {}",
                role,
                memory.profile.technologies.join(", ")
            ),
        });
    }

    // Objective
    sections.push(PromptSection {
        label: "Objective".to_string(),
        content: intent.objective.clone(),
    });

    // Supporting context from input
    if !intent.context.is_empty() {
        sections.push(PromptSection {
            label: "Background".to_string(),
            content: intent.context.join(". "),
        });
    }

    // Constraints
    if !intent.constraints.is_empty() {
        sections.push(PromptSection {
            label: "Constraints".to_string(),
            content: intent.constraints.join("\n"),
        });
    }

    // Questions
    if !intent.questions.is_empty() {
        sections.push(PromptSection {
            label: "Questions".to_string(),
            content: intent.questions.join("\n"),
        });
    }

    // Topics
    if !intent.topics.is_empty() {
        sections.push(PromptSection {
            label: "Topics".to_string(),
            content: intent.topics.join(", "),
        });
    }

    // Response preference
    sections.push(PromptSection {
        label: "Preferred Style".to_string(),
        content: format!("{:?}", memory.profile.preferred_style),
    });

    let text = sections
        .iter()
        .map(|s| format!("## {}\n{}", s.label, s.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    let estimated_tokens = text.len() / 4;

    OptimizedPrompt {
        text,
        mode: OptimizationMode::Balanced,
        estimated_tokens,
        sections,
    }
}
