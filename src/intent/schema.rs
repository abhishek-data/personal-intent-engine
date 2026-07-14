use serde::{Deserialize, Serialize};

/// The core intent extracted from user input.
/// This is PIE's primary output — structured understanding of what the user wants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    /// What the user ultimately wants to achieve
    pub objective: String,

    /// Supporting context extracted from the input
    pub context: Vec<String>,

    /// Constraints or requirements mentioned
    pub constraints: Vec<String>,

    /// Questions embedded in the input
    pub questions: Vec<String>,

    /// Assumptions the user is making
    pub assumptions: Vec<String>,

    /// Missing information that could help
    pub missing_info: Vec<String>,

    /// Confidence level
    pub confidence: IntentConfidence,

    /// Type of conversation
    pub conversation_type: ConversationType,

    /// Raw input text (preserved for reference)
    pub raw_input: String,

    /// Detected language
    pub language: Option<String>,

    /// Key entities/topics mentioned
    pub topics: Vec<String>,
}

/// Confidence level for the extracted intent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IntentConfidence {
    High,
    Medium,
    Low,
}

/// Type of conversation detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConversationType {
    /// Direct question expecting an answer
    Question,
    /// Task to be completed
    Task,
    /// Exploring ideas / thinking aloud
    Brainstorm,
    /// Complaint or problem report
    Problem,
    /// Request for explanation
    Explanation,
    /// Code-related request
    Code,
    /// General conversation
    Other,
}

impl Default for Intent {
    fn default() -> Self {
        Self {
            objective: String::new(),
            context: Vec::new(),
            constraints: Vec::new(),
            questions: Vec::new(),
            assumptions: Vec::new(),
            missing_info: Vec::new(),
            confidence: IntentConfidence::Medium,
            conversation_type: ConversationType::Other,
            raw_input: String::new(),
            language: None,
            topics: Vec::new(),
        }
    }
}
