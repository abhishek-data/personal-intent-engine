# PIE v2 — Auto-Learning & BYOK Design

> 2026-07-23 | Based on review feedback

---

## The 3 Gaps

### Gap 1: No BYOK LLM Config
`LlmRouter` reads `OPENAI_API_KEY` and `OPENAI_BASE_URL` from env vars only.
No UI, no settings.json fields. User can't plug in their own OpenRouter /
Xiaomi MiMo / local Ollama without setting env vars manually.

### Gap 2: Manual Correction is Tedious
User has to add corrections one-by-one in the UI ("next jazz" → "Next.js").
No auto-learning. The 500-entry static seed is hand-curated.

### Gap 3: No Initial Vocabulary Bootstrap
On first install, PIE knows nothing about the user. Zero personal vocabulary.
Should bootstrap from existing conversation history.

---

## Solution Design

### 1. BYOK LLM Configuration

**Goal:** User configures API URL + API key + model from settings UI. No env vars needed.

**Changes:**

```
settings.rs:  Add llm_api_url, llm_api_key fields
LlmRouter:    Accept config struct instead of reading env
UI:           "LLM Provider" settings section with URL + key + model fields
```

**Settings additions:**

```rust
// settings.rs
pub llm_api_url: String,   // e.g. "https://api.openai.com/v1" or "http://localhost:11434/v1"
pub llm_api_key: String,   // "sk-..." or empty for local
pub llm_model: String,     // already exists, currently unused by router
```

**LlmRouter change:**

```rust
// router.rs — instead of from_env(), accept settings
impl LlmRouter {
    pub fn from_settings(settings: &Settings) -> Self {
        if settings.llm_api_url.is_empty() {
            // Fallback to env vars (backward compat)
            Self { client: OpenAiClient::from_env() }
        } else {
            Self {
                client: Some(OpenAiClient::new(&settings.llm_api_url, &settings.llm_api_key))
            }
        }
    }
}
```

**UI section:**

```
┌─────────────────────────────────────┐
│ LLM Provider                        │
│                                     │
│ API URL:  [https://api.openai.com/v1│
│ API Key:  [sk-••••••••••••]         │
│ Model:    [gpt-4o-mini            ] │
│           [Test Connection]         │
└─────────────────────────────────────┘
```

---

### 2. Auto-Learning Vocabulary (No Manual Input)

**Goal:** PIE learns corrections automatically. Zero manual intervention.

**Three learning signals:**

#### Signal A: LLM Background Extraction (after every PIE-corrected interaction)

When the pipeline runs and sends to LLM, we already have:
- Raw whisper transcript
- Corrected transcript (what PIE fixed)
- LLM response

**In the background (non-blocking),** send a lightweight extraction prompt:

```
Given this voice-to-text transcript and the user's response context,
extract any technical terms that were likely misrecognized.

Transcript: "I want to deploy to coober net ease on aws"
Known context: role=backend dev, tech=[rust, kubernetes, aws]

Return JSON: [{"heard": "coober net ease", "canonical": "Kubernetes"}]
Return [] if no corrections needed.
```

**Key rules:**
- Runs async, never blocks the pipeline
- Uses the cheapest/fastest model available
- Only adds entries that don't already exist in the dictionary
- Stores with `source: AutoLearned` and a confidence score
- User never sees this — it just happens

#### Signal B: Initial Sync (one-time bootstrap on install)

On first launch (or when user triggers "Sync my vocabulary"):

```
┌──────────────────────────────────────────────────────┐
│ Sync Your Vocabulary                                 │
│                                                      │
│ PIE can scan your existing conversation history to   │
│ learn your technical vocabulary before you start.    │
│                                                      │
│ Sources found:                                       │
│   ✓ Claude conversations  (~247 conversations)       │
│   ✓ ChatGPT history       (~1,203 conversations)    │
│   ✗ Cursor history        (not found)               │
│                                                      │
│ [Sync Now]  [Skip]                                   │
│                                                      │
│ This runs once. Your data never leaves this device.  │
└──────────────────────────────────────────────────────┘
```

**How sync works:**

```
1. Scan known conversation storage locations:
   - Claude:  ~/Library/Application Support/Claude/ (macOS)
              ~/.config/claude/ (Linux)
   - ChatGPT: ~/Library/Application Support/ChatGPT/ (macOS)
              ~/Downloads/chat*.zip (manual export)
   - Cursor:  ~/.cursor/ (history DB)
   - Generic: Any .json/.sqlite files matching conversation patterns

2. Extract technical terms + corrections:
   - Feed batches to LLM with extraction prompt
   - "Find technical terms, proper nouns, and product names in these conversations"
   - Output: [{"term": "Next.js", "variants": ["nextjs", "next js", "nexus js"]}]

3. Build initial pronunciation dictionary:
   - Each variant → canonical mapping
   - Higher confidence for terms seen 3+ times
   - Store with source: "initial_sync"

4. Estimated time: 2-5 minutes for 1000 conversations (LLM-bound)
```

**Implementation:**

```rust
// src/corrector/sync.rs — NEW
pub struct VocabularySync {
    sources: Vec<HistorySource>,
}

pub struct HistorySource {
    pub name: String,           // "Claude", "ChatGPT", "Cursor"
    pub path: PathBuf,          // Where conversations live
    pub conversation_count: usize,
}

impl VocabularySync {
    /// Discover conversation sources on this machine
    pub fn discover() -> Vec<HistorySource> { ... }

    /// Extract terms from a batch of conversations via LLM
    pub async fn extract_batch(
        &self,
        conversations: &[String],
        llm: &LlmRouter,
    ) -> Vec<ExtractedTerm> { ... }

    /// Run full sync: discover → extract → merge into corrector
    pub async fn run(
        &self,
        corrector: &mut PronunciationCorrector,
        llm: &LlmRouter,
        on_progress: impl Fn(usize, usize),  // (done, total)
    ) -> anyhow::Result<SyncResult> { ... }
}
```

