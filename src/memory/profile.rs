use serde::{Deserialize, Serialize};

/// User profile for personalization.
/// PIE uses this to understand the user's context and preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// User's role/profession
    pub role: Option<String>,

    /// Technologies the user works with
    pub technologies: Vec<String>,

    /// Preferred response style
    pub preferred_style: ResponseStyle,

    /// Preferred detail level
    pub detail_level: DetailLevel,

    /// Custom vocabulary / domain terms
    pub custom_terms: std::collections::HashMap<String, String>,
}

/// The shape of answer the user prefers, fed into prompt optimization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResponseStyle {
    /// Step-by-step instructions
    StepByStep,
    /// Concise, direct answers
    Concise,
    /// Detailed explanations with examples
    Detailed,
    /// Code-heavy responses
    CodeFirst,
    /// Mixed explanation + code
    Balanced,
}

/// How much depth the user wants in a response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DetailLevel {
    /// Minimal — just the answer
    Minimal,
    /// Standard — answer with brief explanation
    Standard,
    /// Comprehensive — full context and alternatives
    Comprehensive,
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            role: None,
            technologies: Vec::new(),
            preferred_style: ResponseStyle::Balanced,
            detail_level: DetailLevel::Standard,
            custom_terms: std::collections::HashMap::new(),
        }
    }
}
