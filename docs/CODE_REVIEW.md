# PIE Codebase Review Report

**Date:** July 14, 2026
**Commit:** 72f533e
**Reviewer:** Hermes Agent (Nous Research)
**Score:** 7.8 / 10

---

## Executive Summary

The PIE codebase is well-structured with clean module boundaries. The audio pipeline is production-quality (borrowed from Handy's patterns). The Tauri integration is solid with proper platform-specific handling. Main issues: missing tests, a self-referential borrow in VAD, and a large Svelte component.

---

## Critical Issues (2)

### 1. SmoothedVad::push_frame - Self-Referential Borrow
**File:** src/audio/vad.rs:86-135
**Problem:** Returns VadFrame::Speech(&self.temp_out) which borrows from self while self is already mutably borrowed.
**Fix:** Change VadFrame::Speech to hold owned Vec<f32> instead of reference.

### 2. Recorder Worker - Channel Disconnect Cleanup
**File:** src/audio/recorder.rs (consumer loop)
**Problem:** If cmd_rx disconnects, loop breaks without setting stop_flag, leaving cpal stream running.
**Fix:** Set stop_flag before breaking the loop.

---

## High Issues (5)

### 1. Memory Save Ignores Errors
**File:** src/pipeline/engine.rs:103
**Problem:** `let _ = self.memory.save()` silently discards save errors.
**Fix:** Log the error: `if let Err(e) = self.memory.save() { log::warn!("..."); }`

### 2. API Key Stored in Memory
**File:** src/llm/openai.rs:8
**Problem:** api_key as plain String in heap memory.
**Fix:** Use zeroize crate or read from env on each request.

### 3. Paste Delay Blocks Thread
**File:** src-tauri/src/paste.rs:38,53
**Problem:** std::thread::sleep blocks for 280ms total.
**Fix:** Use tokio::time::sleep or spawn_blocking.

### 4. CSP Disabled
**File:** src-tauri/tauri.conf.json:28
**Problem:** "csp": null disables Content Security Policy.
**Fix:** Set proper CSP: `"csp": "default-src 'self'; connect-src 'self' https://api.openai.com"`

### 5. Missing Input Validation
**File:** src-tauri/src/main.rs
**Problem:** busy flag uses Ordering::Acquire which may not be immediately visible.
**Fix:** Use Ordering::SeqCst for the busy flag.

---

## Medium Issues (8)

1. **resampler.rs:67** - .expect() in push() could panic on refactor. Use if-let.
2. **All optimizers** - Token estimation uses len()/4. Inaccurate for non-English.
3. **classifier.rs:68-77** - "issue"/"problem" match positive contexts too.
4. **extractor.rs:104** - "with"/"using" match nearly every sentence.
5. **extractor.rs** - No unit tests. Add tests for all extraction methods.
6. **balanced.rs, enhanced.rs** - No unit tests.
7. **App.svelte** - 1219 lines. Split into sub-components.
8. **Cargo.toml** - [workspace] + [package] in same file is confusing.

---

## Low Issues (6)

1. **patterns.rs** - 2-line placeholder. Implement or remove.
2. **Missing doc comments** on some public items.
3. **Inconsistent error types** - anyhow vs String conversion repeated.
4. **uuid, chrono** dependencies unused in source.
5. **thiserror** dependency unused.
6. **Overlay position** hardcoded to bottom-center.

---

## Positive Findings

### Architecture
- Clean module separation (audio, stt, intent, memory, optimizer, llm, pipeline)
- Library-first design with thin CLI and Tauri wrappers
- Feature-gated heavy dependencies (whisper, vad)

### Audio Pipeline
- SmoothedVad state machine (from Handy) is well-implemented
- StreamRouter with atomic-first feed pattern is zero-overhead
- FrameResampler with finish() prevents tail audio loss
- Config caching avoids HAL round-trips

### Tauri Integration
- NSPanel overlay on macOS (non-activating, floating)
- Platform-specific overlay handling
- Global shortcut with proper registration
- Model catalog with download progress

### Testing
- Classifier: 7 tests covering all conversation types
- Adaptive optimizer: 5 tests covering mode selection
- Resampler: tests for passthrough and resampling
- Integration tests for VAD and voice pipeline (macOS-gated)
- Settings: roundtrip and partial-fill tests

### UI
- Svelte 5 with reactive state ($state, $derived)
- Overlay.svelte is clean and minimal (82 lines)
- Hotkey capture with modifier key detection

---

## File Scores

| File | Lines | Score | Notes |
|---|---|---|---|
| src/lib.rs | 18 | 9/10 | Clean re-exports |
| src/main.rs (CLI) | 324 | 8/10 | Well-structured |
| src/audio/recorder.rs | 599 | 8/10 | Solid architecture |
| src/audio/resampler.rs | 272 | 9/10 | Excellent with tests |
| src/audio/vad.rs | 317 | 7/10 | Self-referential borrow |
| src/intent/extractor.rs | 312 | 7/10 | No tests, greedy matching |
| src/intent/classifier.rs | 149 | 9/10 | Good tests |
| src/memory/store.rs | 153 | 8/10 | Good with tests |
| src/optimizer/adaptive.rs | 110 | 9/10 | Excellent tests |
| src/llm/openai.rs | 88 | 7/10 | API key in memory |
| src/stt/stream.rs | 186 | 9/10 | Excellent StreamRouter |
| src-tauri/src/main.rs | 604 | 8/10 | Well-structured |
| src-tauri/src/overlay.rs | 140 | 9/10 | Platform-specific done right |
| ui/src/App.svelte | 1219 | 6/10 | Too large |
| ui/src/Overlay.svelte | 82 | 9/10 | Clean and minimal |

---

## Recommendations

### Immediate (Before Next Release)
1. Fix SmoothedVad self-referential borrow
2. Fix memory save error handling
3. Set proper CSP in tauri.conf.json
4. Remove unused dependencies

### Short-Term
1. Add unit tests for IntentExtractor
2. Add unit tests for balanced/enhanced optimizers
3. Split App.svelte into sub-components
4. Fix greedy constraint matching
5. Add zeroize for API key storage

### Long-Term
1. Implement communication pattern learning
2. Add multi-language intent extraction
3. Add overlay position settings
4. Add clipboard changeCount verification (from OpenSuperWhisper)
5. Add TranscriptionQueue pattern (from OpenSuperWhisper)
