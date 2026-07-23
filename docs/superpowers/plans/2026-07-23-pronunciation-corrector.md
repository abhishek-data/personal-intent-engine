# Pronunciation Corrector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a correction layer that fixes Whisper's mangling of developer jargon, using an always-on local dictionary (exact + context-gated phonetic) plus an opt-in LLM deep pass, learning from user-confirmed saves.

**Architecture:** A new `src/corrector/` module in the `pie-engine` lib exposes `PronunciationCorrector`. It runs synchronously at the top of `PieEngine::process()` — the single choke point the desktop app and `process_audio` both go through. A ships-with static seed (embedded JSON) plus a per-user `pronunciation.json` feed a two-tier matcher: exact longest-phrase replacement, then a context-gated phonetic tier that only fires toward terms the user cares about. The opt-in LLM pass reuses the existing `LlmRouter`, is off the always-on path, and is triggered only by a setting toggle or an on-demand command.

**Tech Stack:** Rust (pie-engine lib + pie-desktop Tauri bin), serde/serde_json, `dirs` for config paths, Svelte 5 UI. No new crate dependencies (phonetic key is hand-rolled).

## Global Constraints

- Rust edition 2021; format touched files only with `rustfmt --edition 2021 <file>` (project-wide `cargo fmt` causes churn — do not run it).
- No new crate dependencies. The phonetic key is implemented in-repo.
- `#[cfg(test)] mod tests` blocks go at the END of each file (clippy `items_after_test_module`).
- Config/persistence follow the existing pattern: `dirs::config_dir()/pie/<name>.json`, serde_json pretty, `create_dir_all` on save, fall back to defaults on read/parse failure.
- The always-on corrector path must be synchronous and I/O-free (it runs inline before every paste). Only the opt-in LLM pass may be async / touch the network.
- Static dictionary entries are read-only at runtime; only the user dict is mutable.
- Commit after each task with the message shown in its final step.
- Run `cargo test -p pie-engine` for lib tasks; expected baseline before this work: 60 lib tests pass.

---

### Task 1: Corrector core types + exact-match dictionary

**Files:**
- Create: `src/corrector/mod.rs`
- Create: `src/corrector/dictionary.rs`
- Modify: `src/lib.rs` (register the module)

**Interfaces:**
- Produces:
  - `pub enum Source { Static, User }`
  - `pub struct Correction { pub heard: String, pub canonical: String, pub source: Source }`
  - `pub enum Tier { Exact, Phonetic, Llm }`
  - `pub struct AppliedFix { pub from: String, pub to: String, pub tier: Tier }`
  - `pub struct CorrectionOutcome { pub text: String, pub applied: Vec<AppliedFix> }`
  - `pub struct CorrectionDict { … }` with `fn from_entries(Vec<Correction>) -> Self` and `fn apply_exact(&self, text: &str) -> CorrectionOutcome`

- [ ] **Step 1: Register the module**

In `src/lib.rs`, add alongside the other `pub mod` lines (e.g. near `pub mod intent;`):

```rust
pub mod corrector;
```

- [ ] **Step 2: Write `src/corrector/mod.rs` with the shared types**

```rust
//! Pronunciation corrector: fixes speech-to-text mangling of technical terms.
//!
//! Two always-on deterministic tiers (exact phrase, then context-gated
//! phonetic) plus an opt-in LLM deep pass (see `llm_correct`). Runs at the top
//! of `PieEngine::process`, so both the desktop app and `process_audio` share
//! one correction path.

pub mod dictionary;

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
```

- [ ] **Step 3: Write the failing test for exact matching**

Create `src/corrector/dictionary.rs` with only the tests first:

```rust
use super::{AppliedFix, CorrectionOutcome, Tier};

/// Origin of a correction entry; user entries override static ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Static,
    User,
}

/// One heard->canonical mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub heard: String,
    pub canonical: String,
    pub source: Source,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> CorrectionDict {
        CorrectionDict::from_entries(vec![
            Correction { heard: "next jazz".into(), canonical: "Next.js".into(), source: Source::Static },
            Correction { heard: "next".into(), canonical: "NEXT-SHOULD-NOT-FIRE".into(), source: Source::Static },
            Correction { heard: "coober net ease".into(), canonical: "Kubernetes".into(), source: Source::Static },
        ])
    }

    #[test]
    fn exact_multiword_replaces_and_records() {
        let out = dict().apply_exact("build a next jazz app");
        assert_eq!(out.text, "build a Next.js app");
        assert_eq!(out.applied, vec![AppliedFix {
            from: "next jazz".into(), to: "Next.js".into(), tier: Tier::Exact,
        }]);
    }

    #[test]
    fn longest_phrase_wins_over_shorter() {
        // "next jazz" must win; the bare "next" entry must not pre-empt it.
        let out = dict().apply_exact("next jazz");
        assert_eq!(out.text, "Next.js");
    }

    #[test]
    fn case_insensitive_match_preserves_canonical_casing() {
        let out = dict().apply_exact("Next Jazz rocks");
        assert_eq!(out.text, "Next.js rocks");
    }

    #[test]
    fn word_boundary_safety_no_substring_match() {
        // "nextel" is one token, normalizes to "nextel" != "next": no match.
        let out = dict().apply_exact("my nextel phone");
        assert_eq!(out.text, "my nextel phone");
        assert!(out.applied.is_empty());
    }

    #[test]
    fn trailing_punctuation_preserved_on_match() {
        let out = dict().apply_exact("i love next jazz!");
        assert_eq!(out.text, "i love Next.js!");
    }

    #[test]
    fn no_match_returns_input_unchanged() {
        let out = dict().apply_exact("hello world");
        assert_eq!(out.text, "hello world");
        assert!(out.applied.is_empty());
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p pie-engine corrector::dictionary`
Expected: FAIL — `CorrectionDict` not found.

- [ ] **Step 5: Implement `CorrectionDict::from_entries` + `apply_exact`**

Insert above the `#[cfg(test)]` block in `src/corrector/dictionary.rs`:

