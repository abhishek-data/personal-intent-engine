# Phase 3: Vocabulary Bootstrap (Import) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Let the user bootstrap PIE's vocabulary by importing their existing conversations — a folder or file they choose (`.md`/`.txt`/`.json`), plus best-effort auto-detection of Cursor's local SQLite — extracting technical terms via the configured LLM and storing them as `Source::Synced` in the existing `learned_vocab.json`. User-initiated only; never auto-runs.

**Architecture:** A new `src/corrector/sync.rs` provides pure, testable text extraction (`extract_texts`) over directories / JSON / text / SQLite, an LLM extraction prompt+parse, and a `VocabularySync::run` that batches conversations, calls the LLM, and applies each term via the corrector's existing `add_synced_correction` (Phase 2). Reuses `LearnedStore`/`Source::Synced` — no new storage. The engine gains a `run_vocabulary_sync` method (it owns the corrector + LLM); Tauri commands drive discovery, a native file picker (`rfd`), and the sync run with progress events. A `sync_state.json` records the last run.

**Tech Stack:** Rust (edition 2021), rusqlite (bundled, already a dep), reqwest/serde_json, tokio; Tauri v2 + `rfd` (native file dialog); Svelte 5 (runes).

## Global Constraints

- Rust edition 2021. No `unwrap()` in library code (tests may). Doc comments on public items. `cargo fmt` + clippy clean (ignore pre-existing `phonetic.rs:37`, `nspanel.rs:116`). Test output pristine.
- **User-initiated only.** Sync NEVER runs automatically; it requires an explicit command triggered by a button.
- Imported terms are stored `Source::Synced` via `add_synced_correction` in `learned_vocab.json` (Phase 2). Do NOT create a parallel store.
- All processing local; conversation text is sent only to the user's own configured LLM endpoint. Never log conversation contents or the API key.
- Sources with no parseable content report **0 conversations** — never an error, never a crash.
- Desktop crate is `pie-desktop`; library is `pie-engine`.
- Tests use synthetic fixtures in temp dirs (unique paths); never touch real user data or real Cursor/ChatGPT files.

---

### Task 1: `sync.rs` — text extraction from folder / JSON / text

**Files:**
- Create: `src/corrector/sync.rs`
- Modify: `src/corrector/mod.rs` (`pub mod sync;`)

