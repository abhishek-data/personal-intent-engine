//! Opt-in background learner: batches pipeline transcripts and mines new
//! pronunciation corrections via the configured LLM, rate-limited so it never
//! burns credits. OFF by default; spawned only when the user enables it.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::corrector::learned::{LearnedSource, LearnedStore};
use crate::llm::LlmRouter;
use crate::pipeline::engine::LearnTask;

const BATCH_SIZE: usize = 5;
const BATCH_WINDOW: Duration = Duration::from_secs(30);
const MIN_LLM_INTERVAL: Duration = Duration::from_secs(30);

/// One correction the LLM extracted from a batch of transcripts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedCorrection {
    pub heard: String,
    pub canonical: String,
}

/// Tolerant parse of an LLM JSON reply (handles ``` / ```json fences).
pub fn parse_extracted(json: &str) -> anyhow::Result<Vec<ExtractedCorrection>> {
    let cleaned = json
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    Ok(serde_json::from_str(cleaned)?)
}

/// Build the conservative extraction prompt for a batch.
pub fn build_extraction_prompt(batch: &[LearnTask]) -> String {
    let role = batch
        .iter()
        .find_map(|t| t.role.clone())
        .unwrap_or_else(|| "developer".to_string());
    let mut tech: Vec<String> = batch.iter().flat_map(|t| t.technologies.clone()).collect();
    tech.sort();
    tech.dedup();
    let transcripts: Vec<&str> = batch.iter().map(|t| t.raw_transcript.as_str()).collect();
    format!(
        "You are a technical vocabulary extractor. These are voice-to-text \
         transcripts from a {role} who works with: {tech}.\n\n\
         Find technical terms likely MISRECOGNIZED by speech-to-text (e.g. \
         'next jazz'='Next.js', 'coober net ease'='Kubernetes', 'engine x'='Nginx'). \
         Be conservative — only clear misrecognitions.\n\n\
         Transcripts:\n{joined}\n\n\
         Return ONLY a JSON array [{{\"heard\":\"what STT heard\",\"canonical\":\"correct term\"}}]. \
         Return [] if none.",
        role = role,
        tech = tech.join(", "),
        joined = transcripts.join("\n---\n"),
    )
}

/// The background learner task.
pub struct BackgroundLearner {
    rx: mpsc::Receiver<LearnTask>,
    llm: LlmRouter,
    learned_path: PathBuf,
    provider: String,
    model: Option<String>,
    known: HashSet<String>,
    last_llm: Option<Instant>,
}

impl BackgroundLearner {
    /// Build a new background learner. `known` should be pre-seeded with the
    /// `heard` keys already present across every correction tier so the
    /// learner never re-mines terms that are already handled.
    pub fn new(
        rx: mpsc::Receiver<LearnTask>,
        llm: LlmRouter,
        learned_path: PathBuf,
        provider: String,
        model: Option<String>,
        known: HashSet<String>,
    ) -> Self {
        Self {
            rx,
            llm,
            learned_path,
            provider,
            model,
            known,
            last_llm: None,
        }
    }

    /// Run forever: batch, rate-limit, extract, persist new corrections.
    pub async fn run(mut self) {
        loop {
            let Some(batch) = self.collect_batch().await else {
                break;
            };
            if batch.is_empty() {
                continue;
            }
            // Rate limit.
            if let Some(prev) = self.last_llm {
                let since = prev.elapsed();
                if since < MIN_LLM_INTERVAL {
                    tokio::time::sleep(MIN_LLM_INTERVAL - since).await;
                }
            }
            self.last_llm = Some(Instant::now());
            let prompt = build_extraction_prompt(&batch);
            let reply = match self
                .llm
                .send(&prompt, &self.provider, self.model.as_deref())
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("background learner LLM error: {e}");
                    continue;
                }
            };
            let terms = match parse_extracted(&reply) {
                Ok(t) => t,
                Err(e) => {
                    log::warn!("background learner parse error: {e}");
                    continue;
                }
            };
            if terms.is_empty() {
                continue;
            }
            let mut store = LearnedStore::load(self.learned_path.clone());
            for t in terms {
                let key = t.heard.trim().to_lowercase();
                if key.is_empty() || self.known.contains(&key) || store.has_entry(&key) {
                    continue;
                }
                if store
                    .add_or_reinforce(&t.heard, &t.canonical, LearnedSource::Auto)
                    .is_ok()
                {
                    self.known.insert(key);
                }
            }
        }
    }

    /// Collect up to BATCH_SIZE tasks, or whatever arrived within BATCH_WINDOW
    /// of the first. Returns None when the channel is closed and drained.
    async fn collect_batch(&mut self) -> Option<Vec<LearnTask>> {
        let first = self.rx.recv().await?; // None => channel closed
        let mut batch = vec![first];
        let deadline = Instant::now() + BATCH_WINDOW;
        while batch.len() < BATCH_SIZE {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match timeout(remaining, self.rx.recv()).await {
                Ok(Some(task)) => batch.push(task),
                Ok(None) => break, // channel closed
                Err(_) => break,   // window elapsed
            }
        }
        Some(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extracted_strips_code_fences() {
        let raw = "```json\n[{\"heard\":\"next jazz\",\"canonical\":\"Next.js\"}]\n```";
        let got = parse_extracted(raw).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].heard, "next jazz");
        assert_eq!(got[0].canonical, "Next.js");
    }

    #[test]
    fn parse_extracted_empty_array_is_ok() {
        assert!(parse_extracted("[]").unwrap().is_empty());
    }

    #[test]
    fn build_prompt_includes_role_tech_and_transcripts() {
        let batch = vec![LearnTask {
            raw_transcript: "deploy to coober net ease".into(),
            role: Some("backend dev".into()),
            technologies: vec!["rust".into(), "kubernetes".into()],
        }];
        let p = build_extraction_prompt(&batch);
        assert!(p.contains("backend dev"));
        assert!(p.contains("kubernetes"));
        assert!(p.contains("coober net ease"));
    }
}
