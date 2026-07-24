# PIE — Implementation Guide (All Enhancements)

> This is the authoritative implementation guide. Follow phases in order.
> Each phase has exact file changes, crate additions, and acceptance criteria.

---

## Table of Contents

1. [Phase 1: BYOK LLM Config](#phase-1-byok-llm-config)
2. [Phase 2: Auto-Learning from Pipeline](#phase-2-auto-learning-from-pipeline)
3. [Phase 3: Initial Vocabulary Sync](#phase-3-initial-vocabulary-sync)
4. [Phase 4: Dual Hotkey System](#phase-4-dual-hotkey-system)
5. [Phase 5: Long Conversation Refinement](#phase-5-long-conversation-refinement)
6. [Phase 6: Code-Aware Post-Processing](#phase-6-code-aware-post-processing)

---

## Phase 1: BYOK LLM Config

**Goal:** User configures API URL + API key + model from settings UI. No env vars needed.

### Files to Change

#### `src-tauri/src/settings.rs` — Add fields

```rust
// Add to Settings struct:
pub llm_api_url: String,     // e.g. "https://api.openai.com/v1"
pub llm_api_key: String,     // e.g. "sk-..." or empty for local
pub llm_model: String,       // already exists, wire it up

// Update Default impl:
llm_api_url: String::new(),
llm_api_key: String::new(),
```

#### `src/llm/router.rs` — Accept settings config

```rust
pub struct LlmConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

impl LlmRouter {
    /// Build from user settings (BYOK). Falls back to env vars if settings empty.
    pub fn from_config(config: &LlmConfig) -> Self {
        if config.api_url.is_empty() {
            // Backward compat: try env vars
            Self { client: OpenAiClient::from_env() }
        } else {
            Self {
                client: Some(OpenAiClient::new(&config.api_url, &config.api_key))
            }
        }
    }

    /// Use settings model as default, fall back to provided
    pub fn default_model(&self) -> &str {
        // ...
    }
}
```

#### `src/pipeline/engine.rs` — Wire config through

```rust
impl PieEngine {
    pub async fn with_config(config: &LlmConfig) -> anyhow::Result<Self> {
        let llm = LlmRouter::from_config(config);
        // ... rest same as new()
    }
}
```

#### `src-tauri/src/main.rs` — Use settings for LLM

```rust
// In transcribe_and_process and send_to_llm:
// Build LlmConfig from settings, pass to engine
```

#### `ui/src/lib/LLMSettings.svelte` — NEW UI section

```svelte
<section class="group">
  <div class="field">
    <span class="field-label">LLM Provider</span>
    <input placeholder="API URL (e.g. https://api.openai.com/v1)"
           bind:value={settings.llm_api_url} />
    <input type="password" placeholder="API Key"
           bind:value={settings.llm_api_key} />
    <input placeholder="Model (e.g. gpt-4o-mini)"
           bind:value={settings.llm_model} />
    <button onclick={testConnection}>Test Connection</button>
  </div>
</section>
```

#### `ui/src/App.svelte` — Add LLMSettings to settings page

```svelte
import LLMSettings from './lib/LLMSettings.svelte'
// Add to settings layout
```

### Acceptance Criteria

- [ ] User can set API URL + key + model in settings UI
- [ ] "Test Connection" button verifies the endpoint works
- [ ] `LlmRouter` uses settings config when available, env vars as fallback
- [ ] `pie --provider openai` CLI still works (env var path)
- [ ] No API key displayed in plain text (password input)

### Crate Additions

None needed — `reqwest` already in deps.

---

## Phase 2: Auto-Learning from Pipeline

**Goal:** PIE automatically learns pronunciation corrections from every interaction. Zero manual input.

### Architecture

```
Pipeline (fast, blocking):
  Whisper → Correct(exact+phonetic, ~0.1ms) → Intent → Optimize → Paste
                                                        ↓
                                                  Fire-and-forget
                                                        ↓
Background Learner (async, non-blocking):
  Queue(transcript, context) → Batch(5 or 30s) → LLM extract → Append to learned_vocab.json
                                                        ↓
                                                  Signal dict reload
```

### New Files

#### `src/corrector/learner.rs` — Background learner

```rust
use tokio::sync::mpsc;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Task sent from pipeline to background learner
pub struct LearnTask {
    pub raw_transcript: String,
    pub corrected_transcript: String,
    pub role: Option<String>,
    pub technologies: Vec<String>,
}

/// Extracted correction from LLM
pub struct ExtractedCorrection {
    pub heard: String,
    pub canonical: String,
}

/// Background learner: batches interactions, extracts corrections via LLM
pub struct BackgroundLearner {
    rx: mpsc::Receiver<LearnTask>,
    corrector: Arc<Mutex<PronunciationCorrector>>,
    llm: LlmRouter,
    batch_size: usize,
    batch_timeout: Duration,
}

impl BackgroundLearner {
    pub fn new(
        rx: mpsc::Receiver<LearnTask>,
        corrector: Arc<Mutex<PronunciationCorrector>>,
        llm: LlmRouter,
    ) -> Self {
        Self {
            rx,
            corrector,
            llm,
            batch_size: 5,
            batch_timeout: Duration::from_secs(30),
        }
    }

    /// Run forever: collect batch → extract → update dict
    pub async fn run(&mut self) {
        loop {
            let batch = self.collect_batch().await;
            if batch.is_empty() { continue; }

            // Build extraction prompt
            let prompt = self.build_extraction_prompt(&batch);

            // Call LLM (uses cheapest model)
            match self.llm.send(&prompt, "balanced", None).await {
                Ok(response) => {
                    if let Ok(terms) = parse_extracted_terms(&response) {
                        let mut dict = self.corrector.lock().await;
                        for term in terms {
                            if !dict.has_entry(&term.heard) {
                                let _ = dict.add_auto_correction(
                                    &term.heard, &term.canonical
                                );
                            }
                        }
                    }
                }
                Err(e) => log::warn!("Background learner LLM error: {e}"),
            }
        }
    }

    fn build_extraction_prompt(&self, batch: &[LearnTask]) -> String {
        // Collect unique transcripts
        let transcripts: Vec<&str> = batch.iter()
            .map(|t| t.raw_transcript.as_str())
            .collect();
        let tech = batch[0].technologies.join(", ");
        let role = batch[0].role.as_deref().unwrap_or("developer");

        format!(
            "You are a technical vocabulary extractor. Given these voice-to-text \
             transcripts from a {role} who works with: {tech}.\n\n\
             Find any technical terms that were likely misrecognized by speech-to-text. \
             Common patterns: 'next jazz' = 'Next.js', 'coober net ease' = 'Kubernetes', \
             'engine x' = 'Nginx'.\n\n\
             Transcripts:\n{transcripts}\n\n\
             Return ONLY a JSON array: [{{\"heard\": \"what STT heard\", \"canonical\": \"correct term\"}}]\n\
             Return [] if no corrections needed. Be conservative — only fix clear misrecognitions.",
            role = role,
            tech = tech,
            transcripts = transcripts.join("\n---\n"),
        )
    }
}

fn parse_extracted_terms(json: &str) -> anyhow::Result<Vec<ExtractedCorrection>> {
    // Parse JSON array from LLM response
    // Handle markdown code blocks if LLM wraps response
    let cleaned = json.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(cleaned).map_err(|e| anyhow::anyhow!("Parse error: {e}"))
}
```

#### `src/corrector/learned.rs` — Learned vocabulary storage

```rust
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedEntry {
    pub heard: String,
    pub canonical: String,
    pub source: String,       // "auto" or "sync"
    pub confidence: f32,      // 0.0-1.0
    pub seen_count: u32,      // times confirmed
    pub first_seen: u64,      // timestamp
    pub last_seen: u64,       // timestamp
}

pub struct LearnedStore {
    entries: Vec<LearnedEntry>,
    path: PathBuf,
}

impl LearnedStore {
    pub fn load(path: PathBuf) -> Self { ... }
    pub fn save(&self) -> anyhow::Result<()> { ... }
    pub fn add_or_reinforce(&mut self, heard: &str, canonical: &str, source: &str) { ... }
    pub fn has_entry(&self, heard: &str) -> bool { ... }
    pub fn entries(&self) -> &[LearnedEntry] { ... }
    pub fn reset(&mut self) { ... }
    pub fn count(&self) -> usize { ... }
}

fn default_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pie")
        .join("learned_vocab.json")
}
```

### Changes to Existing Files

#### `src/corrector/mod.rs` — Integrate learned store

```rust
pub mod learner;  // NEW
pub mod learned;  // NEW

pub struct PronunciationCorrector {
    dict: CorrectionDict,
    user: Vec<Correction>,
    user_path: Option<PathBuf>,
    learned: LearnedStore,  // NEW
}

impl PronunciationCorrector {
    pub fn new() -> Self {
        // ... existing ...
        let learned = LearnedStore::load(default_learned_path());
        // Merge learned entries into dict on rebuild()
    }

    fn rebuild(&mut self)) {
        let mut entries: Vec<Correction> = self.user.clone();
        // Add learned entries (with source: AutoLearned)
        for entry in self.learned.entries() {
            entries.push(Correction {
                heard: entry.heard.clone(),
                canonical: entry.canonical.clone(),
                source: Source::AutoLearned,
            });
        }
        // Add static entries (lowest priority)
        // ... existing logic ...
        self.dict = CorrectionDict::from_entries(entries);
    }

    pub fn add_auto_correction(&mut self, heard: &str, canonical: &str) {
        self.learned.add_or_reinforce(heard, canonical, "auto");
        self.rebuild();
    }

    pub fn has_entry(&self, heard: &str) -> bool {
        self.learned.has_entry(heard) || self.user.iter().any(|e| e.heard == heard)
    }

    pub fn learned_count(&self) -> usize {
        self.learned.count()
    }

    pub fn reset_learned(&mut self) {
        self.learned.reset();
        self.rebuild();
    }
}
```

#### `src/corrector/dictionary.rs` — Add AutoLearned source

```rust
pub enum Source {
    Static,
    User,
    Synced,       // NEW: from initial vocabulary sync
    AutoLearned,  // NEW: from background learner
}
```

#### `src/pipeline/engine.rs` — Fire-and-forget to learner

```rust
pub struct PieEngine {
    // ... existing fields ...
    learner_tx: Option<mpsc::Sender<LearnTask>>,  // NEW
}

impl PieEngine {
    pub async fn process(&mut self, input: &str, mode: &str) -> anyhow::Result<PieResult> {
        // ... existing pipeline (correct → intent → optimize) ...

        // Fire-and-forget: queue for background learning
        if let Some(tx) = &self.learner_tx {
            let _ = tx.try_send(LearnTask {
                raw_transcript: input.to_string(),
                corrected_transcript: correction.text.clone(),
                role: self.memory.profile.role.clone(),
                technologies: self.memory.profile.technologies.clone(),
            });
        }

        Ok(result)
    }

    /// Spawn background learner. Call once at app startup.
    pub fn spawn_learner(&mut self, llm: LlmRouter) -> mpsc::Sender<LearnTask> {
        let (tx, rx) = mpsc::channel(100);
        let corrector = Arc::new(Mutex::new(/* shared ref to self.corrector */));
        let mut learner = BackgroundLearner::new(rx, corrector, llm);
        tokio::spawn(async move { learner.run().await });
        self.learner_tx = Some(tx.clone());
        tx
    }
}
```

#### `src-tauri/src/main.rs` — Wire learner startup + new commands

```rust
// In main(), after engine init:
engine.spawn_learner(llm_for_learner);

// New Tauri commands:
#[tauri::command]
async fn get_learned_vocab_count(state: State<'_, AppState>) -> Result<usize, String> {
    let engine = state.engine.lock().await;
    Ok(engine.corrector_learned_count())
}

#[tauri::command]
async fn reset_learned_vocab(state: State<'_, AppState>) -> Result<(), String> {
    let mut engine = state.engine.lock().await;
    engine.corrector_reset_learned();
    Ok(())
}
```

#### `ui/src/lib/VocabularySettings.svelte` — Replace manual UI

```svelte
<script>
  let learnedCount = $state(0);
  let lastSync = $state('never');

  async function refresh() {
    learnedCount = await invoke('get_learned_vocab_count');
  }
  refresh();

  async function resetLearned() {
    await invoke('reset_learned_vocab');
    await refresh();
  }
</script>

<section class="group">
  <div class="field">
    <span class="field-label">Pronunciation Learning</span>
    <p class="caption">
      PIE learns your technical vocabulary automatically from your conversations.
      No manual setup needed.
    </p>
    <div class="stats">
      <span>{learnedCount} terms learned</span>
    </div>
    <button class="btn sm" onclick={resetLearned}>Reset Learned Vocabulary</button>
  </div>
</section>
```

### Acceptance Criteria

- [ ] After each pipeline run, a background task queues the transcript
- [ ] Background learner batches 5 interactions or 30s, then calls LLM
- [ ] Extracted corrections are appended to `~/.config/pie/learned_vocab.json`
- [ ] Learned entries are loaded on startup and merged into the corrector dict
- [ ] Pipeline never blocks on learner (try_send, not send)
- [ ] UI shows learned count and reset button
- [ ] `reset_learned_vocab` clears learned entries without touching user/static

### Crate Additions

None — `tokio::sync::mpsc` already available.

---

## Phase 3: Initial Vocabulary Sync

**Goal:** On first install, scan local conversation history to bootstrap vocabulary.

### New File

#### `src/corrector/sync.rs` — Conversation scanner + LLM extractor

```rust
use std::path::PathBuf;

pub struct HistorySource {
    pub name: String,           // "Claude", "ChatGPT", "Cursor"
    pub path: PathBuf,
    pub conversation_count: usize,
    pub file_pattern: String,   // "*.json", "*.sqlite", etc.
}

pub struct SyncResult {
    pub sources_scanned: usize,
    pub conversations_processed: usize,
    pub terms_extracted: usize,
    pub terms_added: usize,
}

impl VocabularySync {
    /// Discover conversation sources on this machine
    pub fn discover() -> Vec<HistorySource> {
        let mut sources = Vec::new();

        // Claude Desktop (macOS)
        if let Some(home) = dirs::home_dir() {
            let claude_macos = home.join("Library/Application Support/Claude");
            if claude_macos.exists() {
                sources.push(HistorySource {
                    name: "Claude Desktop".into(),
                    path: claude_macos,
                    conversation_count: 0, // count after scan
                    file_pattern: "*.json".into(),
                });
            }

            // Claude Desktop (Linux)
            let claude_linux = home.join(".config/claude");
            if claude_linux.exists() {
                sources.push(HistorySource {
                    name: "Claude Desktop".into(),
                    path: claude_linux,
                    conversation_count: 0,
                    file_pattern: "*.json".into(),
                });
            }

            // ChatGPT export (user downloads from chat.openai.com)
            let chatgpt_export = home.join("Downloads");
            for entry in std::fs::read_dir(&chatgpt_export).unwrap_or_default().flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.contains("chatgpt") && name.ends_with(".zip") {
                    sources.push(HistorySource {
                        name: "ChatGPT Export".into(),
                        path: entry.path(),
                        conversation_count: 0,
                        file_pattern: "*.zip".into(),
                    });
                }
            }

            // Cursor (if present)
            let cursor = home.join(".cursor");
            if cursor.exists() {
                sources.push(HistorySource {
                    name: "Cursor".into(),
                    path: cursor,
                    conversation_count: 0,
                    file_pattern: "*.json".into(),
                });
            }
        }

        sources
    }

    /// Extract text content from a conversation file
    fn extract_text(&self, path: &Path) -> Option<String> {
        // JSON: parse, extract message text fields
        // SQLite: query conversation tables
        // ZIP: extract, then parse JSON
        // ...
    }

    /// Run full sync with progress callback
    pub async fn run(
        &self,
        corrector: &Arc<Mutex<PronunciationCorrector>>,
        llm: &LlmRouter,
        on_progress: impl Fn(usize, usize),
    ) -> anyhow::Result<SyncResult> {
        let sources = Self::discover();
        let mut total_conversations = 0;
        let mut total_extracted = 0;

        for source in &sources {
            let files = self.scan_source(source)?;
            let batch_size = 10; // conversations per LLM call

            for (i, batch) in files.chunks(batch_size).enumerate() {
                let texts: Vec<String> = batch.iter()
                    .filter_map(|f| self.extract_text(f))
                    .collect();

                if texts.is_empty() { continue; }

                let prompt = self.build_sync_prompt(&texts);
                if let Ok(response) = llm.send(&prompt, "balanced", None).await {
                    if let Ok(terms) = parse_extracted_terms(&response) {
                        let mut dict = corrector.lock().await;
                        for term in terms {
                            dict.add_synced_correction(&term.heard, &term.canonical);
                            total_extracted += 1;
                        }
                    }
                }

                total_conversations += texts.len();
                on_progress(total_conversations, files.len());
            }
        }

        // Save sync state
        self.save_sync_state(total_conversations, total_extracted)?;

        Ok(SyncResult {
            sources_scanned: sources.len(),
            conversations_processed: total_conversations,
            terms_extracted: total_extracted,
            terms_added: total_extracted,
        })
    }

    fn build_sync_prompt(&self, conversations: &[String]) -> String {
        format!(
            "Extract all technical terms, product names, library names, and \
             proper nouns from these developer conversations. Include common \
             mispronunciation variants.\n\n\
             Conversations:\n{}\n\n\
             Return JSON: [{{\"term\": \"Next.js\", \"variants\": [\"nextjs\", \"next js\", \
             \"next jazz\", \"nexus js\"]}}]\n\
             Focus on terms a developer would speak into a microphone.",
            conversations.join("\n---\n"),
        )
    }
}
```

### Tauri Commands

```rust
#[tauri::command]
async fn discover_sync_sources() -> Result<Vec<SyncSourceDto>, String> {
    let sources = VocabularySync::discover();
    Ok(sources.into_iter().map(|s| SyncSourceDto {
        name: s.name,
        path: s.path.to_string_lossy().to_string(),
        conversation_count: s.conversation_count,
    }).collect())
}

#[tauri::command]
async fn run_vocabulary_sync(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<SyncResult, String> {
    let engine = state.engine.lock().await;
    let sync = VocabularySync::new();
    let result = sync.run(
        engine.corrector_ref(),
        engine.llm_ref(),
        |done, total| { let _ = app.emit("sync-progress", (done, total)); },
    ).await.map_err(|e| e.to_string())?;
    Ok(result)
}
```

### UI — `ui/src/lib/VocabularySync.svelte` — NEW

```svelte
<script>
  let sources = $state([]);
  let syncing = $state(false);
  let progress = $state({ done: 0, total: 0 });
  let result = $state(null);

  async function discover() {
    sources = await invoke('discover_sync_sources');
  }
  discover();

  async function startSync() {
    syncing = true;
    result = await invoke('run_vocabulary_sync');
    syncing = false;
  }

  // Listen for progress events
  onMount(() => {
    listen('sync-progress', (event) => {
      progress = { done: event.payload[0], total: event.payload[1] });
    });
  });
</script>

<section class="group">
  <div class="field">
    <span class="field-label">Sync Your Vocabulary</span>
    <p class="caption">
      Scan your existing AI conversations to learn your technical vocabulary.
      All processing happens locally.
    </p>

    {#if sources.length === 0}
      <p class="caption">No conversation sources found on this device.</p>
    {:else}
      <ul>
        {#each sources as source}
          <li>✓ {source.name} ({source.path})</li>
        {/each}
      </ul>

      {#if syncing}
        <progress value={progress.done} max={progress.total}></progress>
        <span>Processing {progress.done}/{progress.total}...</span>
      {:else if result}
        <p>✓ Synced {result.terms_extracted} terms from {result.conversations_processed} conversations</p>
      {:else}
        <button onclick={startSync}>Sync Now</button>
      {/if}
    {/if}
  </div>
</section>
```

### Acceptance Criteria

- [ ] `discover()` finds Claude, ChatGPT exports, Cursor on both macOS and Linux
- [ ] Sync extracts technical terms via LLM in batches of 10 conversations
- [ ] Progress events emitted to frontend during sync
- [ ] Synced entries stored with `source: "sync"` in `learned_vocab.json`
- [ ] Sync state saved (last run timestamp, terms count) to avoid re-scanning
- [ ] All data stays local — only sent to user's configured LLM provider
- [ ] User must click "Sync Now" — never auto-runs without consent

---

## Phase 4: Dual Hotkey System

**Goal:** Two global hotkeys — one pastes raw transcript, one pastes optimized prompt.

### Architecture

```
┌──────────────────────────────────────────────┐
│ Global Hotkey Listener                       │
│                                              │
│ Hotkey A: Ctrl+Shift+V   → Raw Paste        │
│ Hotkey B: Ctrl+Shift+Space → Optimized Paste │
│                                              │
│ Flow:                                        │
│  1. Show recording overlay                   │
│  2. Record audio (VAD-gated)                 │
│  3. Transcribe (whisper)                     │
│  4. Correct (pronunciation dict)             │
│  5a. [Hotkey A] → paste corrected transcript │
│  5b. [Hotkey B] → intent → optimize → paste  │
│  6. Paste into focused app (clipboard + sim) │
└──────────────────────────────────────────────┘
```

### Crate Additions

```toml
# src-tauri/Cargo.toml — already has tauri-plugin-global-shortcut
# Add for clipboard + paste simulation:
arboard = "3"  # Cross-platform clipboard (already using tauri-plugin-clipboard-manager)
```

### Changes to Existing Files

#### `src-tauri/src/settings.rs` — Add second hotkey

```rust
// Change existing hotkey field:
pub hotkey_raw: String,       // "CmdOrCtrl+Shift+V" — raw paste
pub hotkey_optimized: String, // "CmdOrCtrl+Shift+Space" — optimized paste
// Remove old single hotkey field
```

#### `src-tauri/src/main.rs` — Register both hotkeys

```rust
fn register_hotkeys(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();

    // Hotkey A: Raw paste
    let raw_shortcut = Shortcut::from_str(&settings.hotkey_raw)?;
    gs.on_shortcut(raw_shortcut, move |_app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            on_hotkey_raw(_app);
        }
    })?;

    // Hotkey B: Optimized paste
    let opt_shortcut = Shortcut::from_str(&settings.hotkey_optimized)?;
    gs.on_shortcut(opt_shortcut, move |_app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            on_hotkey_optimized(_app);
        }
    })?;

    Ok(())
}

fn on_hotkey_raw(app: &AppHandle) {
    // Same as current on_hotkey but uses paste_output="transcript"
    on_hotkey_with_mode(app, "transcript");
}

fn on_hotkey_optimized(app: &AppHandle) {
    // Same as current on_hotkey but uses paste_output="prompt"
    on_hotkey_with_mode(app, "prompt");
}

fn on_hotkey_with_mode(app: &AppHandle, mode: &str) {
    let state = app.state::<AppState>();
    if state.busy.load(Ordering::Relaxed) { return; }

    // Start recording → show overlay
    // On stop → transcribe → correct → (if mode=="prompt") intent+optimize
    // → clipboard → simulate paste
}
```

#### `ui/src/lib/HotkeyRecorder.svelte` — Two hotkey inputs

```svelte
<div class="field">
  <span class="field-label">Raw Paste Hotkey</span>
  <span class="caption">Record → transcribe → paste raw text</span>
  <HotkeyRecorder bind:value={settings.hotkey_raw} />
</div>
<div class="field">
  <span class="field-label">Optimized Paste Hotkey</span>
  <span class="caption">Record → transcribe → optimize → paste prompt</span>
  <HotkeyRecorder bind:value={settings.hotkey_optimized} />
</div>
```

### Acceptance Criteria

- [ ] Two separate global hotkeys registered
- [ ] Hotkey A: records → transcribes → corrects → pastes raw text
- [ ] Hotkey B: records → transcribes → corrects → intent → optimize → pastes prompt
- [ ] Both show recording overlay during capture
- [ ] Both paste via clipboard simulation into focused app
- [ ] Settings UI has two separate hotkey configuration fields
- [ ] Hotkeys configurable by user (HotkeyRecorder component)
- [ ] Busy flag prevents double-trigger during processing

### Platform Notes

- **macOS**: Existing NSPanel overlay + `enigo` for paste simulation
- **Linux**: `xdotool` or `enigo` for paste; GTK overlay
- **Windows**: `enigo` for paste; borderless window overlay

---

## Phase 5: Long Conversation Refinement

**Goal:** Compress long, rambling voice input into a sharp prompt.

### Changes

#### `src/optimizer/refine.rs` — NEW optimizer mode

```rust
/// Refine mode: for long inputs (>80 words), compress into sharp prompt.
/// For short inputs, pass through to balanced mode.
pub fn optimize(intent: &Intent, memory: &MemoryStore) -> OptimizedPrompt {
    let word_count = intent.raw_input.split_whitespace().count();

    if word_count <= 80 {
        // Short enough, use balanced mode
        return balanced::optimize(intent, memory);
    }

    // Long input — build a refinement prompt for the LLM
    let refine_prompt = format!(
        "The user spoke this long voice request. Extract the core intent as a \
         clear, concise prompt. Keep ALL technical terms and constraints. \
         Remove filler words (um, like, you know, so, basically). \
         Deduplicate repeated ideas. Output ONLY the refined prompt.\n\n\
         User context: role={role:?}, tech={tech:?}.\n\n\
         User said:\n{text}",
        role = memory.profile.role,
        tech = memory.profile.technologies,
        text = intent.raw_input,
    );

    // NOTE: This needs LLM access. Store the refine_prompt for the engine
    // to send asynchronously. Return a marker that tells the engine to
    // run LLM refinement.
    OptimizedPrompt {
        text: intent.raw_input.to_string(), // placeholder, engine will replace
        mode: OptimizationMode::Refine,
        estimated_tokens: estimate_tokens(&intent.raw_input),
        needs_refinement: true,  // NEW field
        refinement_prompt: Some(refine_prompt),  // NEW field
    }
}
```

#### `src/optimizer/mod.rs` — Add Refine variant

```rust
pub enum OptimizationMode {
    Compact,
    Balanced,
    Enhanced,
    Adaptive,
    Refine,  // NEW
}
```

#### `src/pipeline/engine.rs` — Handle refinement

```rust
// In process():
let optimized = match optimization_mode {
    // ... existing modes ...
    OptimizationMode::Refine => refine::optimize(&intent, &self.memory),
};

// If needs_refinement, send to LLM for refinement
let final_prompt = if optimized.needs_refinement {
    if let Some(refine_prompt) = &optimized.refinement_prompt {
        self.llm.send(refine_prompt, "balanced", None).await
            .unwrap_or(optimized.text)
    } else {
        optimized.text
    }
} else {
    optimized.text
};
```

### Acceptance Criteria

- [ ] Inputs >80 words trigger refinement mode automatically (in adaptive)
- [ ] Refinement removes filler, deduplicates, extracts core intent
- [ ] Refined prompt preserves all technical terms and constraints
- [ ] Short inputs pass through unchanged
- [ ] `Refine` mode available as explicit CLI option: `pie --mode refine`

---

## Phase 6: Code-Aware Post-Processing

**Goal:** Translate spoken code patterns into syntax.

### New File

#### `src/corrector/code_phrases.rs` — Code phrase dictionary

```rust
use std::collections::HashMap;

/// Build a map of spoken code patterns to their syntax equivalents.
/// These run AFTER pronunciation correction, BEFORE intent extraction.
pub fn code_phrase_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();

    // Syntax
    m.insert("dot log", ".log(");
    m.insert("dot map", ".map(");
    m.insert("dot filter", ".filter(");
    m.insert("dot find", ".find(");
    m.insert("dot push", ".push(");
    m.insert("arrow function", "() => ");
    m.insert("fat arrow", "() => ");
    m.insert("triple equals", "===");
    m.insert("double equals", "==");
    m.insert("not equal", "!=");
    m.insert("not strictly equal", "!==");
    m.insert("open brace", "{");
    m.insert("close brace", "}");
    m.insert("open bracket", "[");
    m.insert("close bracket", "]");
    m.insert("open paren", "(");
    m.insert("close paren", ")");
    m.insert("semi colon", ";");
    m.insert("single quote", "'");
    m.insert("double quote", "\"");
    m.insert("back tick", "`");
    m.insert("pipe operator", "|");
    m.insert("ampersand", "&");
    m.insert("hash tag", "#");

    // Common code phrases
    m.insert("console dot log", "console.log(");
    m.insert("document dot get element by id", "document.getElementById(");
    m.insert("import from", "import { } from '");
    m.insert("export default", "export default");
    m.insert("async function", "async function");
    m.insert("await", "await ");
    m.insert("try catch", "try { } catch (e) { }");
    m.insert("if else", "if () { } else { }");
    m.insert("for loop", "for (let i = 0; i < ; i++)");
    m.insert("for each", ".forEach(");
    m.insert("const array", "const arr = []");
    m.insert("const object", "const obj = {}");

    m
}

/// Apply code phrase translations. Runs after pronunciation correction.
pub fn apply_code_phrases(text: &str) -> String {
    let map = code_phrase_map();
    let mut result = text.to_string();

    // Sort by phrase length (longest first) to avoid partial matches
    let mut phrases: Vec<(&str, &str)> = map.into_iter().collect();
    phrases.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));

    for (spoken, syntax) in &phrases {
        // Case-insensitive replacement
        let lower = result.to_lowercase();
        if let Some(pos) = lower.find(spoken) {
            result = format!(
                "{}{}{}",
                &result[..pos],
                syntax,
                &result[pos + spoken.len()..]
            );
        }
    }

    result
}
```

#### Integration in `src/corrector/mod.rs`

```rust
pub mod code_phrases;  // NEW

// In correct():
pub fn correct(&self, text: &str, extra_allowed: &HashSet<String>) -> CorrectionOutcome {
    // ... existing exact + phonetic correction ...
    let corrected = phon.text;

    // Apply code phrases
    let with_code = code_phrases::apply_code_phrases(&corrected);

    CorrectionOutcome {
        text: with_code,
        applied: combined_applied,
    }
}
```

### Acceptance Criteria

- [ ] "console dot log hello" → "console.log(hello"
- [ ] "create an arrow function" → "create an () => "
- [ ] Code phrases run after pronunciation dict, before intent extraction
- [ ] Longest-phrase-first matching prevents partial replacements
- [ ] No false positives on normal speech (only activates for code patterns)
- [ ] Code phrase map is extensible (loadable from JSON)

---

## Summary: Build Order

| Phase | What | Effort | Deps |
|-------|------|--------|------|
| **1** | BYOK LLM Config | ~2 hours | None |
| **2** | Auto-Learning (background) | ~1 day | Phase 1 |
| **3** | Initial Vocab Sync | ~1 day | Phase 1 |
| **4** | Dual Hotkey System | ~1 day | None (parallel with 2/3) |
| **5** | Long Conversation Refinement | ~4 hours | Phase 1 |
| **6** | Code-Aware Post-Processing | ~4 hours | None |

**Total estimated effort: 4-5 days**

Phases 4, 5, 6 are independent and can be done in parallel with 2/3.
Phase 1 is the foundation — do it first.
