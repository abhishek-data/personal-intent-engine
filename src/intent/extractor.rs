use super::classifier;
use super::schema::*;
use crate::llm::LlmClient;

/// Inputs at or below this word count (with no question mark) are handled by
/// the rule-based fast path; anything longer goes through the LLM.
pub const LLM_EXTRACTION_WORD_THRESHOLD: usize = 15;

/// Extracts structured intent from user text input.
///
/// Phase 1: Rule-based extraction (no ML model required).
/// Phase 2: Small local model for better extraction (future).
pub struct IntentExtractor;

impl Default for IntentExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl IntentExtractor {
    /// Create a rule-based intent extractor.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Extract structured intent (objective, constraints, questions, topics,
    /// conversation type, confidence) from raw text.
    #[must_use]
    pub fn extract(&self, text: &str) -> Intent {
        let text = text.trim();
        if text.is_empty() {
            return Intent::default();
        }

        let conversation_type = classifier::classify(text);
        let confidence = self.assess_confidence(text);
        let questions = self.extract_questions(text);
        let constraints = self.extract_constraints(text);
        let context = self.extract_context(text);
        let topics = self.extract_topics(text);

        // Clean the text for objective extraction
        let objective = self.extract_objective(text, &conversation_type);

        Intent {
            objective,
            context,
            constraints,
            questions,
            assumptions: Vec::new(),  // Phase 2
            missing_info: Vec::new(), // Phase 2
            confidence,
            conversation_type,
            raw_input: text.to_string(),
            language: None, // Phase 2
            topics,
        }
    }

    /// Extract intent using an LLM for complex/rambling input.
    /// Falls back to rule-based extraction for simple, direct input.
    ///
    /// # Decision Logic
    /// - ≤15 words AND no question mark → use rules (fast, deterministic)
    /// - Otherwise → use LLM (slower, but understands meaning)
    ///
    /// # Errors
    /// Never fails: falls back to rule-based extraction if the LLM call
    /// errors or returns an unparsable response.
    pub async fn extract_with_llm(
        &self,
        text: &str,
        llm_client: &dyn LlmClient,
        user_context: Option<&str>,
    ) -> Intent {
        let text = text.trim();
        if text.is_empty() {
            return Intent::default();
        }

        // Simple threshold: if short and direct, use rules.
        let word_count = text.split_whitespace().count();
        let has_question = text.contains('?');

        if word_count <= LLM_EXTRACTION_WORD_THRESHOLD && !has_question {
            // Simple command — rules are fine.
            return self.extract(text);
        }

        // Complex input — use the LLM.
        let prompt = self.build_extraction_prompt(text, user_context);

        match llm_client.complete(&prompt).await {
            Ok(response) => self.parse_llm_response(&response, text),
            Err(e) => {
                log::warn!("LLM extraction failed: {e}, falling back to rules");
                self.extract(text)
            }
        }
    }

    /// Build the prompt for LLM intent extraction.
    fn build_extraction_prompt(&self, text: &str, user_context: Option<&str>) -> String {
        let mut prompt = String::from(
            r#"Extract the core intent from this speech. Return JSON with:
{
  "objective": "what the user wants to do/achieve",
  "type": "task|question|brainstorm|clarification",
  "topics": ["topic1", "topic2"],
  "constraints": ["constraint1"],
  "questions": ["question1"],
  "confidence": "high|medium|low"
}

Be concise. The user may be rambling or repeating themselves - extract the CORE intent."#,
        );

        if let Some(ctx) = user_context {
            prompt.push_str(&format!("\n\nUser context: {ctx}"));
        }

        prompt.push_str(&format!("\n\nSpeech transcript:\n{text}"));

        prompt
    }

    /// Parse the LLM response into an [`Intent`]. Falls back to rule-based
    /// extraction when no valid JSON object can be found in the response.
    fn parse_llm_response(&self, response: &str, original_text: &str) -> Intent {
        let Some(parsed) = Self::find_json_object(response) else {
            return self.extract(original_text);
        };

        let string_list = |key: &str| -> Vec<String> {
            parsed[key]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };

        Intent {
            objective: parsed["objective"].as_str().unwrap_or("").to_string(),
            context: Vec::new(),
            constraints: string_list("constraints"),
            questions: string_list("questions"),
            assumptions: Vec::new(),
            missing_info: Vec::new(),
            confidence: match parsed["confidence"].as_str() {
                Some("high") => IntentConfidence::High,
                Some("low") => IntentConfidence::Low,
                _ => IntentConfidence::Medium,
            },
            conversation_type: match parsed["type"].as_str() {
                Some("question") => ConversationType::Question,
                Some("brainstorm") => ConversationType::Brainstorm,
                Some("clarification") => ConversationType::Clarification,
                _ => ConversationType::Task,
            },
            raw_input: original_text.to_string(),
            language: None,
            topics: string_list("topics"),
        }
    }

