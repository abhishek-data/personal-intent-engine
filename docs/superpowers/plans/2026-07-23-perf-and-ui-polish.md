# Perf + UI Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make PIE feel faster and calmer — stop the Silero VAD model from reloading on every recording, warm the whisper model on startup, and polish the UI without changing structure.

**Architecture:** Two perceived-speed wins plus a visual polish pass. The reusable, testable pieces (`AudioRecorder::with_vad_shared`, a `VadCache`) live in the `pie-engine` library and are covered by unit tests. The Tauri binary (`pie-desktop`) wires them into `AppState` and adds a fire-and-forget whisper warm-up; that wiring is verified by build + the existing integration test + manual latency checks, because the binary crate has no lib target to unit-test and the hot paths need real audio hardware and model files.

**Tech Stack:** Rust (workspace: `pie-engine` lib + `pie-desktop` Tauri bin), Tauri 2, `ort` (ONNX Runtime) for Silero VAD, whisper.cpp bindings, Svelte 5 + Vite for the UI.

## Global Constraints

- **No UI behavior or navigation change.** Keep all four tabs (Record / History / Models / Settings) and every existing flow. UI work is visual polish only.
- **Existing VAD tests must keep passing:** `cargo test --features whisper,vad --test silero_vad` and `cargo test -p pie-engine --lib` (VadPipeline tests) stay green.
- **No `unwrap()`/`expect()` in library code** (`src/`) — use `?` or map to errors. Test code may unwrap. (AGENTS.md)
- **Doc comments (`/// …`) on all public items.** (AGENTS.md)
- **Run `cargo fmt` before each commit; `cargo clippy` clean.** (AGENTS.md)
- **Whisper warm-up must be non-blocking, silent (no new UI state), and best-effort** (failure logs and falls back to the existing lazy load).
- The Silero session type shared across recordings is `Arc<Mutex<Box<dyn VoiceActivityDetector>>>`. The recorder already resets this detector's recurrent + smoothing state at each session start (`run_consumer`, `Cmd::Start`), so a reused handle is safe — do not add a second reset.

---

### Task 1: `AudioRecorder::with_vad_shared` — inject a shared VAD handle

Lets a caller hand the recorder an already-built, shared detector instead of always boxing a fresh one. This is what allows the cached Silero session to be reused across recordings.

**Files:**
- Modify: `src/audio/recorder.rs` (add `with_vad_shared`, make `with_vad` delegate; add a `#[cfg(test)]` module)

**Interfaces:**
- Consumes: existing `VadConfig { detector: Arc<Mutex<Box<dyn VoiceActivityDetector>>>, offline_hangover_frames, streaming_hangover_frames }` and the private `self.vad` field.
- Produces:
  - `pub fn with_vad_shared(self, detector: Arc<Mutex<Box<dyn VoiceActivityDetector>>>, offline_hangover_frames: usize, streaming_hangover_frames: usize) -> Self`
  - `with_vad` keeps its signature `(self, detector: Box<dyn VoiceActivityDetector>, usize, usize) -> Self`.

- [ ] **Step 1: Write the failing tests**

Append to `src/audio/recorder.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::vad::PassthroughVad;

    #[test]
    fn with_vad_shared_reuses_the_passed_handle() {
        let detector: Arc<Mutex<Box<dyn VoiceActivityDetector>>> =
            Arc::new(Mutex::new(Box::new(PassthroughVad)));
        let recorder = AudioRecorder::new()
            .unwrap()
            .with_vad_shared(Arc::clone(&detector), 5, 10);
        assert!(recorder.vad.is_some(), "recorder should hold a VAD config");
        // Caller holds one ref; the recorder's VadConfig holds the second.
        assert_eq!(Arc::strong_count(&detector), 2, "handle must be shared, not cloned into a new Arc");
    }

    #[test]
    fn with_vad_wraps_detector_in_a_fresh_handle() {
        let recorder = AudioRecorder::new()
            .unwrap()
            .with_vad(Box::new(PassthroughVad), 5, 10);
        assert!(recorder.vad.is_some(), "with_vad should still populate the VAD config");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p pie-engine --lib with_vad`
