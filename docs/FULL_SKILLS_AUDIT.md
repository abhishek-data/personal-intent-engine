# PIE — Full Skills-Based Audit (Rust + Tauri + Svelte)

**Date:** July 14, 2026
**Commit:** 72f533e
**Audited against:** rust-best-practices, tauri-best-practices skills

---

## RUST BEST PRACTICES AUDIT

### ✅ PASS: No &Vec<T> or &String in function params
All function parameters use `&[T]` and `&str` correctly.

### ✅ PASS: impl Default on appropriate types
5 types have `impl Default`: LlmRouter, UserProfile, StreamRouter, Intent, IntentExtractor.

### ✅ PASS: Builder pattern used
AudioRecorder uses `with_vad()`, `with_level_callback()`, `with_audio_callback()` builders.

### ✅ PASS: Feature-gated heavy deps
whisper and vad are behind `#[cfg(feature = "...")]`.

### ✅ PASS: Library-first architecture
All logic in `src/lib.rs` exports. CLI and Tauri are thin wrappers.

### ⚠️ ISSUE: unwrap() in test code only (acceptable)
Found in `store.rs:149-150`, `vad.rs:247-315`, `stream.rs:182-184` — all inside `#[cfg(test)]` blocks. This is acceptable per Rust conventions.

### ⚠️ ISSUE: expect() with messages (acceptable)
16 instances of `.expect("reason")` — all have descriptive messages. Acceptable per skill rules.

### ❌ VIOLATION: #[must_use] missing on key functions
**Skill rule:** "Use #[must_use] on functions whose return value shouldn't be ignored"
**Found:** 0 instances of `#[must_use]` in entire codebase.
**Missing on:**
- `AudioRecorder::new()`
- `AudioRecorder::with_vad()`
- `AudioRecorder::with_audio_callback()`
- `IntentExtractor::new()`
- `IntentExtractor::extract()`
- `MemoryStore::load()`
- `SmoothedVad::new()`
- `FrameResampler::new()`
- `WhisperEngine::load()`
- `LlmRouter::new()`
- All optimizer `optimize()` functions

### ❌ VIOLATION: clone() in hot paths (audio callback)
**Skill rule:** "Prefer borrowing over cloning"
**Critical instances:**
- `recorder.rs:350` — `output_buffer.clone()` in the audio consumer loop (every 30ms frame)
- `recorder.rs:585` — `raw.clone()` in the level callback
- `whisper.rs:135` — `result.clone()` in streaming worker
- `recorder.rs:356` — `config.clone().into()` on every stream build

**Fix:** For `output_buffer.clone()` — use `std::mem::take(&mut output_buffer)` to move without allocation. For `raw.clone()` — pass a slice reference if the callback can accept `&[f32]`.

### ❌ VIOLATION: Vec::new() without capacity in hot paths
**Skill rule:** "Use Vec::with_capacity() when size is known"
**Instances in hot paths:**
- `recorder.rs:323` — `output_buffer: Vec::new()` in audio consumer (resized every frame)
- `resampler.rs:147` — `out: Vec::new()` in test helper (minor)
- `vad.rs:80` — `temp_out: Vec::new()` in SmoothedVad (allocated on every speech onset)

**Fix:** `output_buffer` should use `Vec::with_capacity(FRAME_SAMPLES * 2)`. `temp_out` should use `Vec::with_capacity(FRAME_SAMPLES * (prefill_frames + 1))`.

### ❌ VIOLATION: Missing doc comments on 30+ public items
**Skill rule:** "Doc comments on all public items (`/// ...`)"
**Found:** 74 public items, ~30 missing `///` doc comments.
**Missing on:** LlmRouter, OpenAiClient, all optimizer `optimize()` functions, MemoryStore, CommunicationPatterns fields, UserProfile, ResponseStyle, DetailLevel, VoiceActivityDetector, VadFrame, VadPolicy, SmoothedVad, PassthroughVad, EnergyVad, SileroVad, FrameResampler, AudioRecorder, WhisperEngine, SttEngine, StreamCmd, StreamRouter, classify().

### ❌ VIOLATION: thiserror listed but not used
**Skill rule:** "Prefer thiserror derives over manual impl"
**Found:** `thiserror = "2"` in Cargo.toml but zero `#[derive(thiserror::Error)]` in source.
**Fix:** Either define proper error types with thiserror, or remove the dependency.

### ❌ VIOLATION: uuid and chrono listed but unused
**Found:** `uuid` and `chrono` in Cargo.toml but not imported anywhere in source.
**Fix:** Remove from Cargo.toml to reduce compile time.

---

## TAURI BEST PRACTICES AUDIT

### ❌ CRITICAL: app.state() used instead of try_state()
**Skill rule:** "Use `app.try_state()` not `app.state()` (avoid panics)"
**Found:** 7 instances of `app.state::<AppState>()` and 0 instances of `try_state`.
**Locations:**
- `src-tauri/src/main.rs:77` — do_start_recording
- `src-tauri/src/main.rs:103` — do_stop_recording
- `src-tauri/src/main.rs:126` — do_cancel_recording
- `src-tauri/src/main.rs:169` — hotkey handler
- `src-tauri/src/main.rs:194` — hotkey handler
- `src-tauri/src/main.rs:204` — hotkey handler
- `src-tauri/src/main.rs:394` — setup

