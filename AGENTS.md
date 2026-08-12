# AGENTS.md — PIE (Personal Intent Engine)

This file provides guidance to AI coding assistants working on this repository.

## Project Overview

PIE is a **Rust library + CLI** that acts as intelligent middleware between humans and AI models.
It extracts intent from speech/text, maintains personal memory, optimizes prompts, and routes
them to any LLM.

## Technology Stack

- **Language:** Rust (edition 2021)
- **Audio:** cpal (cross-platform audio I/O)
- **VAD:** Silero VAD (ONNX), behind the `vad` feature
- **STT:** whisper.cpp via transcribe-cpp, behind the `whisper` feature
- **LLM API:** reqwest + OpenAI-compatible JSON API
- **Serialization:** serde + serde_json
- **Async:** tokio
- **CLI:** clap

## Architecture Principles

1. **Library-first:** All logic in `src/lib.rs` exports. CLI is a thin wrapper.
2. **Module isolation:** Each module (audio, stt, intent, memory, optimizer, llm) has a clean `mod.rs` public API.
3. **No global state:** Pass `Arc<T>` or references. No `lazy_static!` for core state.
4. **Error propagation:** Use `anyhow::Result` for application code, `thiserror` for library errors.
5. **Feature-gated heavy deps:** whisper.cpp and ONNX VAD behind cargo features.

## Module Responsibilities

### `src/audio/`
Real-time audio capture via cpal. Produces 16kHz mono f32 frames.
- `recorder.rs` — cpal stream management, device enumeration
- `vad.rs` — Voice Activity Detection (Silero wrapper + smoothed state machine)
- `resampler.rs` — rubato-based resampling to 16kHz

### `src/stt/`
Speech-to-Text. Accepts f32 samples, returns String.
- `whisper.rs` — whisper.cpp integration via transcribe-cpp

### `src/intent/`
Core PIE logic. Extracts structured intent from text.
- `schema.rs` — Intent struct (objective, context, constraints, confidence, etc.)
- `extractor.rs` — Rule-based fast path + LLM-backed extraction (see
  "Intent extraction" below)
- `classifier.rs` — Conversation type (question, task, brainstorm, etc.)

### `src/memory/`
Personal memory that evolves over time.
- `store.rs` — JSON file-based storage with atomic writes
- `profile.rs` — User profile (role, tech stack, preferences)
- `patterns.rs` — Communication pattern tracking

### `src/optimizer/`
Prompt optimization with **exactly two modes** — the whole module is one
`mod.rs`. See "Optimizer modes" below before adding a third.

### `src/llm/`
LLM provider routing.
- `client.rs` — `LlmClient` trait (the LLM seam) + `RouterLlmClient` adapter
- `openai.rs` — OpenAI-compatible API client
- `router.rs` — Provider/model selection

### `src/pipeline/`
Full pipeline orchestration.
- `engine.rs` — Wires: input -> stt -> intent -> memory -> optimize -> llm

## Load-Bearing Conventions

These were decided deliberately. Read before changing the intent, optimizer,
or pipeline modules.

### Seams (traits with 2+ adapters)

Every external capability sits behind a trait so callers and tests cross the
same interface. Do not reach past these to a concrete type:

| Seam | Trait | Adapters |
|---|---|---|
| Speech-to-text | `stt::SttEngine` | `WhisperEngine` (feature-gated), test fakes |
| Voice activity | `audio::VoiceActivityDetector` | `SileroVad`, `EnergyVad` |
| Audio capture | `audio::AudioCapture` | `AudioRecorder` (cpal), `FakeCapture` (test) |
| LLM completion | `llm::LlmClient` | `RouterLlmClient`, `MockLlmClient` (test) |

New external dependency (a second STT engine, a different audio backend)?
Add an adapter, don't add a branch in the caller. New *test* that needs an
LLM? Use `MockLlmClient` via `tests/mock_llm.rs` — never hit the network in
tests.

### Intent extraction: rules are the fast path, not the fallback plan

`IntentExtractor` has two entry points and they are not interchangeable:

- `extract(text)` — rule-based, synchronous, deterministic. Correct for short
  direct commands.
- `extract_with_llm(text, client, ctx)` — routes on input shape:
  ≤`LLM_EXTRACTION_WORD_THRESHOLD` (15) words with no `?` → rules; otherwise
  the LLM.

The rule-based extractor **cannot understand rambling speech** — it echoes the
input back and reports High confidence while doing it (measured, see
`docs/INTENT_EXTRACTION_TEST_RESULTS.md`). Do not "improve" it with more
keyword lists; that approach was removed on purpose. It survives as the fast
path for short commands and as the offline fallback.

