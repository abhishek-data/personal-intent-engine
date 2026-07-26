//! Pronunciation corrector: fixes speech-to-text mangling of technical terms.
//!
//! Two always-on deterministic tiers (exact phrase, then context-gated
//! phonetic) plus an opt-in LLM deep pass (see `llm_correct`). Runs at the top
//! of `PieEngine::process`, so both the desktop app and `process_audio` share
//! one correction path.

use std::collections::HashSet;
use std::path::PathBuf;

pub mod dictionary;
pub mod learned;
pub mod llm_correct;
pub mod phonetic;
pub mod static_seed;

pub use dictionary::{Correction, CorrectionDict, Source};
use learned::{LearnedSource, LearnedStore};

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
    learned: LearnedStore,
    learned_path: Option<PathBuf>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct UserEntry {
    heard: String,
    canonical: String,
}

impl PronunciationCorrector {
    /// Build from the embedded seed + the user dict at the default path.
    pub fn new() -> Self {
        Self::with_paths(default_user_path(), default_learned_path())
    }

    /// Build from the user dict at `path`, with learned vocab at the default
    /// learned path (test seam preserving the old single-path signature).
    pub fn with_user_path(path: PathBuf) -> Self {
        Self::with_paths(path, default_learned_path())
    }

    /// Build with explicit user + learned paths (full test seam).
    pub fn with_paths(user_path: PathBuf, learned_path: PathBuf) -> Self {
        let user = load_user_dict(&user_path);
        let learned = LearnedStore::load(learned_path.clone());
        let mut c = Self {
            dict: CorrectionDict::from_entries(Vec::new()),
            user,
            user_path: Some(user_path),
            learned,
            learned_path: Some(learned_path),
        };
        c.rebuild();
        c
    }

