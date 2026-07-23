# PIE v2 — Enhancement Brainstorm & Roadmap

> Generated: 2026-07-23
> Context: Basic voice-to-text → intent → prompt optimize → LLM pipeline working.
> Goal: Define the next wave of features that make PIE genuinely better than
>        generic OS dictation (Apple, Windows, Google) for developers.

---

## Where We Are Now (What Works)

| Module | Status | Notes |
|--------|--------|-------|
| Audio capture (cpal) | ✅ Working | 16kHz mono, cross-platform |
| Silero VAD | ✅ Working | ONNX-based, smoothed state machine |
| Whisper STT | ✅ Working | Streaming + batch, whisper.cpp via transcribe-cpp |
| Intent extraction | ✅ Working | Rule-based (regex + keyword), no ML |
| Memory store | ✅ Working | JSON file, basic interaction tracking |
| Prompt optimizer | ✅ Working | 4 modes (compact/balanced/enhanced/adaptive) |
| LLM routing | ✅ Working | OpenAI-compatible API |
| History (SQLite) | ✅ Working | Records all interactions |
| Clipboard paste | ❌ Not yet | No hotkey / system paste integration |

**Current mode: Pure voice-to-text with post-processing.** It transcribes,
extracts basic intent, optimizes the prompt, and sends to LLM. That's it.

---

## THE BIG ENHANCEMENTS

### Enhancement 1: Personal Pronunciation Dictionary (PPD)

**Problem:** Developers use technical terms that speech models butcher.
- "Next.js" → "Nexsus JS", "enexus JS", "next jazz"
- "Nginx" → "engine X", "N-gin-x"
- "Kubernetes" → "coobernetes"
- "Wi-Fi" → "why fi"
- "CLI" → "see el eye" vs "clay"
- User-specific mispronunciations compound over time

**Why PIE can win here:** Generic dictation treats every user the same. PIE
knows YOUR voice, YOUR vocabulary, YOUR corrections. Over time it should get
MORE accurate for YOU, not stay generic.

#### Architecture: 3-Layer Correction

```
Layer 1: Static Dictionary (ships with PIE)
├── tech_terms.json — 500+ common dev terms
├── Pattern: "nexus js" → "Next.js"
├── Pattern: "coober net ease" → "Kubernetes"
└── Community-contributed, versioned

Layer 2: User Dictionary (learned from corrections)
├── ~/.config/pie/pronunciation.json
├── User says "transcribe X" → PIE transcribes → user corrects
├── PIE stores: {"heard": "enexus js", "corrected": "Next.js", "confidence": 0.95}
├── Confidence grows with repeated confirmations
└── Decay: corrections not reinforced in 30 days get downgraded

Layer 3: Context-Aware Post-Correction
├── Uses MemoryStore vocabulary + tech stack profile
├── If user's profile says "technologies: [rust, nextjs]"
├── And transcript has "next jazz" → auto-correct to "Next.js"
├── Runs AFTER whisper, BEFORE intent extraction
└── LLM-assisted: feed transcript + user context, ask for correction
```

#### Implementation Plan

```rust
// src/corrector/mod.rs — NEW MODULE
pub struct PronunciationCorrector {
    /// Static tech dictionary (compiled in)
    static_dict: HashMap<String, String>,   // "nexus js" → "Next.js"
    /// User-learned corrections
    user_dict: Vec<UserCorrection>,
    /// User profile for context
    profile: &UserProfile,
}

pub struct UserCorrection {
    heard: String,          // What whisper heard
    corrected: String,      // What user meant
    phonetic_key: String,   // Metaphone/Soundex for fuzzy match
    confidence: f32,        // Grows with reinforcement
    last_seen: u64,         // Timestamp for decay
}

impl PronunciationCorrector {
    /// Pipeline position: after STT, before intent extraction
    pub fn correct(&self, transcript: &str) -> String {
        let mut text = transcript.to_string();
        // 1. Exact match against user dict (highest confidence first)
        // 2. Phonetic fuzzy match (Double Metaphone)
        // 3. Context match (if profile has "nextjs", correct near-matches)
        // 4. Static dict fallback
        text
    }
}
```

