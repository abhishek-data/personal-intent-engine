use super::classifier;
use super::schema::*;

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
            // Get the first sentence as the core objective
            if let Some(first_sentence) = cleaned.split(&['.', ';', '\n'][..]).next() {
                return first_sentence.trim().to_string();
            }
        }

        cleaned
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
}
