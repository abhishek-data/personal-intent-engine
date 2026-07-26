# Phase 2: Auto-Learning Vocabulary — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** PIE improves its pronunciation vocabulary from usage — always-on reinforcement of corrections that fire, plus an opt-in (off-by-default) background LLM miner that discovers new misrecognitions — all stored in a separate, resettable `learned_vocab.json`.

**Architecture:** A new `LearnedStore` persists learned/synced entries to `~/.config/pie/learned_vocab.json`, separate from the user's `pronunciation.json`. `PronunciationCorrector` merges learned entries into its compiled dict at a new `AutoLearned`/`Synced` precedence tier (User → Synced → AutoLearned → Static). The background miner is a detached tokio task that receives fire-and-forget `LearnTask`s over an mpsc channel, batches them, rate-limits LLM calls, and **appends to `learned_vocab.json`**; the engine reloads the store at the top of `process()` when the file's mtime changes — so there is no shared `Arc<Mutex>` on the pipeline hot path. Reinforcement (bumping `seen_count`/confidence when a learned entry's correction fires) happens synchronously in the engine, which owns the corrector.

**Tech Stack:** Rust (edition 2021), tokio (mpsc, time), serde/serde_json, reqwest (existing LLM client); Tauri v2 commands; Svelte 5 (runes).

## Global Constraints

- Rust edition 2021. No `unwrap()` in library code — use `?` or `.expect("reason")`. `unwrap_or_else(|e| e.into_inner())` on poisoned std-mutex locks in `src-tauri` is the established allowed pattern.
- Doc comments (`/// …`) on all public items. `cargo fmt` + `cargo clippy -p pie-engine` + `cargo clippy -p pie-desktop` clean (two PRE-EXISTING warnings exist in `src/corrector/phonetic.rs:37` and `src-tauri/src/nspanel.rs:116` — not yours, ignore). Test output pristine.
- **The pipeline must never block on learning.** The engine fires `LearnTask` with `try_send` (never `send`); dropping a task under backpressure is acceptable.
- **Background mining is OFF by default** (`settings.background_mining` defaults `false`). When off, zero background LLM calls ever happen.
- Learned entries live in `~/.config/pie/learned_vocab.json`, a SEPARATE file from `pronunciation.json`. "Reset learned" must never touch the user's manual dict or the static seed.
- Precedence when merging into the compiled dict, highest first: **User → Synced → AutoLearned → Static**. A `heard` key present at a higher tier suppresses the same key at lower tiers.
- The desktop crate is named `pie-desktop`; the library crate is `pie-engine`. Build the desktop app with `cargo build -p pie-desktop`.
- Tests must use a temp/injected path for `learned_vocab.json` (never the real user config) — mirror the existing `PronunciationCorrector::with_user_path` test seam.

---

### Task 1: `Source` variants + `LearnedStore`

**Files:**
- Modify: `src/corrector/dictionary.rs` (add `Source::Synced`, `Source::AutoLearned`)
- Create: `src/corrector/learned.rs`
- Modify: `src/corrector/mod.rs` (add `pub mod learned;`)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `Source::Synced`, `Source::AutoLearned` (added to the existing enum in `dictionary.rs`).
  - `pub struct LearnedEntry { pub heard: String, pub canonical: String, pub source: LearnedSource, pub confidence: f32, pub seen_count: u32, pub first_seen: u64, pub last_seen: u64 }` (all `pub`, `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]`).
  - `pub enum LearnedSource { Auto, Sync }` (`#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]`, `#[serde(rename_all = "lowercase")]`).
  - `pub struct LearnedStore { /* entries: Vec<LearnedEntry>, path: Option<PathBuf> */ }` with:
    - `pub fn load(path: PathBuf) -> Self` — reads JSON array; missing/corrupt file → empty store, path retained.
    - `pub fn save(&self) -> anyhow::Result<()>` — atomic-ish: create parent dir, write pretty JSON; no-op when path is `None`.
    - `pub fn add_or_reinforce(&mut self, heard: &str, canonical: &str, source: LearnedSource) -> anyhow::Result<()>` — lowercases `heard`; if an entry with that `heard` exists, bump `seen_count += 1`, set `last_seen = now`, raise `confidence` toward 1.0 (e.g. `+= (1.0 - confidence) * 0.34`); else insert with `seen_count = 1`, `confidence = 0.5`, `first_seen = last_seen = now`. Persists via `save()`.
    - `pub fn reinforce(&mut self, heard: &str) -> anyhow::Result<bool>` — if a learned entry with `heard` (lowercased) exists, bump `seen_count`/`last_seen`/`confidence` and `save()`, return `true`; else return `false` (no write).
    - `pub fn has_entry(&self, heard: &str) -> bool` — case-insensitive.
    - `pub fn entries(&self) -> &[LearnedEntry]`.
    - `pub fn reset(&mut self) -> anyhow::Result<()>` — clears entries and `save()`s (writes `[]`).
    - `pub fn count(&self) -> usize`.
  - `now_unix()` private helper: seconds since epoch as `u64`.

- [ ] **Step 1: Write the failing test** (in a `#[cfg(test)] mod tests` at the bottom of `src/corrector/learned.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);
    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pie-learned-{}-{}.json",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn add_then_reinforce_bumps_seen_count_and_persists() {
        let path = temp_path();
        let mut s = LearnedStore::load(path.clone());
        s.add_or_reinforce("next jazz", "Next.js", LearnedSource::Auto).unwrap();
        assert_eq!(s.count(), 1);
        assert_eq!(s.entries()[0].seen_count, 1);
        s.add_or_reinforce("Next Jazz", "Next.js", LearnedSource::Auto).unwrap(); // case-insensitive same key
        assert_eq!(s.count(), 1, "same heard must not duplicate");
        assert_eq!(s.entries()[0].seen_count, 2);

        let reloaded = LearnedStore::load(path.clone());
        assert_eq!(reloaded.count(), 1);
        assert_eq!(reloaded.entries()[0].seen_count, 2);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reinforce_missing_returns_false_no_write() {
        let path = temp_path();
        let mut s = LearnedStore::load(path.clone());
        assert!(!s.reinforce("nope").unwrap());
        assert!(!path.exists(), "reinforce of a missing key must not create the file");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reset_clears_and_persists_empty() {
        let path = temp_path();
        let mut s = LearnedStore::load(path.clone());
        s.add_or_reinforce("engine x", "Nginx", LearnedSource::Sync).unwrap();
        s.reset().unwrap();
        assert_eq!(s.count(), 0);
        assert_eq!(LearnedStore::load(path.clone()).count(), 0);
        let _ = std::fs::remove_file(path);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pie-engine --lib corrector::learned`
Expected: FAIL — `LearnedStore` / `LearnedSource` not found.

- [ ] **Step 3: Write minimal implementation**

Add to `src/corrector/dictionary.rs`, extending the enum (keep existing derives):

```rust
pub enum Source {
    Static,
    User,
    Synced,       // from initial vocabulary sync (Phase 3)
    AutoLearned,  // from the background learner
}
```

Create `src/corrector/learned.rs`:

```rust
//! Learned/synced vocabulary store, persisted separately from the user's
//! manual `pronunciation.json` so it can be reset or inspected on its own.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Where a learned entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LearnedSource {
    /// Background learner (Phase 2).
    Auto,
    /// Initial vocabulary sync (Phase 3).
    Sync,
}

/// One learned heard->canonical mapping with reinforcement metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LearnedEntry {
    pub heard: String,
    pub canonical: String,
    pub source: LearnedSource,
    pub confidence: f32,
    pub seen_count: u32,
    pub first_seen: u64,
    pub last_seen: u64,
}

/// Persistent collection of learned entries.
pub struct LearnedStore {
    entries: Vec<LearnedEntry>,
    path: Option<PathBuf>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl LearnedStore {
    /// Load from `path`; a missing or unparseable file yields an empty store
    /// (the path is retained so later writes land in the right place).
    pub fn load(path: PathBuf) -> Self {
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|j| serde_json::from_str::<Vec<LearnedEntry>>(&j).ok())
            .unwrap_or_default();
        Self { entries, path: Some(path) }
    }

    /// Persist the current entries as pretty JSON. No-op without a path.
    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, serde_json::to_string_pretty(&self.entries)?)?;
        }
        Ok(())
    }

    /// Insert a new entry or reinforce an existing one (case-insensitive on
    /// `heard`), then persist.
    pub fn add_or_reinforce(
        &mut self,
        heard: &str,
        canonical: &str,
        source: LearnedSource,
    ) -> anyhow::Result<()> {
        let key = heard.trim().to_lowercase();
        let now = now_unix();
        if let Some(e) = self.entries.iter_mut().find(|e| e.heard == key) {
            e.seen_count = e.seen_count.saturating_add(1);
            e.last_seen = now;
            e.confidence += (1.0 - e.confidence) * 0.34;
        } else {
            self.entries.push(LearnedEntry {
                heard: key,
                canonical: canonical.trim().to_string(),
                source,
                confidence: 0.5,
                seen_count: 1,
                first_seen: now,
                last_seen: now,
            });
        }
        self.save()
    }

    /// Reinforce an existing entry if present; returns whether one was found.
    /// Does not write when nothing matches.
    pub fn reinforce(&mut self, heard: &str) -> anyhow::Result<bool> {
        let key = heard.trim().to_lowercase();
        let now = now_unix();
        if let Some(e) = self.entries.iter_mut().find(|e| e.heard == key) {
            e.seen_count = e.seen_count.saturating_add(1);
            e.last_seen = now;
            e.confidence += (1.0 - e.confidence) * 0.34;
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn has_entry(&self, heard: &str) -> bool {
        let key = heard.trim().to_lowercase();
        self.entries.iter().any(|e| e.heard == key)
    }

    pub fn entries(&self) -> &[LearnedEntry] {
        &self.entries
    }

    /// Clear all learned entries and persist the empty set.
    pub fn reset(&mut self) -> anyhow::Result<()> {
        self.entries.clear();
        self.save()
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }
}
```

Add to `src/corrector/mod.rs` module declarations: `pub mod learned;`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pie-engine --lib corrector::learned`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/corrector/dictionary.rs src/corrector/learned.rs src/corrector/mod.rs
git commit -m "feat(corrector): add LearnedStore + Synced/AutoLearned sources"
```

---

### Task 2: Integrate `LearnedStore` into `PronunciationCorrector`

**Files:**
- Modify: `src/corrector/mod.rs`

**Interfaces:**
- Consumes: `LearnedStore`, `LearnedSource`, `LearnedEntry` (Task 1); existing `Correction`, `Source`.
- Produces (new methods on `PronunciationCorrector`):
  - `pub fn with_paths(user_path: PathBuf, learned_path: PathBuf) -> Self` — test seam that injects both file paths.
  - `pub fn add_auto_correction(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()>`
  - `pub fn add_synced_correction(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()>`
  - `pub fn reinforce_learned(&mut self, heard: &str) -> anyhow::Result<bool>`
  - `pub fn has_learned(&self, heard: &str) -> bool`
  - `pub fn learned_count(&self) -> usize`
  - `pub fn reset_learned(&mut self) -> anyhow::Result<()>`
  - `pub fn reload_learned(&mut self) -> bool` — reloads the LearnedStore from disk and rebuilds; returns whether the count changed (used by the engine's mtime reload).
- `rebuild()` now merges learned entries between user and static, respecting precedence User → Synced → AutoLearned → Static.

- [ ] **Step 1: Write the failing test** (add to the existing `#[cfg(test)] mod tests` in `src/corrector/mod.rs`; a `with_paths` temp helper is provided inline)

```rust
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
        assert_eq!(out.text, "i love ReactJS", "user tier must win over learned");
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
        assert_eq!(c.user_corrections().len(), 1, "user dict must survive reset");
        let _ = std::fs::remove_file(c_path);
        let _ = std::fs::remove_file(l_path);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pie-engine --lib corrector::tests`
Expected: FAIL — `no function with_paths` / `no method add_auto_correction`.

- [ ] **Step 3: Write minimal implementation**

In `src/corrector/mod.rs`:

1. Add imports: `use learned::{LearnedSource, LearnedStore};`
2. Add field to the struct:

```rust
pub struct PronunciationCorrector {
    dict: CorrectionDict,
    user: Vec<Correction>,
    user_path: Option<PathBuf>,
    learned: LearnedStore,
}
```

3. Update constructors. Change `with_user_path` to build a default learned path, and add `with_paths`:

```rust
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
        let learned = LearnedStore::load(learned_path);
        let mut c = Self {
            dict: CorrectionDict::from_entries(Vec::new()),
            user,
            user_path: Some(user_path),
            learned,
        };
        c.rebuild();
        c
    }
```

> NOTE: the existing tests call `with_user_path(temp_path())`; keeping that signature means those tests now share ONE default learned path. To keep them isolated, this task ALSO updates every existing `with_user_path(...)` call site in `mod.rs`'s own test module to `with_paths(temp_path(), temp_learned_path())`. Do that as part of Step 3.

4. Rewrite `rebuild()` to merge tiers with correct precedence (User → Synced → AutoLearned → Static). Learned entries carry their own `LearnedSource`; map `Sync → Source::Synced`, `Auto → Source::AutoLearned`:

```rust
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
```

5. Add the learned mutators/queries and reload:

```rust
    /// Add or reinforce an auto-learned correction, then recompile.
    pub fn add_auto_correction(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()> {
        self.learned.add_or_reinforce(heard, canonical, LearnedSource::Auto)?;
        self.rebuild();
        Ok(())
    }

    /// Add or reinforce a synced correction (Phase 3), then recompile.
    pub fn add_synced_correction(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()> {
        self.learned.add_or_reinforce(heard, canonical, LearnedSource::Sync)?;
        self.rebuild();
        Ok(())
    }

    /// Reinforce an existing learned entry (bumps confidence/seen_count).
    /// Returns whether a learned entry matched. No rebuild needed (mappings
    /// are unchanged; only metadata moves).
    pub fn reinforce_learned(&mut self, heard: &str) -> anyhow::Result<bool> {
        self.learned.reinforce(heard)
    }

    pub fn has_learned(&self, heard: &str) -> bool {
        self.learned.has_entry(heard)
    }

    pub fn learned_count(&self) -> usize {
        self.learned.count()
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
```

6. Add a private `learned_path()` accessor (store the path so reload works). Simplest: give `LearnedStore` a `pub fn path(&self) -> Option<PathBuf>` in Task 1 — ADD THAT to Task 1's interface now if missing — or track the path in the corrector. To avoid editing Task 1 retroactively, store the learned path on the corrector:

Add field `learned_path: Option<PathBuf>` to the struct, set it in `with_paths`, and implement:

```rust
    fn learned_path(&self) -> Option<PathBuf> {
        self.learned_path.clone()
    }
```

7. Add the default learned path helper next to `default_user_path`:

```rust
fn default_learned_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pie")
        .join("learned_vocab.json")
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pie-engine --lib corrector`
Expected: PASS — the three new tests plus all pre-existing corrector tests (update the pre-existing `with_user_path` call sites per the Step 3 NOTE so they stay isolated).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/corrector/mod.rs
git commit -m "feat(corrector): merge learned vocab into dict with tiered precedence"
```

---

### Task 3: Engine — reload-on-mtime, reinforcement, learner channel

**Files:**
- Modify: `src/pipeline/engine.rs`

**Interfaces:**
- Consumes: corrector methods from Task 2 (`reload_learned`, `reinforce_learned`, `learned_count`, `reset_learned`, `add_auto_correction`).
- Produces (on `PieEngine`):
  - `learner_tx: Option<tokio::sync::mpsc::Sender<LearnTask>>` field (default `None`).
  - `learned_vocab_path: Option<PathBuf>` + last-seen mtime cache for the reload check.
  - `pub struct LearnTask { pub raw_transcript: String, pub role: Option<String>, pub technologies: Vec<String> }` (public; the learner in Task 4 consumes it).
  - `pub fn set_learner_tx(&mut self, tx: tokio::sync::mpsc::Sender<LearnTask>)`.
  - `pub fn corrector_learned_count(&self) -> usize` and `pub fn corrector_reset_learned(&mut self) -> anyhow::Result<()>` (thin passthroughs for Tauri).
  - In `process()`: (a) before correcting, call a private `maybe_reload_learned()` that reloads the corrector's learned store when `learned_vocab.json` mtime advanced; (b) after correcting, for each applied fix whose `from`/`to` corresponds to a learned entry, call `reinforce_learned`; (c) fire-and-forget a `LearnTask` via `learner_tx` with `try_send`.

- [ ] **Step 1: Write the failing test** (add to the `#[cfg(test)] mod tests` in `engine.rs` created in Phase 1)

```rust
    #[tokio::test]
    async fn process_reinforces_a_firing_learned_correction() {
        // Ephemeral engine with injected corrector paths.
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let cpath = dir.join(format!("pie-eng-user-{uid}.json"));
        let lpath = dir.join(format!("pie-eng-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        engine.corrector_add_auto("terra form", "Terraform").unwrap();
        let before = engine.corrector_learned_seen("terra form");
        let _ = engine.process("deploy with terra form now", "balanced").await.unwrap();
        let after = engine.corrector_learned_seen("terra form");
        assert!(after > before, "a firing learned correction must be reinforced");
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }
```

This test needs three tiny test-only helpers on `PieEngine`: `new_ephemeral_with_learned(user, learned)`, `corrector_add_auto(heard, canonical)`, and `corrector_learned_seen(heard) -> u32`. Add them in Step 3 (the last returns the `seen_count` of a learned entry, or 0).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pie-engine --lib pipeline::engine`
Expected: FAIL — helpers/field not present.

- [ ] **Step 3: Write minimal implementation**

In `src/pipeline/engine.rs`:

1. Imports: `use std::path::PathBuf; use tokio::sync::mpsc;`
2. Add the task struct near the top:

```rust
/// Fire-and-forget signal from the pipeline to the background learner.
pub struct LearnTask {
    pub raw_transcript: String,
    pub role: Option<String>,
    pub technologies: Vec<String>,
}
```

3. Add fields to `PieEngine`:

```rust
    learner_tx: Option<mpsc::Sender<LearnTask>>,
    learned_vocab_path: Option<PathBuf>,
    learned_mtime: Option<std::time::SystemTime>,
```

Initialize them (`None`) in `new()`, `new_ephemeral()`, and `with_config()`'s delegation. For the reload path to work in production, set `learned_vocab_path` to the corrector's default learned path in `new()` — add a `pub fn learned_path()` on the corrector OR pass the known default. Simplest: in `new()` set `learned_vocab_path: Some(default_learned_vocab_path())` where that helper mirrors the corrector's `default_learned_path()` (documented duplication, or expose the corrector's path — prefer exposing: add `pub fn corrector_learned_path(&self) -> Option<PathBuf>` to the corrector in Task 2 if you have not; if adding retroactively is undesirable, use a private helper here that returns `dirs::config_dir()…/pie/learned_vocab.json`). Keep it `None` for ephemeral unless the test injects it.

4. `maybe_reload_learned` + reinforcement + fire, wired into `process()`:

```rust
    /// Reload learned vocab if the file changed since we last looked. Cheap
    /// stat; only rebuilds when mtime advanced.
    fn maybe_reload_learned(&mut self) {
        let Some(path) = &self.learned_vocab_path else { return; };
        let Ok(meta) = std::fs::metadata(path) else { return; };
        let Ok(mtime) = meta.modified() else { return; };
        if self.learned_mtime != Some(mtime) {
            self.learned_mtime = Some(mtime);
            let _ = self.corrector.reload_learned();
        }
    }
```

In `process()`, at the very top (before building `allowed`):

```rust
        self.maybe_reload_learned();
```

After `let correction = self.corrector.correct(input, &allowed);` and before reassigning `input`, reinforce learned entries that fired:

```rust
        for fix in &correction.applied {
            // `from` is the lowercased heard phrase; reinforce if it's learned.
            let _ = self.corrector.reinforce_learned(&fix.from);
        }
```

Near the end of `process()`, fire the learn task (non-blocking):

```rust
        if let Some(tx) = &self.learner_tx {
            let _ = tx.try_send(LearnTask {
                raw_transcript: input.to_string(),
                role: self.memory.profile.role.clone(),
                technologies: self.memory.profile.technologies.clone(),
            });
        }
```

(Use the ORIGINAL raw transcript for `raw_transcript`. Capture it into a local `let raw = input.to_string();` at the very start of `process`, before correction reassigns `input`, and use `raw` here.)

5. Passthroughs + test helpers:

```rust
    pub fn set_learner_tx(&mut self, tx: mpsc::Sender<LearnTask>) {
        self.learner_tx = Some(tx);
    }

    pub fn corrector_learned_count(&self) -> usize {
        self.corrector.learned_count()
    }

    pub fn corrector_reset_learned(&mut self) -> anyhow::Result<()> {
        self.corrector.reset_learned()
    }

    #[doc(hidden)]
    pub fn new_ephemeral_with_learned(user: PathBuf, learned: PathBuf) -> Self {
        let mut e = Self::new_ephemeral(user.clone());
        e.corrector = crate::corrector::PronunciationCorrector::with_paths(user, learned.clone());
        e.learned_vocab_path = Some(learned);
        e
    }

    #[doc(hidden)]
    pub fn corrector_add_auto(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()> {
        self.corrector.add_auto_correction(heard, canonical)
    }

    #[doc(hidden)]
    pub fn corrector_learned_seen(&self, heard: &str) -> u32 {
        // 0 when absent.
        self.corrector
            .learned_entries_seen(heard)
            .unwrap_or(0)
    }
```

Add `pub fn learned_entries_seen(&self, heard: &str) -> Option<u32>` to the corrector (Task 2 addendum — include it there): returns the matching learned entry's `seen_count`. If you did not add it in Task 2, add it now in `mod.rs` and note it in your report.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pie-engine --lib`
Expected: PASS — the new engine test plus all existing (90 from before + Tasks 1-2 additions).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/pipeline/engine.rs src/corrector/mod.rs
git commit -m "feat(engine): reinforce learned corrections + reload-on-change + learner channel"
```

---

### Task 4: Background learner (opt-in miner)

**Files:**
- Create: `src/corrector/learner.rs`
- Modify: `src/corrector/mod.rs` (`pub mod learner;`)

**Interfaces:**
- Consumes: `LearnTask` (Task 3), `LlmRouter` (`crate::llm::LlmRouter`), `LearnedStore`/`LearnedSource` (Task 1).
- Produces:
  - `pub struct ExtractedCorrection { pub heard: String, pub canonical: String }` (`Serialize, Deserialize`).
  - `pub fn parse_extracted(json: &str) -> anyhow::Result<Vec<ExtractedCorrection>>` — tolerant parse: strips ``` / ```json fences, then `serde_json::from_str`.
  - `pub fn build_extraction_prompt(batch: &[LearnTask]) -> String` — conservative prompt (role + tech + transcripts; ask for `[{"heard","canonical"}]`, `[]` when nothing).
  - `pub struct BackgroundLearner { /* rx, llm, learned_path, provider, model, known: HashSet<String> */ }` with:
    - `pub fn new(rx: mpsc::Receiver<LearnTask>, llm: LlmRouter, learned_path: PathBuf, provider: String, model: Option<String>, known: HashSet<String>) -> Self`
    - `pub async fn run(mut self)` — loop: collect a batch (up to 5 items, or 30s since first item), skip empty, enforce ≥30s since the last LLM call (rate limit), build prompt, `llm.send`, parse, and for each extracted correction whose `heard` is not already in `known` (and not already in the on-disk store), `LearnedStore::load(path).add_or_reinforce(.., LearnedSource::Auto)` then insert into `known`.
  - Batch/rate-limit constants: `const BATCH_SIZE: usize = 5; const BATCH_WINDOW: Duration = Duration::from_secs(30); const MIN_LLM_INTERVAL: Duration = Duration::from_secs(30);`

- [ ] **Step 1: Write the failing test** (unit tests for the pure helpers — the async `run` loop is integration-verified manually; do NOT write a test that makes real LLM calls)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracted_strips_code_fences() {
        let raw = "```json\n[{\"heard\":\"next jazz\",\"canonical\":\"Next.js\"}]\n```";
        let got = parse_extracted(raw).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].heard, "next jazz");
        assert_eq!(got[0].canonical, "Next.js");
    }

    #[test]
    fn parse_extracted_empty_array_is_ok() {
        assert!(parse_extracted("[]").unwrap().is_empty());
    }

    #[test]
    fn build_prompt_includes_role_tech_and_transcripts() {
        let batch = vec![LearnTask {
            raw_transcript: "deploy to coober net ease".into(),
            role: Some("backend dev".into()),
            technologies: vec!["rust".into(), "kubernetes".into()],
        }];
        let p = build_extraction_prompt(&batch);
        assert!(p.contains("backend dev"));
        assert!(p.contains("kubernetes"));
        assert!(p.contains("coober net ease"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pie-engine --lib corrector::learner`
Expected: FAIL — module/functions not found.

- [ ] **Step 3: Write minimal implementation**

Create `src/corrector/learner.rs`:

```rust
//! Opt-in background learner: batches pipeline transcripts and mines new
//! pronunciation corrections via the configured LLM, rate-limited so it never
//! burns credits. OFF by default; spawned only when the user enables it.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::corrector::learned::{LearnedSource, LearnedStore};
use crate::llm::LlmRouter;
use crate::pipeline::engine::LearnTask;

const BATCH_SIZE: usize = 5;
const BATCH_WINDOW: Duration = Duration::from_secs(30);
const MIN_LLM_INTERVAL: Duration = Duration::from_secs(30);

/// One correction the LLM extracted from a batch of transcripts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedCorrection {
    pub heard: String,
    pub canonical: String,
}

/// Tolerant parse of an LLM JSON reply (handles ``` / ```json fences).
pub fn parse_extracted(json: &str) -> anyhow::Result<Vec<ExtractedCorrection>> {
    let cleaned = json
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    Ok(serde_json::from_str(cleaned)?)
}

/// Build the conservative extraction prompt for a batch.
pub fn build_extraction_prompt(batch: &[LearnTask]) -> String {
    let role = batch
        .iter()
        .find_map(|t| t.role.clone())
        .unwrap_or_else(|| "developer".to_string());
    let mut tech: Vec<String> = batch.iter().flat_map(|t| t.technologies.clone()).collect();
    tech.sort();
    tech.dedup();
    let transcripts: Vec<&str> = batch.iter().map(|t| t.raw_transcript.as_str()).collect();
    format!(
        "You are a technical vocabulary extractor. These are voice-to-text \
         transcripts from a {role} who works with: {tech}.\n\n\
         Find technical terms likely MISRECOGNIZED by speech-to-text (e.g. \
         'next jazz'='Next.js', 'coober net ease'='Kubernetes', 'engine x'='Nginx'). \
         Be conservative — only clear misrecognitions.\n\n\
         Transcripts:\n{joined}\n\n\
         Return ONLY a JSON array [{{\"heard\":\"what STT heard\",\"canonical\":\"correct term\"}}]. \
         Return [] if none.",
        role = role,
        tech = tech.join(", "),
        joined = transcripts.join("\n---\n"),
    )
}

/// The background learner task.
pub struct BackgroundLearner {
    rx: mpsc::Receiver<LearnTask>,
    llm: LlmRouter,
    learned_path: PathBuf,
    provider: String,
    model: Option<String>,
    known: HashSet<String>,
    last_llm: Option<Instant>,
}

impl BackgroundLearner {
    pub fn new(
        rx: mpsc::Receiver<LearnTask>,
        llm: LlmRouter,
        learned_path: PathBuf,
        provider: String,
        model: Option<String>,
        known: HashSet<String>,
    ) -> Self {
        Self { rx, llm, learned_path, provider, model, known, last_llm: None }
    }

    /// Run forever: batch, rate-limit, extract, persist new corrections.
    pub async fn run(mut self) {
        loop {
            let Some(batch) = self.collect_batch().await else { break; };
            if batch.is_empty() {
                continue;
            }
            // Rate limit.
            if let Some(prev) = self.last_llm {
                let since = prev.elapsed();
                if since < MIN_LLM_INTERVAL {
                    tokio::time::sleep(MIN_LLM_INTERVAL - since).await;
                }
            }
            self.last_llm = Some(Instant::now());
            let prompt = build_extraction_prompt(&batch);
            let reply = match self.llm.send(&prompt, &self.provider, self.model.as_deref()).await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("background learner LLM error: {e}");
                    continue;
                }
            };
            let terms = match parse_extracted(&reply) {
                Ok(t) => t,
                Err(e) => {
                    log::warn!("background learner parse error: {e}");
                    continue;
                }
            };
            if terms.is_empty() {
                continue;
            }
            let mut store = LearnedStore::load(self.learned_path.clone());
            for t in terms {
                let key = t.heard.trim().to_lowercase();
                if key.is_empty() || self.known.contains(&key) || store.has_entry(&key) {
                    continue;
                }
                if store.add_or_reinforce(&t.heard, &t.canonical, LearnedSource::Auto).is_ok() {
                    self.known.insert(key);
                }
            }
        }
    }

    /// Collect up to BATCH_SIZE tasks, or whatever arrived within BATCH_WINDOW
    /// of the first. Returns None when the channel is closed and drained.
    async fn collect_batch(&mut self) -> Option<Vec<LearnTask>> {
        let first = self.rx.recv().await?; // None => channel closed
        let mut batch = vec![first];
        let deadline = Instant::now() + BATCH_WINDOW;
        while batch.len() < BATCH_SIZE {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match timeout(remaining, self.rx.recv()).await {
                Ok(Some(task)) => batch.push(task),
                Ok(None) => break, // channel closed
                Err(_) => break,   // window elapsed
            }
        }
        Some(batch)
    }
}
```

Add `pub mod learner;` to `src/corrector/mod.rs`. Ensure `pub mod learned;` remains and the engine's `LearnTask` path (`crate::pipeline::engine::LearnTask`) is public.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pie-engine --lib corrector::learner`
Expected: PASS (3 tests). Then `cargo test -p pie-engine --lib` — everything green.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/corrector/learner.rs src/corrector/mod.rs
git commit -m "feat(corrector): opt-in background learner (batched, rate-limited)"
```

---

### Task 5: Settings flag, Tauri commands, learner spawn, UI

**Files:**
- Modify: `src-tauri/src/settings.rs` (add `background_mining: bool`)
- Modify: `src-tauri/src/main.rs` (commands + spawn)
- Modify: `ui/src/lib/VocabularySettings.svelte`

**Interfaces:**
- Consumes: `PieEngine::corrector_learned_count`, `corrector_reset_learned`, `set_learner_tx` + `LearnTask`, `BackgroundLearner` (Tasks 3-4); `LlmRouter::from_config`, `llm_config(&Settings)` helper (Phase 1).
- Produces: `#[tauri::command] get_learned_vocab_count -> usize`, `#[tauri::command] reset_learned_vocab -> Result<(), String>`; a `spawn_background_learner` setup step gated on `settings.background_mining`.

- [ ] **Step 1: Settings field + roundtrip test**

Add to `Settings` (next to `deep_correct_ai`):

```rust
    /// When true, run the opt-in background learner that mines new
    /// pronunciation corrections from transcripts via the configured LLM.
    pub background_mining: bool,
```

Default: `background_mining: false`. Add to the `partial_settings_fill_defaults` test an assertion `assert!(!loaded.background_mining);` OR a dedicated test — verify default is false and it roundtrips.

Run: `cargo test background_mining` (or the settings test name) — expected PASS after adding the field.

- [ ] **Step 2: Tauri commands**

Add near the other corrector commands (around `main.rs:700`):

```rust
#[tauri::command]
async fn get_learned_vocab_count(state: State<'_, AppState>) -> Result<usize, String> {
    let engine = state.engine.lock().await;
    Ok(engine.corrector_learned_count())
}

#[tauri::command]
async fn reset_learned_vocab(state: State<'_, AppState>) -> Result<(), String> {
    let mut engine = state.engine.lock().await;
    engine.corrector_reset_learned().map_err(|e| e.to_string())
}
```

Register both in `generate_handler![...]` next to `delete_correction`.

- [ ] **Step 3: Spawn the learner when mining is enabled**

In the `.setup(|app| { … })` closure, AFTER `AppState` is managed (so the engine exists) and settings are loaded, add — gated on the flag:

```rust
            if settings.background_mining {
                let (tx, rx) = tokio::sync::mpsc::channel::<pie_engine::pipeline::engine::LearnTask>(100);
                {
                    let state = app.state::<AppState>();
                    // Attach the sender to the engine so process() fires tasks.
                    tauri::async_runtime::block_on(async {
                        state.engine.lock().await.set_learner_tx(tx);
                    });
                }
                let llm = pie_engine::llm::LlmRouter::from_config(&llm_config(&settings));
                let learned_path = dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("pie")
                    .join("learned_vocab.json");
                let provider = settings.provider.clone();
                let model = (!settings.llm_model.is_empty()).then(|| settings.llm_model.clone());
                let known: std::collections::HashSet<String> = std::collections::HashSet::new();
                let learner = pie_engine::corrector::learner::BackgroundLearner::new(
                    rx, llm, learned_path, provider, model, known,
                );
                tauri::async_runtime::spawn(async move { learner.run().await });
            }
```

Confirm `pie_engine::corrector::learner::BackgroundLearner`, `pie_engine::pipeline::engine::LearnTask`, and `pie_engine::llm::LlmRouter` are all public paths (they are per Tasks 3-4 + Phase 1). If `settings` was already moved into `AppState` before this point, read the needed fields from `state.settings.lock()` instead, or clone the flags before the move. Keep ownership correct — do not use a moved `settings`.

- [ ] **Step 4: UI — learned count + reset + mining toggle**

In `ui/src/lib/VocabularySettings.svelte`, add above "Your corrections":
- A learned-vocab line: `{learnedCount} terms learned automatically` with a **Reset learned** button calling `invoke("reset_learned_vocab")` then refreshing the count via `invoke("get_learned_vocab_count")`.
- A **Background learning** checkbox bound to `settings.background_mining`, `onchange={onSave}`, with a caption noting it uses the configured LLM and takes effect on restart (the learner spawns at startup).

Load `learnedCount` in the existing `refresh()` (add `learnedCount = await invoke("get_learned_vocab_count")`). Reuse existing classes (`field`, `field-label`, `caption`, `btn sm`, `toggle-row`, `toggle-label`, `toggle-caption`).

- [ ] **Step 5: Build + verify**

Run: `cargo build -p pie-desktop` (clean), `cargo test -p pie-engine --lib` (all pass), `npm run build` from `ui/` (clean).

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine && cargo clippy -p pie-desktop
git add src-tauri/src/settings.rs src-tauri/src/main.rs ui/src/lib/VocabularySettings.svelte
git commit -m "feat(app): learned-vocab count/reset + background-mining toggle"
```

---

## Acceptance (Phase 2 spec)

- [ ] A firing dict correction reinforces its learned entry (seen_count/confidence rise) — Task 3.
- [ ] With mining OFF (default), no background LLM calls happen — Tasks 4/5 (learner only spawned when the flag is on).
- [ ] With mining ON: batch 5-or-30s, ≤1 LLM call/30s, appends to `learned_vocab.json`, only new `heard` keys — Task 4.
- [ ] Learned entries load on startup and merge at the AutoLearned/Synced tier — Task 2.
- [ ] Pipeline never blocks on the learner (`try_send`) — Task 3.
- [ ] `reset_learned_vocab` clears learned/synced without touching user/static — Tasks 2/5.
- [ ] UI shows learned count, a reset button, and the mining toggle — Task 5.

## Notes for later phases
- Phase 3 (sync) reuses `LearnedStore` + `add_synced_correction` (`Source::Synced`) — do not reinvent storage.
- The `known` set handed to the learner starts empty each launch; the learner also checks the on-disk store, so restarts won't duplicate. A future refinement could seed `known` from the current dict.
