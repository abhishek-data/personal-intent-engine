# Phase 5: Long-Conversation Refinement — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A `refine` optimization mode that compresses long, rambling voice input (>80 words) into a sharp prompt via the configured LLM, while short inputs pass through to the balanced optimizer unchanged. LLM failure falls back to the original text (never drops input).

**Architecture:** `src/optimizer/refine.rs` is pure and LLM-free: `refine::optimize` returns either a ready balanced `OptimizedPrompt` (≤80 words) or a `Refine` result carrying a deterministic base (the original text as fallback) plus a `RefineRequest` (the LLM instruction). The engine owns all LLM I/O: `process()` attaches the optional `RefineRequest` to `PieResult`, and a separate `apply_refine` method performs the LLM call — mirroring the existing `deep_correct` pattern where `process()` stays deterministic and the app orchestrates the LLM pass afterward.

**Tech Stack:** Rust (edition 2021), existing `LlmRouter`; Tauri v2; Svelte 5.

## Global Constraints

- Rust edition 2021. No `unwrap()` in library code (tests may). Doc comments on public items. `cargo fmt` + clippy clean (ignore pre-existing `phonetic.rs:37`, `nspanel.rs:116`). Test output pristine.
- `process()` stays deterministic (no LLM call inside it); the LLM refine pass is a separate `apply_refine` the app calls after `process()`, exactly like `deep_correct`.
- Refinement MUST preserve all technical terms and constraints (prompt instruction).
- LLM failure during refine falls back to the original input text — never drops or empties the user's input.
- Word threshold: **>80 words** triggers refinement; **≤80** delegates to `balanced::optimize`.
- Desktop crate `pie-desktop`; library `pie-engine`. CLI is `src/main.rs` (feature-gated).

---

### Task 1: `refine.rs` + `OptimizationMode::Refine` (pure, LLM-free)

**Files:**
- Modify: `src/optimizer/mod.rs` (add `pub mod refine;`, `OptimizationMode::Refine`)
- Create: `src/optimizer/refine.rs`