#### Phonetic Matching Strategy

Use **Double Metaphone** (or a simpler Soundex variant) to catch pronunciation
variants without exact string matching:

| User speaks | Phonetic key | Dictionary entry | Match? |
|-------------|-------------|-----------------|--------|
| "nexus js" | NKSS JS | "Next.js" → NXTJS | ✅ fuzzy |
| "enexus" | ENKSS | "Next.js" → NXTJS | ✅ fuzzy |
| "next jazz" | NKST JS | "Next.js" → NXTJS | ✅ close |

Crate: `phonetic` or `doublemetaphone` on crates.io (~5KB, no deps).

#### Correction Workflow

```
1. User speaks → Whisper transcribes → "I want to build a nexus js app"
2. Corrector runs:
   a. "nexus js" fuzzy-matches "Next.js" in static dict → auto-correct
   b. Output: "I want to build a Next.js app"
3. If confidence < threshold → flag for user review
4. User confirms or overrides → update user_dict
```

---

### Enhancement 2: Dual Hotkey System

**Problem:** Users have to:
1. Open PIE
2. Speak
3. Copy result
4. Switch to target app
5. Paste

That's 5 steps. Should be 1-2.

#### Two Hotkeys

| Hotkey | Action | Use Case |
|--------|--------|----------|
| **Hotkey A** (e.g., Ctrl+Shift+V) | Raw paste — transcribe + paste directly into focused app | Quick messages, chats, notes |
| **Hotkey B** (e.g., Ctrl+Shift+Space) | Optimized paste — transcribe → intent → optimize → paste | Sending to LLM, coding prompts, structured requests |

#### System Integration Architecture

```
┌─────────────────────────────────────────┐
│  Global Hotkey Listener (separate thread)│
│  ┌──────────┐  ┌──────────┐             │
│  │ Hotkey A │  │ Hotkey B │             │
│  │ Raw Paste│  │Opt. Paste│             │
│  └────┬─────┘  └────┬─────┘             │
│       │              │                   │
│       ▼              ▼                   │
│  ┌─────────┐  ┌──────────────┐          │
│  │ Record → │  │ Record →     │          │
│  │ Transcribe│ │ Transcribe → │          │
│  │ Paste    │  │ Intent →     │          │
│  └─────────┘  │ Optimize →   │          │
│               │ Paste        │          │
│               └──────────────┘          │
│                                         │
│  Paste target: focused input field      │
│  via xdotool (Linux) / CGEvent (macOS)  │
│  / SendInput (Windows)                  │
└─────────────────────────────────────────┘
```

#### Platform Crates

| Platform | Key Grab | Paste Simulation |
|----------|----------|-----------------|
| Linux | `x11rb` or `evdev` | `xdotool` / `enigo` |
| macOS | `core-graphics` CGEvent | `enigo` or CGEventPost |
| Windows | `winapi` RegisterHotKey | `enigo` or SendInput |

Best cross-platform crate: **`rdev`** (keyboard hook + simulate) or **`global-hotkey`**
(from Tauri team — battle-tested, ~50KB).

For clipboard: **`arboard`** (cross-platform clipboard, 0 deps).

#### Implementation

