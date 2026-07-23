# AGENTS.md — PIE (Personal Intent Engine)

This file provides guidance to AI coding assistants working on this repository.

## Project Overview

PIE is a **Rust library + CLI** that acts as intelligent middleware between humans and AI models.
It extracts intent from speech/text, maintains personal memory, optimizes prompts, and routes
them to any LLM.

## Technology Stack

- **Language:** Rust (edition 2021)
- **Audio:** cpal (cross-platform audio I/O)
- **VAD:** Silero VAD (ONNX) — to be integrated
- **STT:** whisper.cpp via transcribe-cpp — to be integrated
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
- `extractor.rs` — Rule-based + small-model intent extraction
- `classifier.rs` — Conversation type (question, task, brainstorm, etc.)

### `src/memory/`
Personal memory that evolves over time.
- `store.rs` — JSON file-based storage with atomic writes
- `profile.rs` — User profile (role, tech stack, preferences)
- `patterns.rs` — Communication pattern tracking

### `src/optimizer/`
Prompt optimization with three modes.
- `compact.rs` — Remove filler, minimize tokens
- `balanced.rs` — Keep context, remove noise
- `enhanced.rs` — Enrich with missing context
- `adaptive.rs` — Auto-select mode based on context

### `src/llm/`
LLM provider routing.
- `openai.rs` — OpenAI-compatible API client
- `router.rs` — Provider/model selection

### `src/pipeline/`
Full pipeline orchestration.
- `engine.rs` — Wires: input -> stt -> intent -> memory -> optimize -> llm

## Reference Codebases

These repos are studied for architectural patterns. Do NOT copy-paste from them.
Use them as reference for HOW to solve specific problems.

- **Handy** (`~/handy/`) — Audio capture, VAD, streaming, cross-platform patterns
  - `src-tauri/src/audio_toolkit/audio/recorder.rs` — cpal streaming architecture
  - `src-tauri/src/audio_toolkit/vad/smoothed.rs` — SmoothedVad state machine
  - `src-tauri/src/managers/transcription.rs` — StreamRouter, multi-engine support
  - `src-tauri/src/transcription_coordinator.rs` — Single-thread lifecycle serialization
- **OpenSuperWhisper** (`~/OpenSuperWhisper/`) — whisper.cpp bridge, clipboard, UX
  - `OpenSuperWhisper/Whis/Whis.swift` — Complete whisper.cpp C bridge
  - `OpenSuperWhisper/Engines/WhisperEngine.swift` — VAD pre-gate pattern
  - `OpenSuperWhisper/Utils/ClipboardUtil.swift` — Keyboard layout handling

## Development Commands

```bash
cargo build                    # Build library + CLI
cargo run -- "text input"      # Run CLI with text
cargo test                     # Run all tests
cargo clippy                   # Lint
cargo fmt                      # Format
```

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