    /// Recompile the combined dictionary. Precedence, highest first:
    /// User -> Synced -> AutoLearned -> Static. A heard key seen at a higher
    /// tier suppresses the same key at every lower tier.
    fn rebuild(&mut self) {
        let mut entries: Vec<Correction> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Tier 1: user.
        for e in &self.user {
            if seen.insert(e.heard.clone()) {
                entries.push(e.clone());
            }
        }
        // Tier 2/3: learned — synced first, then auto.
        for want in [LearnedSource::Sync, LearnedSource::Auto] {
            for le in self.learned.entries().iter().filter(|e| e.source == want) {
                let heard = le.heard.to_lowercase();
                if seen.insert(heard.clone()) {
                    entries.push(Correction {
                        heard,
                        canonical: le.canonical.clone(),
                        source: match want {
                            LearnedSource::Sync => Source::Synced,
                            LearnedSource::Auto => Source::AutoLearned,
                        },
                    });
                }
            }
        }
        // Tier 4: static seed.
        for e in static_seed::load() {
            if seen.insert(e.heard.clone()) {
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
        let mut candidate: Vec<Correction> = self
            .user
            .iter()
            .filter(|e| e.heard != heard)
            .cloned()
            .collect();
        candidate.push(Correction {
            heard,
            canonical,
            source: Source::User,
        });
        Self::persist_entries(&self.user_path, &candidate)?;
        self.user = candidate;
        self.rebuild();
        Ok(())
    }

    pub fn remove_user_correction(&mut self, heard: &str) -> anyhow::Result<()> {
        let heard = heard.trim().to_lowercase();
        let candidate: Vec<Correction> = self
            .user
            .iter()
            .filter(|e| e.heard != heard)
            .cloned()
            .collect();
        Self::persist_entries(&self.user_path, &candidate)?;
        self.user = candidate;
        self.rebuild();
        Ok(())
    }

    /// Add or reinforce an auto-learned correction, then recompile.
    pub fn add_auto_correction(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()> {
        self.learned
            .add_or_reinforce(heard, canonical, LearnedSource::Auto)?;
        self.rebuild();
        Ok(())
    }

    /// Add or reinforce a synced correction (Phase 3), then recompile.
    pub fn add_synced_correction(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()> {
        self.learned
            .add_or_reinforce(heard, canonical, LearnedSource::Sync)?;
        self.rebuild();
        Ok(())
    }

    /// Reinforce an existing learned entry (bumps confidence/seen_count).
    /// Returns whether a learned entry matched. No rebuild needed (mappings
    /// are unchanged; only metadata moves).
    pub fn reinforce_learned(&mut self, heard: &str) -> anyhow::Result<bool> {
        self.learned.reinforce(heard)
    }

    /// Check if a learned entry with the given heard key exists.
    pub fn has_learned(&self, heard: &str) -> bool {
        self.learned.has_entry(heard)
    }

    /// Number of learned entries (auto + synced).
    pub fn learned_count(&self) -> usize {
        self.learned.count()
    }

    /// The `seen_count` of the matching learned entry, if any.
    pub fn learned_entries_seen(&self, heard: &str) -> Option<u32> {
        let key = heard.trim().to_lowercase();
        self.learned
            .entries()
            .iter()
            .find(|e| e.heard == key)
            .map(|e| e.seen_count)
    }

    /// Clear learned/synced vocab (never touches the user dict), then recompile.
    pub fn reset_learned(&mut self) -> anyhow::Result<()> {
        self.learned.reset()?;
        self.rebuild();
        Ok(())
    }

    /// Reload the learned store from disk and recompile. Returns whether the
    /// entry count changed (used by the engine to reload after the background
    /// learner appends). Cheap no-op when nothing changed on disk.
    pub fn reload_learned(&mut self) -> bool {
        let before = self.learned.count();
        if let Some(path) = self.learned_path() {
            self.learned = LearnedStore::load(path);
            self.rebuild();
        }
        self.learned.count() != before
    }

    fn learned_path(&self) -> Option<PathBuf> {
        self.learned_path.clone()
    }

    fn persist_entries(path: &Option<PathBuf>, user: &[Correction]) -> anyhow::Result<()> {
        if let Some(path) = path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let entries: Vec<UserEntry> = user
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

/// Default on-disk location for the learned/synced vocabulary store.
fn default_learned_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pie")
        .join("learned_vocab.json")
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_id() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("pie-pron-{}.json", unique_id()))
    }

    fn temp_learned_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("pie-learned-{}.json", unique_id()))
    }

    #[test]
    fn auto_learned_entry_corrects_and_is_counted() {
        let c_path = temp_path();
        let l_path = temp_learned_path();
        let mut c = PronunciationCorrector::with_paths(c_path.clone(), l_path.clone());
        c.add_auto_correction("terra form", "Terraform").unwrap();
        assert_eq!(c.learned_count(), 1);
        let out = c.correct("i use terra form daily", &std::collections::HashSet::new());
        assert_eq!(out.text, "i use Terraform daily");
        let _ = std::fs::remove_file(c_path);
        let _ = std::fs::remove_file(l_path);
    }

    #[test]
    fn user_entry_overrides_learned_same_heard() {
        let c_path = temp_path();
        let l_path = temp_learned_path();
        let mut c = PronunciationCorrector::with_paths(c_path.clone(), l_path.clone());
        c.add_auto_correction("react", "React").unwrap();
        c.add_user_correction("react", "ReactJS").unwrap();
        let out = c.correct("i love react", &std::collections::HashSet::new());
        assert_eq!(
            out.text, "i love ReactJS",
            "user tier must win over learned"
        );
        let _ = std::fs::remove_file(c_path);
        let _ = std::fs::remove_file(l_path);
    }

    #[test]
    fn reset_learned_keeps_user_entries() {
        let c_path = temp_path();
        let l_path = temp_learned_path();
        let mut c = PronunciationCorrector::with_paths(c_path.clone(), l_path.clone());
        c.add_user_correction("svelte", "Svelte").unwrap();
        c.add_auto_correction("terra form", "Terraform").unwrap();
        c.reset_learned().unwrap();
        assert_eq!(c.learned_count(), 0);
        assert_eq!(
            c.user_corrections().len(),
            1,
            "user dict must survive reset"
        );
        let _ = std::fs::remove_file(c_path);
        let _ = std::fs::remove_file(l_path);
    }

    #[test]
    fn correct_applies_static_exact_then_returns_outcome() {
        let c = PronunciationCorrector::with_paths(temp_path(), temp_learned_path());
        let out = c.correct("build a next jazz app", &HashSet::new());
        assert_eq!(out.text, "build a Next.js app");
    }

    #[test]
    fn user_entry_overrides_static_same_heard() {
        let path = temp_path();
        let mut c = PronunciationCorrector::with_paths(path.clone(), temp_learned_path());
        c.add_user_correction("kubernetes", "K8s").unwrap();
        let out = c.correct("i love kubernetes", &HashSet::new());
        assert_eq!(out.text, "i love K8s");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn user_dict_roundtrips_through_disk() {
        let path = temp_path();
        let l_path = temp_learned_path();
        {
            let mut c = PronunciationCorrector::with_paths(path.clone(), l_path.clone());
            c.add_user_correction("react", "React").unwrap();
        }
        let c2 = PronunciationCorrector::with_paths(path.clone(), l_path.clone());
        assert!(c2
            .user_corrections()
            .iter()
            .any(|e| e.heard == "react" && e.canonical == "React"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn remove_user_correction_deletes_it() {
        let path = temp_path();
        let mut c = PronunciationCorrector::with_paths(path.clone(), temp_learned_path());
        c.add_user_correction("svelte", "Svelte").unwrap();
        c.remove_user_correction("svelte").unwrap();
        assert!(c.user_corrections().is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_persist_leaves_state_consistent() {
        // Make the parent path a regular file so create_dir_all fails.
        let mut file = std::env::temp_dir();
        file.push(format!("pie-notadir-{}", unique_id()));
        std::fs::write(&file, b"x").unwrap();
        let bad_path = file.join("pronunciation.json"); // parent is a file
        let mut c = PronunciationCorrector::with_paths(bad_path, temp_learned_path());
        let res = c.add_user_correction("kubernetes", "K8s");
        assert!(res.is_err(), "persist to a bad path must error");
        // In-memory state must not have drifted.
        assert!(c.user_corrections().is_empty());
        assert_eq!(
            c.correct("i love kubernetes", &HashSet::new()).text,
            "i love Kubernetes"
        );
        let _ = std::fs::remove_file(file);
    }
}