`extract_with_llm` never fails: LLM error, non-JSON reply, or missing fields
all fall back to `extract`. LLM replies are often fenced in ```json, so parse
via `find_json_object`, not bare `serde_json::from_str`.

### Optimizer modes: two, and it should stay two

`OptimizationMode` is `Direct | Enhanced`. It was 5 modes
(compact/balanced/enhanced/adaptive/refine); they were deleted because the
variation was prompt-template phrasing, not behaviour, and users had no basis
to choose. Apply the deletion test before adding a mode: if the new mode's
complexity would reappear across callers, it earns its keep — otherwise it's a
template, and templates belong inside `enhanced()`.

- `Direct` — pass the objective through. Short, clear commands.
- `Enhanced` — structure objective + constraints + questions. Drives
  LLM-backed intent extraction in the pipeline.

### Pipeline mode strings are permissive by design

`PieEngine::process(input, mode)` takes a `&str`, not the enum, because saved
user settings and the CLI both feed it:

- `"direct"` / `"enhanced"` — honored exactly.
- **anything else** — auto-selects from input complexity
  (>`ENHANCED_WORD_THRESHOLD` (20) words, or contains `?`, → `Enhanced`).

The catch-all is deliberate: it means legacy settings files still holding
`"balanced"`, `"compact"`, `"adaptive"`, or `"refine"` keep working after
upgrade instead of erroring. Don't tighten this into an exhaustive match
without a settings migration.

## Provenance & Attribution (do not break)

The audio-capture and desktop layers are **derived work** from two MIT-licensed
projects. MIT permits this and requires only that the copyright notice be
retained in derived work. That notice lives in two places:

- `NOTICE` — upstream copyright lines
- `README.md` → Acknowledgements

**Never delete either.** Removing them while the derived code ships turns a
compliant project into a license violation. If you rewrite a derived module
from scratch, the notice for that upstream can go — but only once the derived
code is actually gone.

Do not introduce new copied code. New functionality is written for this
codebase, behind the seams listed above.

## Development Commands

```bash
cargo build                    # Build library + CLI
cargo run -- "text input"      # Run CLI with text
cargo test                     # Run all tests
cargo clippy                   # Lint
cargo fmt                      # Format
```

### Running the desktop app (read this before debugging a blank window)

**Never build the desktop app with plain `cargo build` / `cargo build --release`.**
Doing so produces a binary whose webview points at `devUrl`
(`http://localhost:5173`). With no Vite server on that port the window opens
**completely blank white**, which looks like a frontend crash but is not one.

Only the Tauri CLI runs `beforeBuildCommand` and embeds `frontendDist`:

```bash
cargo tauri dev                # dev: starts Vite + the app together
cargo tauri build --no-bundle  # real standalone binary, frontend embedded
cargo tauri build              # full .app / .dmg bundle
```

Two consequences worth remembering:

- `npm --prefix ui run build` on its own changes nothing about an already-built
  binary. The frontend is embedded at Tauri-CLI build time, not read from disk.
- To tell the two apart: start a Vite server on 5173, launch the app, and check
  `lsof -nP -iTCP:5173 | grep ESTABLISHED`. A correctly built app makes **zero**
  connections; a `cargo build` one connects and depends on the dev server.

The app is a tray app. Closing the window hides it rather than quitting, and
`Cmd-Q` while it is focused quits it. Creating the NSPanel overlay briefly flips
the activation policy, which can order the main window out on launch — reopen it
from the menu-bar icon.

## Commit Convention

```
feat: add intent extraction schema
fix: handle empty audio buffer in recorder
docs: add architecture diagram
refactor: extract VAD into separate module
test: add intent extractor unit tests
```

## Coding Standards

- Run `cargo fmt` and `cargo clippy` before committing
- No `unwrap()` in library code — use `?` or `.expect("reason")`
- Doc comments on all public items (`/// ...`)
- Max function length: ~50 lines. Extract helpers.
- Prefer `impl Trait` over `Box<dyn Trait>` where possible

## macOS Signing (do not break)

macOS releases are signed with a **stable self-signed cert** (`PIE Developers`,
leaf SHA-1 `d318…d854`). macOS TCC pins users' Accessibility/Microphone grants
to that cert, so **signing with any different identity — a regenerated `.p12` or
an ad-hoc fallback — silently breaks every user's permissions on update.** Never
regenerate the cert to "refresh" it. Full rules, the CI pin-check, and the
rotation procedure: [docs/signing.md](docs/signing.md).