#### Signal C: Background Learner (ongoing, after each interaction)

After every pipeline run, queue a background task:

```rust
// src/corrector/learner.rs — NEW
pub struct BackgroundLearner {
    /// Queue of (raw_transcript, context) to process
    queue: mpsc::Receiver<LearnTask>,
    /// Reference to corrector for adding entries
    corrector: Arc<Mutex<PronunciationCorrector>>,
    /// LLM for extraction
    llm: LlmRouter,
}

impl BackgroundLearner {
    /// Spawn as a tokio task. Runs forever, processes queue.
    pub async fn run(mut self) {
        while let Some(task) = self.queue.recv().await {
            // Debounce: don't run LLM for every single interaction
            // Batch up 5 interactions or wait 30 seconds
            let batch = self.collect_batch(5, Duration::from_secs(30)).await;
            if let Ok(terms) = self.extract_terms(&batch).await {
                let mut dict = self.corrector.lock().await;
                for term in terms {
                    let _ = dict.add_auto_correction(&term.heard, &term.canonical);
                }
            }
        }
    }
}
```

**Pipeline integration:**

```rust
// engine.rs — after send_to_llm, fire-and-forget
pub async fn process(&mut self, input: &str, mode: &str) -> anyhow::Result<PieResult> {
    // ... existing pipeline ...

    // Fire-and-forget: queue for background learning
    if let Some(learner) = &self.learner_tx {
        let _ = learner.try_send(LearnTask {
            raw_transcript: input.to_string(),
            corrected_transcript: correction.text.clone(),
            context: self.memory.profile.clone(),
        });
    }

    Ok(result)
}
```

---

### 3. Auto-Correction Storage

**New source type:**

```rust
// dictionary.rs
pub enum Source {
    Static,       // Shipped with PIE
    User,         // Manually added (kept for explicit overrides)
    Synced,       // From initial vocabulary sync
    AutoLearned,  // Background learner extracted
}
```

**Persistence:**

```
~/.config/pie/
├── settings.json          # BYOK LLM config
├── memory.json            # User profile + patterns
├── pronunciation.json     # Static + user dict (shipped)
├── learned_vocab.json     # Auto-learned + synced entries (separate file)
└── sync_state.json        # Last sync timestamp, sources scanned
```

Auto-learned entries are in a **separate file** so users can:
- Reset learned vocabulary without losing manual entries
- See what PIE learned (debug/transparency)
- Export/share learned vocab

---

## What Changes in the UI

### Settings Page — Add LLM Config Section

```
┌────────────────────────────────────────────────┐
│ LLM Provider                                    │
│                                                 │
│ API URL:    [                    ] (placeholder:│
│             https://api.openai.com/v1)         │
│ API Key:    [                    ] (password)   │
│ Model:      [gpt-4o-mini         ]             │
│ [Test Connection]                               │
│                                                 │
│ ────────────────────────────────────────────── │
│ Pronunciation                                   │
│                                                 │
│ ● 47 auto-learned entries                       │
│ ● Last synced: 2 days ago                       │
│ [Re-sync Vocabulary]  [Reset Learned]           │
│                                                 │
│ ────────────────────────────────────────────── │
│ [Save]                                          │
└────────────────────────────────────────────────┘
```

### Remove the Manual Correction UI
The one-by-one "heard → canonical" add form in VocabularySettings.svelte
gets replaced with:
- Auto-learned count + last sync time
- Re-sync button
- Reset learned button
- (Keep manual override as a hidden/advanced option for power users)

---

## Architecture: Non-Blocking Design

```
Main Pipeline (blocking, fast):
  Record → Whisper → Correct(exact+phonetic) → Intent → Optimize → Paste
                    ↑
                    Uses learned vocab (loaded at startup, read-only during pipeline)
                    ~0.1ms overhead for dict lookup

Background Tasks (async, non-blocking):
  ┌─────────────────────────────────────────────┐
  │ tokio::spawn(background_learner.run())       │
  │                                              │
  │  Queue: pipeline fires (transcript, context) │
  │  Debounce: batch 5 or 30s timeout            │
  │  LLM: extract corrections from batch         │
  │  Write: append to learned_vocab.json          │
  │  Reload: signal main thread to reload dict    │
  └─────────────────────────────────────────────┘

  Main pipeline NEVER waits for background learner.
  Dict reload happens between interactions (not mid-pipeline).
```

---

## Build Order (Revised)

| Phase | What | Effort | Depends On |
|-------|------|--------|-----------|
| **1** | **BYOK LLM config** (settings + router + UI) | Low | Nothing |
| **2** | **Auto-learning from pipeline** (background learner) | Medium | #1 |
| **3** | **Initial vocabulary sync** (scan local conversations) | Medium | #1 |
| **4** | **Dual hotkey system** | Medium | Nothing (parallel) |

Phase 1 is the quickest win — makes PIE usable for anyone without env vars.
Phase 2+3 make PIE self-improving. Phase 4 is independent, can be done in parallel.

---

## Privacy & Safety

- **All processing is local.** Conversations never leave the device except
  to the user's own configured LLM provider.
- **Sync scans only known conversation storage** (Claude, ChatGPT, Cursor).
  User must approve before scan starts.
- **Auto-learned entries are transparent.** Stored in a separate file,
  inspectable, resettable.
- **LLM extraction prompts are minimal.** Only send transcript + tech context,
  never full conversation history.
- **Background learner has rate limiting.** Max 1 LLM call per 30 seconds.
  Won't burn API credits.
