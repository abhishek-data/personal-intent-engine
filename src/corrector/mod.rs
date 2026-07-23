//! Pronunciation corrector: fixes speech-to-text mangling of technical terms.
//!
//! Two always-on deterministic tiers (exact phrase, then context-gated
//! phonetic) plus an opt-in LLM deep pass (see `llm_correct`). Runs at the top
//! of `PieEngine::process`, so both the desktop app and `process_audio` share
//! one correction path.

use std::collections::HashSet;
use std::path::PathBuf;

pub mod dictionary;
pub mod phonetic;
pub mod static_seed;

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

/// The full corrector: static seed + user dict, with an always-on deterministic
/// `correct` and mutation helpers for the user dict.
pub struct PronunciationCorrector {
    dict: CorrectionDict,
    user: Vec<Correction>,
    user_path: Option<PathBuf>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct UserEntry {
    heard: String,
    canonical: String,
}

impl PronunciationCorrector {
    /// Build from the embedded seed + the user dict at the default path.
    pub fn new() -> Self {
        Self::with_user_path(default_user_path())
    }

    /// Build from the embedded seed + the user dict at `path` (test seam).
    pub fn with_user_path(path: PathBuf) -> Self {
        let user = load_user_dict(&path);
        let mut c = Self {
            dict: CorrectionDict::from_entries(Vec::new()),
            user,
            user_path: Some(path),
        };
        c.rebuild();
        c
    }

    /// Recompile the combined dictionary. User entries come first so they
    /// override static entries with the same heard key.
    fn rebuild(&mut self) {
        let mut entries: Vec<Correction> = self.user.clone();
        let user_heards: HashSet<String> = self.user.iter().map(|e| e.heard.clone()).collect();
        for e in static_seed::load() {
            if !user_heards.contains(&e.heard) {
                entries.push(e);
            }
        }
        self.dict = CorrectionDict::from_entries(entries);
    }

    /// Always-on correction: exact phrase pass, then context-gated phonetic.
    /// User-dict canonicals are always allowed for phonetic; `extra_allowed`
    /// (lowercased) enables static-entry phonetic matches for terms the user
    /// is known to use.
    pub fn correct(&self, text: &str, extra_allowed: &HashSet<String>) -> CorrectionOutcome {
        let mut allowed = extra_allowed.clone();
        for e in &self.user {
            allowed.insert(e.canonical.to_lowercase());
        }
        let exact = self.dict.apply_exact(text);
        let phon = self.dict.apply_phonetic(&exact.text, &allowed);
        let mut applied = exact.applied;
        applied.extend(phon.applied);
        CorrectionOutcome {
            text: phon.text,
            applied,
        }
    }

    pub fn user_corrections(&self) -> Vec<Correction> {
        self.user.clone()
    }

    pub fn add_user_correction(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()> {
        let heard = heard.trim().to_lowercase();
        let canonical = canonical.trim().to_string();
        if heard.is_empty() || canonical.is_empty() {
            anyhow::bail!("heard and canonical must be non-empty");
        }
        self.user.retain(|e| e.heard != heard);
        self.user.push(Correction {
            heard,
            canonical,
            source: Source::User,
        });
        self.persist()?;
        self.rebuild();
        Ok(())
    }

    pub fn remove_user_correction(&mut self, heard: &str) -> anyhow::Result<()> {
        let heard = heard.trim().to_lowercase();
        self.user.retain(|e| e.heard != heard);
        self.persist()?;
        self.rebuild();
        Ok(())
    }

    fn persist(&self) -> anyhow::Result<()> {
        if let Some(path) = &self.user_path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let entries: Vec<UserEntry> = self
                .user
                .iter()
                .map(|e| UserEntry {
                    heard: e.heard.clone(),
                    canonical: e.canonical.clone(),
                })
                .collect();
            std::fs::write(path, serde_json::to_string_pretty(&entries)?)?;
        }
        Ok(())
    }
}

impl Default for PronunciationCorrector {
    fn default() -> Self {
        Self::new()
    }
}

fn default_user_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pie")
        .join("pronunciation.json")
}

fn load_user_dict(path: &std::path::Path) -> Vec<Correction> {
    let Ok(json) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<UserEntry>>(&json) {
        Ok(raw) => raw
            .into_iter()
            .map(|e| Correction {
                heard: e.heard.to_lowercase(),
                canonical: e.canonical,
                source: Source::User,
            })
            .collect(),
        Err(e) => {
            log::warn!("Failed to parse pronunciation.json: {e}; starting empty");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PronunciationCorrector;
    use std::collections::HashSet;

    fn temp_path() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "pie-pron-{}.json",
            std::process::id().wrapping_add(rand_suffix())
        ));
        p
    }

    // Cheap unique-ish suffix without adding a dep.
    fn rand_suffix() -> u32 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    }

    #[test]
    fn correct_applies_static_exact_then_returns_outcome() {
        let c = PronunciationCorrector::with_user_path(temp_path());
        let out = c.correct("build a next jazz app", &HashSet::new());
        assert_eq!(out.text, "build a Next.js app");
    }

    #[test]
    fn user_entry_overrides_static_same_heard() {
        let path = temp_path();
        let mut c = PronunciationCorrector::with_user_path(path.clone());
        c.add_user_correction("kubernetes", "K8s").unwrap();
        let out = c.correct("i love kubernetes", &HashSet::new());
        assert_eq!(out.text, "i love K8s");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn user_dict_roundtrips_through_disk() {
        let path = temp_path();
        {
            let mut c = PronunciationCorrector::with_user_path(path.clone());
            c.add_user_correction("react", "React").unwrap();
        }
        let c2 = PronunciationCorrector::with_user_path(path.clone());
        assert!(c2
            .user_corrections()
            .iter()
            .any(|e| e.heard == "react" && e.canonical == "React"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn remove_user_correction_deletes_it() {
        let path = temp_path();
        let mut c = PronunciationCorrector::with_user_path(path.clone());
        c.add_user_correction("svelte", "Svelte").unwrap();
        c.remove_user_correction("svelte").unwrap();
        assert!(c.user_corrections().is_empty());
        let _ = std::fs::remove_file(path);
    }
}
