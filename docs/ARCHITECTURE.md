# PIE Architecture Document

## Overview

PIE (Personal Intent Engine) is a Rust library + CLI that acts as intelligent middleware
between humans and AI models. It extracts intent from speech/text, maintains personal
memory, optimizes prompts, and routes them to any LLM.

## Data Flow

```
User Input (Voice/Text)
        |
        v
  [Speech-to-Text] (whisper.cpp)  -- optional, can use text directly
        |
        v
  [Intent Extractor] (rule-based + small model)
        |
        v
  [Personal Memory] (JSON file, evolves over time)
        |
        v
  [Prompt Optimizer] (compact / balanced / enhanced / adaptive)
        |
        v
  [LLM Router] (OpenAI-compatible API)
        |
        v
  Any LLM (GPT / Claude / Gemini / Qwen / Local)
```

## Module Architecture

### 1. Audio Module (src/audio/)
Real-time audio capture using cpal. Based on Handy's architecture.

**Components:**
- `AudioRecorder` — cpal-based streaming recorder with worker thread
- `SmoothedVad` — Voice Activity Detection with onset/hangover/prefill
- `FrameResampler` — rubato-based resampling to 16kHz mono

**Key Design:**
- Dedicated worker thread for audio processing
- Channel-based communication (mpsc)
- Cached device config to avoid HAL round-trips
- VAD filters silence in real-time

**Based on:** Handy's audio_toolkit architecture

### 2. STT Module (src/stt/)
Speech-to-Text engine. Currently a stub — will integrate whisper.cpp.

**Interface:**
```rust
pub trait SttEngine: Send + Sync {
    fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String>;
    fn is_ready(&self) -> bool;
}
```

### 3. Intent Module (src/intent/)
Core PIE logic. Extracts structured intent from text.

**Components:**
- `IntentExtractor` — Rule-based extraction (Phase 1)
- `Intent` schema — Objective, context, constraints, confidence, etc.
- `classifier` — Conversation type detection

**Intent Schema:**
```rust
struct Intent {
    objective: String,
    context: Vec<String>,
    constraints: Vec<String>,
    questions: Vec<String>,
    confidence: IntentConfidence,
    conversation_type: ConversationType,
    topics: Vec<String>,
}
```

### 4. Memory Module (src/memory/)
Personal memory that evolves over time.

**Components:**
- `MemoryStore` — JSON-based storage with atomic writes
- `UserProfile` — Role, technologies, preferences
- `CommunicationPatterns` — Learned behavior

**Storage:** ~/.config/pie/memory.json

### 5. Optimizer Module (src/optimizer/)
Prompt optimization with four modes.

**Modes:**
- `compact` — Minimize tokens, remove filler
- `balanced` — Keep context, remove noise (default)
- `enhanced` — Enrich with missing context
- `adaptive` — Auto-select based on input characteristics

### 6. LLM Module (src/llm/)
LLM provider routing.

**Components:**
- `OpenAiClient` — OpenAI-compatible API client
- `LlmRouter` — Provider selection and routing

### 7. Pipeline Module (src/pipeline/)
Full pipeline orchestration.

**Components:**
- `PieEngine` — Wires everything together
- `PieResult` — Intent + optimized prompt + metadata

## Reference Architectures

### From Handy (Rust + Tauri)
- Audio capture via cpal with worker thread
- SmoothedVad with onset/hangover/prefill state machine
- StreamRouter with atomic zero-overhead no-op
- TranscriptionCoordinator single-thread command loop
- Two-implementation shortcut system with fallback

### From OpenSuperWhisper (Swift + whisper.cpp)
- VAD pre-gate pattern (prevent hallucinations on silence)
- TranscriptionQueue (queue when busy instead of reject)
- Clipboard changeCount verification
- 0.25s stopTailDuration for last-word protection
- Caret-aware indicator positioning

## Build Phases

### Phase 1 (Current) — Text Pipeline
- [x] Project structure
- [x] Intent extraction (rule-based)
- [x] Memory store (JSON)
- [x] Prompt optimizer (all modes)
- [x] LLM router (OpenAI-compatible)
- [x] CLI entry point
- [ ] Compilation and testing

### Phase 2 — Voice Input
- [ ] Audio capture (cpal)
- [ ] VAD (Silero ONNX)
- [ ] STT (whisper.cpp integration)
- [ ] Streaming transcription

### Phase 3 — Desktop UI (Tauri)
- [ ] Tauri app shell
- [ ] React frontend
- [ ] Recording overlay (like OpenSuperWhisper)
- [ ] Settings UI
- [ ] Model management

### Phase 4 — Intelligence
- [ ] Small local model for intent extraction
- [ ] Communication pattern learning
- [ ] Adaptive optimization
- [ ] Multi-language support
