pub mod adaptive;
pub mod balanced;
pub mod compact;
pub mod enhanced;

use serde::{Deserialize, Serialize};

/// Optimization mode for prompt construction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OptimizationMode {
    /// Minimize tokens — remove filler, compress
    Compact,
    /// Balance tokens and quality (default)
    Balanced,
    /// Maximize reasoning quality — enrich prompt
    Enhanced,
    /// Auto-select based on context
    Adaptive,
}

/// Result of prompt optimization
#[derive(Debug, Clone)]
pub struct OptimizedPrompt {
    /// The optimized prompt text
    pub text: String,

    /// Mode used
    pub mode: OptimizationMode,

    /// Estimated token count
    pub estimated_tokens: usize,

    /// Sections included in the prompt
    pub sections: Vec<PromptSection>,
}

/// A section of the optimized prompt
#[derive(Debug, Clone)]
pub struct PromptSection {
    pub label: String,
    pub content: String,
}
