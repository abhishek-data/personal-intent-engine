//! Pronunciation corrector: fixes speech-to-text mangling of technical terms.
//!
//! Two always-on deterministic tiers (exact phrase, then context-gated
//! phonetic) plus an opt-in LLM deep pass (see `llm_correct`). Runs at the top
//! of `PieEngine::process`, so both the desktop app and `process_audio` share
//! one correction path.

pub mod dictionary;
pub mod phonetic;

pub use dictionary::{Correction, CorrectionDict, Source};

/// Which tier produced a fix — surfaced to the UI for transparency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tier {
    Exact,
    Phonetic,
    Llm,
}

/// A single applied correction, e.g. `next jazz` -> `Next.js`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedFix {
    pub from: String,
    pub to: String,
    pub tier: Tier,
}

/// Corrected text plus the list of what changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CorrectionOutcome {
    pub text: String,
    pub applied: Vec<AppliedFix>,
}
