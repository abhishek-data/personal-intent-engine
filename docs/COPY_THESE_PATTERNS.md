# PIE — Copy These Exact Patterns

This document tells Claude Code EXACTLY which functions to copy from Handy and OpenSuperWhisper.
Do NOT rewrite these from scratch. Copy the logic, adapt to Rust if needed.

Both repos must be cloned on your local machine:
```bash
git clone https://github.com/cjpais/handy.git ~/handy
git clone https://github.com/starmel/OpenSuperWhisper.git ~/OpenSuperWhisper
```

---

## 1. AUDIO CAPTURE — Copy from Handy

### File: `~/handy/src-tauri/src/audio_toolkit/audio/recorder.rs`

**Copy these exact patterns:**

| Pattern | Lines | What to copy |
|---|---|---|
| `enum Cmd` | 22-28 | Command enum (Start, Stop, Shutdown) — exact same structure |
| `enum AudioChunk` | 31-34 | Samples/EndOfStream — exact same |
| `pub enum VadPolicy` | 38-44 | Disabled/Offline/Streaming — exact same |
| `AudioFrameCallback` type | 71 | `Arc<dyn Fn(&[f32]) + Send + Sync + 'static>` |
| `AudioRecorder::new()` | 90-102 | Constructor pattern — same fields |
| `with_vad()` builder | 105-128 | Builder pattern with VadConfig |
| `with_audio_callback()` | 131-138 | Builder for streaming frames |
| `open()` method | 139-340 | **THE KEY METHOD** — cpal stream setup, worker thread, consumer loop. Copy the entire pattern: channel setup, device config, stream build, resample loop |
| `build_stream::<T>()` | 345-400 | Generic cpal stream builder — copy exactly |
| `get_preferred_config()` | 403+ | Device config with caching |

**What to adapt:**
- Remove Tauri-specific imports (`tauri::Manager`)
- Replace `log::info!` with `log::info!` (same)
- Keep `cpal` usage identical

**Do NOT rewrite:** The cpal stream setup, the worker thread pattern, the channel-based communication. These are battle-tested.

---

## 2. VAD — Copy from Handy

### File: `~/handy/src-tauri/src/audio_toolkit/vad/smoothed.rs`

**Copy the ENTIRE file (110 lines).** This is the SmoothedVad state machine.

| Pattern | Lines | What to copy |
|---|---|---|
| `struct SmoothedVad` | 1-16 | All fields (prefill, hangover, onset, frame_buffer, in_speech) |
| `SmoothedVad::new()` | 20-37 | Constructor — exact same |
| `push_frame()` | 41-96 | **THE CORE** — 4-state machine (false/true, true/true, true/false, false/false). Copy EXACTLY. This is the most important algorithm in the VAD. |
| `set_hangover_frames()` | 98-100 | Exact same |
| `reset()` | 102-108 | Clear all state — exact same |

**Constants (from `~/handy/src-tauri/src/audio_toolkit/vad/mod.rs`):**
```
VAD_PREFILL_FRAMES = 15        // 450ms pre-speech
VAD_OFFLINE_HANGOVER_FRAMES = 30  // 900ms post-speech
VAD_STREAMING_HANGOVER_FRAMES = 60  // 1.8s for streaming
VAD_ONSET_FRAMES = 3           // 90ms onset detection
```

### File: `~/handy/src-trai/src/audio_toolkit/vad/silero.rs`

**Copy when integrating Silero ONNX:**

| Pattern | Lines | What to copy |
|---|---|---|
| `struct SileroVad` | 13-16 | engine + threshold fields |
| `SileroVad::new()` | 19-28 | Load ONNX model, validate threshold |
| `push_frame()` impl | 33-51 | Frame validation, `engine.compute()`, probability check |
| `reset()` | 53-55 | `self.engine.reset()` — clears LSTM state |
| Constants | 9-11 | `SILERO_FRAME_MS = 30`, `SILERO_FRAME_SAMPLES = 480` |

---

## 3. STREAMING TRANSCRIPTION — Copy from Handy

### File: `~/handy/src-tauri/src/managers/transcription.rs`

**Copy these exact patterns:**