Expected: FAIL to compile — `no method named with_vad_shared found`.

- [ ] **Step 3: Add `with_vad_shared` and make `with_vad` delegate**

In `src/audio/recorder.rs`, replace the existing `with_vad` method body with a delegating version and add `with_vad_shared` beside it:

```rust
    /// Attach a single VAD engine, reconfigured per session for the offline vs
    /// streaming hangover tail. The two policies are mutually exclusive within
    /// a recording, so one engine covers both instead of two resident instances.
    #[must_use]
    pub fn with_vad(
        self,
        detector: Box<dyn VoiceActivityDetector>,
        offline_hangover_frames: usize,
        streaming_hangover_frames: usize,
    ) -> Self {
        self.with_vad_shared(
            Arc::new(Mutex::new(detector)),
            offline_hangover_frames,
            streaming_hangover_frames,
        )
    }

    /// Attach a VAD engine via a *shared* handle so an expensive-to-load
    /// detector (e.g. the Silero ONNX session) can be reused across recordings
    /// instead of rebuilt each time. The recorder resets the detector's state
    /// at each session start, so the caller may keep its own clone of `detector`
    /// alive between recordings.
    #[must_use]
    pub fn with_vad_shared(
        mut self,
        detector: Arc<Mutex<Box<dyn VoiceActivityDetector>>>,
        offline_hangover_frames: usize,
        streaming_hangover_frames: usize,
    ) -> Self {
        self.vad = Some(VadConfig {
            detector,
            offline_hangover_frames,
            streaming_hangover_frames,
        });
        self
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pie-engine --lib with_vad`
Expected: PASS (2 tests).

- [ ] **Step 5: Format, lint, commit**

```bash
cargo fmt
cargo clippy -p pie-engine --all-targets
git add src/audio/recorder.rs
git commit -m "feat(audio): add AudioRecorder::with_vad_shared for a reusable VAD handle"
```

---

### Task 2: `VadCache` — memoize the loaded VAD by model path

A small cache that builds the detector once per model path and returns a shared handle on subsequent calls. Injected builder keeps it unit-testable without a real ONNX model.

**Files:**
- Create: `src/audio/vad_cache.rs`
- Modify: `src/audio/mod.rs` (declare module + re-export)

**Interfaces:**
- Consumes: `VoiceActivityDetector` (from `super::vad`).
- Produces:
  - `pub type SharedVad = Arc<Mutex<Box<dyn VoiceActivityDetector>>>;`
  - `pub struct VadCache` with `pub fn new() -> Self` and
    `pub fn get_or_build<F>(&mut self, model_path: &Path, build: F) -> anyhow::Result<SharedVad> where F: FnOnce() -> anyhow::Result<Box<dyn VoiceActivityDetector>>`.

- [ ] **Step 1: Write the failing tests**

