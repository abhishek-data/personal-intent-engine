<p align="center">
  <img src="assets/icon.png" alt="PIE" width="128" height="128" />
</p>

# PIE — Personal Intent Engine

**Speak anywhere. PIE turns your voice into a structured, well-formed prompt and drops it right where your cursor is.**

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey.svg)]()
[![Built with Rust](https://img.shields.io/badge/Rust-stable-000000.svg?logo=rust)]()
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB.svg?logo=tauri)]()
[![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00.svg?logo=svelte)]()

PIE is intelligent middleware between you and any large language model. Press a global hotkey in any app, speak, and PIE transcribes your speech **locally** (whisper.cpp on Apple's Metal GPU), extracts what you actually want, rewrites it into a clean prompt, and pastes it into whatever text field has focus — or sends it straight to an LLM.

Your audio never leaves your machine. Only the finished prompt does, and only if you choose to send it.

---

## The idea

Most people type rambling, half-formed requests into ChatGPT. PIE fixes that at the source: it listens to how you actually talk, then hands the model something it can act on.

```
  "um can you help me set up docker with postgres          →   ## Objective
   for my rust project, oh and it shouldn't use an ORM"        set up Docker with Postgres for a Rust project

        speech                                                 ## Constraints
          │                                                    no ORM
          ▼
   ┌──────────────┐                                            ## Topics
   │  transcribe  │  whisper.cpp · Metal · fully local         rust, docker, postgres
   └──────┬───────┘
          ▼                                                    ## Preferred style
   ┌──────────────┐                                            Balanced
   │   extract    │  objective · constraints · questions
   │   intent     │  topics · confidence · type
   └──────┬───────┘
          ▼
   ┌──────────────┐
   │  optimize    │  compact / balanced / enhanced / adaptive
   │  the prompt  │
   └──────┬───────┘
          ▼
    paste into the focused app  ·or·  send to an LLM
```

## Features

- **Global hotkey, works everywhere.** Press `⌘⇧Space` (rebindable) in any application to start recording; press again to stop. The transcript or optimized prompt is pasted at your cursor.
- **Fully local speech-to-text.** whisper.cpp via [transcribe-cpp](https://crates.io/crates/transcribe-cpp), accelerated on Apple Silicon (Metal). No audio ever leaves the device.
- **Voice activity detection.** Silero VAD trims silence so only speech is transcribed and the model doesn't hallucinate on quiet gaps.
- **Intent extraction.** Rule-based extraction of objective, constraints, questions, topics, conversation type, and a confidence estimate.
- **Prompt optimization** in four modes — `compact` (strip filler), `balanced` (default), `enhanced` (enrich context), and `adaptive` (auto-pick per input).
- **Paste anywhere or route to an LLM.** Paste the raw transcript or the optimized prompt into the focused app, or send the prompt to any OpenAI-compatible API.
- **Runs in the background.** Menu-bar tray app with a floating recording indicator; the window hides to the tray so the hotkey is always live.
- **In-app model manager.** Download whisper and Silero models from the Models tab with a progress bar — no `curl`, no manual setup.
- **Three ways to use it:** a desktop app, a CLI, and a reusable Rust library.

## Installation

PIE is currently distributed as **unsigned (ad-hoc signed)** builds — there is no Apple Developer ID, so macOS Gatekeeper would normally block the app on first launch. The install methods below handle that for you automatically.

### macOS (Apple Silicon) — recommended

**One-line install** (downloads the latest release, installs to `/Applications`, and strips the Gatekeeper quarantine attribute so PIE opens without any prompts):

```bash
curl -fsSL https://raw.githubusercontent.com/abhishek-data/personal-intent-engine/main/scripts/install.sh | bash
```

> Prefer to review the script first? Download it, read it, then run it:
> ```bash
curl -fsSL https://raw.githubusercontent.com/abhishek-data/personal-intent-engine/main/scripts/install.sh -o install.sh
less install.sh && bash install.sh
```

**Homebrew** (for Homebrew users):

```bash
brew tap abhishek-data/pie https://github.com/abhishek-data/homebrew-pie
brew install --cask abhishek-data/pie/pie
# the app is ad-hoc signed, so strip the quarantine attribute after install:
xattr -cr /Applications/PIE.app
```

**Manual DMG** — download `PIE_<version>_aarch64.dmg` from the [releases page](https://github.com/abhishek-data/personal-intent-engine/releases), drag PIE to Applications, then strip the quarantine attribute:

```bash
xattr -cr /Applications/PIE.app
```

(Without that, right-click PIE in Applications → **Open** the first time, then confirm at the Gatekeeper prompt.)

### Windows

Download and run the `.exe` installer from the [releases page](https://github.com/abhishek-data/personal-intent-engine/releases). Windows SmartScreen may warn about an unrecognized app; click **More info → Run anyway**.

> Builds are not yet code-signed or notarized. Proper Developer ID signing + notarization may follow; until then the install methods above bypass Gatekeeper for you.

Prefer to build it yourself? See [Quick start](#quick-start-desktop-app) below.

## How it's built

PIE is a Cargo workspace with a clean separation between the engine and the app shell:

| Crate | What it is |
|---|---|
| `pie-engine` (`src/`) | The core library: audio capture, VAD, STT, intent, memory, optimizer, LLM router. Also ships the `pie-cli` binary. |
| `pie-desktop` (`src-tauri/`) | The Tauri desktop app — a thin shell over `pie-engine`. |
| `ui/` | Svelte 5 + Vite frontend (main window + recording overlay). |

**Stack:** Rust · [Tauri 2](https://tauri.app) · [Svelte 5](https://svelte.dev) · [cpal](https://crates.io/crates/cpal) (audio) · [rubato](https://crates.io/crates/rubato) (resampling) · [vad-rs](https://github.com/cjpais/vad-rs) (Silero VAD) · [transcribe-cpp](https://crates.io/crates/transcribe-cpp) (whisper.cpp) · [enigo](https://crates.io/crates/enigo) (paste) · [tauri-plugin-global-shortcut](https://crates.io/crates/tauri-plugin-global-shortcut) · [tauri-nspanel](https://github.com/ahkohd/tauri-nspanel) (macOS overlay).

## Requirements

- **macOS 11+ on Apple Silicon** (developed and tested there; Metal acceleration). The non-macOS code paths exist but are untested.
- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) (for the desktop UI)
- [CMake](https://cmake.org) — needed to build whisper.cpp (`brew install cmake`)

## Quick start (desktop app)

```bash
git clone https://github.com/abhishek-data/personal-intent-engine.git
cd personal-intent-engine

# install the Tauri CLI once
cargo install tauri-cli --version "^2" --locked

# run the app (starts the Vite dev server and the app together)
cargo tauri dev
```

Then, in the app:

1. Open **Models** and download a whisper model (start with *Whisper Tiny*) and *Silero VAD*.
2. Go to **Record**, click the button, and speak — or just press the hotkey in any app.
3. First run, macOS asks for **Microphone** and **Accessibility** permission (the latter is needed to paste). Grant both, then relaunch.

> **Tip:** the app hides to the menu-bar tray when you close the window. Quit it from **tray → Quit PIE**. When running via `cargo tauri dev`, stop it with `Ctrl+C` — and make sure no old instance is left running, or it will keep holding the hotkey.

### Build a standalone app

```bash
cargo tauri build
# → src-tauri/target/release/bundle/macos/PIE.app
```

Drag `PIE.app` to `/Applications`. As a real app it gets its own Microphone and Accessibility permissions listed under "PIE" (no Terminal involved). The build is unsigned, so strip the quarantine attribute (or right-click → **Open** the first time):

```bash
xattr -cr /Applications/PIE.app
```

## CLI

The engine also ships a CLI (`pie-cli`) for text and audio-file input.

```bash
# text in, optimized prompt out (echo provider needs no API key)
cargo run -- --verbose --mode balanced --provider echo "help me set up docker with postgres for rust, no ORM"

# send to a real LLM
OPENAI_API_KEY=sk-... cargo run -- --provider openai --model gpt-4o-mini "what is a lifetime in Rust?"

# transcribe a WAV file (requires the whisper feature)
cargo run --features whisper -- \
  --audio-file recording.wav \
  --whisper-model ~/.cache/pie/models/ggml-tiny.en.bin \
  --provider echo
```

Key flags: `--mode {compact|balanced|enhanced|adaptive}`, `--provider {echo|openai|openrouter}`, `--model`, `--language`, `--audio-file`, `--voice`, `--verbose`.

## Library

```rust
use pie_engine::PieEngine;

let mut engine = PieEngine::new().await?;
let result = engine.process("build a rest api in rust, must use postgres", "balanced").await?;

println!("{:?}", result.intent.conversation_type); // Task
println!("{}", result.optimized_prompt);
```

## Configuration

Settings live at `~/Library/Application Support/pie/settings.json` and are managed from the app's Settings panes:

| Setting | Description |
|---|---|
| Whisper / Silero model | Paths to the local models (set by the Models tab). |
| Language | Spoken language ISO code, or `auto` to detect. |
| Optimization mode | How speech becomes a prompt. |
| Provider / model | LLM target for "Send to LLM". `echo` reflects the prompt back for testing; `openai`/`openrouter` need `OPENAI_API_KEY`. |
| Hotkey | Global shortcut, rebindable by pressing a combo. |
| Paste output | Whether the hotkey pastes the raw transcript or the optimized prompt. |

## Privacy

Speech capture, VAD, and transcription all run **on your machine**. Nothing is uploaded during recording or transcription. The only outbound request is when you explicitly send an optimized prompt to an LLM provider you configured — and even then it's the text prompt, never your audio.

## Development

```bash
cargo test                          # engine unit tests
cargo test --features whisper,vad   # + voice/VAD integration tests (need models + macOS `say`)
cargo clippy --all-targets
cargo fmt
```

Cargo features on `pie-engine`: `cli` (default), `whisper` (whisper.cpp STT + WAV loading), `vad` (Silero VAD). See [`AGENTS.md`](AGENTS.md) for module responsibilities and coding standards, and [`docs/`](docs/) for architecture notes.

### Project layout

```
personal-intent-engine/
├── src/                    # pie-engine library + pie-cli
│   ├── audio/              # cpal capture, resampler, VAD, Silero
│   ├── stt/                # whisper.cpp engine + streaming router
│   ├── intent/             # extractor, classifier, schema
│   ├── memory/             # JSON store, profile, patterns
│   ├── optimizer/          # compact / balanced / enhanced / adaptive
│   ├── llm/                # OpenAI-compatible client + router
│   └── pipeline/           # end-to-end engine orchestration
├── src-tauri/              # pie-desktop (Tauri app)
├── ui/                     # Svelte frontend (window + overlay)
├── tests/                  # voice + VAD end-to-end tests
└── docs/                   # architecture & implementation notes
```

## Roadmap

- [x] Text pipeline: intent → memory → optimize → LLM
- [x] Local voice input: cpal → Silero VAD → whisper.cpp (Metal)
- [x] Desktop app: global hotkey, paste-to-anywhere, tray, overlay, model manager
- [ ] Hold-to-record mode
- [ ] Searchable transcription history
- [ ] Streaming transcription UI (infrastructure is in place)
- [ ] ML-based intent extraction (currently rule-based)
- [ ] Windows / Linux support

## Acknowledgements

PIE's audio and desktop architecture was studied from two excellent open-source projects:

- [**Handy**](https://github.com/cjpais/Handy) — the audio capture worker, SmoothedVad state machine, streaming router, and model management patterns.
- [**OpenSuperWhisper**](https://github.com/starmel/OpenSuperWhisper) — the recording indicator, clipboard paste flow, and settings UX.

Built on [whisper.cpp](https://github.com/ggerganov/whisper.cpp), [Silero VAD](https://github.com/snakers4/silero-vad), and the [transcribe-cpp](https://crates.io/crates/transcribe-cpp) / [vad-rs](https://github.com/cjpais/vad-rs) crates.

## License

[Apache-2.0](LICENSE).