```rust
/// A compiled set of corrections with an exact longest-phrase matcher.
pub struct CorrectionDict {
    entries: Vec<Correction>,
    /// Entry indices sorted by descending heard-word-count (longest first),
    /// so multi-word phrases match before single words.
    by_len: Vec<usize>,
}

/// Lowercase and trim surrounding ASCII punctuation for matching only.
/// Returns (normalized, trailing_punctuation_of_token).
fn normalize(token: &str) -> (String, &str) {
    let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    let trailing = &token[trimmed.len() + (token.len() - token.trim_start_matches(|c: char| !c.is_ascii_alphanumeric()).len())..];
    (trimmed.to_lowercase(), trailing)
}

impl CorrectionDict {
    pub fn from_entries(entries: Vec<Correction>) -> Self {
        let mut by_len: Vec<usize> = (0..entries.len()).collect();
        by_len.sort_by_key(|&i| std::cmp::Reverse(entries[i].heard.split_whitespace().count()));
        Self { entries, by_len }
    }

    /// Replace exact (case-insensitive, whitespace-tokenized) phrase matches,
    /// longest phrase first. Trailing punctuation on the last matched token is
    /// preserved. Non-matching tokens are emitted verbatim.
    pub fn apply_exact(&self, text: &str) -> CorrectionOutcome {
        let tokens: Vec<&str> = text.split_whitespace().collect();
        let norm: Vec<String> = tokens.iter().map(|t| normalize(t).0).collect();

        let mut out_tokens: Vec<String> = Vec::with_capacity(tokens.len());
        let mut applied = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let mut matched = None;
            for &idx in &self.by_len {
                let phrase: Vec<&str> = self.entries[idx].heard.split_whitespace().collect();
                let n = phrase.len();
                if n == 0 || i + n > norm.len() {
                    continue;
                }
                if (0..n).all(|k| norm[i + k] == phrase[k]) {
                    matched = Some((idx, n));
                    break;
                }
            }
            if let Some((idx, n)) = matched {
                // Preserve trailing punctuation of the final matched token.
                let trailing = normalize(tokens[i + n - 1]).1;
                out_tokens.push(format!("{}{}", self.entries[idx].canonical, trailing));
                applied.push(AppliedFix {
                    from: (0..n).map(|k| norm[i + k].as_str()).collect::<Vec<_>>().join(" "),
                    to: self.entries[idx].canonical.clone(),
                    tier: Tier::Exact,
                });
                i += n;
            } else {
                out_tokens.push(tokens[i].to_string());
                i += 1;
            }
        }

        CorrectionOutcome { text: out_tokens.join(" "), applied }
    }
}
```