Create `src/audio/vad_cache.rs` with the tests first (module body added in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::vad::PassthroughVad;
    use std::cell::Cell;

    fn passthrough() -> anyhow::Result<Box<dyn VoiceActivityDetector>> {
        Ok(Box::new(PassthroughVad))
    }

    #[test]
    fn same_path_builds_once_and_shares_the_handle() {
        let mut cache = VadCache::new();
        let calls = Cell::new(0);
        let p = Path::new("/models/silero.onnx");
        let a = cache
            .get_or_build(p, || {
                calls.set(calls.get() + 1);
                passthrough()
            })
            .unwrap();
        let b = cache
            .get_or_build(p, || {
                calls.set(calls.get() + 1);
                passthrough()
            })
            .unwrap();
        assert_eq!(calls.get(), 1, "second call with same path must hit the cache");
        assert!(Arc::ptr_eq(&a, &b), "same path must return the same shared handle");
    }

    #[test]
    fn different_path_rebuilds() {
        let mut cache = VadCache::new();
        let calls = Cell::new(0);
        let a = cache
            .get_or_build(Path::new("/models/a.onnx"), || {
                calls.set(calls.get() + 1);
                passthrough()
            })
            .unwrap();
        let b = cache
            .get_or_build(Path::new("/models/b.onnx"), || {
                calls.set(calls.get() + 1);
                passthrough()
            })
            .unwrap();
        assert_eq!(calls.get(), 2, "a new path must rebuild");
        assert!(!Arc::ptr_eq(&a, &b), "different paths must not share a handle");
    }

    #[test]
    fn build_error_propagates_and_leaves_cache_empty() {
        let mut cache = VadCache::new();
        let r = cache.get_or_build(Path::new("/models/a.onnx"), || {
            Err(anyhow::anyhow!("load failed"))
        });
        assert!(r.is_err(), "a build error must propagate");
        // A later successful build for the same path must still run (not cached).
        let ok = cache.get_or_build(Path::new("/models/a.onnx"), || {
            Ok(Box::new(PassthroughVad) as Box<dyn VoiceActivityDetector>)
        });
        assert!(ok.is_ok());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p pie-engine --lib vad_cache`
Expected: FAIL to compile — `cannot find … VadCache` / module not declared.

- [ ] **Step 3: Write the `VadCache` implementation**

Prepend to `src/audio/vad_cache.rs` (above the test module):

```rust
//! Cache for the loaded VAD detector.
//!
//! Building a Silero detector calls `commit_from_file`, which constructs an
//! ONNX inference session from disk — too slow to redo on every recording. This
//! memoizes the detector by model path and hands out a shared handle so the
//! session persists. A path change rebuilds. The recorder resets the detector's
//! recurrent + smoothing state at each session start, so reuse is safe.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;

use super::vad::VoiceActivityDetector;

/// A VAD detector shared between the cache and a recorder, behind a mutex so the
/// recorder's worker thread can drive it.
pub type SharedVad = Arc<Mutex<Box<dyn VoiceActivityDetector>>>;

/// Memoizes the loaded VAD detector, keyed by model path.
#[derive(Default)]
pub struct VadCache {
    cached: Option<(PathBuf, SharedVad)>,
}

impl VadCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a shared detector for `model_path`, invoking `build` only on a
    /// cache miss (a different or absent path). On a hit, returns a clone of the
    /// cached handle without rebuilding. A `build` error propagates and leaves
    /// the cache unchanged.
    pub fn get_or_build<F>(&mut self, model_path: &Path, build: F) -> Result<SharedVad>
    where
        F: FnOnce() -> Result<Box<dyn VoiceActivityDetector>>,
    {
        if let Some((path, detector)) = &self.cached {
            if path == model_path {
                return Ok(Arc::clone(detector));
            }
        }
        let detector: SharedVad = Arc::new(Mutex::new(build()?));
        self.cached = Some((model_path.to_path_buf(), Arc::clone(&detector)));
        Ok(detector)
    }
}
```

Then declare and re-export it in `src/audio/mod.rs`. Add after the `pub mod vad;` line:

```rust
pub mod vad_cache;
```

And add to the re-export block (after the `pub use vad::{ … };` block):

```rust
pub use vad_cache::{SharedVad, VadCache};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pie-engine --lib vad_cache`
Expected: PASS (3 tests).

- [ ] **Step 5: Format, lint, commit**

```bash
cargo fmt
cargo clippy -p pie-engine --all-targets
git add src/audio/vad_cache.rs src/audio/mod.rs
git commit -m "feat(audio): add VadCache to memoize the loaded VAD by model path"
```

---

### Task 3: Reuse the cached VAD in the Tauri app

Wire `VadCache` + `with_vad_shared` into `pie-desktop` so pressing the hotkey no longer rebuilds the ONNX session each time.

**Files:**
- Modify: `src-tauri/src/main.rs` (`AppState`, `build_recorder`, `do_start_recording`, imports)

**Interfaces:**
- Consumes: `pie_engine::audio::{VadCache, VoiceActivityDetector, with_vad_shared}` (Task 1 + 2).
- Produces: `AppState.vad_cache: Mutex<VadCache>`; `build_recorder(settings: &Settings, vad_cache: &mut VadCache) -> anyhow::Result<(AudioRecorder, bool)>`.

- [ ] **Step 1: Extend the imports**

In `src-tauri/src/main.rs`, update the `use pie_engine::audio::{…}` block to add `VadCache` and `VoiceActivityDetector`:

```rust
use pie_engine::audio::{
    AudioRecorder, SileroVad, VadCache, VadPipeline, VadPolicy, VoiceActivityDetector,
    PIE_VAD_THRESHOLD, VAD_CONTEXT_FRAMES, VAD_HANGOVER_FRAMES, VAD_SPEECH_THRESHOLD_FRAMES,
    VAD_STREAM_HANGOVER_FRAMES,
};
```

- [ ] **Step 2: Add the cache to `AppState`**

Add a field to the `AppState` struct (after the `recorder` field):

```rust
    /// Cached Silero VAD session, reused across recordings so the ONNX model
    /// isn't rebuilt from disk on every start. Keyed by model path.
    vad_cache: Mutex<VadCache>,
```

And initialize it where `AppState` is constructed in `setup()` (alongside `recorder: Mutex::new(None),`):

```rust
                vad_cache: Mutex::new(VadCache::new()),
```

- [ ] **Step 3: Rewrite `build_recorder` to use the cache**

Replace the existing `build_recorder` function with:

```rust
/// Recorder with Silero VAD when configured; VAD-free otherwise. The Silero
/// session is loaded once and reused across recordings via `vad_cache`.
fn build_recorder(
    settings: &Settings,
    vad_cache: &mut VadCache,
) -> anyhow::Result<(AudioRecorder, bool)> {
    if settings.silero_model.is_empty() {
        return Ok((AudioRecorder::new()?, false));
    }
    let model_path = Settings::expand(&settings.silero_model);
    let detector = vad_cache.get_or_build(&model_path, || {
        let silero = SileroVad::new(&model_path, PIE_VAD_THRESHOLD)?;
        let smoothed = VadPipeline::new(
            Box::new(silero),
            VAD_CONTEXT_FRAMES,
            VAD_HANGOVER_FRAMES,
            VAD_SPEECH_THRESHOLD_FRAMES,
        );
        Ok(Box::new(smoothed) as Box<dyn VoiceActivityDetector>)
    })?;
    let recorder = AudioRecorder::new()?.with_vad_shared(
        detector,
        VAD_HANGOVER_FRAMES,
        VAD_STREAM_HANGOVER_FRAMES,
    );
    Ok((recorder, true))
}
```

- [ ] **Step 4: Update the `do_start_recording` call site**

In `do_start_recording`, replace the line
`let (mut recorder, vad_active) = build_recorder(&settings).map_err(|e| e.to_string())?;`
with a version that passes the cache lock:

```rust
    let (mut recorder, vad_active) = {
        let mut vad_cache = state.vad_cache.lock().unwrap_or_else(|e| e.into_inner());
        build_recorder(&settings, &mut vad_cache).map_err(|e| e.to_string())?
    };
```

- [ ] **Step 5: Build and verify the existing VAD test still passes**

Run: `npm --prefix ui run build && cargo build -p pie-desktop`
Expected: builds clean (no errors/clippy warnings introduced).

Run: `cargo test --features whisper,vad --test silero_vad`
Expected: PASS, or "ok. 0 passed; 0 filtered out" style skip if the Silero model isn't in `~/.cache/pie/models/` — either is acceptable (the test skips cleanly without the model).

- [ ] **Step 6: Manual verification — VAD no longer reloads, correctness holds**

With a Silero model configured and a whisper model downloaded, launch the app:

```bash
cargo run -p pie-desktop
```

- Record twice in a row using the hotkey. Confirm the **start** of the second recording is not visibly delayed (before this change every start paid an ONNX session build).
- Confirm the second recording still transcribes correctly (proves the per-session reset of the shared detector works — a broken reset shows as the second recording's VAD mis-gating its opening words).
- Optional: add a temporary `log::info!` timing around `build_recorder` to confirm the closure runs only on the first recording; remove before commit.

- [ ] **Step 7: Format, lint, commit**

```bash
cargo fmt
cargo clippy -p pie-desktop
git add src-tauri/src/main.rs
git commit -m "perf(recording): reuse cached Silero VAD session across recordings"
```

---

### Task 4: Warm the whisper model on startup and on model change

Populate the whisper cache off the launch path so the first transcription each session isn't cold.

**Files:**
- Modify: `src-tauri/src/main.rs` (add `warm_whisper`, call it in `setup()` and after a whisper-model `select_model`)

**Interfaces:**
- Consumes: existing `get_or_load_whisper(state: &State<'_, AppState>, settings: &Settings) -> Result<Arc<WhisperEngine>, String>` and `models::ModelKind`.
- Produces: `fn warm_whisper(app: &AppHandle)` (fire-and-forget).

- [ ] **Step 1: Add the `warm_whisper` helper**

Add near `get_or_load_whisper` in `src-tauri/src/main.rs`:

```rust
/// Load the configured whisper model into the cache on a background thread so
/// the first transcription of a session isn't cold. Non-blocking, silent, and
/// best-effort: a no-op when no model is configured, and a logged warning on
/// failure (the next real transcription falls back to the lazy load).
fn warm_whisper(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let settings = state
            .settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if settings.whisper_model.is_empty() {
            return;
        }
        match get_or_load_whisper(&state, &settings) {
            Ok(_) => log::info!("Whisper model warmed"),
            Err(e) => log::warn!("Whisper warm-up failed (will load on first use): {e}"),
        }
    });
}
```

- [ ] **Step 2: Warm on startup**

In `setup()`, after the tray is built and just before the closure's final `Ok(())`, add:

```rust
            // Warm the whisper model off the launch path so the first
            // transcription isn't cold. Non-blocking.
            warm_whisper(app.handle());
```

- [ ] **Step 3: Warm after a whisper-model selection**

In `select_model`, after `settings.save()…?` and the `emit_event(&app, "pie://models-changed", ())` line, add a warm for whisper models only:

```rust
    if matches!(kind, models::ModelKind::Whisper) {
        warm_whisper(&app);
    }
```

(`kind` is already bound at the top of `select_model` from `models::resolve(&id)`. This also covers `download_model`, which calls `select_model` on completion.)

- [ ] **Step 4: Build**

Run: `cargo build -p pie-desktop`
Expected: builds clean.

- [ ] **Step 5: Manual verification — first transcription is warm**

```bash
cargo run -p pie-desktop
```

- With a whisper model configured, launch and watch the logs for `Whisper model warmed` shortly after startup (before any recording).
- Confirm the window still appears promptly at launch (warm-up runs off-thread; no paint delay).
- Record once immediately after launch; the first transcription should no longer stall on model load.
- With **no** whisper model configured, confirm launch is a clean no-op (no warning, no crash).

- [ ] **Step 6: Commit**

```bash
cargo fmt
cargo clippy -p pie-desktop
git add src-tauri/src/main.rs
git commit -m "perf(startup): warm the whisper model on launch and after model change"
```

---

### Task 5: UI polish — calmer tokens and transitions, structure unchanged

Refine the existing design system and state transitions. No markup restructuring, no navigation change. Executed through the frontend-design skill with visual verification.

**Files:**
- Modify: `ui/src/app.css` (design tokens, state transitions)
- Modify (only if a transition needs a state hook): the relevant `.svelte` component — no structural markup changes.

- [ ] **Step 1: Invoke the frontend-design skill**

This task is design work best done against the running app. Invoke `frontend-design:frontend-design` and apply it to the polish targets below. If it reaches concrete before/after layout comparisons, offer the brainstorming visual companion at that point.

- [ ] **Step 2: Apply the polish (within the existing token system)**

Working in `ui/src/app.css`, staying inside the current token set (`--space-*`, `--fg*`, `--surface*`, `--accent*`, `--transition`):

- Tighten the spacing and type scale for a calmer vertical rhythm.
- Soften color/contrast where the resting state reads as busy (the idle Record view especially).
- Make the `idle → recording → decoding → result` transitions smoother and quieter — gentle fades/eases over abrupt swaps; no attention-grabbing motion.

Do **not** add tabs, remove controls, or change any flow.

- [ ] **Step 3: Build the UI**

Run: `npm --prefix ui run build`
Expected: builds clean.

- [ ] **Step 4: Manual visual verification across all states and tabs**

```bash
cargo run -p pie-desktop
```

- Walk all four tabs (Record / History / Models / Settings) — confirm nothing moved, disappeared, or changed behavior.
- Trigger a recording and watch `idle → recording → decoding → result`; confirm transitions read as calmer, not different in function.
- Confirm keyboard focus rings and the `Saved ✓` affordance still behave.

- [ ] **Step 5: Commit**

```bash
git add ui/src/app.css ui/src/lib/*.svelte
git commit -m "style(ui): calmer tokens and quieter state transitions (no structural change)"
```

---

## Self-Review

**Spec coverage:**
- Spec §1 "Cache the Silero VAD session" → Tasks 1 (`with_vad_shared`), 2 (`VadCache`), 3 (wiring). ✅
- Spec §1 "recurrent-state reset" → covered by existing recorder reset (Global Constraints note); verified in Task 3 Step 6. ✅
- Spec §1 "ownership refactor" → Task 1 (`with_vad_shared` shares the existing `Arc<Mutex>`). ✅
- Spec §2 "warm whisper on startup" → Task 4 Steps 1–2. ✅
- Spec §2 "warm on model change" → Task 4 Step 3. ✅
- Spec §3 "UI polish, keep structure" → Task 5. ✅
- Spec Non-goals (binary size, restructure, startup paint) → none introduced. ✅
- Spec Verification (VAD latency + back-to-back, whisper first-use latency + no-model no-op, UI visual) → Task 3 Step 6, Task 4 Step 5, Task 5 Step 4. ✅

**Placeholder scan:** No TBD/TODO; every code step shows complete code; every command has an expected result. ✅

**Type consistency:** `with_vad_shared(Arc<Mutex<Box<dyn VoiceActivityDetector>>>, usize, usize)` is defined in Task 1 and consumed in Task 3. `VadCache::get_or_build(&Path, FnOnce() -> Result<Box<dyn VoiceActivityDetector>>) -> Result<SharedVad>` defined in Task 2, consumed in Task 3. `warm_whisper(&AppHandle)` defined and called in Task 4. `SharedVad` alias and `build_recorder(&Settings, &mut VadCache)` names match across tasks. ✅

## Notes on testability (honest scope)

Tasks 1–2 are pure-library units with real failing-first tests. Tasks 3–4 are Tauri-binary wiring that needs real audio hardware and model files and has no lib target to unit-test, so they are verified by build + the existing integration test + scripted manual checks. Task 5 is visual and verified by inspection. This matches the codebase's existing pattern (the whisper cache is likewise inline and integration-verified).