**Interfaces:**
- Produces:
  - `pub fn extract_texts(path: &std::path::Path) -> Vec<String>` — dispatch on the path:
    - directory → recursively walk (bounded depth 8), read every `.md`/`.txt`/`.json` file, extracting one `String` per file (for `.json`, harvest all string values via `harvest_json_strings`; for `.md`/`.txt`, the raw contents). Unreadable files are skipped.
    - a `.json` file → `harvest_json_strings` of its parsed value (one `String`).
    - a `.md`/`.txt` file → its raw contents (one `String`).
    - anything else / unreadable → empty `Vec`.
  - `pub fn harvest_json_strings(v: &serde_json::Value) -> String` — recursively collect every JSON string value (objects' values + array elements), joined by `\n`. This generically captures ChatGPT `conversations.json` message text without hard-coding its schema.
  - Private `fn walk_files(dir, exts, out, depth)` helper.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);
    fn temp_dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "pie-sync-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn harvest_json_strings_collects_nested_values() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"a":"deploy to kubernetes","b":{"c":["next.js","nginx"]},"n":42}"#,
        )
        .unwrap();
        let got = harvest_json_strings(&v);
        assert!(got.contains("deploy to kubernetes"));
        assert!(got.contains("next.js"));
        assert!(got.contains("nginx"));
        assert!(!got.contains("42"), "numbers are not harvested");
    }

    #[test]
    fn extract_texts_reads_dir_of_md_txt_json() {
        let d = temp_dir();
        std::fs::write(d.join("a.md"), "I use Terraform daily").unwrap();
        std::fs::write(d.join("b.txt"), "spin up on AWS").unwrap();
        std::fs::write(d.join("c.json"), r#"{"msg":"scale with Kubernetes"}"#).unwrap();
        std::fs::write(d.join("ignore.png"), b"\x89PNG").unwrap();
        let mut texts = extract_texts(&d);
        texts.sort();
        assert_eq!(texts.len(), 3, "3 recognized files, png ignored");
        assert!(texts.iter().any(|t| t.contains("Terraform")));
        assert!(texts.iter().any(|t| t.contains("Kubernetes")));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn extract_texts_single_json_file() {
        let d = temp_dir();
        let f = d.join("conversations.json");
        std::fs::write(&f, r#"[{"content":"deploy nextjs on vercel"}]"#).unwrap();
        let texts = extract_texts(&f);
        assert_eq!(texts.len(), 1);
        assert!(texts[0].contains("nextjs"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn extract_texts_missing_path_is_empty() {
        assert!(extract_texts(std::path::Path::new("/no/such/path/xyz")).is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pie-engine --lib corrector::sync`
Expected: FAIL — module/functions not found.

- [ ] **Step 3: Write minimal implementation**

Create `src/corrector/sync.rs`:

```rust
//! Vocabulary bootstrap: extract technical terms from the user's existing
//! conversations (a chosen folder/file, or an auto-detected local source) via
//! the configured LLM, storing them as `Source::Synced`. User-initiated only.

use std::path::Path;

use serde_json::Value;

/// Recursively collect every JSON string value, newline-joined. Generically
/// captures message text from exports like ChatGPT's `conversations.json`
/// without hard-coding a schema.
pub fn harvest_json_strings(v: &Value) -> String {
    let mut out = Vec::new();
    harvest_into(v, &mut out);
    out.join("\n")
}

fn harvest_into(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|e| harvest_into(e, out)),
        Value::Object(o) => o.values().for_each(|e| harvest_into(e, out)),
        _ => {}
    }
}

const TEXT_EXTS: [&str; 4] = ["md", "txt", "json", "markdown"];

/// Extract conversation-ish text blocks from a chosen path (folder or file).
/// Unreadable/unknown inputs yield an empty Vec — never an error.
pub fn extract_texts(path: &Path) -> Vec<String> {
    if path.is_dir() {
        let mut out = Vec::new();
        walk_files(path, &mut out, 0);
        out
    } else if path.is_file() {
        extract_one(path).into_iter().collect()
    } else {
        Vec::new()
    }
}

fn ext_lower(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_lowercase())
}

fn extract_one(path: &Path) -> Option<String> {
    let ext = ext_lower(path)?;
    if !TEXT_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    if ext == "json" {
        let v: Value = serde_json::from_str(&raw).ok()?;
        Some(harvest_json_strings(&v))
    } else {
        Some(raw)
    }
}

fn walk_files(dir: &Path, out: &mut Vec<String>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_files(&p, out, depth + 1);
        } else if let Some(text) = extract_one(&p) {
            if !text.trim().is_empty() {
                out.push(text);
            }
        }
    }
}
```

Add `pub mod sync;` to `src/corrector/mod.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pie-engine --lib corrector::sync`
Expected: PASS (4 tests). Then `cargo test -p pie-engine --lib` all green.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/corrector/sync.rs src/corrector/mod.rs
git commit -m "feat(sync): text extraction from folder/json/text files"
```

---

### Task 2: `sync.rs` — Cursor SQLite (best-effort), LLM extraction, `run`, sync_state

**Files:**
- Modify: `src/corrector/sync.rs`

**Interfaces:**
- Consumes: `extract_texts`/`harvest_json_strings` (Task 1); `crate::llm::LlmRouter`; rusqlite (already a dep).
- Produces:
  - `pub struct SyncedTerm { pub term: String, pub variants: Vec<String> }` (`Serialize, Deserialize`).
  - `pub fn parse_synced_terms(reply: &str) -> anyhow::Result<Vec<SyncedTerm>>` — fence-tolerant JSON parse.
  - `pub fn build_sync_prompt(texts: &[String]) -> String` — conservative: "extract technical terms, product/library names, proper nouns a developer speaks; include mispronunciation variants; JSON `[{"term","variants":[...]}]`."
  - `pub fn extract_cursor_texts(vscdb: &Path) -> Vec<String>` — best-effort: open the SQLite file read-only, `SELECT value FROM ItemTable`, `harvest_json_strings` over any value that parses as JSON (else the raw string). Any error (missing table, bad file) → empty Vec. Documented as best-effort/fragile.
  - `pub struct SyncResult { pub conversations: usize, pub terms_added: usize }` (`Serialize`).
  - `pub struct SyncState { pub last_run_unix: u64, pub terms_added: usize }` with `load(path)`/`save(path)` (tolerant like `LearnedStore`).
  - `pub async fn run_sync<F>(paths: &[PathBuf], llm: &LlmRouter, provider: &str, model: Option<&str>, mut add: F, on_progress: impl Fn(usize, usize)) -> anyhow::Result<SyncResult>` where `F: FnMut(&str, &str) -> anyhow::Result<()>` — for each path, `extract_texts` (or `extract_cursor_texts` for `.vscdb`/`.sqlite`), batch 10 texts/LLM call, parse terms, and for each `(variant → term)` call `add(variant, term)`; emit progress `(done, total)`. The `add` closure is how the engine wires `add_synced_correction` without `sync.rs` depending on the corrector.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn parse_synced_terms_tolerates_fences() {
        let raw = "```json\n[{\"term\":\"Next.js\",\"variants\":[\"nextjs\",\"next jazz\"]}]\n```";
        let got = parse_synced_terms(raw).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].term, "Next.js");
        assert_eq!(got[0].variants.len(), 2);
    }

    #[test]
    fn build_sync_prompt_includes_texts() {
        let p = build_sync_prompt(&["deploy to coober net ease".to_string()]);
        assert!(p.contains("coober net ease"));
        assert!(p.to_lowercase().contains("json"));
    }

    #[test]
    fn extract_cursor_texts_reads_itemtable() {
        let d = temp_dir();
        let db = d.join("state.vscdb");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute("CREATE TABLE ItemTable (key TEXT, value TEXT)", []).unwrap();
            conn.execute(
                "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                rusqlite::params!["chat", r#"{"messages":["how do I deploy nextjs"]}"#],
            )
            .unwrap();
        }
        let texts = extract_cursor_texts(&db);
        assert!(texts.iter().any(|t| t.contains("nextjs")), "reads ItemTable JSON values");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn extract_cursor_texts_missing_table_is_empty() {
        let d = temp_dir();
        let db = d.join("empty.vscdb");
        { let _ = rusqlite::Connection::open(&db).unwrap(); } // no ItemTable
        assert!(extract_cursor_texts(&db).is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    #[tokio::test]
    async fn run_sync_applies_terms_via_closure() {
        // echo provider: LlmRouter with no client + provider "echo" returns the
        // prompt, which parse_synced_terms will fail on -> 0 terms. So instead
        // test the wiring: a dir with one file, and assert progress + no panic.
        let d = temp_dir();
        std::fs::write(d.join("a.md"), "deploy nextjs").unwrap();
        let llm = crate::llm::LlmRouter::new(); // echo-capable; "echo" provider
        let mut added = 0usize;
        let res = run_sync(
            &[d.clone()],
            &llm,
            "echo",
            None,
            |_v, _c| { added += 1; Ok(()) },
            |_done, _total| {},
        )
        .await
        .unwrap();
        // echo returns non-JSON, so parse yields nothing; conversations counted.
        assert_eq!(res.conversations, 1);
        let _ = std::fs::remove_dir_all(d);
    }
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p pie-engine --lib corrector::sync`
Expected: FAIL — new items not found.

- [ ] **Step 3: Implement**

Add to `src/corrector/sync.rs` (imports: `use std::path::PathBuf; use serde::{Deserialize, Serialize}; use crate::llm::LlmRouter; use std::time::{SystemTime, UNIX_EPOCH};`):

```rust
/// A term plus spoken/misrecognized variants, as returned by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedTerm {
    pub term: String,
    pub variants: Vec<String>,
}

/// Result of a sync run.
#[derive(Debug, Clone, Serialize)]
pub struct SyncResult {
    pub conversations: usize,
    pub terms_added: usize,
}

/// Persistent record of the last sync.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncState {
    pub last_run_unix: u64,
    pub terms_added: usize,
}

impl SyncState {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Fence-tolerant parse of the LLM's term list.
pub fn parse_synced_terms(reply: &str) -> anyhow::Result<Vec<SyncedTerm>> {
    let cleaned = reply
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    Ok(serde_json::from_str(cleaned)?)
}

/// Conservative extraction prompt for a batch of conversation texts.
pub fn build_sync_prompt(texts: &[String]) -> String {
    format!(
        "Extract technical terms, product names, library names, and proper nouns \
         a developer would SPEAK into a microphone, from these conversation \
         excerpts. Include likely speech-to-text misrecognition variants.\n\n\
         Excerpts:\n{}\n\n\
         Return ONLY JSON: [{{\"term\":\"Next.js\",\"variants\":[\"nextjs\",\"next js\",\"next jazz\"]}}]. \
         Return [] if none.",
        texts.join("\n---\n"),
    )
}

/// Best-effort read of Cursor's `state.vscdb` ItemTable. Fragile by nature
/// (undocumented schema); any failure yields an empty Vec.
pub fn extract_cursor_texts(vscdb: &Path) -> Vec<String> {
    let Ok(conn) = rusqlite::Connection::open_with_flags(
        vscdb,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare("SELECT value FROM ItemTable") else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for v in rows.flatten() {
        match serde_json::from_str::<Value>(&v) {
            Ok(json) => {
                let s = harvest_json_strings(&json);
                if !s.trim().is_empty() {
                    out.push(s);
                }
            }
            Err(_) => out.push(v),
        }
    }
    out
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Run a sync over the given paths. `add(variant, canonical)` applies each
/// mapping (the engine wires it to `add_synced_correction`). Progress is
/// reported as (conversations_done, conversations_total).
pub async fn run_sync<F>(
    paths: &[PathBuf],
    llm: &LlmRouter,
    provider: &str,
    model: Option<&str>,
    mut add: F,
    on_progress: impl Fn(usize, usize),
) -> anyhow::Result<SyncResult>
where
    F: FnMut(&str, &str) -> anyhow::Result<()>,
{
    // Gather all texts first (for an accurate total).
    let mut texts: Vec<String> = Vec::new();
    for p in paths {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase());
        if matches!(ext.as_deref(), Some("vscdb") | Some("sqlite") | Some("db")) {
            texts.extend(extract_cursor_texts(p));
        } else {
            texts.extend(extract_texts(p));
        }
    }
    let total = texts.len();
    let mut done = 0usize;
    let mut terms_added = 0usize;

    for batch in texts.chunks(10) {
        let prompt = build_sync_prompt(batch);
        if let Ok(reply) = llm.send(&prompt, provider, model).await {
            if let Ok(terms) = parse_synced_terms(&reply) {
                for t in terms {
                    for variant in &t.variants {
                        if add(variant, &t.term).is_ok() {
                            terms_added += 1;
                        }
                    }
                }
            }
        }
        done += batch.len();
        on_progress(done, total);
    }

    Ok(SyncResult { conversations: total, terms_added })
}

/// Default path for the sync-state record.
pub fn default_sync_state_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pie")
        .join("sync_state.json")
}

/// Convenience: write a completed sync's state.
pub fn record_sync_state(terms_added: usize) -> anyhow::Result<()> {
    let state = SyncState { last_run_unix: now_unix(), terms_added };
    state.save(&default_sync_state_path())
}
```

Ensure `use serde_json::Value;` is present (Task 1 uses it).

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p pie-engine --lib corrector::sync` then `cargo test -p pie-engine --lib`.
Expected: PASS (all).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/corrector/sync.rs
git commit -m "feat(sync): LLM extraction, Cursor sqlite best-effort, run + state"
```

---

### Task 3: Engine method + Tauri commands + `rfd` picker

**Files:**
- Modify: `src/pipeline/engine.rs` (add `run_vocabulary_sync`)
- Modify: `src-tauri/Cargo.toml` (add `rfd`)
- Modify: `src-tauri/src/main.rs` (commands)

**Interfaces:**
- Produces on `PieEngine`:
  - `pub async fn run_vocabulary_sync(&mut self, paths: Vec<PathBuf>, on_progress: impl Fn(usize, usize)) -> anyhow::Result<crate::corrector::sync::SyncResult>` — calls `sync::run_sync` with `self.llm`, the current provider/model, and an `add` closure `|v, c| self.corrector.add_synced_correction(v, c)`. NOTE the borrow: gather into a local, then apply. Because `run_sync` borrows `self.corrector` mutably in the closure AND `self.llm` immutably, restructure: have `run_sync` collect `(variant, canonical)` pairs to apply (return them or apply via closure that captures only the corrector). Simplest: change the engine method to (a) call a variant of run that returns `Vec<(String,String)>` plus SyncResult without applying, then (b) apply them via `self.corrector.add_synced_correction`. To avoid double-borrow, DON'T pass a closure capturing `self.corrector` while also borrowing `self.llm`. Implement the engine method as: collect texts→llm→terms into pairs using only `&self.llm`, then loop `self.corrector.add_synced_correction`. Reuse `sync::run_sync` by passing an `add` closure that pushes into a local `Vec` (captures the Vec, not the corrector), then apply the Vec to the corrector afterward. Record sync state.
  - The engine needs provider/model: read from a stored config or accept them as params. Add params: `pub async fn run_vocabulary_sync(&mut self, paths, provider: &str, model: Option<&str>, on_progress)`.
- Tauri commands in `main.rs`:
  - `pick_import_target() -> Result<Option<String>, String>` — uses `rfd::FileDialog` (or `AsyncFileDialog`) to pick a folder OR file; returns the path string or `None` if cancelled. (Offer both: two commands `pick_import_folder` and `pick_import_file`, or one with a `folder: bool` arg. Use one command `pick_import_target(folder: bool)`.)
  - `discover_sync_sources() -> Vec<SyncSourceDto>` — returns auto-detected sources: check for Cursor `~/Library/Application Support/Cursor/User/globalStorage/state.vscdb` (macOS) / `~/.config/Cursor/User/globalStorage/state.vscdb` (Linux); include it if it exists. (Always also surface the manual-import option in the UI, not here.)
  - `run_vocabulary_sync(app, state, paths: Vec<String>) -> Result<SyncResultDto, String>` — locks the engine, reads provider/model from settings, calls `engine.run_vocabulary_sync`, emits `sync-progress` `(done,total)` via `app.emit`, records sync state.
  - `get_sync_state() -> SyncStateDto` — reads `sync_state.json`.
- `src-tauri/Cargo.toml`: add `rfd = "0.15"` (native file dialog).

- [ ] **Step 1: Add `rfd` dependency**

In `src-tauri/Cargo.toml` `[dependencies]`, add:

```toml
# Native file/folder picker for vocabulary import.
rfd = "0.15"
```

Run `cargo build -p pie-desktop` to confirm it resolves.

- [ ] **Step 2: Engine method (TDD)**

Write a test in `engine.rs` tests that syncs a temp dir with the echo provider and asserts `conversations` count and no panic (echo → 0 terms added):

```rust
    #[tokio::test]
    async fn run_vocabulary_sync_counts_conversations_echo() {
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let src = dir.join(format!("pie-syncsrc-{uid}"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.md"), "deploy nextjs on vercel").unwrap();
        let cpath = dir.join(format!("pie-eng-user-{uid}.json"));
        let lpath = dir.join(format!("pie-eng-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        let res = engine
            .run_vocabulary_sync(vec![src.clone()], "echo", None, |_d, _t| {})
            .await
            .unwrap();
        assert_eq!(res.conversations, 1);
        let _ = std::fs::remove_dir_all(src);
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }
```

Implement `run_vocabulary_sync` on `PieEngine` (borrow-safe: collect pairs with a Vec-capturing closure, then apply):

```rust
    /// Import vocabulary from the given paths via the configured LLM, storing
    /// terms as `Source::Synced`. User-initiated only.
    pub async fn run_vocabulary_sync(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        provider: &str,
        model: Option<&str>,
        on_progress: impl Fn(usize, usize),
    ) -> anyhow::Result<crate::corrector::sync::SyncResult> {
        let mut pairs: Vec<(String, String)> = Vec::new();
        let result = crate::corrector::sync::run_sync(
            &paths,
            &self.llm,
            provider,
            model,
            |variant, canonical| {
                pairs.push((variant.to_string(), canonical.to_string()));
                Ok(())
            },
            on_progress,
        )
        .await?;
        for (variant, canonical) in &pairs {
            let _ = self.corrector.add_synced_correction(variant, canonical);
        }
        let _ = crate::corrector::sync::record_sync_state(result.terms_added);
        Ok(result)
    }
```

Run: `cargo test -p pie-engine --lib pipeline::engine` — PASS.

- [ ] **Step 3: Tauri commands + registration**

Add DTOs (`#[derive(Serialize)] struct SyncSourceDto { name: String, path: String }`, `SyncResultDto`, `SyncStateDto`) and commands in `main.rs`. `pick_import_target`:

```rust
#[tauri::command]
async fn pick_import_target(folder: bool) -> Result<Option<String>, String> {
    let picked = if folder {
        rfd::FileDialog::new().pick_folder()
    } else {
        rfd::FileDialog::new()
            .add_filter("conversations", &["json", "md", "txt", "markdown"])
            .pick_file()
    };
    Ok(picked.map(|p| p.to_string_lossy().to_string()))
}
```

`discover_sync_sources` (Cursor detection), `run_vocabulary_sync` (locks engine, reads provider/model via existing `model_opt`, emits `sync-progress`), `get_sync_state`. Register all four in `generate_handler!`.

- [ ] **Step 4: Build + verify**

Run: `cargo build -p pie-desktop` clean; `cargo test -p pie-engine --lib` all pass; clippy no new warnings.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine && cargo clippy -p pie-desktop
git add src/pipeline/engine.rs src-tauri/Cargo.toml src-tauri/src/main.rs Cargo.lock
git commit -m "feat(app): vocabulary-sync engine method + Tauri commands + rfd picker"
```

---

### Task 4: `VocabularySync.svelte` UI + mount

**Files:**
- Create: `ui/src/lib/VocabularySync.svelte`
- Modify: `ui/src/App.svelte`

**Interfaces:**
- Consumes: `discover_sync_sources`, `pick_import_target`, `run_vocabulary_sync`, `get_sync_state`, and the `sync-progress` event.

- [ ] **Step 1: Read a sibling** for the runes + class contract (`ui/src/lib/VocabularySettings.svelte`).

- [ ] **Step 2: Create `VocabularySync.svelte`**

Svelte 5 runes. Content:
- A "Sync Your Vocabulary" section with a caption that all processing is local and text is sent only to the configured LLM.
- Auto-detected sources list from `discover_sync_sources` (e.g. "✓ Cursor").
- Two buttons: "Choose folder…" (`pick_import_target(true)`) and "Choose file…" (`pick_import_target(false)`); picked paths accumulate into a `paths` array shown as a list with remove.
- "Sync Now" button → `run_vocabulary_sync({ paths })`; disabled while syncing.
- A progress bar driven by the `sync-progress` event (`listen` in `onMount`), and a result line ("Imported N terms from M conversations").
- "Last synced" from `get_sync_state`.
Reuse existing classes only.

- [ ] **Step 3: Mount in `App.svelte`** next to `VocabularySettings` (import + render in the settings layout).

- [ ] **Step 4: Build** — `npm run build` from `ui/` clean.

- [ ] **Step 5: Manual smoke test** — run the app, choose a folder of markdown, Sync Now with a real LLM configured, confirm progress + imported count, and that terms then correct in transcripts.

- [ ] **Step 6: Commit**

```bash
git add ui/src/lib/VocabularySync.svelte ui/src/App.svelte
git commit -m "feat(ui): vocabulary import (sync) section"
```

---

## Acceptance (Phase 3 spec)

- [ ] Cursor local history is auto-detected when present (best-effort) — Tasks 2/3.
- [ ] User can point PIE at a ChatGPT/Claude export (json) or a folder of md/txt and import it — Tasks 1/3/4.
- [ ] Sources with no parseable content report 0, no error/crash — Tasks 1/2.
- [ ] Extraction batches 10 conversations/LLM call; progress events reach the frontend — Tasks 2/3.
- [ ] Synced entries stored `Source::Synced` in `learned_vocab.json` — Task 3 (via `add_synced_correction`).
- [ ] `sync_state.json` records the last run — Tasks 2/3.
- [ ] Import runs only on explicit user action — Tasks 3/4 (button-triggered command).

## Known limitations (be honest in code comments)
- ChatGPT/Claude conversations are server-side; the reliable path is a user-chosen export/folder, not auto-scan. Only Cursor is auto-detected, and its `state.vscdb` schema is undocumented — `extract_cursor_texts` is best-effort and returns 0 on any mismatch.
- `.zip` exports are not parsed directly; the user unzips and points at the folder or `conversations.json`.