> Note: this normalizes whitespace to single spaces — acceptable for short dictation transcripts. The `normalize` trailing-punctuation slice is fiddly; if it proves awkward, simplify to: `let trailing: String = token.chars().rev().take_while(|c| !c.is_ascii_alphanumeric()).collect::<String>().chars().rev().collect();` and adjust the signature to return `String`. Keep the tests green either way.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p pie-engine corrector::dictionary`
Expected: PASS (6 tests).

- [ ] **Step 7: Format and commit**

```bash
rustfmt --edition 2021 src/corrector/mod.rs src/corrector/dictionary.rs src/lib.rs
git add src/corrector/mod.rs src/corrector/dictionary.rs src/lib.rs
git commit -m "feat(corrector): exact longest-phrase correction dictionary"
```

---

### Task 2: Phonetic key + context-gated phonetic tier

**Files:**
- Create: `src/corrector/phonetic.rs`
- Modify: `src/corrector/mod.rs` (register `pub mod phonetic;`)
- Modify: `src/corrector/dictionary.rs` (add `apply_phonetic`)

**Interfaces:**
- Consumes: `CorrectionDict`, `CorrectionOutcome`, `AppliedFix`, `Tier` (Task 1).
- Produces:
  - `pub fn phonetic_key(word: &str) -> String` in `phonetic.rs`
  - `CorrectionDict::apply_phonetic(&self, text: &str, allowed: &std::collections::HashSet<String>) -> CorrectionOutcome`

- [ ] **Step 1: Write the failing test for `phonetic_key`**

Create `src/corrector/phonetic.rs`:

```rust
//! A small, deterministic phonetic key for single-word fuzzy matching.
//!
//! Not Double Metaphone — it maps consonants to sound classes and drops vowels
//! so different spellings of the same term collide (e.g. "kubernetes" and
//! "coobernetes"). Multi-word garbles are handled by exact seed phrases, not
//! here.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelling_variants_of_same_term_collide() {
        assert_eq!(phonetic_key("kubernetes"), phonetic_key("coobernetes"));
        assert_eq!(phonetic_key("Next"), phonetic_key("next"));
    }

    #[test]
    fn distinct_terms_do_not_collide() {
        assert_ne!(phonetic_key("kubernetes"), phonetic_key("postgres"));
    }

    #[test]
    fn empty_or_symbol_input_is_empty_key() {
        assert_eq!(phonetic_key(""), "");
        assert_eq!(phonetic_key("123"), "");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p pie-engine corrector::phonetic`
Expected: FAIL — `phonetic_key` not found.

- [ ] **Step 3: Implement `phonetic_key`**

Insert above the test module in `src/corrector/phonetic.rs`:

```rust
/// Compute the phonetic key for a single word.
pub fn phonetic_key(word: &str) -> String {
    let lower: String = word.to_lowercase().chars().filter(|c| c.is_ascii_alphabetic()).collect();
    // Digraph normalization before per-char classing.
    let s = lower
        .replace("sch", "sk")
        .replace("tch", "ch")
        .replace("ph", "f")
        .replace("ck", "k")
        .replace("gh", "")
        .replace('x', "ks");

    let mut out = String::new();
    for c in s.chars() {
        let cls = match c {
            'a' | 'e' | 'i' | 'o' | 'u' => continue, // drop vowels
            'c' | 'k' | 'q' => 'k',
            's' | 'z' => 's',
            'f' | 'v' => 'f',
            'g' | 'j' => 'j',
            'd' | 't' => 't',
            'b' | 'p' => 'p',
            'm' | 'n' => 'n',
            other => other, // l, r, w, h, y
        };
        if out.chars().last() != Some(cls) {
            out.push(cls);
        }
    }
    out
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p pie-engine corrector::phonetic`
Expected: PASS (3 tests).

- [ ] **Step 5: Register the module**

In `src/corrector/mod.rs`, add under the existing `pub mod dictionary;`:

```rust
pub mod phonetic;
```

- [ ] **Step 6: Write the failing test for the gated phonetic tier**

Add these tests inside the `mod tests` block in `src/corrector/dictionary.rs` (before the closing brace):

```rust
    use std::collections::HashSet;

    fn phon_dict() -> CorrectionDict {
        CorrectionDict::from_entries(vec![
            Correction { heard: "kubernetes".into(), canonical: "Kubernetes".into(), source: Source::Static },
        ])
    }

    #[test]
    fn phonetic_fires_when_canonical_is_allowed() {
        let allowed: HashSet<String> = ["kubernetes".into()].into_iter().collect();
        let out = phon_dict().apply_phonetic("deploy to coobernetes today", &allowed);
        assert_eq!(out.text, "deploy to Kubernetes today");
        assert_eq!(out.applied.len(), 1);
        assert_eq!(out.applied[0].tier, Tier::Phonetic);
    }

    #[test]
    fn phonetic_does_not_fire_with_empty_allow_set() {
        // The core anti-over-correction guarantee.
        let out = phon_dict().apply_phonetic("deploy to coobernetes today", &HashSet::new());
        assert_eq!(out.text, "deploy to coobernetes today");
        assert!(out.applied.is_empty());
    }

    #[test]
    fn phonetic_leaves_exact_word_alone_but_is_recorded_when_changed() {
        let allowed: HashSet<String> = ["kubernetes".into()].into_iter().collect();
        let out = phon_dict().apply_phonetic("kubernetes rocks", &allowed);
        // Already correct spelling maps to same key -> canonical casing applied.
        assert_eq!(out.text, "Kubernetes rocks");
    }
```

- [ ] **Step 7: Run to verify it fails**

Run: `cargo test -p pie-engine corrector::dictionary`
Expected: FAIL — `apply_phonetic` not found.

- [ ] **Step 8: Implement `apply_phonetic`**

In `src/corrector/dictionary.rs`, add `use super::phonetic::phonetic_key;` to the top `use` lines, and add this method to `impl CorrectionDict`:

```rust
    /// Replace single tokens whose phonetic key matches a single-word entry,
    /// but only when that entry's canonical is in `allowed` (lowercased). This
    /// gate is what prevents over-correcting generic words.
    pub fn apply_phonetic(
        &self,
        text: &str,
        allowed: &std::collections::HashSet<String>,
    ) -> CorrectionOutcome {
        // Build a phonetic index over single-word, allowed entries.
        let mut index: std::collections::HashMap<String, &Correction> = std::collections::HashMap::new();
        for e in &self.entries {
            if e.heard.split_whitespace().count() != 1 {
                continue;
            }
            if !allowed.contains(&e.canonical.to_lowercase()) {
                continue;
            }
            index.entry(phonetic_key(&e.heard)).or_insert(e);
        }

        let tokens: Vec<&str> = text.split_whitespace().collect();
        let mut out_tokens = Vec::with_capacity(tokens.len());
        let mut applied = Vec::new();
        for tok in tokens {
            let (norm, trailing) = normalize(tok);
            if !norm.is_empty() {
                if let Some(e) = index.get(&phonetic_key(&norm)) {
                    if e.canonical.to_lowercase() != norm {
                        applied.push(AppliedFix {
                            from: norm.clone(),
                            to: e.canonical.clone(),
                            tier: Tier::Phonetic,
                        });
                    }
                    out_tokens.push(format!("{}{}", e.canonical, trailing));
                    continue;
                }
            }
            out_tokens.push(tok.to_string());
        }
        CorrectionOutcome { text: out_tokens.join(" "), applied }
    }
```

- [ ] **Step 9: Run to verify all dictionary + phonetic tests pass**

Run: `cargo test -p pie-engine corrector`
Expected: PASS (all corrector tests).

- [ ] **Step 10: Format and commit**

```bash
rustfmt --edition 2021 src/corrector/mod.rs src/corrector/phonetic.rs src/corrector/dictionary.rs
git add src/corrector/phonetic.rs src/corrector/mod.rs src/corrector/dictionary.rs
git commit -m "feat(corrector): context-gated phonetic matching tier"
```

---

### Task 3: Static seed + user dict + `PronunciationCorrector`

**Files:**
- Create: `src/corrector/tech_terms.json`
- Create: `src/corrector/static_seed.rs`
- Modify: `src/corrector/mod.rs` (the `PronunciationCorrector` type + `pub mod static_seed;`)

**Interfaces:**
- Consumes: `CorrectionDict`, `Correction`, `Source`, `CorrectionOutcome` (Tasks 1–2).
- Produces:
  - `PronunciationCorrector::new() -> Self` (embedded seed + user dict from default path)
  - `PronunciationCorrector::with_user_path(path: PathBuf) -> Self` (for tests)
  - `fn correct(&self, text: &str, allowed: &HashSet<String>) -> CorrectionOutcome`
  - `fn user_corrections(&self) -> Vec<Correction>`
  - `fn add_user_correction(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()>`
  - `fn remove_user_correction(&mut self, heard: &str) -> anyhow::Result<()>`

- [ ] **Step 1: Create the curated static seed**

Create `src/corrector/tech_terms.json` (start modest and correct; grow later). `heard` keys are lowercased; multi-word garbles are exact phrases:

```json
[
  { "heard": "next jazz", "canonical": "Next.js" },
  { "heard": "next js", "canonical": "Next.js" },
  { "heard": "node js", "canonical": "Node.js" },
  { "heard": "engine x", "canonical": "Nginx" },
  { "heard": "nginx", "canonical": "Nginx" },
  { "heard": "coobernetes", "canonical": "Kubernetes" },
  { "heard": "kubernetes", "canonical": "Kubernetes" },
  { "heard": "cube control", "canonical": "kubectl" },
  { "heard": "postgres", "canonical": "PostgreSQL" },
  { "heard": "post gres", "canonical": "PostgreSQL" },
  { "heard": "my sequel", "canonical": "MySQL" },
  { "heard": "no sequel", "canonical": "NoSQL" },
  { "heard": "type script", "canonical": "TypeScript" },
  { "heard": "java script", "canonical": "JavaScript" },
  { "heard": "git hub", "canonical": "GitHub" },
  { "heard": "git lab", "canonical": "GitLab" },
  { "heard": "vs code", "canonical": "VS Code" },
  { "heard": "tail wind", "canonical": "Tailwind" },
  { "heard": "web assembly", "canonical": "WebAssembly" },
  { "heard": "graph ql", "canonical": "GraphQL" },
  { "heard": "rest api", "canonical": "REST API" },
  { "heard": "o auth", "canonical": "OAuth" },
  { "heard": "json web token", "canonical": "JWT" },
  { "heard": "redis", "canonical": "Redis" },
  { "heard": "docker", "canonical": "Docker" },
  { "heard": "kafka", "canonical": "Kafka" }
]
```

- [ ] **Step 2: Write the failing test for seed loading**

Create `src/corrector/static_seed.rs`:

```rust
//! Loads the curated static correction seed embedded at compile time.

use super::dictionary::{Correction, Source};

/// The seed JSON, compiled into the binary.
const SEED_JSON: &str = include_str!("tech_terms.json");

#[derive(serde::Deserialize)]
struct SeedEntry {
    heard: String,
    canonical: String,
}

/// Parse the embedded seed into corrections. Panics only if the shipped JSON is
/// malformed, which a test guarantees it is not.
pub fn load() -> Vec<Correction> {
    let raw: Vec<SeedEntry> = serde_json::from_str(SEED_JSON).expect("embedded tech_terms.json is valid");
    raw.into_iter()
        .map(|e| Correction { heard: e.heard.to_lowercase(), canonical: e.canonical, source: Source::Static })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_parses_and_is_nonempty() {
        let seed = load();
        assert!(seed.len() >= 20, "expected a real seed, got {}", seed.len());
    }

    #[test]
    fn seed_heard_keys_are_lowercase() {
        for c in load() {
            assert_eq!(c.heard, c.heard.to_lowercase(), "heard key must be lowercase: {:?}", c.heard);
        }
    }
}
```

- [ ] **Step 3: Register the module and run the test**

In `src/corrector/mod.rs` add:

```rust
pub mod static_seed;
```

Run: `cargo test -p pie-engine corrector::static_seed`
Expected: PASS (2 tests).

- [ ] **Step 4: Write the failing test for `PronunciationCorrector`**

Add to the bottom of `src/corrector/mod.rs` (a `#[cfg(test)] mod tests` block at END of file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn temp_path() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("pie-pron-{}.json", std::process::id().wrapping_add(rand_suffix())));
        p
    }

    // Cheap unique-ish suffix without adding a dep.
    fn rand_suffix() -> u32 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
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
        assert!(c2.user_corrections().iter().any(|e| e.heard == "react" && e.canonical == "React"));
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
```

- [ ] **Step 5: Run to verify it fails**

Run: `cargo test -p pie-engine corrector::tests`
Expected: FAIL — `PronunciationCorrector` not found.

- [ ] **Step 6: Implement `PronunciationCorrector`**

In `src/corrector/mod.rs`, add these imports at the top and the type below the shared structs (above the test module):

```rust
use std::collections::HashSet;
use std::path::PathBuf;

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
        let mut c = Self { dict: CorrectionDict::from_entries(Vec::new()), user, user_path: Some(path) };
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
        CorrectionOutcome { text: phon.text, applied }
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
        self.user.push(Correction { heard, canonical, source: Source::User });
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
            let entries: Vec<UserEntry> = self.user.iter()
                .map(|e| UserEntry { heard: e.heard.clone(), canonical: e.canonical.clone() })
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
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("pie").join("pronunciation.json")
}

fn load_user_dict(path: &std::path::Path) -> Vec<Correction> {
    let Ok(json) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<UserEntry>>(&json) {
        Ok(raw) => raw.into_iter()
            .map(|e| Correction { heard: e.heard.to_lowercase(), canonical: e.canonical, source: Source::User })
            .collect(),
        Err(e) => {
            log::warn!("Failed to parse pronunciation.json: {e}; starting empty");
            Vec::new()
        }
    }
}
```

- [ ] **Step 7: Run to verify all corrector tests pass**

Run: `cargo test -p pie-engine corrector`
Expected: PASS.

- [ ] **Step 8: Format and commit**

```bash
rustfmt --edition 2021 src/corrector/mod.rs src/corrector/static_seed.rs
git add src/corrector/tech_terms.json src/corrector/static_seed.rs src/corrector/mod.rs
git commit -m "feat(corrector): static seed + persistent user dictionary"
```

---

### Task 4: Wire the corrector into `PieEngine::process`

**Files:**
- Modify: `src/pipeline/engine.rs`
- Test: `tests/corrector_pipeline.rs` (new integration test)

**Interfaces:**
- Consumes: `PronunciationCorrector`, `CorrectionOutcome`, `AppliedFix` (Task 3).
- Produces: `PieResult` gains `pub corrected_transcript: String` and `pub applied: Vec<AppliedFix>`.

- [ ] **Step 1: Write the failing integration test**

Create `tests/corrector_pipeline.rs`:

```rust
//! The corrector must sit inside the real pipeline: a garbled term in the input
//! becomes its canonical form in the optimized prompt.

use pie_engine::PieEngine;

#[tokio::test]
async fn corrector_fixes_jargon_in_the_optimized_prompt() {
    let mut engine = PieEngine::new().await.expect("engine");
    let result = engine.process("build a next jazz app", "balanced").await.expect("process");
    assert!(
        result.optimized_prompt.contains("Next.js"),
        "expected corrected term in prompt, got: {}",
        result.optimized_prompt
    );
    assert_eq!(result.corrected_transcript, "build a Next.js app");
    assert!(result.applied.iter().any(|f| f.to == "Next.js"));
}

#[tokio::test]
async fn clean_input_is_unchanged_by_the_corrector() {
    let mut engine = PieEngine::new().await.expect("engine");
    let result = engine.process("please summarize this document", "balanced").await.expect("process");
    assert_eq!(result.corrected_transcript, "please summarize this document");
    assert!(result.applied.is_empty());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p pie-engine --test corrector_pipeline`
Expected: FAIL — `corrected_transcript` field missing / trait bounds.

- [ ] **Step 3: Add the corrector to `PieEngine` and `PieResult`**

In `src/pipeline/engine.rs`:

Add imports near the top:

```rust
use crate::corrector::{AppliedFix, PronunciationCorrector};
```

Add fields to `PieResult` (after `estimated_tokens`):

```rust
    /// The transcript after correction (what intent/optimize actually saw).
    pub corrected_transcript: String,

    /// Corrections applied to the transcript, for UI transparency.
    pub applied: Vec<AppliedFix>,
```

Add a field to `PieEngine` (after `stt`):

```rust
    corrector: PronunciationCorrector,
```

In `PieEngine::new`, construct it and add to the returned struct:

```rust
        let corrector = PronunciationCorrector::new();
```
```rust
        Ok(Self {
            memory,
            extractor,
            llm,
            stt: None,
            corrector,
        })
```

- [ ] **Step 4: Correct the input at the top of `process`**

Replace the body of `process` up to intent extraction so correction runs first. The new top of `process`:

```rust
    pub async fn process(&mut self, input: &str, mode: &str) -> anyhow::Result<PieResult> {
        // Step 0: Correct speech-to-text jargon errors before anything else.
        // Allow-set: terms the user is known to use, so static phonetic entries
        // only fire for relevant terms. Derived from the profile's tech stack.
        let allowed: std::collections::HashSet<String> = self
            .memory
            .profile
            .technologies
            .iter()
            .map(|t| t.to_lowercase())
            .collect();
        let correction = self.corrector.correct(input, &allowed);
        let input = correction.text.as_str();

        // Step 1: Extract intent
        let intent = self.extractor.extract(input);
```

Then at the `Ok(PieResult { … })` construction at the end of `process`, add the two new fields:

```rust
        Ok(PieResult {
            intent,
            optimized_prompt: optimized.text,
            mode: optimized.mode,
            estimated_tokens: optimized.estimated_tokens,
            corrected_transcript: correction.text.clone(),
            applied: correction.applied,
        })
```

> The rest of `process` (record_interaction, optimize, save) is unchanged and now operates on the corrected `input`.

- [ ] **Step 5: Run the integration test + full lib suite**

Run: `cargo test -p pie-engine --test corrector_pipeline`
Expected: PASS (2 tests).

Run: `cargo test -p pie-engine`
Expected: PASS (baseline 60 + new corrector + pipeline tests).

- [ ] **Step 6: Format and commit**

```bash
rustfmt --edition 2021 src/pipeline/engine.rs
git add src/pipeline/engine.rs tests/corrector_pipeline.rs
git commit -m "feat(corrector): run correction at the top of the pipeline"
```

---

### Task 5: Opt-in LLM deep-correct pass

**Files:**
- Create: `src/corrector/llm_correct.rs`
- Modify: `src/corrector/mod.rs` (register `pub mod llm_correct;`)
- Modify: `src/pipeline/engine.rs` (add `PieEngine::deep_correct`)

**Interfaces:**
- Consumes: `LlmRouter` (via `PieEngine`), `UserProfile`, `CorrectionOutcome`, `AppliedFix`, `Tier`.
- Produces:
  - `llm_correct::build_prompt(transcript: &str, role: Option<&str>, tech: &[String]) -> String`
  - `llm_correct::diff_fixes(before: &str, after: &str) -> Vec<AppliedFix>`
  - `PieEngine::deep_correct(&self, transcript: &str, provider: &str, model: Option<&str>) -> anyhow::Result<CorrectionOutcome>`

- [ ] **Step 1: Write the failing test for the prompt builder + diff**

Create `src/corrector/llm_correct.rs`:

```rust
//! The opt-in LLM deep-correct pass: builds a correction-only meta-prompt and
//! extracts what changed. Off the always-on path; reuses the configured LLM.

use super::{AppliedFix, Tier};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_includes_transcript_and_context() {
        let p = build_prompt("deploy to coobernetes", Some("backend dev"), &["Rust".into()]);
        assert!(p.contains("coobernetes"));
        assert!(p.contains("backend dev"));
        assert!(p.contains("Rust"));
        // Correction-only instruction, not a rewrite.
        assert!(p.to_lowercase().contains("only change") || p.to_lowercase().contains("preserve"));
    }

    #[test]
    fn diff_records_changed_words_as_llm_fixes() {
        let fixes = diff_fixes("deploy to coobernetes now", "deploy to Kubernetes now");
        assert_eq!(fixes, vec![AppliedFix {
            from: "coobernetes".into(), to: "Kubernetes".into(), tier: Tier::Llm,
        }]);
    }

    #[test]
    fn diff_of_identical_text_is_empty() {
        assert!(diff_fixes("same text here", "same text here").is_empty());
    }
}
```

- [ ] **Step 2: Register module + run to verify it fails**

In `src/corrector/mod.rs` add `pub mod llm_correct;`.

Run: `cargo test -p pie-engine corrector::llm_correct`
Expected: FAIL — `build_prompt` / `diff_fixes` not found.

- [ ] **Step 3: Implement the builder and diff**

Insert above the test module in `src/corrector/llm_correct.rs`:

```rust
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
        tech = if tech.is_empty() { "unknown".to_string() } else { tech.join(", ") },
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
            fixes.push(AppliedFix { from: b[i].to_string(), to: a[i].to_string(), tier: Tier::Llm });
        }
    }
    fixes
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p pie-engine corrector::llm_correct`
Expected: PASS (3 tests).

- [ ] **Step 5: Add `PieEngine::deep_correct` + its test**

In `src/pipeline/engine.rs`, add `CorrectionOutcome` to the corrector import:

```rust
use crate::corrector::{AppliedFix, CorrectionOutcome, PronunciationCorrector};
use crate::corrector::llm_correct;
```

Add the method to `impl PieEngine`:

```rust
    /// Opt-in deep correction via the configured LLM. NOT on the always-on
    /// path — called only from the settings toggle or the on-demand command.
    /// Falls back to the input on any LLM error (never worse than no deep pass).
    pub async fn deep_correct(
        &self,
        transcript: &str,
        provider: &str,
        model: Option<&str>,
    ) -> anyhow::Result<CorrectionOutcome> {
        let prompt = llm_correct::build_prompt(
            transcript,
            self.memory.profile.role.as_deref(),
            &self.memory.profile.technologies,
        );
        let corrected = self.llm.send(&prompt, provider, model).await?;
        let corrected = corrected.trim().to_string();
        let applied = llm_correct::diff_fixes(transcript, &corrected);
        Ok(CorrectionOutcome { text: corrected, applied })
    }