```rust
// src/hotkey/mod.rs — NEW MODULE
use global_hotkey::{GlobalHotKey, HotKeyState};

pub enum HotkeyAction {
    RawPaste,       // Transcribe → clipboard → simulate Ctrl+V
    OptimizedPaste, // Transcribe → intent → optimize → clipboard → simulate Ctrl+V
}

pub struct HotkeyManager {
    raw_key: GlobalHotKey,      // Ctrl+Shift+V
    opt_key: GlobalHotKey,      // Ctrl+Shift+Space
    pipeline: PieEngine,
}

impl HotkeyManager {
    pub fn run(&self) -> ! {
        loop {
            match self.receiver.recv() {
                Ok(event) if event.state == HotKeyState::Pressed => {
                    match self.resolve_action(event.id) {
                        HotkeyAction::RawPaste => self.raw_paste(),
                        HotkeyAction::OptimizedPaste => self.optimized_paste(),
                    }
                }
                _ => {}
            }
        }
    }

    fn raw_paste(&self) {
        let text = self.record_and_transcribe();
        clipboard_copy(&text);
        simulate_paste(); // Ctrl+V / Cmd+V
    }

    fn optimized_paste(&self) {
        let text = self.record_and_transcribe();
        let result = self.pipeline.process(&text, "adaptive").await;
        clipboard_copy(&result.optimized_prompt);
        simulate_paste();
    }
}
```

#### UX: Visual Feedback

When hotkey triggers, show a **tiny floating overlay** (like macOS dictation):
- 🔴 Recording... (red dot)
- ✍️ Transcribing...
- ✅ Pasted!

Cross-platform overlay options:
- **macOS**: NSPanel (we already have PIE overlay code from Handy)
- **Linux**: `gtk::Window` with `set_decorated(false)`, `set_keep_above(true)`
- **Windows**: Borderless `winapi` window

---

### Enhancement 3: Smart Vocabulary Learning

**Problem:** Users develop domain-specific vocabulary over weeks/months. PIE
should learn it automatically without manual dictation training.

#### Auto-Learn Mechanism

```
Phase 1: Observation
├── Log every transcript + user's subsequent correction
├── If user types a correction (e.g., edits pasted text), detect it
├── Compare: whisper_output vs user_final_text
└── Store divergence as candidate pronunciation mapping

Phase 2: Reinforcement
├── Same correction seen 3+ times → promote to active dictionary
├── Track confidence per mapping
└── Share anonymized patterns to static dict (opt-in)

Phase 3: Context Enrichment
├── Associate corrections with conversation context
├── "nexus" in coding context → "Next.js"
├── "nexus" in phone context → "Nexus" (Google phone)
└── Disambiguate using intent classifier
```

#### Where Corrections Come From

| Source | Signal | Reliability |
|--------|--------|------------|
| User edits pasted text | Before/after diff | High |
| User says "I meant X" | Natural correction | High |
| LLM returns garbled term | Proxy for wrong transcription | Medium |
| Repeated similar phrases | Frequency pattern | Medium |
| User explicitly trains | `pie learn "nexus js" → "Next.js"` | Highest |

#### Implementation: Correction Detector

```rust
// Detect when user corrects a PIE output
pub struct CorrectionDetector;

impl CorrectionDetector {
    /// Watch clipboard for post-paste edits
    /// If user pastes, then edits within 30s, capture the diff
    pub fn watch_clipboard_edits(&self) -> Option<(String, String)> {
        // 1. PIE pastes "nexus js app"
        // 2. User edits to "Next.js app"
        // 3. Diff: {"nexus js" → "Next.js"}
        // 4. Store as candidate correction
    }
}
```

---

### Enhancement 4: Long Conversation Refinement

**Problem:** Developers speak in long, rambling streams. The raw transcript is
verbose, repetitive, and unclear. We need to compress it into a sharp prompt.

#### Multi-Mode Long Input Handling

```
Mode: "refine" (NEW optimization mode)
├── Input: 200-word rambling transcript
├── Step 1: Remove filler ("um", "like", "you know", "so basically")
├── Step 2: Deduplicate repeated ideas
├── Step 3: Extract core intent + constraints
├── Step 4: Restructure into clear prompt
└── Output: 50-word sharp prompt
```

#### Integration with Intent Extractor

Current extractor is rule-based. For long inputs, we need LLM-assisted
extraction:

