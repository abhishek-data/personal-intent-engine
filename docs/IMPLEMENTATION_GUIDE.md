# PIE — Implementation Guide for AI Assistants

This document contains EVERYTHING needed to continue building PIE.
Read this FIRST before making any changes.

## Current Status (as of 2026-07-14)

### What's DONE (code written, not yet compiled)
1. Project structure — all directories and files created
2. Audio module — cpal recorder, SmoothedVad, resampler (needs compile test)
3. Intent extraction — rule-based extractor, classifier, schema
4. Memory — JSON store, user profile, communication patterns
5. Prompt optimizer — compact, balanced, enhanced, adaptive modes
6. LLM router — OpenAI-compatible API client
7. Pipeline engine — full orchestration wiring
8. CLI — clap-based entry point

### What's NOT done
1. **Compilation fix** — code was written on a machine without Rust. Run `cargo check` and fix ALL errors.
2. **Unit tests** — none exist yet
3. **whisper.cpp integration** — `src/stt/whisper.rs` is a stub
4. **Silero VAD integration** — `src/audio/vad.rs` has EnergyVad placeholder, needs Silero ONNX
5. **Tauri UI** — not started. See Phase 3 below.
6. **Streaming transcription** — not implemented (offline only)

## Known Issues to Fix First

### 1. Compilation Errors Expected
The code was written without a Rust compiler. Expect:
- Missing imports in some files
- Incorrect trait bounds
- Lifetime issues in vad.rs (the `push_frame` method returns references to `self.temp_out` which needs careful lifetime handling)
- The `FrameResampler` may need adjustment for rubato API

### 2. vad.rs Lifetime Issue
The `SmoothedVad::push_frame` returns `VadFrame::Speech(&self.temp_out)` — this is a self-referential borrow. You'll need one of:
- Return owned `Vec<f32>` instead of `&[f32]` (simpler, slight allocation)
- Use `Cow<'a, [f32]>` for zero-copy when possible
- Restructure to avoid self-referential borrow

### 3. AudioRecorder Architecture
Based on Handy's pattern. Key points:
- cpal callback pushes to mpsc channel
- Consumer thread reads, resamples, applies VAD
- Cmd enum: Start(VadPolicy), Stop(sender), Shutdown
- Config cache per device name (avoids 40-85ms HAL round-trips)

## Phase 2: Voice Input (Next Priority)

### Step 1: Add whisper.cpp dependency
```toml
# In Cargo.toml, uncomment:
transcribe-cpp = { version = "0.1.3", default-features = false, features = ["metal"] }
# For non-macOS:
transcribe-cpp = { version = "0.1.3", default-features = false }
```

### Step 2: Implement WhisperEngine
Reference: `~/handy/src-tauri/src/managers/transcription.rs` lines 1-100
- Use `transcribe_cpp::Session` for inference
- Load model from path
- Call `session.run(samples)` for transcription

### Step 3: Add Silero VAD
```toml
vad-rs = { git = "https://github.com/cjpais/vad-rs", default-features = false }
```
Reference: `~/handy/src-tauri/src/audio_toolkit/vad/silero.rs`
- Load ONNX model from path
- 30ms frames (480 samples at 16kHz)
- Threshold: 0.3

## Phase 3: Tauri UI (Desktop App)

### Architecture
```
src-tauri/          # Rust backend (Tauri)
├── Cargo.toml
├── tauri.conf.json
└── src/
    └── main.rs     # Tauri app entry, registers commands

taui-ui/            # Frontend (Svelte recommended over React)
├── package.json
└── src/
    ├── App.svelte
    ├── components/
    │   ├── SettingsPanel.svelte
    │   ├── ModelSelector.svelte
    │   ├── RecordingOverlay.svelte
    │   └── HistoryView.svelte
    └── lib/
        └── bindings.ts  # Auto-generated Tauri bindings
```

### UI Framework Choice: Svelte (NOT React)
Why Svelte over React:
- **No virtual DOM** — compiles to vanilla JS, smaller bundle
- **Faster startup** — critical for overlay/indicator windows
- **Less memory** — no React runtime overhead
- **Simpler state** — reactive declarations, no hooks complexity
- **Tauri-friendly** — smaller JS bundle = faster webview load

Reference for overlay: `~/OpenSuperWhisper/OpenSuperWhisper/Indicator/IndicatorWindow.swift`
- Floating window near caret position
- States: idle, connecting, recording, decoding, busy
- Blinking animation during recording

### Tauri Commands to Implement
```rust
#[tauri::command]
fn start_recording(app: AppHandle) -> Result<(), String>

#[tauri::command]
fn stop_recording(app: AppHandle) -> Result<String, String>

#[tauri::command]
fn get_settings(app: AppHandle) -> AppSettings

#[tauri::command]
fn update_settings(app: AppHandle, settings: AppSettings) -> Result<(), String>

#[tauri::command]
fn list_models(app: AppHandle) -> Vec<ModelInfo>

#[tauri::command]
fn download_model(app: AppHandle, model_id: String) -> Result<(), String>
```

## Key Reference Files (DO NOT COPY — use as reference only)

### Handy (~/handy/)
| File | What to learn from it |
|---|---|
| `src-tauri/src/audio_toolkit/audio/recorder.rs` | cpal streaming architecture, worker thread pattern |
| `src-tauri/src/audio_toolkit/vad/smoothed.rs` | SmoothedVad state machine (onset/hangover/prefill) |
| `src-tauri/src/audio_toolkit/vad/silero.rs` | Silero ONNX integration |
| `src-tauri/src/managers/transcription.rs` | StreamRouter, multi-engine, streaming |
| `src-tauri/src/transcription_coordinator.rs` | Single-thread lifecycle serialization |
| `src-tauri/src/clipboard.rs` | Cross-platform paste (enigo + Linux tools) |
| `src-tauri/src/shortcut/mod.rs` | Two-implementation shortcut system |
| `src-tauri/src/overlay.rs` | NSPanel (macOS), GTK layer shell (Linux) |
| `src-tauri/src/managers/model.rs` | Model catalog, HuggingFace Hub, download |

### OpenSuperWhisper (~/OpenSuperWhisper/)
| File | What to learn from it |
|---|---|
| `OpenSuperWhisper/Whis/Whis.swift` | whisper.cpp C bridge (init, encode, decode) |
| `OpenSuperWhisper/Engines/WhisperEngine.swift` | VAD pre-gate, abort callback, progress |
| `OpenSuperWhisper/AudioRecorder.swift` | File-based recording, stopTailDuration |
| `OpenSuperWhisper/ShortcutManager.swift` | Modifier-only hotkey, double-press, hold-to-record |
| `OpenSuperWhisper/Indicator/IndicatorWindow.swift` | Floating indicator near caret |
| `OpenSuperWhisper/Utils/ClipboardUtil.swift` | Keyboard layout awareness (UCKeyTranslate) |
| `OpenSuperWhisper/Utils/FocusUtils.swift` | AX API caret position with timeout |

## Coding Rules
1. Run `cargo fmt` and `cargo clippy` before every commit
2. No `unwrap()` in library code — use `?` or `.expect("reason")`
3. Doc comments on all public items (`/// ...`)
4. Commit format: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`
5. Max function length: ~50 lines
6. Feature-gate heavy deps (whisper, ONNX VAD)

## Environment Setup
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
cargo build

# Run
cargo run -- --verbose "test input"

# With LLM
OPENAI_API_KEY=sk-... cargo run -- "what is React?"
```
