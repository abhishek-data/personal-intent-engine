# PIE - Personal Intent Engine

**Intelligent middleware between humans and AI models.**

PIE sits between you and any LLM (GPT, Claude, Gemini, Qwen, DeepSeek, local models).
It extracts intent from your speech/text, maintains personal memory, optimizes prompts,
and routes them to the best model for the task.

## Architecture

```
User Input (Voice/Text)
        |
        v
  [Speech-to-Text] (whisper.cpp)
        |
        v
  [Intent Extractor] (small local model / rules)
        |
        v
  [Personal Memory] (JSON + embeddings)
        |
        v
  [Prompt Optimizer] (compact / balanced / enhanced)
        |
        v
  [LLM Router] (OpenAI-compatible API)
        |
        v
  Any LLM (GPT/Claude/Gemini/Qwen/Local)
```

## Project Structure

```
pie-engine/
├── src/
│   ├── lib.rs              # Public API
│   ├── main.rs             # CLI entry point
│   ├── audio/              # Audio capture pipeline
│   │   ├── mod.rs
│   │   ├── recorder.rs     # cpal-based real-time capture
│   │   ├── vad.rs          # Voice Activity Detection
│   │   └── resampler.rs    # Sample rate conversion
│   ├── stt/                # Speech-to-Text
│   │   ├── mod.rs
│   │   └── whisper.rs      # whisper.cpp integration
│   ├── intent/             # Intent extraction
│   │   ├── mod.rs
│   │   ├── extractor.rs    # Core extraction logic
│   │   ├── schema.rs       # Intent data structures
│   │   └── classifier.rs   # Conversation type classification
│   ├── memory/             # Personal memory layer
│   │   ├── mod.rs
│   │   ├── store.rs        # JSON-based storage
│   │   ├── profile.rs      # User profile (role, tech stack, style)
│   │   └── patterns.rs     # Communication pattern learning
│   ├── optimizer/          # Prompt optimization
│   │   ├── mod.rs
│   │   ├── compact.rs      # Minimize tokens
│   │   ├── balanced.rs     # Balance tokens + quality
│   │   ├── enhanced.rs     # Maximize reasoning quality
│   │   └── adaptive.rs     # Auto-select mode
│   ├── llm/                # LLM routing
│   │   ├── mod.rs
│   │   ├── router.rs       # Provider selection
│   │   └── openai.rs       # OpenAI-compatible client
│   └── pipeline/           # Full pipeline orchestration
│       ├── mod.rs
│       └── engine.rs       # Wires everything together
├── src-tauri/              # Tauri desktop app (Phase 2)
├── tauri-ui/               # React frontend (Phase 2)
├── docs/                   # Architecture docs
├── tests/                  # Integration tests
└── examples/               # Usage examples
```

## Quick Start

```bash
# Text mode (no voice)
cargo run -- "Help me write a React component for a login form"

# With optimization mode
cargo run -- --mode compact "What's the best way to handle auth in Next.js?"

# Pipe input
echo "Explain closures in Rust" | cargo run
```

## Development Status

- [x] Project structure
- [ ] Audio capture (cpal)
- [ ] VAD (Silero)
- [ ] STT (whisper.cpp)
- [ ] Intent extraction
- [ ] Personal memory
- [ ] Prompt optimizer
- [ ] LLM router
- [ ] Tauri UI

## License

Apache-2.0