```rust
// Long conversation → structured prompt
async fn refine_long_input(text: &str, memory: &MemoryStore) -> String {
    // If text > ~80 words, use LLM to refine
    if text.split_whitespace().count() > 80 {
        let meta_prompt = format!(
            "The user spoke this long request. Extract the core intent as a \
             clear, concise prompt suitable for an AI assistant. \
             Keep all technical terms and constraints. Remove filler. \
             User context: role={:?}, tech={:?}.\n\nUser said:\n{}",
            memory.profile.role,
            memory.profile.technologies,
            text
        );
        // Send to LLM for refinement
        llm.send(&meta_prompt, "compact", None).await
    } else {
        text.to_string() // Short enough, use as-is
    }
}
```

---

### Enhancement 5: Developer-Specific Patterns

**Problem:** Developers have unique voice-to-code needs that generic dictation
ignores entirely.

#### Code-Aware Transcription Post-Processing

```
"create a function called get user data" → "create a function called get_user_data"
"add a try catch block" → "add a try/catch block"
"the variable is snake case my variable" → "the variable is my_variable"
"import react from react" → "import React from 'react'"
"console dot log hello world" → "console.log('hello world')"
"equals equals true" → "=== true"
```

#### Implementation: Code Phrase Dictionary

```json
{
  "snake_case_next": ["called", "named", "variable", "function", "method"],
  "translations": {
    "dot log": ".log(",
    "arrow function": "() => ",
    "triple equals": "===",
    "not equal": "!=",
    "open brace": "{",
    "close brace": "}",
    "open paren": "(",
    "close paren": ")",
    "semi colon": ";",
    "single quote": "'",
    "double quote": "\""
  }
}
```

---

## Priority Ranking

| # | Enhancement | Impact | Effort | Dependencies |
|---|-------------|--------|--------|-------------|
| 1 | **Dual Hotkey System** | 🔥🔥🔥 | Medium | `global-hotkey`, `arboard` |
| 2 | **Pronunciation Dictionary** | 🔥🔥🔥 | Medium | `phonetic` crate |
| 3 | **Long Conversation Refinement** | 🔥🔥 | Low | LLM API (already have) |
| 4 | **Smart Vocabulary Learning** | 🔥🔥 | Medium | Clipboard watching, #2 |
| 5 | **Code-Aware Post-Processing** | 🔥 | Low | Phrase dictionary |

---

## Suggested Build Order

### Phase 1: Hotkey System (Makes PIE immediately useful)
1. Add `global-hotkey` + `arboard` deps
2. Implement dual hotkey listener in `src/hotkey/`
3. Wire: Hotkey A → record → transcribe → clipboard → paste
4. Wire: Hotkey B → record → transcribe → intent → optimize → clipboard → paste
5. Add minimal overlay feedback (OS notification or floating dot)
6. Test on macOS first (primary dev machine)

### Phase 2: Pronunciation Corrector (Makes PIE accurate)
1. Create `src/corrector/` module
2. Ship `data/tech_terms.json` with 500+ common dev terms
3. Implement Double Metaphone fuzzy matching
4. Wire into pipeline: STT → **corrector** → intent → optimize
5. Add `pie learn "heard" → "corrected"` CLI command
6. Add user dictionary persistence in `~/.config/pie/pronunciation.json`

### Phase 3: Long Input + Learning (Makes PIE smart)
1. Add `refine` optimization mode for long inputs
2. Implement LLM-assisted refinement for >80 word inputs
3. Add clipboard edit detection for auto-learning
4. Build correction reinforcement pipeline
5. Add code-aware phrase dictionary

---

## How This Beats Generic Dictation

