//! # PIE - Personal Intent Engine
//!
//! Intelligent middleware between humans and AI models.
//! Extracts intent, maintains memory, optimizes prompts, routes to any LLM.

pub mod audio;
pub mod intent;
pub mod llm;
pub mod memory;
pub mod optimizer;
pub mod pipeline;
pub mod stt;

// Re-export main pipeline
pub use pipeline::engine::PieEngine;
pub use intent::schema::{Intent, IntentConfidence, ConversationType};
pub use optimizer::OptimizationMode;
pub use memory::store::MemoryStore;