    /// Find and parse the first JSON object in `response`. LLMs often wrap
    /// JSON in prose or ```json fences; parse the outermost `{...}` span.
    fn find_json_object(response: &str) -> Option<serde_json::Value> {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
            if v.is_object() {
                return Some(v);
            }
        }
        let start = response.find('{')?;
        let end = response.rfind('}')?;
        if end <= start {
            return None;
        }
        serde_json::from_str::<serde_json::Value>(&response[start..=end])
            .ok()
            .filter(serde_json::Value::is_object)
    }

    /// Assess confidence based on input characteristics
    fn assess_confidence(&self, text: &str) -> IntentConfidence {
        let words: Vec<&str> = text.split_whitespace().collect();

        // Short, direct inputs are high confidence
        if words.len() <= 10 && !text.contains('?') {
            return IntentConfidence::High;
        }

        // Uncertainty markers
        let uncertainty_words = [
            "maybe", "perhaps", "not sure", "i think", "might", "possibly", "kind of", "sort of",
            "i guess", "probably",
        ];
        let lower = text.to_lowercase();
        let has_uncertainty = uncertainty_words.iter().any(|w| lower.contains(w));

        // Self-corrections
        let has_correction = lower.contains("actually")
            || lower.contains("wait")
            || lower.contains("i mean")
            || lower.contains("sorry");

        if has_correction {
            IntentConfidence::Low
        } else if has_uncertainty || words.len() > 40 {
            IntentConfidence::Medium
        } else {
            IntentConfidence::High
        }
    }

    /// Extract questions from the text
    fn extract_questions(&self, text: &str) -> Vec<String> {
        text.split(&['?', '.'][..])
            .filter(|s| s.trim().ends_with('?') || text.contains(&format!("{}?", s.trim())))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() > 3)
            .collect()
    }

    /// Extract constraints (keywords indicating requirements)
    fn extract_constraints(&self, text: &str) -> Vec<String> {
        let mut constraints = Vec::new();
        let lower = text.to_lowercase();

        let patterns = [
            ("must", "requirement"),
            ("should", "preference"),
            ("don't", "negative constraint"),
            ("do not", "negative constraint"),
            ("only", "limitation"),
            ("without", "exclusion"),
            ("using", "technology preference"),
            ("with", "technology preference"),
            ("prefer", "preference"),
        ];

        for (keyword, _label) in &patterns {
            if lower.contains(keyword) {
                // Extract the clause containing the keyword
                for sentence in text.split(&['.', ',', ';'][..]) {
                    if sentence.to_lowercase().contains(keyword) {
                        constraints.push(sentence.trim().to_string());
                    }
                }
            }
        }

        constraints.dedup();
        constraints
    }

    /// Extract context (background information)
    fn extract_context(&self, text: &str) -> Vec<String> {
        let mut context = Vec::new();

        // Context markers
        let markers = [
            "i'm working on",
            "i am working on",
            "i have",
            "i've been",
            "currently",
            "right now",
            "for example",
            "e.g.",
            "background:",
            "context:",
            "i'm using",
            "i am using",
            "my project",
            "our project",
        ];

        for sentence in text.split(&['.', ';'][..]) {
            let s = sentence.trim();
            if markers.iter().any(|m| s.to_lowercase().contains(m)) {
                context.push(s.to_string());
            }
        }

        context
    }

    /// Extract topics/entities
    fn extract_topics(&self, text: &str) -> Vec<String> {
        let mut topics = Vec::new();

        // Technology keywords
        let tech_words = [
            "react",
            "nextjs",
            "next.js",
            "node",
            "python",
            "rust",
            "typescript",
            "javascript",
            "docker",
            "kubernetes",
            "aws",
            "gcp",
            "azure",
            "postgres",
            "mysql",
            "redis",
            "mongodb",
            "graphql",
            "rest",
            "whisper",
            "llm",
            "gpt",
            "claude",
            "gemini",
            "api",
        ];

        let lower = text.to_lowercase();
        for tech in &tech_words {
            if lower.contains(tech) {
                topics.push(tech.to_string());
            }
        }

        topics
    }

    /// Extract the core objective from the input
    fn extract_objective(&self, text: &str, conv_type: &ConversationType) -> String {
        // Remove filler phrases
        let cleaned = text
            .replace("can you", "")
            .replace("could you", "")
            .replace("please", "")
            .replace("I want to", "")
            .replace("I need to", "")
            .replace("I'd like to", "")
            .replace("help me", "")
            .trim()
            .to_string();

        // For questions, use the full text
        if matches!(conv_type, ConversationType::Question) {
            return cleaned;
        }

        // For tasks, try to extract the action
        if matches!(conv_type, ConversationType::Task) {
            // Get the first sentence as the core objective.
            return match Self::first_sentence_end(&cleaned) {
                Some(end) => cleaned[..end].trim().to_string(),
                None => cleaned.trim().to_string(),
            };
        }

        cleaned
    }

    /// Byte index of the first sentence-ending delimiter: `;`, `\n`, or a `.`
    /// that is NOT immediately followed by a non-whitespace character.
    ///
    /// Plain `.split('.')` would shred mid-word periods in technical terms
    /// like "Next.js" or "Node.js" into two fragments ("Next" + "js app"),
    /// dropping the second half of the term. A `.` only ends a sentence here
    /// when it's followed by whitespace or end-of-string.
    fn first_sentence_end(text: &str) -> Option<usize> {
        let bytes = text.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b';' | b'\n' => return Some(i),
                b'.' => {
                    let is_boundary = bytes.get(i + 1).is_none_or(|c| c.is_ascii_whitespace());
                    if is_boundary {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(text: &str) -> Intent {
        IntentExtractor::new().extract(text)
    }

    #[test]
    fn empty_input_yields_default_intent() {
        let intent = extract("   ");
        assert!(intent.objective.is_empty());
        assert_eq!(intent.conversation_type, ConversationType::Other);
    }

    #[test]
    fn short_direct_input_is_high_confidence() {
        let intent = extract("deploy the app");
        assert_eq!(intent.confidence, IntentConfidence::High);
    }

    #[test]
    fn uncertainty_markers_lower_confidence() {
        let intent =
            extract("maybe we could try adding a cache here, not sure if that helps at all though because the data changes often");
        assert_eq!(intent.confidence, IntentConfidence::Medium);
    }

    #[test]
    fn self_corrections_are_low_confidence() {
        let intent = extract(
            "add a retry to the fetch call, wait actually I mean the upload call, sorry about the confusion there",
        );
        assert_eq!(intent.confidence, IntentConfidence::Low);
    }

    #[test]
    fn extracts_questions() {
        let intent = extract("The build is slow. How can I speed it up? What about caching?");
        assert!(
            !intent.questions.is_empty(),
            "expected questions, got {:?}",
            intent.questions
        );
    }

    #[test]
    fn extracts_constraints() {
        let intent = extract("Build the API. It must use Postgres, without any ORM.");
        assert!(
            !intent.constraints.is_empty(),
            "expected constraints, got {:?}",
            intent.constraints
        );
    }

    #[test]
    fn extracts_tech_topics() {
        let intent = extract("Set up Docker with Postgres for the Rust service");
        assert!(intent.topics.contains(&"docker".to_string()));
        assert!(intent.topics.contains(&"postgres".to_string()));
        assert!(intent.topics.contains(&"rust".to_string()));
    }

    #[test]
    fn objective_strips_filler_phrases() {
        let intent = extract("can you help me deploy the service");
        assert!(
            !intent.objective.contains("can you"),
            "filler not stripped: {:?}",
            intent.objective
        );
        assert!(intent.objective.contains("deploy the service"));
    }

    #[test]
    fn raw_input_is_preserved() {
        let intent = extract("explain lifetimes");
        assert_eq!(intent.raw_input, "explain lifetimes");
    }

    #[test]
    fn objective_keeps_mid_word_periods_in_task_input() {
        // A bare `.split('.')` would shred "Next.js" into "Next" + "js app",
        // dropping the framework name from the objective.
        let intent = extract("build a Next.js app");
        assert_eq!(intent.objective, "build a Next.js app");
    }

    #[test]
    fn objective_still_splits_on_real_sentence_boundaries() {
        let intent = extract("build the API. It must use Postgres.");
        assert_eq!(intent.objective, "build the API");
    }
}
