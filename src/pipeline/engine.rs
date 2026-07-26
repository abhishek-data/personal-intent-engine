use crate::corrector::llm_correct;
use crate::corrector::{AppliedFix, CorrectionOutcome, PronunciationCorrector};
use crate::intent::{Intent, IntentExtractor};
use crate::llm::LlmRouter;
use crate::memory::store::MemoryStore;
use crate::optimizer::refine::{self, RefineResult};
use crate::optimizer::OptimizationMode;
use crate::optimizer::{adaptive, balanced, compact, enhanced};
use crate::stt::SttEngine;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Fire-and-forget signal from the pipeline to the background learner.
pub struct LearnTask {
    /// The raw (pre-correction) transcript, for the learner to mine.
    pub raw_transcript: String,
    /// The user's configured role, if any, for context-aware learning.
    pub role: Option<String>,
    /// The user's known technologies, for context-aware learning.
    pub technologies: Vec<String>,
}

/// Result of processing input through the PIE pipeline
#[derive(Debug)]
pub struct PieResult {
    /// Extracted intent
    pub intent: Intent,

    /// Optimized prompt
    pub optimized_prompt: String,

    /// Optimization mode used
    pub mode: OptimizationMode,

    /// Estimated token count
    pub estimated_tokens: usize,

    /// The transcript after correction (what intent/optimize actually saw).
    pub corrected_transcript: String,

    /// Corrections applied to the transcript, for UI transparency.
    pub applied: Vec<AppliedFix>,

    /// Present only in `refine` mode on long input: the LLM instruction to
    /// compress `optimized_prompt` (the deterministic fallback) further, via
    /// `apply_refine`. `None` for every other mode, and for short input in
    /// `refine` mode (which needs no LLM pass).
    pub refine_request: Option<crate::optimizer::refine::RefineRequest>,
}

/// The main PIE engine that orchestrates the full pipeline.
///
/// Pipeline: Input -> Intent Extraction -> Memory Lookup -> Prompt Optimization -> LLM
pub struct PieEngine {
    memory: MemoryStore,
    extractor: IntentExtractor,
    llm: LlmRouter,
    stt: Option<Box<dyn SttEngine>>,
    corrector: PronunciationCorrector,
    learner_tx: Option<mpsc::Sender<LearnTask>>,
    learned_vocab_path: Option<PathBuf>,
    learned_mtime: Option<std::time::SystemTime>,
}

/// Default on-disk location for the learned/synced vocabulary store. Mirrors
/// `corrector::default_learned_path` (kept private there) so the engine can
/// stat the file for reload-on-change without exposing the path externally.
fn default_learned_vocab_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pie")
        .join("learned_vocab.json")
}

impl PieEngine {
    /// Initialize the PIE engine
    pub async fn new() -> anyhow::Result<Self> {
        let memory = MemoryStore::load();
        let extractor = IntentExtractor::new();
        let llm = LlmRouter::new();
        let corrector = PronunciationCorrector::new();

        Ok(Self {
            memory,
            extractor,
            llm,
            stt: None,
            corrector,
            learner_tx: None,
            learned_vocab_path: Some(default_learned_vocab_path()),
            learned_mtime: None,
        })
    }

    /// Initialize the engine with a BYOK LLM config (falls back to env vars
    /// when the config URL is empty).
    pub async fn with_config(config: &crate::llm::LlmConfig) -> anyhow::Result<Self> {
        let mut engine = Self::new().await?;
        engine.set_llm_config(config);
        Ok(engine)
    }

    /// Rebuild the LLM router from a new config (e.g. after the user edits
    /// LLM settings). Does not touch memory, STT, or the corrector.
    pub fn set_llm_config(&mut self, config: &crate::llm::LlmConfig) {
        self.llm = crate::llm::LlmRouter::from_config(config);
    }

    /// Test/ephemeral engine: performs NO disk persistence. Memory lives only
    /// in-process (never saved), and the corrector reads/writes an isolated
    /// `user_dict_path` instead of the real user config — so integration tests
    /// never touch or pollute real app data.
    #[doc(hidden)]
    pub fn new_ephemeral(user_dict_path: std::path::PathBuf) -> Self {
        Self {
            memory: MemoryStore::default(),
            extractor: IntentExtractor::new(),
            llm: LlmRouter::new(),
            stt: None,
            corrector: PronunciationCorrector::with_user_path(user_dict_path),
            learner_tx: None,
            learned_vocab_path: None,
            learned_mtime: None,
        }
    }

    /// Attach a speech-to-text engine, enabling `process_audio`.
    pub fn with_stt(mut self, stt: Box<dyn SttEngine>) -> Self {
        self.stt = Some(stt);
        self
    }

