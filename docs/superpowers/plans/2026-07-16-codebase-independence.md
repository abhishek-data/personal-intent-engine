# Phase 3: Codebase Independence Plan

**Goal:** Remove all references and dependencies on Handy and OpenSuperWhisper. Make PIE fully self-contained so upstream changes can never break it.

**Approach:** This is a task list for an AI coding agent (Claude Code). Each task is self-contained with clear inputs, outputs, and verification steps. The agent should research alternatives where noted and make the best choice.

**Rules:**
- Do NOT modify any `.md` files in `docs/` or `AGENTS.md` — those are development guides, not shipped code
- Do NOT modify `README.md` Acknowledgements section — credits stay
- Every change must pass `cargo build` and `cargo test` after
- Commit after each logical task group

---

## Task Group 1: Replace `vad-rs` Git Dependency

**Problem:** `vad-rs = { git = "https://github.com/cjpais/vad-rs" }` in `Cargo.toml` is a git dependency on a personal repo. If cjpais changes or deletes it, PIE breaks.

**Current usage:** Only `src/audio/silero.rs` uses it — 4 API calls:
- `Vad::new(&model_path, sample_rate)` — creates engine
- `vad.compute(frame)` — runs inference, returns `{ prob: f32 }`
- `vad.reset()` — clears LSTM state

**Research task for the agent:**

1. Check if `vad-rs` is published on crates.io. If yes, switch to a versioned dependency.
2. If not on crates.io, evaluate these alternatives:
   - **`ort` crate** (ONNX Runtime for Rust) — load Silero ONNX directly. `ort` is on crates.io with 500+ stars. Run the ONNX model manually: load model, create session, run inference on 512-sample frames.
   - **Fork `vad-rs`** to `abhishek-data/vad-rs`, pin to a specific commit.
   - **Inline the VAD logic** — `vad-rs` is thin (~200 lines). Copy its core into `src/audio/silero_vad_engine.rs` and remove the dependency entirely.

3. **Preferred approach: Copy the code into PIE.** `vad-rs` is thin (~200 lines). Absorb its core into `src/audio/silero_vad_engine.rs` and remove the external dependency entirely. If crates.io has a stable version, that's the only exception where we'd keep a dependency. Do NOT fork — forks still depend on someone else's repo.

**Files to change:**
- `Cargo.toml` — remove or replace `vad-rs` dependency
- `src/audio/silero.rs` — update imports and API calls if the interface changes

**Verification:** `cargo build --features vad && cargo test --features vad`

---

## Task Group 2: Replace `tauri-nspanel` Git Dependency

**Problem:** `tauri-nspanel = { git = "https://github.com/ahkohd/tauri-nspanel", branch = "v2.1" }` is a git dependency on a personal repo.

**Current usage:** Only `src-tauri/src/overlay.rs` (macOS-only section, ~30 lines). Creates an NSPanel for the floating recording indicator — a non-activating, always-on-top panel that doesn't steal focus.

**Research task for the agent:**

1. Check if `tauri-nspanel` is on crates.io. If yes, switch to a versioned dependency.
2. If not on crates.io, evaluate:
   - **Fork to `abhishek-data/tauri-nspanel`**, pin to a specific commit.
   - **Use raw Cocoa/objc bindings** — the NSPanel code is ~30 lines. Use `objc2` or `cocoa` crate to create an NSPanel directly. The overlay.rs already has the logic; just replace the `tauri_nspanel` API calls with raw AppKit calls.
   - **Check if Tauri 2 has built-in NSPanel support** in newer versions.

3. **Preferred approach: Copy the code into PIE.** The NSPanel usage is only ~30 lines in overlay.rs. Use raw Cocoa/objc2 bindings to create the NSPanel directly — copy the specific patterns we need from tauri-nspanel into our own overlay.rs. Remove the external dependency. If crates.io has a stable version, that's the only exception. Do NOT fork.

**Files to change:**
- `src-tauri/Cargo.toml` — remove or replace `tauri-nspanel` dependency
- `src-tauri/src/overlay.rs` — update macOS platform module
- `src-tauri/src/main.rs` — remove `tauri_nspanel::init()` plugin registration (line ~671)

**Verification:** `cargo build` on macOS (or cross-check that the `#[cfg(target_os = "macos")]` block compiles)

---

## Task Group 3: Replace `blob.handy.computer` URLs

**Problem:** The Silero VAD ONNX model is downloaded from `https://blob.handy.computer/silero_vad_v4.onnx`. If Handy's server goes down, model downloads break.

**Research task for the agent:**

1. Check if Silero VAD v4 ONNX is available on HuggingFace at `snakers4/silero-vad` or `onnx-community/silero-vad`.
2. Check if it's available on GitHub releases in the official Silero repo.
3. Download the file and host it under PIE's own GitHub releases, or point to a stable public URL.

**Files to change:**
- `src-tauri/src/models.rs` line 83 — change the URL
- `src/main.rs` line 299 — update the help text URL
- `src-tauri/src/models.rs` line 40 — update the comment

**Verification:** `cargo build` and verify the URL resolves (curl the new URL)

---

## Task Group 4: Clean Code Comments

**Problem:** 10 code comments reference Handy by name. These should describe PIE's own architecture.

**Files and changes:**

