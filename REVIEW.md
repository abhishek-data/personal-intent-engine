# PIE Codebase Review — Dependencies & Optimization
> Generated: 2026-07-20 | Commit: `b23b13b` (v0.1.4)

---

## 1. Repository Independence Audit

### ✅ External Service Dependencies — CLEAN

| Source Location | External Reference | Verdict |
|---|---|---|
| `src/llm/openai.rs:51` | `https://api.openai.com/v1` default | ✅ Expected — LLM endpoint, configurable via env |
| `src/main.rs:161,299` | HuggingFace/GitHub model URLs | ✅ Help text only — tells users where to download |
| `src-tauri/src/models.rs:42-88` | Model catalog URLs | ✅ Expected — download catalog for Whisper + Silero |
| `tauri.conf.json:28` | CSP allows `api.openai.com`, `openrouter.ai` | ⚠️ Unnecessary — LLM calls go through Rust, not webview |
| `tauri.conf.json:8` | `http://localhost:5173` dev URL | ✅ Dev only, not in production build |

**No hidden external service dependencies found.** The app makes network calls only for:
1. LLM API calls (user-configured)
2. Model downloads (user-initiated, catalog)

### ✅ External Crate Dependencies — Well-Gated

**Always-compiled core:**
- `cpal`, `rubato` — audio capture + resampling
- `serde`/`serde_json` — serialization
- `tokio` — async runtime
- `reqwest` (rustls-tls, no OpenSSL) — HTTP for LLM calls
- `log`/`env_logger` — logging
- `anyhow` — error handling
- `dirs` — platform paths
- `rusqlite` (bundled) — SQLite history

**Feature-gated (heavy deps behind `whisper`/`vad`/`cli`):**
- `transcribe-cpp` → whisper feature (Metal on macOS, CPU elsewhere)
- `ort` + `ndarray` → vad feature (ONNX runtime for Silero)
- `hound` → whisper feature (WAV loader)
- `clap` → cli feature

**Tauri app adds:**
- `tauri` 2.x + plugins (global-shortcut, clipboard-manager)
- `enigo` — keystroke simulation for paste
- `objc2` + app-kit + foundation (macOS) — NSPanel overlay

### ✅ Repo Self-Ownership — Complete

The `nspanel.rs` module (167 lines) was built specifically to replace an external NSPanel plugin dependency. PIE now owns the exact subset of NSWindow→NSPanel subclassing it needs via raw `objc2` calls. **No external macOS plugin dependencies remain.**

### 🟡 One Minor Issue: CSP Bloat

`tauri.conf.json` allows `connect-src` to `api.openai.com` and `openrouter.ai`, but the Tauri app makes all LLM calls through Rust commands (never from the webview). These CSP entries are harmless but unnecessary. Removing them tightens the security surface.

---

## 2. Optimization Opportunities

### 🔴 High Impact

#### 2.1 `reqwest::Client` — New Connection Pool Per Instance
**File:** `src/llm/openai.rs:41`
```rust
client: reqwest::Client::new(),  // creates new connection pool every time
```
`reqwest::Client` is designed to be long-lived and shared — it manages a connection pool internally. Creating one per `OpenAiClient::new()` means every LLM call gets a fresh TCP connection (no HTTP/2 multiplexing, no TLS session reuse).

**Fix:** Make it `Arc<reqwest::Client>` or use a `Lazy<Client>`:
```rust
use std::sync::LazyLock;
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);
```

#### 2.2 `tokio` Features = "full" in Library Crate
**File:** `Cargo.toml:26`
```toml
tokio = { version = "1", features = ["full"] }
```
The library only needs `sync`, `fs`, `io-util`, `rt-multi-thread` (as the Tauri crate correctly does). `"full"` pulls in `net`, `process`, `signal`, `io-std`, etc. — increases compile time and binary size unnecessarily.

**Fix:**
```toml
tokio = { version = "1", features = ["sync", "fs", "io-util", "rt-multi-thread"] }
```

#### 2.3 Repeated `to_lowercase()` in Intent Extraction
**File:** `src/intent/extractor.rs`

`assess_confidence()`, `extract_constraints()`, and `extract_topics()` each call `text.to_lowercase()` independently. For a 100-word input, this is 3 redundant string allocations.

**Fix:** Lowercase once in `extract()` and pass the `&str` down:
```rust
pub fn extract(&self, text: &str) -> Intent {
    let lower = text.to_lowercase();
    // ... pass &lower to sub-functions
}
```

### 🟡 Medium Impact

#### 2.4 `VadPipeline::push_frame` — Allocates on Every Frame
**File:** `src/audio/vad.rs:175`
```rust
self.frame_buffer.push_back(frame.to_vec());  // 480 * 4 bytes = 1920 bytes/frame
```
During silence (VAD returns Noise), frames are still cloned into the prefill buffer. At 30ms/frame, this is ~64 KB/s of allocations that are immediately popped.

**Fix:** Use a pre-allocated ring buffer with fixed-size arrays:
```rust
frame_buffer: VecDeque<[f32; FRAME_SAMPLES]>,  // no heap alloc per frame
```
Or at minimum, only buffer when approaching onset threshold.

#### 2.5 `HistoryStore` — No Prepared Statement Caching
**File:** `src/history/mod.rs`

