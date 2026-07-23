use crate::corrector::{AppliedFix, PronunciationCorrector};
use crate::intent::{Intent, IntentExtractor};
use crate::llm::LlmRouter;
use crate::memory::store::MemoryStore;
use crate::optimizer::OptimizationMode;
use crate::optimizer::{adaptive, balanced, compact, enhanced};
use crate::stt::SttEngine;

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
        })
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

    /// Process text input through the full PIE pipeline.
    /// Returns the extracted intent and optimized prompt.
    pub async fn process(&mut self, input: &str, mode: &str) -> anyhow::Result<PieResult> {
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
            _ => OptimizationMode::Adaptive,
        };

        let optimized = match optimization_mode {
            OptimizationMode::Compact => compact::optimize(&intent, &self.memory),
            OptimizationMode::Balanced => balanced::optimize(&intent, &self.memory),
            OptimizationMode::Enhanced => enhanced::optimize(&intent, &self.memory),
            OptimizationMode::Adaptive => adaptive::optimize(&intent, &self.memory),
        };

        // Step 4: Save memory
        let _ = self.memory.save();

        Ok(PieResult {
            intent,
            optimized_prompt: optimized.text,
            mode: optimized.mode,
            estimated_tokens: optimized.estimated_tokens,
            corrected_transcript: correction.text.clone(),
            applied: correction.applied,
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

    /// Get the current memory store (for inspection)
    pub fn memory(&self) -> &MemoryStore {
        &self.memory
    }

    /// Get a mutable reference to memory (for profile updates)
    pub fn memory_mut(&mut self) -> &mut MemoryStore {
        &mut self.memory
    }
}
