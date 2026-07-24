# PIE v3 — BYOK, Auto-Learning & Enhancements — Design

> Date: 2026-07-24
> Supersedes the sketches in `BRAINSTORM_V3_AUTOLEARN_BYOK.md` and
> `IMPLEMENTATION_GUIDE.md` (both kept for history; this is the authoritative
> spec). Status: approved design, ready for implementation planning.

## Problem

PIE works but has three usability gaps and three missing enhancements:

1. **No BYOK LLM config.** `LlmRouter` reads `OPENAI_API_KEY` / `OPENAI_BASE_URL`
   from env only. A user cannot plug in OpenRouter, a local Ollama, etc. from the
   UI — they must set env vars by hand.
2. **Learning is manual and tedious.** Corrections are added one-by-one. PIE never
   learns from what the user actually does.
3. **Cold start.** On first install PIE knows nothing about the user's vocabulary.
4. **Single output only.** One hotkey; no fast path for raw-vs-optimized paste.
5. **Long rambling voice input** is pasted verbatim instead of compressed.
6. **Spoken code** ("console dot log", "triple equals") is transcribed literally.

This spec delivers all six as one coherent set of changes, built in phase order.
Phase 1 is the foundation; 2/3/5 depend on it; 4/6 are independent.

## Decisions locked in

| # | Decision | Choice | Why |
|---|----------|--------|-----|
| 1 | LLM config | **BYOK from settings UI**, env vars as fallback | Makes PIE usable without touching env; CLI `--provider` path still works. |
| 2 | Auto-learn trigger | **Edit-based, always-on; speculative background mining behind an off-by-default toggle** | Learning from real signals (user edits, dict hits) is high-signal and near-zero cost. Continuous LLM mining is speculative token spend and ships idle speech to the LLM, so it is opt-in only. |
| 3 | Vocab bootstrap | **Import an export file/folder + auto-detect Cursor's local SQLite** | Claude Desktop and ChatGPT keep history server-side — a folder scan finds nothing. Cursor genuinely stores history locally, so we auto-detect it; everything else is an explicit user-chosen import. Honest and reliable. |
| 4 | Hotkeys | **Two global hotkeys** (raw paste / optimized paste), migrate the old single field | Distinct, muscle-memory-friendly outputs without a mode switch. |
| 5 | Long input | **Auto-refine >80 words via LLM; engine owns the LLM call** | Optimizer stays LLM-free; the engine already owns all LLM I/O, so refine returns a request the engine resolves. |
| 6 | Code phrases | **Gated behind an explicit code mode**, not always-on | Always-on would corrupt ordinary dictation containing "bracket", "quote", "pipe", etc. |

## Storage layout (`~/.config/pie/`)

```
settings.json        # + BYOK fields, + dual hotkeys, + background_mining, + code_mode
memory.json          # unchanged
pronunciation.json   # static + user dict (existing, unchanged)
learned_vocab.json   # NEW — auto-learned + synced entries (separate file)
sync_state.json      # NEW — last import timestamp, sources imported
```

Learned/synced entries live in a **separate file** so "Reset learned" never
touches the user's manual `pronunciation.json`, and so learned vocab is
inspectable and exportable.

## Cross-cutting foundations

### `Source` enum (`src/corrector/dictionary.rs`)

```rust
pub enum Source {
    Static,       // shipped seed
    User,         // manual, explicit override
    Synced,       // imported from conversation history
    AutoLearned,  // learned from edits / (optional) background mining
}
```

Precedence in `PronunciationCorrector::rebuild()`, highest first:
**User → Synced → AutoLearned → Static**. A `heard` key present at a higher tier
suppresses lower tiers (extends the existing user-over-static logic).

### `LlmConfig` plumbing

```rust
pub struct LlmConfig { pub api_url: String, pub api_key: String, pub model: String }
```

Threads: `Settings` → `LlmConfig` → `LlmRouter::from_config()` → `PieEngine`.
When `api_url` is empty, `from_config` falls back to `OpenAiClient::from_env()`
(preserves `pie --provider openai` and existing env-based installs).

---

## Phase 1 — BYOK LLM config

**Goal:** configure API URL + key + model from settings; no env vars required.

### Changes
- `src-tauri/src/settings.rs`: add `llm_api_url: String`, `llm_api_key: String`
  (both default empty). Wire up the existing unused `llm_model`.
- `src/llm/router.rs`: add `LlmConfig` and `LlmRouter::from_config(&LlmConfig)`;
  keep `new()`/`from_env()` as the empty-URL fallback path.
- `src/pipeline/engine.rs`: `PieEngine::with_config(&LlmConfig)` builds the router
  from settings; `new()` retained (delegates to empty config).
- `src-tauri/src/main.rs`: build `LlmConfig` from loaded settings; new
  `test_llm_connection` command doing a minimal 1-token round-trip.
- `ui/src/lib/LLMSettings.svelte` (NEW): URL field, password key field, model
  field, **Test Connection** button surfacing ok/error. Add to settings page in
  `App.svelte`.

