//! Prompt optimization: turn an extracted [`Intent`] into the text sent to an
//! LLM (or pasted at the cursor).
//!
//! Two modes only. `Direct` passes the objective through for short, clear
//! commands. `Enhanced` structures the (LLM-extracted) intent — objective,
//! constraints, questions — into a clear prompt for complex/rambling input.

use serde::{Deserialize, Serialize};

use crate::intent::Intent;

/// Optimization modes for prompt generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationMode {
    /// Direct mode: minimal processing for simple commands.
    /// Used when intent is clear and input is short.
    Direct,

    /// Enhanced mode: structures LLM-extracted intent for complex/rambling
    /// input. Used when input is long, unclear, or needs restructuring.
    Enhanced,
}

/// Optimize intent into a prompt based on mode.
#[must_use]
pub fn optimize(intent: &Intent, mode: OptimizationMode) -> String {
    match mode {
        OptimizationMode::Direct => direct(intent),
        OptimizationMode::Enhanced => enhanced(intent),
    }
}

/// Direct mode: pass the user's (corrected) words through unchanged.
///
/// Deliberately uses `raw_input` rather than `objective`. `objective` is
/// truncated at the first sentence boundary, so returning it silently dropped
/// everything after the first sentence — "deploy the app. do not restart the
/// database." optimized to just "deploy the app", losing the constraint. For
/// short, already-clear input the safest prompt is what the user actually
/// said. Falls back to `objective` only if `raw_input` is unset.
fn direct(intent: &Intent) -> String {
    if intent.raw_input.trim().is_empty() {
        return intent.objective.clone();
    }
    intent.raw_input.clone()
}

/// Enhanced mode: structure the intent into a clear prompt.
/// For complex input where we need to organize the extracted intent.
fn enhanced(intent: &Intent) -> String {
    let mut prompt = String::new();

    prompt.push_str(&intent.objective);

    if !intent.constraints.is_empty() {
        prompt.push_str("\n\nConstraints:");
        for c in &intent.constraints {
            prompt.push_str(&format!("\n- {c}"));
        }
    }

    if !intent.questions.is_empty() {
        prompt.push_str("\n\nQuestions:");
        for q in &intent.questions {
            prompt.push_str(&format!("\n- {q}"));
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(objective: &str) -> Intent {
        Intent {
            objective: objective.to_string(),
            raw_input: objective.to_string(),
            ..Intent::default()
        }
    }

    #[test]
    fn direct_passes_objective_through() {
        let out = optimize(&intent("deploy the app"), OptimizationMode::Direct);
        assert_eq!(out, "deploy the app");
    }

    #[test]
    fn direct_does_not_drop_content_after_the_first_sentence() {
        // Regression: Direct mode returned `objective`, which is truncated at
        // the first sentence boundary — so a trailing instruction (here, a
        // "do not" constraint) vanished from the prompt entirely.
        let mut i = intent("deploy the app. do not restart the database.");
        i.objective = "deploy the app".to_string();
        i.constraints = vec!["do not restart the database".to_string()];
        let out = optimize(&i, OptimizationMode::Direct);
        assert!(
            out.contains("do not restart the database"),
            "Direct mode dropped the constraint: {out:?}"
        );
    }

    #[test]
    fn direct_falls_back_to_objective_when_raw_input_is_empty() {
        let mut i = intent("x");
        i.raw_input = String::new();
        i.objective = "recovered objective".to_string();
        assert_eq!(
            optimize(&i, OptimizationMode::Direct),
            "recovered objective"
        );
    }

    #[test]
    fn enhanced_keeps_trailing_constraints_too() {
        let mut i = intent("deploy the app. do not restart the database.");
        i.objective = "deploy the app".to_string();
        i.constraints = vec!["do not restart the database".to_string()];
        let out = optimize(&i, OptimizationMode::Enhanced);
        assert!(out.contains("do not restart the database"), "{out:?}");
    }

    #[test]
    fn enhanced_with_bare_objective_is_just_the_objective() {
        let out = optimize(&intent("build a parser"), OptimizationMode::Enhanced);
        assert_eq!(out, "build a parser");
    }

    #[test]
    fn enhanced_structures_constraints_and_questions() {
        let mut i = intent("build the API");
        i.constraints = vec!["must use Postgres".into(), "no ORM".into()];
        i.questions = vec!["which auth scheme?".into()];
        let out = optimize(&i, OptimizationMode::Enhanced);
        assert_eq!(
            out,
            "build the API\n\nConstraints:\n- must use Postgres\n- no ORM\n\nQuestions:\n- which auth scheme?"
        );
    }
}