Every `add()`, `list()`, `get()`, `delete()` call prepares SQL from scratch. For the list command with the COLUMNS constant, this means parsing and planning the same query on every call.

**Fix:** Cache `Statement` objects, or use `rusqlite::Connection::prepare_cached()`:
```rust
// Instead of:
let mut stmt = self.conn.prepare(&sql)?;
// Use:
let mut stmt = self.conn.prepare_cached(&sql)?;
```

#### 2.6 Empty `src/memory/patterns.rs`
**File:** `src/memory/patterns.rs` — 2 lines (just a comment)
```rust
// Communication pattern tracking — placeholder for Phase 2
// Will analyze user's communication style over time
```
The actual `CommunicationPatterns` struct lives in `store.rs`. This empty file adds confusion.

**Fix:** Either populate it or delete it and keep the struct in `store.rs` where it's used.

### 🟢 Low Impact (Good Practices)

#### 2.7 `CommunicationPatterns::common_types` Eviction
**File:** `src/memory/store.rs:94-97`
```rust
if !self.patterns.common_types.contains(&conv_type.to_string()) {
    self.patterns.common_types.push(conv_type.to_string());
    if self.patterns.common_types.len() > 5 {
        self.patterns.common_types.remove(0);  // O(n) shift
    }
}
```
`Vec::remove(0)` shifts all elements. With a cap of 5, this is negligible, but using `VecDeque<String>` would be the correct data structure for FIFO semantics.

#### 2.8 No `#[inline]` on Hot-Path VAD Functions
The `is_voice()` and `push_frame()` methods in `EnergyVad`, `SileroVad`, and `VadPipeline` are called on every 30ms frame (33 calls/second). Adding `#[inline]` on the small forwarding methods helps the compiler:
- `EnergyVad::is_voice` (pure computation)
- `SileroVad::is_voice` (thin wrapper)
- `VadPipeline::is_voice` (forwarding call)

#### 2.9 `SttEngine` Trait — `&self` for `transcribe()`
**File:** `src/stt/mod.rs:17`
```rust
fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String>;
```
`WhisperEngine` wraps a `Session` in `Mutex<Session>` to satisfy `&self + Sync`. This is correct but means every transcription takes a mutex lock. Since transcriptions are sequential (never concurrent), this is fine — just noting the design choice.

---

## 3. Architecture Quality Notes

### ✅ What's Done Well
- **Feature gating** — whisper and ONNX are behind cargo features; builds without them are lightweight
- **Error handling** — No `unwrap()` in production code; all lib code uses `?` or `.expect("reason")`
- **Module isolation** — Each module has a clean `mod.rs` public API
- **Library-first** — All logic in `src/lib.rs`; CLI and Tauri are thin wrappers
- **Config caching** — `AudioRecorder::config_cache` avoids repeated HAL property queries
- **Streaming architecture** — `TranscriptRouter` uses atomic-first check (zero-overhead when idle)
- **Test coverage** — Unit tests on VAD state machine, intent extraction, resampling, memory, history
- **Platform abstraction** — `#[cfg(target_os)]` for macOS NSPanel vs generic WebviewWindow
- **Release workflow** — Self-signed with ad-hoc fallback, install script, Homebrew tap

### 🟡 Observations
- `src/lib.rs` re-exports only 4 types — most users need to reach into modules
- `PieEngine::new()` is `async` but does no async work (just loads JSON + creates structs)
- The `Input` enum in CLI `main.rs` has a dead `Audio(Vec<f32>)` variant — audio goes through `process_audio` which takes `&[f32]`, but `Input::Audio` is never used after `voice_session` wraps it as `Input::Text`
- `src/llm/router.rs` — "openai" and "openrouter" share the same code path; no real routing difference

---

## 4. Recommended Action Plan

### Priority 1 — Quick Wins (do now)
- [ ] Remove or populate `src/memory/patterns.rs`
- [ ] Narrow `tokio` features from `"full"` to specific features
- [ ] Share `reqwest::Client` (static `LazyLock`)
- [ ] Remove unnecessary CSP entries from `tauri.conf.json`

### Priority 2 — Performance (do before v1.0)
- [ ] Pre-lowercase once in `IntentExtractor::extract()`
- [ ] Use `prepare_cached()` in `HistoryStore`
- [ ] Add `#[inline]` to hot-path VAD forwarding methods
- [ ] Consider pre-allocated ring buffer for VAD prefill

### Priority 3 — Architecture Polish (backlog)
- [ ] Remove dead `Input::Audio` variant in CLI
- [ ] Make `PieEngine::new()` synchronous (no async work)
- [ ] Add `--dry-run` CLI flag (skip LLM call)
- [ ] Add Linux to release matrix
- [ ] Add benchmarks for VAD, intent extraction, resampling
- [ ] Consider making `LlmRouter` distinguish providers more meaningfully

---

## 5. Verdict

**The repo is fully independent.** No external plugin dependencies remain (nspanel.rs owns its macOS code). All network calls are either expected (model downloads, LLM API) or configurable. Dependencies are well-gated behind cargo features. The codebase is clean, well-tested, and follows Rust best practices.

The optimization opportunities above are improvements, not problems — the current code works correctly. Priority 1 items are trivial to fix; Priority 2 items matter for production performance; Priority 3 items are polish for a v1.0 release.