**Risk:** If AppState is not managed (e.g., during shutdown), these will panic.
**Fix:** Replace all `app.state::<T>()` with `app.try_state::<T>().ok_or("...")?`.

### ❌ CRITICAL: CSP disabled
**Skill rule:** "Use CSP headers"
**Found:** `"csp": null` in tauri.conf.json:28
**Risk:** Webview can load arbitrary external resources.
**Fix:** `"csp": "default-src 'self'; connect-src 'self' https://api.openai.com https://api.openrouter.ai; style-src 'self' 'unsafe-inline'"`

### ❌ HIGH: No capability-based permissions
**Skill rule:** "Use capability-based permissions (Tauri v2)"
**Found:** capabilities/default.json only has `"core:default"`. Missing:
- `clipboard-manager:allow-write-text` (needed for paste)
- `clipboard-manager:allow-read-text` (needed for clipboard save/restore)
- `global-shortcut:allow-register` (needed for hotkey)

**Fix:** Add these permissions to capabilities/default.json.

### ❌ HIGH: No event listener cleanup in Svelte
**Skill rule:** "Clean up listeners with unlisten() on component unmount"
**Found:** App.svelte has 5 `listen()` calls in `onMount()` but does NOT return a cleanup function.
**Locations:**
- Line 151: `listen("pie://download", ...)`
- Line 166: `listen("pie://models-changed", ...)`
- Line 167: `listen("pie://state", ...)`
- Line 171: `listen("pie://outcome", ...)`
- Line 176: `listen("pie://error", ...)`

**Risk:** Event listeners accumulate on hot-reload or component re-mount, causing memory leaks and duplicate handlers.
**Fix:**
```javascript
onMount(async () => {
    const unlisteners = [
        await listen("pie://download", (e) => { ... }),
        await listen("pie://state", (e) => { ... }),
        // ...
    ];
    return () => unlisteners.forEach(u => u());
});
```

### ⚠️ ISSUE: lock().expect() 14 times (potential panics)
**Skill rule:** "Never hold a lock across an .await"
**Found:** 14 instances of `.lock().expect("...")` in main.rs. All have messages (good), but some are in async contexts.
**Risk:** If a lock is poisoned (a previous holder panicked), the app crashes.
**Fix:** Use `.lock().unwrap_or_else(|e| e.into_inner())` for poisoned mutex recovery, or use `tokio::sync::Mutex` for async contexts.

### ⚠️ ISSUE: Events silently drop errors
**Found:** 8 instances of `let _ = app.emit(...)` which discards emit errors.
**Fix:** Log emit errors: `if let Err(e) = app.emit(...) { log::warn!("emit failed: {e}"); }`

### ✅ PASS: Overlay configuration correct
- macOS: NSPanel with `can_become_key_window: false`, `is_floating_panel: true`
- Non-macOS: `decorations(false)`, `transparent(true)`, `always_on_top(true)`, `skip_taskbar(true)`
- Both correct per skill rules.

### ✅ PASS: Commands return Result<T, String>
All Tauri commands return `Result<_, String>` as required.

### ✅ PASS: Events use namespaced keys
All events use `pie://` prefix: `pie://state`, `pie://outcome`, `pie://error`, `pie://download`, `pie://models-changed`.

---

## SVELTE AUDIT

### ❌ ISSUE: App.svelte is 1219 lines (too large)
**Rule:** Components should be focused and small.
**Fix:** Split into: RecordingView.svelte, SettingsPanel.svelte, ModelManager.svelte, HotkeyRecorder.svelte, LlmTestPanel.svelte.

### ❌ ISSUE: No event listener cleanup (same as Tauri audit above)
5 `listen()` calls with no cleanup on unmount.

### ✅ PASS: Uses Svelte 5 runes ($state, $derived)
15 state declarations using modern Svelte 5 syntax.

### ✅ PASS: Minimal inline styles (only 2)
Styles are in `<style>` blocks, not inline.

### ✅ PASS: No console.log statements
Clean production code.

### ✅ PASS: Good error handling (21 try/catch blocks)
Every `invoke()` call has error handling.

### ✅ PASS: No unused imports
All 3 imports (invoke, listen, onMount) are used.

### ⚠️ ISSUE: Only 1 accessibility attribute
**Rule:** Components should be accessible.
**Fix:** Add `aria-label` on buttons, `role` on interactive elements.

---

## SUMMARY

| Category | Pass | Fail | Score |
|---|---|---|---|
| Rust Error Handling | 2 | 0 | ✅ |
| Rust Ownership | 1 | 2 | ❌ |
| Rust Idioms | 2 | 1 | ⚠️ |
| Rust Performance | 0 | 2 | ❌ |
| Rust Project Structure | 3 | 2 | ⚠️ |
| Tauri State | 0 | 1 | ❌ CRITICAL |
| Tauri Security | 0 | 2 | ❌ CRITICAL |
| Tauri Events | 2 | 2 | ⚠️ |
| Tauri Overlay | 1 | 0 | ✅ |
| Svelte Structure | 4 | 2 | ⚠️ |
| Svelte Quality | 3 | 1 | ⚠️ |

**Overall: 19 pass, 15 fail — needs work before production release.**