```

Add an integration test to `tests/corrector_pipeline.rs` using the `echo` provider (which returns its input, so the diff is empty but the call path is exercised):

```rust
#[tokio::test]
async fn deep_correct_runs_through_the_echo_provider() {
    let engine = PieEngine::new().await.expect("engine");
    let out = engine.deep_correct("deploy to coobernetes", "echo", None).await.expect("deep");
    // echo returns the prompt/text; we only assert the call path works and
    // returns a CorrectionOutcome without erroring.
    assert!(!out.text.is_empty());
}
```

> Verify the `echo` provider's exact return contract in `src/llm/router.rs` before relying on it; if `echo` returns the full prompt rather than the bare text, assert `out.text.contains("coobernetes")` instead.

- [ ] **Step 6: Run to verify + format + commit**

Run: `cargo test -p pie-engine`
Expected: PASS.

```bash
rustfmt --edition 2021 src/corrector/llm_correct.rs src/corrector/mod.rs src/pipeline/engine.rs
git add src/corrector/llm_correct.rs src/corrector/mod.rs src/pipeline/engine.rs tests/corrector_pipeline.rs
git commit -m "feat(corrector): opt-in LLM deep-correct pass"
```

---

### Task 6: Tauri surface — settings, Outcome, and commands

**Files:**
- Modify: `src-tauri/src/settings.rs` (add `deep_correct_ai`)
- Modify: `src-tauri/src/main.rs` (Outcome fields, deep-correct wiring, new commands, corrector accessors)

**Interfaces:**
- Consumes: `PieEngine::deep_correct`, `PronunciationCorrector` accessors (Tasks 3, 5), `AppliedFix`.
- Produces Tauri commands: `list_corrections`, `add_correction`, `delete_correction`, `recorrect_with_ai`. `Outcome` gains `applied: Vec<AppliedFixDto>`.

- [ ] **Step 1: Add the setting**

In `src-tauri/src/settings.rs`, add to the `Settings` struct (after `history_limit`):

```rust
    /// When true, run the opt-in LLM deep-correct pass on every transcript.
    pub deep_correct_ai: bool,