    /// Transcribe 16 kHz mono samples and run them through the full pipeline.
    /// The transcript is available afterwards as `intent.raw_input`.
    pub async fn process_audio(
        &mut self,
        samples: &[f32],
        mode: &str,
    ) -> anyhow::Result<PieResult> {
        let stt = self
            .stt
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No STT engine configured. Use with_stt()."))?;

        let text = stt.transcribe(samples)?;
        let text = text.trim();
        log::info!("Transcribed {} samples: {text:?}", samples.len());
        if text.is_empty() {
            anyhow::bail!("Transcription produced no text (silence or unintelligible audio)");
        }

        self.process(text, mode).await
    }

    /// Reload learned vocab if the file changed since we last looked. Cheap
    /// stat; only rebuilds when mtime advanced.
    fn maybe_reload_learned(&mut self) {
        let Some(path) = &self.learned_vocab_path else {
            return;
        };
        let Ok(meta) = std::fs::metadata(path) else {
            return;
        };
        let Ok(mtime) = meta.modified() else {
            return;
        };
        if self.learned_mtime != Some(mtime) {
            self.learned_mtime = Some(mtime);
            let _ = self.corrector.reload_learned();
        }
    }

    /// Process text input through the full PIE pipeline.
    /// Returns the extracted intent and optimized prompt.
    pub async fn process(&mut self, input: &str, mode: &str) -> anyhow::Result<PieResult> {
        let raw = input.to_string();

        // Reload learned vocab if the background learner appended since we
        // last looked, so this turn benefits from it.
        self.maybe_reload_learned();

        // Step 0: Correct speech-to-text jargon errors before anything else.
        // Allow-set: terms the user is known to use, so static phonetic entries
        // only fire for relevant terms. Derived from the profile's tech stack.
        let allowed: std::collections::HashSet<String> = self
            .memory
            .profile
            .technologies
            .iter()
            .map(|t| t.to_lowercase())
            .collect();
        let correction = self.corrector.correct(input, &allowed);
        for fix in &correction.applied {
            // `from` is the lowercased heard phrase; reinforce if it's learned.
            // NOTE: when background mining is on, this reinforcement and the
            // learner's `run()` (learner.rs) are two independent writers of
            // learned_vocab.json, each doing load-modify-save from its own
            // snapshot, so a concurrent write can lose an update. Bounded to
            // opt-in auto-learned vocab (never user/static data); it self-heals
            // on the next mtime reload. Planned fix: funnel writes through a
            // single owner (tracked follow-up).
            // Synced (user-imported) entries from `corrector::sync` also live in
            // this same file, sharing this race, but they do NOT self-heal: an
            // import is one-shot, so a lost update from a concurrent write is
            // permanent until the user re-imports. The tracked single-writer
            // follow-up must cover synced data too, not just auto-learned vocab.
            let _ = self.corrector.reinforce_learned(&fix.from);
        }
        let input = correction.text.as_str();

        // Step 1: Extract intent
        let intent = self.extractor.extract(input);

        // Step 2: Record interaction in memory
        let conv_type = format!("{:?}", intent.conversation_type);
        self.memory.record_interaction(input, &conv_type);

        // Step 3: Optimize prompt based on mode
        let optimization_mode = match mode {
            "compact" => OptimizationMode::Compact,
            "balanced" => OptimizationMode::Balanced,
            "enhanced" => OptimizationMode::Enhanced,
            "refine" => OptimizationMode::Refine,
            _ => OptimizationMode::Adaptive,
        };

        let mut refine_request = None;
        let optimized = match optimization_mode {
            OptimizationMode::Compact => compact::optimize(&intent, &self.memory),
            OptimizationMode::Balanced => balanced::optimize(&intent, &self.memory),
            OptimizationMode::Enhanced => enhanced::optimize(&intent, &self.memory),
            OptimizationMode::Adaptive => adaptive::optimize(&intent, &self.memory),
            OptimizationMode::Refine => match refine::optimize(&intent, &self.memory) {
                RefineResult::Balanced(p) => p,
                RefineResult::Refine { base, request } => {
                    refine_request = Some(request);
                    base
                }
            },
        };

        // Step 4: Save memory
        let _ = self.memory.save();

        // Fire-and-forget: hand the raw transcript to the background learner,
        // if one is attached. Never blocks the pipeline.
        if let Some(tx) = &self.learner_tx {
            let _ = tx.try_send(LearnTask {
                raw_transcript: raw,
                role: self.memory.profile.role.clone(),
                technologies: self.memory.profile.technologies.clone(),
            });
        }

        Ok(PieResult {
            intent,
            optimized_prompt: optimized.text,
            mode: optimized.mode,
            estimated_tokens: optimized.estimated_tokens,
            corrected_transcript: correction.text.clone(),
            applied: correction.applied,
            refine_request,
        })
    }