| File | Line | Current | Replace with |
|---|---|---|---|
| `src-tauri/src/paste.rs` | 3 | `Handy's clipboard-paste flow` | `Clipboard-paste flow: save clipboard, write text, send platform paste keystroke, restore` |
| `src-tauri/src/paste.rs` | 22 | `(Handy manages it the same way.)` | Remove the parenthetical entirely |
| `src-tauri/src/models.rs` | 40 | `VAD ONNX from Handy's mirror` | `Silero VAD ONNX model` |
| `src/audio/vad.rs` | 37 | `matching Handy's current tuning` | `tuned for 30ms frames at 16kHz` |
| `src/audio/vad.rs` | 45 | `Based on Handy's SmoothedVad architecture` | `Smoothed VAD with onset detection, hangover tail, and prefill buffering` |
| `src/audio/silero.rs` | 9 | `matching Handy's tuning` | `empirically tuned for speech detection` |
| `src/audio/resampler.rs` | 9 | `Based on Handy's FrameResampler` | `Frame-based resampler: accepts native-rate frames, outputs 16kHz for whisper` |
| `src/audio/recorder.rs` | 59 | `Architecture based on Handy's recorder` | `Audio recorder: opens device, streams frames through VAD, collects samples` |
| `src/stt/stream.rs` | 27 | `Handy's zero-overhead feed` | `zero-overhead feed` |
| `Cargo.toml` | 60 | `mirrors Handy's per-target backend selection` | `platform-specific backend: Metal on macOS, CPU elsewhere` |

**Verification:** `cargo build && cargo test`

---

## Task Group 5: Rename Copied Struct/Constant Names

**Problem:** Several struct and constant names are directly copied from Handy. Renaming makes the codebase feel original and avoids confusion.

**Renames:**

| Current name | New name | Location |
|---|---|---|
| `SmoothedVad` | `VadPipeline` | `src/audio/vad.rs`, `src/audio/mod.rs`, `src-tauri/src/main.rs`, `src/main.rs`, tests |
| `FrameResampler` | `AudioResampler` | `src/audio/resampler.rs`, `src/audio/mod.rs`, `src/audio/recorder.rs`, `src/stt/mod.rs`, tests |
| `StreamRouter` | `TranscriptRouter` | `src/stt/stream.rs`, `src/stt/mod.rs`, `src-tauri/src/main.rs`, tests |
| `VAD_PREFILL_FRAMES` | `VAD_CONTEXT_FRAMES` | `src/audio/vad.rs`, `src/audio/mod.rs`, `src-tauri/src/main.rs`, `src/main.rs`, tests |
| `VAD_ONSET_FRAMES` | `VAD_SPEECH_THRESHOLD_FRAMES` | same locations |
| `VAD_OFFLINE_HANGOVER_FRAMES` | `VAD_HANGOVER_FRAMES` | same locations |
| `VAD_STREAMING_HANGOVER_FRAMES` | `VAD_STREAM_HANGOVER_FRAMES` | same locations |
| `SILERO_DEFAULT_THRESHOLD` | `PIE_VAD_THRESHOLD` | `src/audio/silero.rs`, `src/audio/mod.rs`, `src-tauri/src/main.rs`, `src/main.rs`, tests |

**Approach:** Use IDE-level rename (or `sed`) to ensure all references update. The agent should search for each name across the entire codebase before renaming.

**Verification:** `cargo build && cargo test` — no compile errors means all references were caught.

---

## Task Group 6: Update Dependencies in Cargo.toml

**Problem:** Some comments in `Cargo.toml` reference Handy. Clean these up.

**Files to change:**
- `Cargo.toml` line 60 — comment mentions Handy (covered in Task 4)
- Verify all other deps are from crates.io or official sources

**Verification:** `cargo build`

---

## Execution Order

The recommended order minimizes risk:

1. **Task Group 3** (URLs) — simplest, no code logic changes
2. **Task Group 4** (comments) — text-only changes, zero risk
3. **Task Group 1** (vad-rs) — research first, then implement
4. **Task Group 2** (tauri-nspanel) — research first, then implement
5. **Task Group 5** (renames) — do last since it touches the most files
6. **Task Group 6** (cleanup) — final sweep

Each task group should be its own commit. If a task fails or is too risky, skip it and document why — a fork is always the fallback.

---

## Decision Matrix for the Agent

When choosing between options, use this priority:

| Option | When to use |
|---|---|
| **Copy the code into PIE** | Default approach. Absorb small, stable code directly. No external dependency. |
| **crates.io version exists** | Only exception to copying — published crates are versioned and stable. |
| **Write from scratch using reference** | When the pattern is simple and we want PIE-idiomatic code. Use Handy/OpenSuperWhisper as reference only. |
| **Fork the repo** | Last resort, only if the code is too large to inline and no crates.io version exists. |

**Core principle:** PIE should own every line of code it depends on. External references are for learning, not for runtime dependencies.

---

## What NOT to Touch

- `docs/` directory — all markdown files are dev guides
- `AGENTS.md` — reference for AI assistants
- `README.md` Acknowledgements — credits stay
- `transcribe-cpp` — published crate on crates.io, stable
- `cpal`, `rubato`, `enigo`, `reqwest`, `serde`, `tokio` — published crates, fine
- `tauri-plugin-global-shortcut`, `tauri-plugin-clipboard-manager` — official Tauri plugins