| Pattern | Lines | What to copy |
|---|---|---|
| `enum StreamCmd` | 88-96 | Feed/Finalize/Cancel command enum |
| `struct StreamRouter` | 101-108 | tx: Mutex + open: AtomicBool |
| `StreamRouter::new()` | 111-119 | Constructor |
| `StreamRouter::open()` | 121-128 | Create channel, set open=true |
| `StreamRouter::take()` | 130-136 | Take sender, set open=false |
| `StreamRouter::feed()` | 144-152 | **ZERO-OVERHEAD** — atomic load check first, then mutex lock only if open. Copy EXACTLY. |
| `StreamRouter::is_open()` | 154-156 | Atomic load |

**What to adapt:**
- Remove Tauri `AppHandle` dependencies
- Use `std::sync::mpsc` (same as Handy)

**Do NOT rewrite:** The feed() method's atomic-first-then-mutex pattern. This is clever optimization.

---

## 4. TRANSCRIPTION COORDINATOR — Copy from Handy

### File: `~/handy/src-tauri/src/transcription_coordinator.rs`

**Copy these exact patterns:**

| Pattern | Lines | What to copy |
|---|---|---|
| `DEBOUNCE` constant | 10 | 30ms debounce |
| `RELEASE_GRACE` constant | 11 | 50ms grace for PTT release |
| `enum PttAction` | 14-18 | Passthrough/DeferRelease/CancelRelease |
| `struct PendingRelease` | 20-24 | binding_id, hotkey_string, deadline |
| `enum Command` | 27-39 | Input/Cancel/ProcessingFinished |
| `enum Stage` | 41-45 | Idle/Recording/Processing |
| `classify_ptt_event()` | 47-70 | **THE KEY FUNCTION** — PTT state machine. Copy EXACTLY. |
| Coordinator thread loop | 83-250 | Single-thread command processing, recv_timeout for pending release |

**Do NOT rewrite:** The PTT debounce/grace logic. Handy's implementation handles edge cases (X11 auto-repeat, rapid double-tap).

---

## 5. CLIPBOARD — Copy from OpenSuperWhisper

### File: `~/OpenSuperWhisper/OpenSuperWhisper/Utils/ClipboardUtil.swift`

**Copy these exact patterns (adapt Swift to Rust):**

| Pattern | Lines | What to copy |
|---|---|---|
| `clipboardRestoreDelay` | 12 | 1.5 seconds — use this exact value |
| `insertText()` flow | 30-54 | Save clipboard -> write text -> paste -> delay -> restore if unchanged |
| `changeCountAfterCopy` | 39 | Save pasteboard changeCount before paste |
| `restoreIfUnchanged()` | 56-62 | **KEY PATTERN** — only restore if changeCount matches. Prevents clobbering user clipboard. |
| `sendCmdV()` | 68-95 | Layout-aware keycode resolution |
| `isQwertyCommandLayout()` | 98-120 | Detect Dvorak-QWERTY command layouts |
| `findKeycodeForCharacter()` | 122-160 | UCKeyTranslate for non-QWERTY layouts |

**What to adapt for Rust/macOS:**
- `NSPasteboard` -> `objc` crate or `clipboard` crate
- `CGEvent` -> `core-graphics` crate
- `UCKeyTranslate` -> `core-foundation` crate
- `changeCount` -> platform clipboard API

**For Linux/Windows:** Use Handy's clipboard.rs patterns instead:
- `~/handy/src-tauri/src/clipboard.rs` lines 85+ for Linux key combo tools
- `~/handy/src-tauri/src/clipboard.rs` lines 16-80 for paste_via_clipboard

---

## 6. HOTKEYS — Copy from Both

### Handy: `~/handy/src-tauri/src/shortcut/mod.rs`
- Lines 32-90: Two-implementation system (Tauri plugin + handy_keys)
- Runtime fallback: if HandyKeys fails, switch to Tauri and persist

### OpenSuperWhisper: `~/OpenSuperWhisper/OpenSuperWhisper/ShortcutManager.swift`
- Lines 125-174: `handleKeyDown()` — optimistic state update, anchor resolution with timeout
- Lines 182-207: `resolveAnchorPoint()` — 150ms timeout for AX API
- Lines 209-224: `handleKeyUp()` — hold-to-record stop

**Copy for PIE:**
- Use Handy's two-implementation pattern (rdev + fallback)
- Use OpenSuperWhisper's optimistic recording start (don't wait for device)
- Use OpenSuperWhisper's hold-to-record (300ms threshold)