### Acceptance
- [ ] User sets URL + key + model in the UI and it persists.
- [ ] Test Connection verifies the endpoint (success and failure both visible).
- [ ] Router uses settings config when present, env vars when URL is empty.
- [ ] `pie --provider openai` (env path) still works.
- [ ] API key uses a password input; never rendered in plaintext.

---

## Phase 2 — Auto-learning vocabulary

**Goal:** PIE improves from real usage with zero manual entry, at near-zero cost
by default.

### 2a. Edit-based learning (always-on, primary)
Signals that carry real information:
- **A dict correction fired** during a run → reinforce that entry
  (`seen_count += 1`, bump confidence).
- **The user edited the pasted output** → diff raw transcript vs. final text; a
  changed technical token is a candidate correction.

These require **no speculative LLM call**. Reinforcement is pure local
bookkeeping. Diff-derived candidates are added to `learned_vocab.json` with a
starting confidence and `source: AutoLearned`.

> Capturing post-paste edits needs an edit-capture hook. If the desktop app
> cannot observe the user's edit in the target app, this reduces to the
> reinforcement signal plus any in-app correction UI; the implementation plan
> must confirm what edit signal is actually available before relying on diffs.

### 2b. Background mining (opt-in, off by default)
Behind `settings.background_mining` (default `false`). When on, the engine
fire-and-forgets `try_send(LearnTask)` after each run (never blocks the
pipeline). A background tokio task:
- batches 5 interactions **or** 30s, whichever first;
- rate-limits to **max 1 LLM call / 30s** (won't burn credits);
- uses the cheapest configured model, conservative extraction prompt;
- only adds `heard` keys not already present;
- writes to `learned_vocab.json`; dict reload happens **between** interactions.

### New files
- `src/corrector/learned.rs` — `LearnedStore` (`load`/`save`/`add_or_reinforce`/
  `has_entry`/`entries`/`reset`/`count`); `LearnedEntry { heard, canonical,
  source, confidence, seen_count, first_seen, last_seen }`.
- `src/corrector/learner.rs` — `LearnTask`, `BackgroundLearner` holding
  `Arc<Mutex<PronunciationCorrector>>` (the shared handle the engine also holds —
  resolves the doc's `/* shared ref */` placeholder), batching + extraction +
  robust JSON parse (strips ``` fences).

### Changes
- `src/corrector/mod.rs`: `PronunciationCorrector` gains a `LearnedStore`;
  `rebuild()` merges learned entries at the AutoLearned tier;
  `add_auto_correction`, `reinforce`, `has_entry`, `learned_count`,
  `reset_learned`.
- `src/pipeline/engine.rs`: `learner_tx: Option<Sender<LearnTask>>`;
  `spawn_learner()` (called once at startup when mining is on).
- `src-tauri/src/main.rs`: commands `get_learned_vocab_count`,
  `reset_learned_vocab`; spawn learner when the toggle is on.
- `ui/src/lib/VocabularySettings.svelte`: replace the one-by-one add form with
  learned-count + last-activity + **Reset Learned** + a **background mining**
  toggle. Keep manual add as an advanced/collapsed option (explicit overrides
  are still useful).

### Acceptance
- [ ] A firing dict correction reinforces its entry (seen_count/confidence rise).
- [ ] With mining **off** (default), no background LLM calls ever happen.
- [ ] With mining **on**, batches 5-or-30s, ≤1 LLM call/30s, appends to
      `learned_vocab.json`, only new `heard` keys.
- [ ] Learned entries load on startup and merge at the AutoLearned tier.
- [ ] Pipeline never blocks on the learner (`try_send`, not `send`).
- [ ] `reset_learned_vocab` clears learned/synced without touching user/static.

---

## Phase 3 — Vocabulary bootstrap (import)

**Goal:** seed vocabulary on first run from the user's real history — reliably.

### New file: `src/corrector/sync.rs`
- `discover()` returns available sources:
  - **Cursor** — auto-detected local SQLite (`~/.cursor` / workspace
    `state.vscdb`) when present.
  - **Import targets** — the user explicitly chooses a file/folder: a ChatGPT/
    Claude export `.zip` (`conversations.json`), or any folder of `.md`/`.txt`/
    `.json`.
- `extract_text(path)` handles json / sqlite / zip / plain text; unknown or
  empty sources report 0 conversations rather than erroring.
- `run(corrector, llm, on_progress)`: batches 10 conversations/LLM call, extracts
  terms + variants with a conservative prompt, stores `source: Synced`, emits
  `sync-progress`, writes `sync_state.json`. **Never auto-runs** — user-initiated.

### Tauri commands
`discover_sync_sources`, `pick_import_target` (file/folder dialog),
`run_vocabulary_sync`.

### UI: `ui/src/lib/VocabularySync.svelte` (NEW)
Lists auto-detected sources (Cursor) and a **"Choose export / folder…"** picker,
a **Sync Now** button, a progress bar, and a result summary. Copy is explicit
that data stays local and is only sent to the user's own configured LLM.

### Acceptance
- [ ] Cursor local history is auto-detected when present.
- [ ] User can point PIE at a ChatGPT/Claude export or a folder and import it.
- [ ] Sources with no parseable local history report 0 (no error, no crash).
- [ ] Extraction batches 10 conversations; progress events reach the frontend.
- [ ] Synced entries stored `source: Synced` in `learned_vocab.json`.
- [ ] `sync_state.json` records last run so imports aren't blindly repeated.
- [ ] Import only runs on explicit user action.

---

## Phase 4 — Dual hotkey system

**Goal:** two global hotkeys — raw paste and optimized paste.

### Changes
- `src-tauri/src/settings.rs`: replace `hotkey` with `hotkey_raw`
  (default `CmdOrCtrl+Shift+V`) and `hotkey_optimized`
  (default `CmdOrCtrl+Shift+Space`). **Migration:** on load, if the legacy
  `hotkey` is present and the new fields are empty, map it into
  `hotkey_optimized` so existing installs keep their binding. (The existing
  `paste_output` setting is subsumed by hotkey choice.)
- `src-tauri/src/main.rs`: register both; `on_hotkey_with_mode(app, mode)` where
  A → `"transcript"`, B → `"prompt"`. Reuse the existing overlay and busy-flag.
- `ui/src/lib/*`: two `HotkeyRecorder` fields with captions.

### Acceptance
- [ ] Both hotkeys register and show the recording overlay.
- [ ] A: record → transcribe → correct → paste raw text.
- [ ] B: record → transcribe → correct → intent → optimize → paste prompt.
- [ ] Busy flag prevents double-trigger.
- [ ] Legacy single-hotkey installs migrate without losing their binding.
- [ ] Both hotkeys configurable in settings.

---

## Phase 5 — Long-conversation refinement

**Goal:** compress long, rambling voice input into a sharp prompt.

### New file: `src/optimizer/refine.rs`
`optimize(intent, memory)` returns:
- ≤80 words → delegate to `balanced::optimize` (unchanged).
- >80 words → a `RefineRequest { prompt, original }` describing the LLM
  compression (strip filler, dedupe, **preserve all technical terms and
  constraints**). The optimizer performs **no** LLM I/O.

### Changes
- `src/optimizer/mod.rs`: add `OptimizationMode::Refine`; optimize result carries
  an optional `RefineRequest` instead of the doc's "placeholder, engine replaces"
  marker.
- `src/pipeline/engine.rs`: when a `RefineRequest` is present, the engine calls
  `llm.send`; on error it falls back to the original text.
- CLI: `pie --mode refine`.

### Acceptance
- [ ] >80-word inputs auto-refine (in adaptive) and via explicit `--mode refine`.
- [ ] Refinement strips filler, dedupes, preserves all technical terms/constraints.
- [ ] ≤80-word inputs pass through unchanged.
- [ ] LLM failure falls back to the original text (never drops input).

---

## Phase 6 — Code-aware post-processing (code-mode gated)

**Goal:** translate spoken code into syntax — only when the user is dictating code.

### New file: `src/corrector/code_phrases.rs`
- Phrase → syntax map (`"console dot log"` → `"console.log("`, `"triple equals"`
  → `"==="`, brackets/quotes/operators, etc.), **loadable from JSON** for
  extensibility.
- `apply_code_phrases(text)`: longest-phrase-first, **replace all occurrences**
  (fixes the doc's single-`.find()` first-match-only bug), case-insensitive.

### Gating
- `settings.code_mode: bool` (default `false`), toggleable (setting and/or a
  dedicated hotkey). Only when active does `apply_code_phrases` run, **after**
  pronunciation correction, **before** intent extraction. Normal dictation is
  never touched.

### Acceptance
- [ ] With code mode **off**, transcripts are never altered by code phrases.
- [ ] With code mode **on**: "console dot log hello" → "console.log(hello".
- [ ] Longest-phrase-first prevents partial replacements; all occurrences fixed.
- [ ] Runs after pronunciation dict, before intent extraction.
- [ ] Map is loadable/extensible from JSON.

---

## Non-goals
- No cloud sync of learned vocab; everything stays local except calls to the
  user's own configured LLM.
- No new LLM provider abstractions beyond OpenAI-compatible (existing client).
- Windows/Linux paste parity for Phase 4 is best-effort per existing platform
  support, not a new requirement here.

## Privacy & safety
- All processing local; conversations/transcripts leave the device only to the
  user's own configured LLM endpoint.
- Background mining is off by default and rate-limited when on.
- Import runs only on explicit user action; learned/synced vocab is a separate,
  inspectable, resettable file.

## Build order

| Phase | What | Depends on |
|-------|------|-----------|
| 1 | BYOK LLM config | — |
| 2 | Auto-learning (edit-based + opt-in mining) | 1 |
| 3 | Vocabulary bootstrap (import) | 1 |
| 4 | Dual hotkey system | — (parallel) |
| 5 | Long-conversation refinement | 1 |
| 6 | Code-aware post-processing | — (parallel) |
