# Design: Make PIE feel fast and calm

**Date:** 2026-07-23
**Status:** Approved (brainstorm), pending implementation plan
**Branch:** `feat/perf-and-ui-polish`

## Goal

Make PIE *feel* faster and calmer without changing what it does. Three focused
changes across two perceived-speed wins and one visual polish pass. Download/
binary size is explicitly **not** a goal (see Non-goals).

## Context: what the measurements showed

Before scoping, the current state was measured so "optimization" targets real
weight, not assumptions:

- **Web bundle is already minimal:** ~92 KB uncompressed (~30 KB gzipped), Svelte
  5, one runtime dependency (`@tauri-apps/api`). Nothing meaningful to win here.
- **Size lives in the native binary:** 39 MB, of which ~24 MB is machine code
  from the statically-linked ML runtimes (whisper.cpp/ggml + ONNX runtime for
  Silero). Stripping symbols would save ~9 MB but the user deprioritized size.
- **Startup is not blocked on anything heavy:** `PieEngine::new()` only loads a
  small memory profile and constructs lightweight structs — no models. The
  window paints quickly.
- **The two real latency sources are model I/O in the recording pipeline** —
  detailed below.

## Scope

Three changes:

1. Cache the Silero VAD session (recurring per-recording win).
2. Warm the whisper model on startup and on model change (first-use win).
3. UI polish that keeps the existing structure (visual calm).

## 1. Cache the Silero VAD session

### Problem
Every `do_start_recording` → `build_recorder` constructs a fresh `SileroVad`,
whose `SileroVadEngine::new` calls `commit_from_file()` — building a new ONNX
inference session from disk **on every recording start**. This is a latency
spike at the exact moment the user wants instant feedback. Whisper is already
cached this way; the VAD is not.

### Change
Hold the loaded Silero engine in `AppState`, keyed by `(model_path,
sample_rate)`, mirroring the existing `whisper` cache field and
`get_or_load_whisper` pattern. `build_recorder` reuses the cached engine instead
of reconstructing it. A settings change to the Silero model path invalidates the
cache entry and reloads (same key-check pattern as whisper).

### Recurrent-state reset (required)
`SileroVadEngine` carries LSTM recurrent state (`h_tensor`, `c_tensor`, shape
`(2, 1, 64)`) that updates on every `compute()`. Today each recording gets a
fresh engine with zeroed state; a cached engine would carry state across
recordings and pollute detection. Add `SileroVadEngine::reset()` that restores
`h_tensor`/`c_tensor` to `Array3::zeros(STATE_SHAPE)`, called at each session
start (in `do_start_recording`, before recording begins). The constant
`sample_rate_tensor` and the loaded `session` persist untouched.

### Ownership refactor (approved)
The recorder currently *owns* the VAD: `SileroVad` is boxed into `VadPipeline`,
boxed into the `AudioRecorder`. Reusing a cached session means the session must
outlive a single recording. The engine lives in `AppState` and the recorder
borrows it through a shared handle (`Arc<Mutex<…>>`) rather than owning a fresh
box each session. The implementation plan will pin the exact ownership shape
(where the `Arc<Mutex>` sits, how `VadPipeline`/`AudioRecorder` take a shared vs.
owned VAD) and keep the change contained to the recorder/VAD boundary in
`src/audio/`.

### Interfaces touched
- `src/audio/silero_vad_engine.rs` — add `reset()`.
- `src/audio/silero.rs`, `src/audio/vad.rs` (`VadPipeline`), `src/audio/recorder.rs`
  — accept a shared VAD handle instead of always owning a freshly boxed one.
- `src-tauri/src/main.rs` — `AppState` gains a cached-Silero field; `build_recorder`
  reuses it; `do_start_recording` resets state at session start.

## 2. Warm the whisper model

### Problem
Whisper loads lazily on the first transcription (`get_or_load_whisper` during
`stop_recording`), so the first recording after every launch stalls during the
"decoding" state while the model loads.

### Change
After `AppState` is managed in `setup()`, spawn a background blocking thread
(`std::thread` / `tauri::async_runtime::spawn_blocking`) that runs the existing
whisper load path to populate the cache **if a whisper model is configured**.
Properties:

- **Non-blocking:** never delays window paint or the setup closure returning.
- **Silent:** no new UI state; the model is simply warm when first used.
- **Best-effort:** on failure, log and continue — the first real transcription
  falls back to the existing lazy load.
- **Idempotent with the cache:** uses the same `(path, language)` cache key, so a
  later real use reuses the warmed engine.

### Warm on model change (approved extension)
When the whisper model changes — `select_model` for a whisper model, and after a
successful `download_model` auto-select — kick the same background warm so the
newly selected model is hot before first use. Reuses the same warm helper; a few
lines. VAD model selection does not need warming (the VAD cache in change 1
loads on next record start regardless).

### Interfaces touched
- `src-tauri/src/main.rs` — a `warm_whisper(app)` helper called from `setup()` and
  after whisper-model selection; reuses `get_or_load_whisper`'s load logic.

## 3. UI polish — keep structure

### Constraint
Keep the four tabs (Record / History / Models / Settings) and every existing
flow and behavior. No navigation change, no elements removed, no command
changes. This is refinement only.

### Change
Refine the existing design system in `ui/src/app.css` and the state transitions:

- **Tokens:** tighten the spacing and type scale for a calmer rhythm; soften
  color/contrast where the current values read as busy. Work within the existing
  token set (`--space-*`, `--fg*`, `--surface*`, `--accent*`) rather than
  introducing a parallel system.
- **State transitions:** make `idle → recording → decoding → result` smoother and
  quieter — gentler than abrupt swaps, no attention-grabbing motion.

Executed through the frontend-design skill at implementation time. If it reaches
concrete before/after layout comparisons, a visual companion will be offered
then.

### Interfaces touched
- `ui/src/app.css` (tokens, transitions).
- Component `.svelte` files only if a transition needs a class/state hook; no
  structural markup changes.

## Non-goals

- **Binary / download size reduction** (symbol stripping, runtime swaps) — user
  deprioritized it.
- **Structural UI change** — no collapsing tabs to one surface, no result-view
  redesign.
- **Startup window-paint optimization** — already not blocked on heavy work; the
  felt cold-start is change 2 (first transcription), addressed there.

## Verification

- **VAD caching:**
  - Measure start-recording latency before vs. after (expect the per-recording
    ONNX session build to disappear from the hot path).
  - Correctness across consecutive recordings: run two back-to-back recordings
    and confirm the second still gates speech correctly (proves `reset()` clears
    state — a regression here would show as the second recording's VAD behaving
    oddly at its start).
  - Existing `tests/silero_vad.rs` and `tests/voice_pipeline.rs` pass unchanged.
- **Whisper warm-up:**
  - Measure first-transcription latency after a cold launch before vs. after.
  - Confirm no startup regression (window still paints promptly; warm runs off
    the critical path).
  - Confirm the "no model configured" path is a silent no-op.
- **UI polish:**
  - Dev-run visual check across all four states and all four tabs; confirm no
    behavior or navigation change.

## Risks

- **VAD ownership refactor** is the highest-touch change; it crosses the
  recorder/VAD boundary. Mitigation: keep it contained to `src/audio/`, lean on
  the existing VAD tests, and pin the ownership shape in the plan before coding.
- **State-reset correctness** — forgetting to reset, or resetting at the wrong
  point, silently degrades VAD quality rather than crashing. Mitigation: the
  back-to-back recording check above is a required verification step.