**Interfaces:**
- Consumes: `Intent` (has `raw_input: String`, `objective`, `constraints`, etc.), `MemoryStore` (`profile.role: Option<String>`, `profile.technologies: Vec<String>`), `balanced::optimize`.
- Produces:
  - `OptimizationMode::Refine` variant.
  - `pub struct RefineRequest { pub prompt: String }` (the LLM instruction to run).
  - `pub enum RefineResult { Balanced(OptimizedPrompt), Refine { base: OptimizedPrompt, request: RefineRequest } }`.
  - `pub const REFINE_WORD_THRESHOLD: usize = 80;`
  - `pub fn optimize(intent: &Intent, memory: &MemoryStore) -> RefineResult` — counts words in `intent.raw_input`; `<= threshold` → `Balanced(balanced::optimize(intent, memory))`; else → `Refine { base, request }` where `base` is an `OptimizedPrompt { text: intent.raw_input.clone(), mode: Refine, estimated_tokens: raw.len()/4, sections: vec![] }` (the fallback), and `request.prompt` is the compression instruction built from `intent.raw_input` + role/tech.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::IntentExtractor;
    use crate::memory::store::MemoryStore;

    fn intent_for(text: &str) -> crate::intent::Intent {
        IntentExtractor::new().extract(text)
    }

    #[test]
    fn short_input_delegates_to_balanced() {
        let intent = intent_for("build a rust cli that parses json");
        let mem = MemoryStore::default();
        match optimize(&intent, &mem) {
            RefineResult::Balanced(p) => assert_eq!(p.mode, OptimizationMode::Balanced),
            RefineResult::Refine { .. } => panic!("short input must not refine"),
        }
    }

    #[test]
    fn long_input_yields_refine_request_with_original_fallback() {
        // > 80 words
        let long = "so ".to_string() + &"word ".repeat(90);
        let intent = intent_for(&long);
        let mem = MemoryStore::default();
        match optimize(&intent, &mem) {
            RefineResult::Refine { base, request } => {
                assert_eq!(base.mode, OptimizationMode::Refine);
                assert_eq!(base.text, intent.raw_input, "fallback base is the original text");
                assert!(request.prompt.contains(&intent.raw_input), "prompt includes the input to compress");
                assert!(!request.prompt.is_empty());
            }
            RefineResult::Balanced(_) => panic!("long input must refine"),
        }
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p pie-engine --lib optimizer::refine`
Expected: FAIL — module/variant not found.

- [ ] **Step 3: Implement**

In `src/optimizer/mod.rs`: add `pub mod refine;` and add `Refine` to `OptimizationMode`:

```rust
pub enum OptimizationMode {
    Compact,
    Balanced,
    Enhanced,
    Adaptive,
    /// Compress long rambling input into a sharp prompt via the LLM.
    Refine,
}
```

Create `src/optimizer/refine.rs`:

```rust
//! Refine mode: compress long, rambling voice input into a sharp prompt.
//! Pure and LLM-free — for long inputs it returns a `RefineRequest` the engine
//! runs against the configured LLM; short inputs delegate to balanced mode.

use super::{balanced, OptimizationMode, OptimizedPrompt};
use crate::intent::Intent;
use crate::memory::store::MemoryStore;

/// Inputs longer than this many words are refined; shorter ones pass through.
pub const REFINE_WORD_THRESHOLD: usize = 80;

/// The LLM instruction to compress a long input.
pub struct RefineRequest {
    pub prompt: String,
}

/// Outcome of refine-mode optimization.
pub enum RefineResult {
    /// Short input — a ready balanced prompt, no LLM needed.
    Balanced(OptimizedPrompt),
    /// Long input — a deterministic fallback plus an LLM instruction.
    Refine {
        base: OptimizedPrompt,
        request: RefineRequest,
    },
}

/// Decide whether to refine. Counts words in `intent.raw_input`.
#[must_use]
pub fn optimize(intent: &Intent, memory: &MemoryStore) -> RefineResult {
    let word_count = intent.raw_input.split_whitespace().count();
    if word_count <= REFINE_WORD_THRESHOLD {
        return RefineResult::Balanced(balanced::optimize(intent, memory));
    }

    let role = memory.profile.role.as_deref().unwrap_or("developer");
    let tech = memory.profile.technologies.join(", ");
    let prompt = format!(
        "The user spoke this long voice request. Rewrite it as ONE clear, concise \
         prompt. Keep ALL technical terms, names, and constraints. Remove filler \
         (um, like, you know, so, basically) and deduplicate repeated ideas. \
         Output ONLY the refined prompt, nothing else.\n\n\
         User context: role={role}, tech={tech}.\n\n\
         User said:\n{text}",
        role = role,
        tech = tech,
        text = intent.raw_input,
    );

    let base = OptimizedPrompt {
        text: intent.raw_input.clone(),
        mode: OptimizationMode::Refine,
        estimated_tokens: intent.raw_input.len() / 4,
        sections: Vec::new(),
    };
    RefineResult::Refine {
        base,
        request: RefineRequest { prompt },
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p pie-engine --lib optimizer::refine` then `cargo test -p pie-engine --lib`.
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/optimizer/mod.rs src/optimizer/refine.rs
git commit -m "feat(optimizer): refine mode — word-gated LLM compression request"
```

---

### Task 2: Engine — `PieResult.refine_request`, `process()` dispatch, `apply_refine`

**Files:**
- Modify: `src/pipeline/engine.rs`

**Interfaces:**
- Consumes: `refine::{optimize, RefineResult, RefineRequest}` (Task 1).
- Produces:
  - `PieResult` gains `pub refine_request: Option<crate::optimizer::refine::RefineRequest>` (None for all non-refine modes).
  - `process()` maps `mode == "refine"` → `OptimizationMode::Refine`, calls `refine::optimize`, and: `Balanced(p)` → use `p`, `refine_request = None`; `Refine { base, request }` → use `base`, `refine_request = Some(request)`.
  - `pub async fn apply_refine(&self, request: &RefineRequest, original: &str, provider: &str, model: Option<&str>) -> String` — `self.llm.send(&request.prompt, provider, model)`; on Ok, trimmed result (if non-empty), else `original`; on Err, `original`.

- [ ] **Step 1: Write the failing test** (engine tests)

```rust
    #[tokio::test]
    async fn refine_mode_attaches_request_for_long_input_and_falls_back() {
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let cpath = dir.join(format!("pie-rf-user-{uid}.json"));
        let lpath = dir.join(format!("pie-rf-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        let long = "so ".to_string() + &"refactor the widget ".repeat(30); // > 80 words
        let res = engine.process(&long, "refine").await.unwrap();
        assert!(res.refine_request.is_some(), "long input attaches a refine request");
        // echo provider returns the prompt; apply_refine returns it trimmed (non-empty) -> not the fallback.
        let req = res.refine_request.as_ref().unwrap();
        let refined = engine.apply_refine(req, &res.optimized_prompt, "echo", None).await;
        assert!(!refined.is_empty());
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }

    #[tokio::test]
    async fn refine_mode_short_input_no_request() {
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let cpath = dir.join(format!("pie-rf2-user-{uid}.json"));
        let lpath = dir.join(format!("pie-rf2-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        let res = engine.process("build a rust cli", "refine").await.unwrap();
        assert!(res.refine_request.is_none(), "short input needs no refine");
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p pie-engine --lib pipeline::engine` → FAIL (field/method/variant missing).

- [ ] **Step 3: Implement**

1. Add to `PieResult`: `pub refine_request: Option<crate::optimizer::refine::RefineRequest>,`. Update every `PieResult { .. }` construction in `process()` to set it (default `None`, set for refine).
2. In `process()`, add `"refine" => OptimizationMode::Refine` to the mode match, and handle it in the optimize match:

```rust
        use crate::optimizer::refine::{self, RefineResult};
        let mut refine_request = None;
        let optimized = match optimization_mode {
            OptimizationMode::Compact => compact::optimize(&intent, &self.memory),
            OptimizationMode::Balanced => balanced::optimize(&intent, &self.memory),
            OptimizationMode::Enhanced => enhanced::optimize(&intent, &self.memory),
            OptimizationMode::Adaptive => adaptive::optimize(&intent, &self.memory),
            OptimizationMode::Refine => match refine::optimize(&intent, &self.memory) {
                RefineResult::Balanced(p) => p,
                RefineResult::Refine { base, request } => {
                    refine_request = Some(request);
                    base
                }
            },
        };
```

Then include `refine_request` in the returned `PieResult`.

3. Add the method:

```rust
    /// Run the refine LLM pass. Returns the compressed prompt on success, or
    /// `original` on any LLM failure or empty reply (never drops input).
    pub async fn apply_refine(
        &self,
        request: &crate::optimizer::refine::RefineRequest,
        original: &str,
        provider: &str,
        model: Option<&str>,
    ) -> String {
        match self.llm.send(&request.prompt, provider, model).await {
            Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => original.to_string(),
        }
    }
```

- [ ] **Step 4: Run to verify pass** — `cargo test -p pie-engine --lib` all green.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine
git add src/pipeline/engine.rs
git commit -m "feat(engine): refine mode dispatch + apply_refine LLM pass"
```

---

### Task 3: Wire refine into desktop app + CLI

**Files:**
- Modify: `src-tauri/src/main.rs`
- Modify: `src/main.rs` (CLI)

**Interfaces:**
- Consumes: `PieResult.refine_request`, `engine.apply_refine` (Task 2); existing `model_opt(&Settings)`.

- [ ] **Step 1: Desktop — `transcribe_and_process`**

In `src-tauri/src/main.rs`, after `engine.process(&transcript, &settings.mode)` returns `result` (around main.rs:201), and while the engine lock is held, apply refine when requested:

```rust
    let mut result = /* existing process() result */;
    if let Some(req) = result.refine_request.take() {
        let refined = engine
            .apply_refine(&req, &result.optimized_prompt, &settings.provider, model_opt(&settings))
            .await;
        result.optimized_prompt = refined;
    }
```

Confirm the engine binding is a mutable/locked reference in scope and that `result` is declared `mut`. (The existing `deep_correct` pass runs after this; ordering: refine the prompt, deep_correct is about the transcript — keep both, they're independent.) If `result.refine_request` needs `PieResult` to derive nothing special, fine — it holds a `RefineRequest` (no Serialize needed; it never crosses the Tauri boundary — only `optimized_prompt` does).

- [ ] **Step 2: CLI — `src/main.rs`**

Find where the CLI calls `engine.process(...)` and prints `optimized_prompt`. After process, apply refine the same way, using the CLI's provider/model args (mirror how the CLI already selects provider for any LLM send; if the CLI has no provider concept, use `"echo"` when none and the configured provider otherwise — match existing CLI patterns). Ensure `pie --mode refine "<long text>"` produces a compressed prompt (or the original on echo/failure).

- [ ] **Step 3: Build + verify**

Run: `cargo build -p pie-desktop` clean; `cargo build` (CLI, with default features) clean; `cargo test -p pie-engine --lib` all pass; clippy no new warnings.

- [ ] **Step 4: Commit**

```bash
cargo fmt && cargo clippy -p pie-engine && cargo clippy -p pie-desktop
git add src-tauri/src/main.rs src/main.rs
git commit -m "feat(app): apply refine pass after process in desktop + CLI"
```

---

### Task 4: UI — add `refine` mode option

**Files:**
- Modify: `ui/src/lib/TranscriptionSettings.svelte`

**Interfaces:** none new — just adds `refine` to the mode selector.

- [ ] **Step 1: Add the mode**

In `ui/src/lib/TranscriptionSettings.svelte`, change `const MODES = ["compact", "balanced", "enhanced", "adaptive"];` to include `"refine"`. Add a short caption near the Optimization selector noting refine compresses long dictation via the LLM (uses your provider; long inputs only).

- [ ] **Step 2: Build** — `npm run build` from `ui/` clean.

- [ ] **Step 3: Manual smoke test** — with a real LLM configured, set mode = refine, dictate a long rambling request, confirm the pasted/prompt output is a compressed version; set a short input and confirm it's unchanged (balanced).

- [ ] **Step 4: Commit**

```bash
git add ui/src/lib/TranscriptionSettings.svelte
git commit -m "feat(ui): add refine optimization mode"
```

---

## Acceptance (Phase 5 spec)

- [ ] Inputs >80 words trigger refinement (explicit `refine` mode) — Tasks 1/2.
- [ ] Refinement strips filler, dedupes, preserves technical terms/constraints (prompt instruction) — Task 1.
- [ ] ≤80-word inputs pass through unchanged (balanced) — Tasks 1/2.
- [ ] LLM failure falls back to the original text (never drops input) — Task 2 (`apply_refine`).
- [ ] `Refine` available as explicit `pie --mode refine` and in the UI mode selector — Tasks 3/4.

## Scope note
Adaptive auto-selecting refine for long inputs is intentionally NOT included: it would make the default path sometimes call the LLM unpredictably. Refine is opt-in via explicit mode. (Deferred; can revisit.)