---

## 7. INDICATOR/OVERLAY — Copy from OpenSuperWhisper

### File: `~/OpenSuperWhisper/OpenSuperWhisper/Indicator/IndicatorWindow.swift`

**Copy these exact patterns:**

| Pattern | Lines | What to copy |
|---|---|---|
| `enum RecordingState` | 5-13 | idle/connecting/recording/decoding/busy/noMicrophone |
| `cancelConfirmationThreshold` | 22 | 10 seconds |
| `cancelConfirmationWindow` | 23 | 5 seconds |
| `isTranscriptionBusy` | 70-72 | Check both isTranscribing and isProcessing |
| `startRecording()` | 89-112 | Optimistic state, busy check, blinking |
| `startDecoding()` | 140-200 | Stop recording, start transcription, queue if busy |
| `handleCancelRequest()` | 114-128 | Confirmation for long recordings |

**For Tauri/Svelte overlay:**
- Copy state machine exactly
- Use Svelte reactive declarations instead of Combine
- CSS animations for blinking (not timer-based)

---

## 8. FOCUS/CARET POSITION — Copy from OpenSuperWhisper

### File: `~/OpenSuperWhisper/OpenSuperWhisper/Utils/FocusUtils.swift`

**Copy these exact patterns:**

| Pattern | Lines | What to copy |
|---|---|---|
| `axCallTimeoutSeconds` | 25 | 0.25 seconds — prevents AX hangs |
| `getFocusedElement()` | 27-44 | AXUIElement system-wide + focused element |
| `getCaretRect()` | 47-105 | Bounds for range + character fallback |
| `getInputAnchorPoint()` | 135-180 | Caret -> element frame -> nil fallback |
| `isValidCaretRect()` | 189-191 | `rect.height > 0` check |
| `convertAXPointToCocoa()` | 195-210 | AX coordinate -> Cocoa coordinate |

**Platform-specific:**
- macOS: Copy OpenSuperWhisper's AX API approach
- Windows: Use UI Automation (different API)
- Linux: Use AT-SPI2 (different API)

---

## 9. MODEL MANAGEMENT — Copy from Handy

### File: `~/handy/src-tauri/src/managers/model.rs`

**Copy these exact patterns:**

| Pattern | Lines | What to copy |
|---|---|---|
| `enum EngineType` | 24-33 | TranscribeCpp/Parakeet/Moonshine/etc |
| `enum ModelSource` | 37-46 | Url/HuggingFace/Local |
| `struct ModelInfo` | 48-68 | Full model metadata struct |
| `struct ModelDescriptor` | 130-155 | Catalog model spec |
| `struct QuantFile` | 115-120 | Quantization file info |
| `default_quant_file()` | 123-128 | Pick default quant |

**For PIE:** Use HuggingFace Hub for model downloads (same as Handy).

---

## Summary: What to Copy vs What to Write

### COPY EXACTLY (don't rewrite):
1. SmoothedVad state machine (Handy smoothed.rs)
2. StreamRouter atomic-first feed pattern (Handy transcription.rs)
3. TranscriptionCoordinator PTT logic (Handy transcription_coordinator.rs)
4. AudioRecorder worker thread + channel pattern (Handy recorder.rs)
5. Clipboard changeCount verification (OpenSuperWhisper ClipboardUtil.swift)
6. Indicator state machine (OpenSuperWhisper IndicatorWindow.swift)
7. AX call timeout pattern (OpenSuperWhisper FocusUtils.swift)

### ADAPT (copy logic, change language/API):
1. SileroVad ONNX integration (Handy silero.rs -> Rust)
2. Clipboard save/restore (OpenSuperWhisper Swift -> Rust)
3. Keyboard layout handling (OpenSuperWhisper UCKeyTranslate -> platform-specific)
4. Hotkey system (Handy rdev + OpenSuperWhisper hold-to-record)

### WRITE NEW (PIE-specific):
1. Intent extraction (extractor.rs, classifier.rs, schema.rs)
2. Memory store (store.rs, profile.rs)
3. Prompt optimizer (compact.rs, balanced.rs, enhanced.rs, adaptive.rs)
4. LLM router (openai.rs, router.rs)
5. Pipeline orchestration (engine.rs)