```

Add to `Default` (after `history_limit: 10,`):

```rust
            deep_correct_ai: false,
```

Update `settings_roundtrip_json`'s assertions are unaffected (serde default covers it), but add one line to `partial_settings_fill_defaults`:

```rust
        assert!(!loaded.deep_correct_ai);
```

- [ ] **Step 2: Run the settings test**

Run: `cargo test -p pie-desktop settings`
Expected: PASS.

- [ ] **Step 3: Expose corrections on the Outcome + a DTO**

The engine exposes `AppliedFix`; the corrector's user dict lives inside `PieEngine`. Add accessors to `PieEngine` in `src/pipeline/engine.rs` (lib side, so commit them there — but this task's commit can include the lib change since it's the interface for the commands):

```rust
    pub fn corrector_user_corrections(&self) -> Vec<crate::corrector::Correction> {
        self.corrector.user_corrections()
    }
    pub fn corrector_add(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()> {
        self.corrector.add_user_correction(heard, canonical)
    }
    pub fn corrector_remove(&mut self, heard: &str) -> anyhow::Result<()> {
        self.corrector.remove_user_correction(heard)
    }
```

In `src-tauri/src/main.rs`, add a serializable DTO near `Outcome`:

```rust
#[derive(Clone, Serialize)]
struct AppliedFixDto {
    from: String,
    to: String,
    tier: String,
}
```

Add `applied: Vec<AppliedFixDto>` to the `Outcome` struct, and populate it in `transcribe_and_process` where `Outcome` is built:

```rust
        applied: result
            .applied
            .iter()
            .map(|f| AppliedFixDto { from: f.from.clone(), to: f.to.clone(), tier: format!("{:?}", f.tier) })
            .collect(),
```

- [ ] **Step 4: Wire the deep pass into `transcribe_and_process`**

In `transcribe_and_process`, after the `engine.process(...)` result and before building `Outcome`, run the deep pass when the setting is on. Replace the corrected transcript and applied list when it succeeds:

```rust
    // Opt-in deep correction (off the always-on path).
    let (final_transcript, mut applied_fixes) = (result.corrected_transcript.clone(), result.applied.clone());
    let (final_transcript, applied_fixes) = if settings.deep_correct_ai {
        match engine.deep_correct(&final_transcript, &settings.provider, model_opt(&settings)).await {
            Ok(deep) => {
                applied_fixes.extend(deep.applied);
                (deep.text, applied_fixes)
            }
            Err(e) => {
                log::warn!("deep-correct failed, using deterministic result: {e}");
                (final_transcript, applied_fixes)
            }
        }
    } else {
        (final_transcript, applied_fixes)
    };
    drop(engine);
```

> `model_opt` is a small helper: `fn model_opt(s: &Settings) -> Option<&str> { (!s.llm_model.is_empty()).then_some(s.llm_model.as_str()) }`. If such a helper already exists in main.rs (check the LLM send path), reuse it instead of adding a duplicate.

Then use `final_transcript` as `Outcome.transcript` (instead of the raw `transcript`) and `applied_fixes` for `Outcome.applied`. Note the raw `transcript` variable is still what goes to history unless you prefer the corrected one — set history's `transcript` to `final_transcript.clone()` so history reflects what the user saw.

- [ ] **Step 5: Add the corrector commands**

Add these `#[tauri::command]` functions in `src-tauri/src/main.rs` (near the other commands), following the existing `state.engine.lock().await` pattern:

```rust
#[derive(Clone, Serialize)]
struct CorrectionDto {
    heard: String,
    canonical: String,
}

#[tauri::command]
async fn list_corrections(state: State<'_, AppState>) -> Result<Vec<CorrectionDto>, String> {
    let engine = state.engine.lock().await;
    Ok(engine
        .corrector_user_corrections()
        .into_iter()
        .map(|c| CorrectionDto { heard: c.heard, canonical: c.canonical })
        .collect())
}

#[tauri::command]
async fn add_correction(state: State<'_, AppState>, heard: String, canonical: String) -> Result<(), String> {
    let mut engine = state.engine.lock().await;
    engine.corrector_add(&heard, &canonical).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_correction(state: State<'_, AppState>, heard: String) -> Result<(), String> {
    let mut engine = state.engine.lock().await;
    engine.corrector_remove(&heard).map_err(|e| e.to_string())
}

#[tauri::command]
async fn recorrect_with_ai(state: State<'_, AppState>, transcript: String) -> Result<Outcome, String> {
    let settings = { state.settings.lock().unwrap_or_else(|e| e.into_inner()).clone() };
    let mut engine = state.engine.lock().await;
    let deep = engine
        .deep_correct(&transcript, &settings.provider, model_opt(&settings))
        .await
        .map_err(|e| e.to_string())?;
    // Re-run intent/optimize on the deep-corrected text for a fresh Outcome.
    let result = engine.process(&deep.text, &settings.mode).await.map_err(|e| e.to_string())?;
    Ok(Outcome {
        transcript: deep.text,
        objective: result.intent.objective,
        conversation_type: format!("{:?}", result.intent.conversation_type),
        confidence: format!("{:?}", result.intent.confidence),
        optimized_prompt: result.optimized_prompt,
        estimated_tokens: result.estimated_tokens,
        mode: format!("{:?}", result.mode),
        applied: deep.applied.iter().map(|f| AppliedFixDto {
            from: f.from.clone(), to: f.to.clone(), tier: format!("{:?}", f.tier),
        }).collect(),
    })
}
```

Register all four in the `tauri::generate_handler![...]` macro list (find the existing list in `main.rs` and add the names).

- [ ] **Step 6: Build the desktop binary**

Run: `cargo build -p pie-desktop`
Expected: compiles clean (no errors; my changes clippy-clean).

Run: `cargo test -p pie-desktop`
Expected: PASS.

- [ ] **Step 7: Format and commit**

```bash
rustfmt --edition 2021 src-tauri/src/settings.rs src-tauri/src/main.rs src/pipeline/engine.rs
git add src-tauri/src/settings.rs src-tauri/src/main.rs src/pipeline/engine.rs
git commit -m "feat(corrector): tauri commands, deep-correct wiring, applied fixes on outcome"
```

---

### Task 7: UI — Vocabulary settings + result-view transparency

**Files:**
- Create: `ui/src/lib/VocabularySettings.svelte`
- Modify: `ui/src/lib/RecordingView.svelte` (corrected line, Re-correct button, Save chip)
- Modify: the settings pane component that hosts sections (locate the parent that renders `HotkeyRecorder`) to include `VocabularySettings`
- Modify: `ui/src/app.css` (styles for the corrections list + corrected line, following existing tokens)

**Interfaces:**
- Consumes Tauri commands: `list_corrections`, `add_correction`, `delete_correction`, `recorrect_with_ai`; `Outcome.applied` (Task 6).

- [ ] **Step 1: Build the Vocabulary settings component**

Create `ui/src/lib/VocabularySettings.svelte` following the `HotkeyRecorder.svelte` structure (props `settings`, `onSave`, `onError`; `invoke` from `@tauri-apps/api/core`):

```svelte
<script>
  import { invoke } from "@tauri-apps/api/core";

  let { settings, onSave, onError } = $props();

  let corrections = $state([]);
  let heard = $state("");
  let canonical = $state("");

  async function refresh() {
    try { corrections = await invoke("list_corrections"); }
    catch (e) { onError(String(e)); }
  }
  refresh();

  async function add() {
    if (!heard.trim() || !canonical.trim()) return;
    try {
      await invoke("add_correction", { heard, canonical });
      heard = ""; canonical = "";
      await refresh();
    } catch (e) { onError(String(e)); }
  }

  async function remove(h) {
    try { await invoke("delete_correction", { heard: h }); await refresh(); }
    catch (e) { onError(String(e)); }
  }
</script>

<section class="group">
  <div class="field">
    <span class="field-label">Deep-correct with AI</span>
    <label class="toggle">
      <input type="checkbox" bind:checked={settings.deep_correct_ai} onchange={onSave} />
      <span>Use the configured LLM to fix garbled terms (slower; uses your provider)</span>
    </label>
  </div>

  <div class="field">
    <span class="field-label">Your corrections</span>
    <div class="correction-add">
      <input placeholder="heard (e.g. next jazz)" bind:value={heard} />
      <span aria-hidden="true">→</span>
      <input placeholder="correct (e.g. Next.js)" bind:value={canonical} />
      <button class="btn" onclick={add} aria-label="Add correction">Add</button>
    </div>
    {#if corrections.length}
      <ul class="correction-list">
        {#each corrections as c}
          <li>
            <span class="mono">{c.heard}</span>
            <span aria-hidden="true">→</span>
            <span class="mono">{c.canonical}</span>
            <button class="text-btn" onclick={() => remove(c.heard)} aria-label={`Delete ${c.heard}`}>Delete</button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="caption">No custom corrections yet. Add one above, or save one from a result.</p>
    {/if}
  </div>
</section>
```

- [ ] **Step 2: Mount it in the settings pane**

Locate the component that renders `<HotkeyRecorder ... />` (grep `ui/src` for `HotkeyRecorder`). Import and place `<VocabularySettings {settings} {onSave} {onError} />` next to it, passing the same props it passes to `HotkeyRecorder`.

Run: `grep -rn "HotkeyRecorder" ui/src`
Then edit that parent to import from `./VocabularySettings.svelte` and render it.

- [ ] **Step 3: Add the corrected line + actions to the result view**

In `ui/src/lib/RecordingView.svelte`, extend props and the "Heard" step. Add `applied` and `onRecorrect` to the `$props()` destructure:

```svelte
  let { recState, outcome, llmResponse, llmBusy, hotkey, stateLabel, onToggle, onCancel, onSend, onCopy, onRecorrect } = $props();
```

Under the transcript `<p>` in the "Heard" `result-step`, show applied fixes and the Re-correct button:

```svelte
        <div class="result-step">
          <span class="eyebrow">Heard</span>
          <p class="transcript">{outcome.transcript}</p>
          {#if outcome.applied && outcome.applied.length}
            <p class="corrected-note">
              corrected {#each outcome.applied as f}<span class="fix">{f.from} → {f.to}</span>{/each}
            </p>
          {/if}
          <button class="text-btn" onclick={onRecorrect} disabled={llmBusy} aria-label="Re-correct with AI">
            Re-correct with AI
          </button>
        </div>
```

- [ ] **Step 4: Wire `onRecorrect` in the parent (App)**

In the component that renders `<RecordingView ... />` (grep for it), add a handler that calls the command and replaces the current outcome:

```js
  async function onRecorrect() {
    if (!outcome) return;
    llmBusy = true;
    try {
      outcome = await invoke("recorrect_with_ai", { transcript: outcome.transcript });
    } catch (e) {
      // surface via the existing error path
      error = String(e);
    } finally {
      llmBusy = false;
    }
  }
```

Pass `onRecorrect` and `applied` through to `<RecordingView ... {onRecorrect} />` (the `applied` is already inside `outcome`).

- [ ] **Step 5: Add styles**

In `ui/src/app.css`, add (matching existing muted/kbd tokens; keep it quiet):

```css
.corrected-note { font-size: 0.8rem; opacity: 0.7; display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px; }
.corrected-note .fix { background: rgba(127,127,127,0.12); border-radius: 4px; padding: 0 6px; }
.correction-add { display: flex; align-items: center; gap: 6px; }
.correction-list { list-style: none; padding: 0; margin: 8px 0 0; display: flex; flex-direction: column; gap: 4px; }
.correction-list li { display: flex; align-items: center; gap: 8px; }
.correction-list .mono, .mono { font-family: ui-monospace, monospace; }
```

- [ ] **Step 6: Build the UI**

Run: `cd ui && npm run build`
Expected: Vite build succeeds with no errors.

- [ ] **Step 7: Commit**

```bash
git add ui/src/lib/VocabularySettings.svelte ui/src/lib/RecordingView.svelte ui/src/app.css ui/src/App.svelte
git commit -m "feat(corrector): vocabulary settings + result-view correction transparency"
```

> Replace `ui/src/App.svelte` in the `git add` with whatever the actual parent files are (from the greps in Steps 2 and 4).

- [ ] **Step 8: Manual verification (interactive, owed by user)**

Run the app (`npm run tauri dev` or the built binary) and confirm:
1. Speaking "build a next jazz app" pastes/optimizes with "Next.js"; the "corrected: next jazz → Next.js" line shows.
2. Toggling "Deep-correct with AI" on (with a real LLM provider configured) fixes a novel garble the dictionary misses.
3. "Re-correct with AI" on a result updates it.
4. Adding a correction in Vocabulary settings, then restarting the app, shows it persisted (`pronunciation.json` in the pie config dir).

---

## Self-Review

**Spec coverage:**
- Module layout (`mod/dictionary/phonetic/static_seed/llm_correct` + `tech_terms.json`) → Tasks 1–5. ✓
- Data model (`Correction`, `Source`, `CorrectionOutcome`, `AppliedFix`, `Tier`; no confidence/decay) → Task 1. ✓
- Static seed embedded + modest size → Task 3. ✓
- User dict `pronunciation.json`, override same-heard, `custom_terms` untouched → Task 3. ✓
- Self-bootstrapping allow-set (user canonicals always allowed + profile.technologies) → Tasks 3–4. ✓
- Pipeline integration at `process()` choke point → Task 4. ✓
- Exact longest-match + boundary safety + punctuation + no-op on clean input → Tasks 1, 4. ✓
- Context-gated phonetic + empty-allow-set non-fire guarantee → Task 2. ✓
- Opt-in LLM pass: setting toggle + on-demand command, correction-only prompt, diff→applied, reuses router → Tasks 5–6. ✓
- Learning bridge (save an AI correction to user dict) → the `add_correction` command exists (Task 6); the result-view "Save" chip is a thin call to it. NOTE: Task 7 wires Re-correct but does not add the one-tap Save chip — see gap below.
- UI: Vocabulary section (toggle + editable list) + result-view corrected line + Re-correct button → Task 7. ✓
- Testing strategy (unit tiers, integration pipeline, echo-provider LLM test, manual list) → Tasks 1–5, 7. ✓

**Gap found & fixed inline:** The spec's one-tap "Save 'X' → 'Y'" chip after an LLM correction is not its own step. It is a trivial addition on top of Task 6's `add_correction`: in `RecordingView.svelte` render, for each `outcome.applied` fix with `tier === "Llm"`, show a `<button class="text-btn" onclick={() => invoke("add_correction", { heard: f.from, canonical: f.to })}>Save</button>`. Add this to Task 7 Step 3 when implementing (the command already exists, so no plan-level type gap).

**Placeholder scan:** No TBD/TODO. Two "verify the exact contract" notes (normalize trailing-punct helper; echo provider return) are guidance with concrete fallbacks, not placeholders.

**Type consistency:** `PronunciationCorrector`, `CorrectionOutcome`, `AppliedFix { from, to, tier }`, `Tier { Exact, Phonetic, Llm }`, `apply_exact`, `apply_phonetic`, `correct`, `deep_correct`, `corrector_add/remove/user_corrections` used consistently across tasks. `PieResult.corrected_transcript` / `.applied` defined in Task 4 and consumed in Task 6. Command names (`list_corrections`, `add_correction`, `delete_correction`, `recorrect_with_ai`) consistent between Tasks 6 and 7.
