//! The opt-in LLM deep-correct pass: builds a correction-only meta-prompt and
//! extracts what changed. Off the always-on path; reuses the configured LLM.

use super::{AppliedFix, Tier};

/// Build a correction-only meta-prompt. Scoped to fixing garbled technical
/// terms — explicitly NOT a rewrite.
pub fn build_prompt(transcript: &str, role: Option<&str>, tech: &[String]) -> String {
    format!(
        "You fix speech-to-text errors in technical dictation. Correct only \
         words that are garbled versions of technical terms. Preserve meaning, \
         wording, and structure — do not rephrase, summarize, or add anything. \
         Return only the corrected text, nothing else.\n\
         User context: role={role}, tech={tech}.\n\n\
         Text:\n{transcript}",
        role = role.unwrap_or("unknown"),
        tech = if tech.is_empty() {
            "unknown".to_string()
        } else {
            tech.join(", ")
        },
        transcript = transcript,
    )
}

/// Positional word-level diff: words that differ become `Llm` fixes. Simple by
/// design — the deep pass is expected to preserve structure.
pub fn diff_fixes(before: &str, after: &str) -> Vec<AppliedFix> {
    let b: Vec<&str> = before.split_whitespace().collect();
    let a: Vec<&str> = after.split_whitespace().collect();
    let mut fixes = Vec::new();
    for i in 0..b.len().min(a.len()) {
        if b[i] != a[i] {
            fixes.push(AppliedFix {
                from: b[i].to_string(),
                to: a[i].to_string(),
                tier: Tier::Llm,
            });
        }
    }
    fixes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_includes_transcript_and_context() {
        let p = build_prompt(
            "deploy to coobernetes",
            Some("backend dev"),
            &["Rust".into()],
        );
        assert!(p.contains("coobernetes"));
        assert!(p.contains("backend dev"));
        assert!(p.contains("Rust"));
        // Correction-only instruction, not a rewrite.
        assert!(p.to_lowercase().contains("only change") || p.to_lowercase().contains("preserve"));
    }

    #[test]
    fn diff_records_changed_words_as_llm_fixes() {
        let fixes = diff_fixes("deploy to coobernetes now", "deploy to Kubernetes now");
        assert_eq!(
            fixes,
            vec![AppliedFix {
                from: "coobernetes".into(),
                to: "Kubernetes".into(),
                tier: Tier::Llm,
            }]
        );
    }

    #[test]
    fn diff_of_identical_text_is_empty() {
        assert!(diff_fixes("same text here", "same text here").is_empty());
    }
}
