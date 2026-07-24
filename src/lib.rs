//! # PIE - Personal Intent Engine
//!
//! Intelligent middleware between humans and AI models.
//! Extracts intent, maintains memory, optimizes prompts, routes to any LLM.

pub mod audio;
pub mod corrector;
pub mod history;
pub mod intent;
pub mod llm;
pub mod memory;
pub mod optimizer;
pub mod pipeline;
pub mod stt;

// Re-export main pipeline
pub use intent::schema::{ConversationType, Intent, IntentConfidence};
pub use memory::store::MemoryStore;
pub use optimizer::OptimizationMode;
pub use pipeline::engine::PieEngine;