| Feature | Apple/Windows/Google | PIE |
|---------|---------------------|-----|
| Generic accuracy | ✅ Good | ✅ Same (uses Whisper) |
| Personal accuracy | ❌ No learning | ✅ Learns YOUR pronunciation |
| Tech term accuracy | ❌ Garbles jargon | ✅ 500+ term dictionary + personal |
| Direct paste | ✅ Built-in | ✅ Hotkey A |
| Optimized paste | ❌ Doesn't exist | ✅ Hotkey B (intent + optimize) |
| Knows your context | ❌ Zero personalization | ✅ Profile + memory + patterns |
| Long input handling | ❌ Just transcribes | ✅ Refines into sharp prompt |
| Code-aware | ❌ No | ✅ Phrase-to-syntax mapping |
| Improves over time | ❌ Static | ✅ Continuous learning |

**PIE's core differentiator:** It's not just transcription — it's transcription
that understands YOU, learns YOUR patterns, and produces better output over time.

---

## Research Findings (What Exists Today)

### Platform Capabilities vs PIE Opportunities

| Platform | Custom Vocab | Pronunciation Control | User Learning |
|----------|-------------|----------------------|---------------|
| Apple Dictation | ❌ Text replace only | ❌ None | ❌ |
| Windows Voice | ❌ Text replace only | ❌ None | ❌ |
| Google Cloud STT | ✅ Phrase Hints (boost) | ⚠️ Probabilistic (70-85%) | ❌ |
| AWS Transcribe | ✅ `SoundsLike` field | ✅ Phonetic aliases | ❌ |
| Azure Custom Speech | ✅ Lexicon files | ✅ IPA phonetic | ❌ |
| Dragon NaturallySpeaking | ✅ Full vocab editor | ✅ Speak-to-train (gold standard) | ⚠️ Windows only |
| Kaldi/Vosk | ✅ CMU dict aliases | ✅ Deterministic | ❌ |
| **PIE (target)** | ✅ 3-layer dict | ✅ Fuzzy + LLM | ✅ Continuous |

### Best-In-Class Approaches (Ranked by Practicality)

| Approach | Accuracy | Effort | How It Works |
|----------|----------|--------|-------------|
| 1. Post-correction dictionary | 85-95% | Low | Exact + fuzzy match known misrecognitions |
| 2. LLM post-correction | 95-99% | Medium | Language model fixes STT errors with domain context |
| 3. Phonetic alias mapping | 90-95% | Medium | Tell engine "nexsus" sounds like "Next.js" |
| 4. N-best reranking | 80-90% | Medium | Take alt transcriptions, re-rank by personal vocab |
| 5. Cloud phrase hints | 70-85% | Low | Boost correct terms in STT decoder |

**PIE's recommended stack:** Whisper → personal correction dictionary (fuzzy) → LLM post-correction → feedback loop to auto-expand dictionary.

### Talon Voice / Developer Tools Insights

- Talon: `insert()` (keystroke sim) vs `paste()` (clipboard atomic). Clipboard paste wins for anything >1 word.
- Serenade.ai: "change [wrong] to [correct]" — voice correction after dictation (good UX pattern).
- Dragon: Speak word 1-3 times to train pronunciation (gold standard UX for correction).

**Key insight:** LLM post-correction is the modern best approach (95-99% accuracy) and we already have the LLM pipeline. This is PIE's unfair advantage — no other tool combines STT + personal memory + LLM in one pipeline.

---

## Open Questions for Discussion

1. **Hotkey conflict:** Ctrl+Shift+V is "paste without formatting" in many apps.
   Better defaults? Maybe Ctrl+Alt+V and Ctrl+Alt+Space?

2. **Overlay vs notification:** Floating overlay is sexier but platform-specific.
   OS notification is simpler. Start with notification, upgrade later?

3. **LLM cost for refinement:** Using LLM to refine long inputs costs tokens.
   Only trigger for >80 words? Or always refine?

4. **Clipboard watching privacy:** Watching what user edits post-paste feels
   invasive. Opt-in per-session? Or only when PIE itself pasted?

5. **Correction auto-detection:** How to reliably detect user edits to pasted
   text without root-level keylogging? Clipboard polling + timestamp?
