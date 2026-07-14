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
}

/// The main PIE engine that orchestrates the full pipeline.
///
/// Pipeline: Input -> Intent Extraction -> Memory Lookup -> Prompt Optimization -> LLM
pub struct PieEngine {
    memory: MemoryStore,
    extractor: IntentExtractor,
    llm: LlmRouter,
    stt: Option<Box<dyn SttEngine>>,
}

impl PieEngine {
    /// Initialize the PIE engine
    pub async fn new() -> anyhow::Result<Self> {
        let memory = MemoryStore::load();
        let extractor = IntentExtractor::new();
        let llm = LlmRouter::new();

        Ok(Self {
            memory,
            extractor,
            llm,
            stt: None,
        })
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