    /// Send optimized prompt to an LLM provider
    pub async fn send_to_llm(
        &self,
        prompt: &str,
        provider: &str,
        model: Option<&str>,
    ) -> anyhow::Result<String> {
        self.llm.send(prompt, provider, model).await
    }

    /// Run the refine LLM pass. Returns the compressed prompt on success, or
    /// `original` on any LLM failure or empty reply (never drops input).
    pub async fn apply_refine(
        &self,
        request: &crate::optimizer::refine::RefineRequest,
        original: &str,
        provider: &str,
        model: Option<&str>,
    ) -> String {
        match self.llm.send(&request.prompt, provider, model).await {
            Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => original.to_string(),
        }
    }

    /// Get the current memory store (for inspection)
    pub fn memory(&self) -> &MemoryStore {
        &self.memory
    }

    /// Get a mutable reference to memory (for profile updates)
    pub fn memory_mut(&mut self) -> &mut MemoryStore {
        &mut self.memory
    }

    /// The user's own heard->canonical corrections (for UI listing/editing).
    pub fn corrector_user_corrections(&self) -> Vec<crate::corrector::Correction> {
        self.corrector.user_corrections()
    }

    /// Add or update a user correction.
    pub fn corrector_add(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()> {
        self.corrector.add_user_correction(heard, canonical)
    }

    /// Remove a user correction.
    pub fn corrector_remove(&mut self, heard: &str) -> anyhow::Result<()> {
        self.corrector.remove_user_correction(heard)
    }

    /// Attach a background learner: `process` will forward a `LearnTask` for
    /// every turn (non-blocking; dropped if the channel is full or closed).
    pub fn set_learner_tx(&mut self, tx: mpsc::Sender<LearnTask>) {
        self.learner_tx = Some(tx);
    }

    /// Number of learned (auto + synced) corrections, for UI display.
    pub fn corrector_learned_count(&self) -> usize {
        self.corrector.learned_count()
    }

    /// Clear all learned/synced corrections (never touches the user dict).
    pub fn corrector_reset_learned(&mut self) -> anyhow::Result<()> {
        self.corrector.reset_learned()
    }

    /// Test-only ephemeral engine with an isolated learned-vocab path in
    /// addition to the isolated user dict, so tests can exercise learned
    /// corrections/reload without touching real app data.
    #[doc(hidden)]
    pub fn new_ephemeral_with_learned(user: PathBuf, learned: PathBuf) -> Self {
        let mut e = Self::new_ephemeral(user.clone());
        e.corrector = crate::corrector::PronunciationCorrector::with_paths(user, learned.clone());
        e.learned_vocab_path = Some(learned);
        e
    }

    /// Test-only: add an auto-learned correction directly to the corrector.
    #[doc(hidden)]
    pub fn corrector_add_auto(&mut self, heard: &str, canonical: &str) -> anyhow::Result<()> {
        self.corrector.add_auto_correction(heard, canonical)
    }

    /// Test-only: the `seen_count` of a learned entry, or 0 when absent.
    #[doc(hidden)]
    pub fn corrector_learned_seen(&self, heard: &str) -> u32 {
        self.corrector.learned_entries_seen(heard).unwrap_or(0)
    }

    /// Import vocabulary from the given paths via the configured LLM, storing
    /// terms as `Source::Synced`. User-initiated only.
    ///
    /// Borrow-safe by construction: `run_sync`'s `add` closure captures only a
    /// local `Vec` (not `self`), so it can run alongside the immutable borrow
    /// of `self.llm` for the duration of the `.await`. Once `run_sync`
    /// returns, that borrow ends and the collected pairs are applied via
    /// `self.corrector.add_synced_correction`, which needs `&mut self`.
    pub async fn run_vocabulary_sync(
        &mut self,
        paths: Vec<PathBuf>,
        provider: &str,
        model: Option<&str>,
        on_progress: impl Fn(usize, usize),
    ) -> anyhow::Result<crate::corrector::sync::SyncResult> {
        let mut pairs: Vec<(String, String)> = Vec::new();
        let result = crate::corrector::sync::run_sync(
            &paths,
            &self.llm,
            provider,
            model,
            |variant, canonical| {
                pairs.push((variant.to_string(), canonical.to_string()));
                Ok(())
            },
            on_progress,
        )
        .await?;
        for (variant, canonical) in &pairs {
            let _ = self.corrector.add_synced_correction(variant, canonical);
        }
        let _ = crate::corrector::sync::record_sync_state(result.terms_added);
        Ok(result)
    }

    /// Opt-in deep correction via the configured LLM. NOT on the always-on
    /// path — called only from the settings toggle or the on-demand command.
    /// Returns `Err` on any LLM failure; callers decide whether to fall back to
    /// the deterministic result (toggle path) or surface the error (on-demand).
    pub async fn deep_correct(
        &self,
        transcript: &str,
        provider: &str,
        model: Option<&str>,
    ) -> anyhow::Result<CorrectionOutcome> {
        let prompt = llm_correct::build_prompt(
            transcript,
            self.memory.profile.role.as_deref(),
            &self.memory.profile.technologies,
        );
        let corrected = self.llm.send(&prompt, provider, model).await?;
        let corrected = corrected.trim().to_string();
        let applied = llm_correct::diff_fixes(transcript, &corrected);
        Ok(CorrectionOutcome {
            text: corrected,
            applied,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmConfig;

    #[tokio::test]
    async fn with_config_uses_configured_client() {
        let cfg = LlmConfig {
            api_url: "https://api.example.com/v1".into(),
            api_key: "sk-x".into(),
            model: "gpt-4o-mini".into(),
        };
        let engine = PieEngine::with_config(&cfg).await.unwrap();
        assert!(engine.llm.is_available("openai"));
    }

    #[tokio::test]
    async fn set_llm_config_rebuilds_router() {
        std::env::remove_var("OPENAI_API_KEY");
        let mut engine = PieEngine::new().await.unwrap();
        assert!(!engine.llm.is_available("openai"));
        engine.set_llm_config(&LlmConfig {
            api_url: "https://api.example.com/v1".into(),
            api_key: "sk-x".into(),
            model: String::new(),
        });
        assert!(engine.llm.is_available("openai"));
    }

    #[tokio::test]
    async fn process_reinforces_a_firing_learned_correction() {
        // Ephemeral engine with injected corrector paths.
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let cpath = dir.join(format!("pie-eng-user-{uid}.json"));
        let lpath = dir.join(format!("pie-eng-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        engine
            .corrector_add_auto("terra form", "Terraform")
            .unwrap();
        let before = engine.corrector_learned_seen("terra form");
        let _ = engine
            .process("deploy with terra form now", "balanced")
            .await
            .unwrap();
        let after = engine.corrector_learned_seen("terra form");
        assert!(
            after > before,
            "a firing learned correction must be reinforced"
        );
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }

    #[tokio::test]
    async fn refine_mode_attaches_request_for_long_input_and_falls_back() {
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let cpath = dir.join(format!("pie-rf-user-{uid}.json"));
        let lpath = dir.join(format!("pie-rf-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        let long = "so ".to_string() + &"refactor the widget ".repeat(30); // > 80 words
        let res = engine.process(&long, "refine").await.unwrap();
        assert!(
            res.refine_request.is_some(),
            "long input attaches a refine request"
        );
        // echo provider returns the prompt; apply_refine returns it trimmed (non-empty) -> not the fallback.
        let req = res.refine_request.as_ref().unwrap();
        let refined = engine
            .apply_refine(req, &res.optimized_prompt, "echo", None)
            .await;
        assert!(!refined.is_empty());
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }

    #[tokio::test]
    async fn refine_mode_short_input_no_request() {
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let cpath = dir.join(format!("pie-rf2-user-{uid}.json"));
        let lpath = dir.join(format!("pie-rf2-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        let res = engine.process("build a rust cli", "refine").await.unwrap();
        assert!(res.refine_request.is_none(), "short input needs no refine");
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }

    #[tokio::test]
    async fn run_vocabulary_sync_counts_conversations_echo() {
        let dir = std::env::temp_dir();
        let uid = format!("{}-{}", std::process::id(), line!());
        let src = dir.join(format!("pie-syncsrc-{uid}"));
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.md"), "deploy nextjs on vercel").unwrap();
        let cpath = dir.join(format!("pie-eng-user-{uid}.json"));
        let lpath = dir.join(format!("pie-eng-learned-{uid}.json"));
        let mut engine = PieEngine::new_ephemeral_with_learned(cpath.clone(), lpath.clone());
        let res = engine
            .run_vocabulary_sync(vec![src.clone()], "echo", None, |_d, _t| {})
            .await
            .unwrap();
        assert_eq!(res.conversations, 1);
        let _ = std::fs::remove_dir_all(src);
        let _ = std::fs::remove_file(cpath);
        let _ = std::fs::remove_file(lpath);
    }
}
