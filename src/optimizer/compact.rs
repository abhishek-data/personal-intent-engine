use super::{OptimizationMode, OptimizedPrompt, PromptSection};
use crate::intent::Intent;
use crate::memory::store::MemoryStore;

/// Compact mode: Minimize token usage.
/// Removes filler words, compresses context, keeps only the essential intent.
pub fn optimize(intent: &Intent, memory: &MemoryStore) -> OptimizedPrompt {
    let mut sections = Vec::new();

    // Only include the core objective
    let objective = remove_filler(&intent.objective);
    sections.push(PromptSection {
        label: "Task".to_string(),
        content: objective.clone(),
    });

    // Include constraints only if they exist
    if !intent.constraints.is_empty() {
        let constraints: Vec<&str> = intent.constraints.iter().map(|s| s.as_str()).collect();
        sections.push(PromptSection {
            label: "Constraints".to_string(),
            content: constraints.join("; "),
        });
    }

    // Include user role if available
    if let Some(role) = &memory.profile.role {
        sections.push(PromptSection {
            label: "Role".to_string(),
            content: format!("I am a {}", role),
        });
    }

    let text = sections
        .iter()
        .map(|s| format!("{}: {}", s.label, s.content))
        .collect::<Vec<_>>()
        .join("\n");

    OptimizedPrompt {
        text,
        mode: OptimizationMode::Compact,
        estimated_tokens: estimate_tokens(&sections),
        sections,
    }
}

/// Remove common filler words/phrases
fn remove_filler(text: &str) -> String {
    let fillers = [
        "um",
        "uh",
        "like",
        "you know",
        "basically",
        "actually",
        "literally",
        "so yeah",
        "i mean",
        "well,",
        "ok so",
    ];
    let mut result = text.to_string();
    for filler in &fillers {
        result = result.replace(filler, "");
    }
    // Collapse multiple spaces
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }
    result.trim().to_string()
}

fn estimate_tokens(sections: &[PromptSection]) -> usize {
    let total_chars: usize = sections.iter().map(|s| s.content.len()).sum();
    total_chars / 4 // rough estimate: ~4 chars per token
}
