use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::profile::UserProfile;

/// JSON-based personal memory store.
/// Stores user profile, preferences, and learned patterns.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryStore {
    /// User profile (role, tech stack, preferences)
    pub profile: UserProfile,

    /// Learned communication patterns
    pub patterns: CommunicationPatterns,

    /// Frequently referenced terms/concepts
    pub vocabulary: std::collections::HashMap<String, String>,

    /// Storage path
    #[serde(skip)]
    path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommunicationPatterns {
    /// How the user typically structures requests
    pub request_style: Option<String>,
    /// Preferred response format
    pub response_format: Option<String>,
    /// Average input length (words)
    pub avg_input_length: f32,
    /// Number of interactions processed
    pub interaction_count: u64,
    /// Most common conversation types
    pub common_types: Vec<String>,
}

impl MemoryStore {
    /// Load memory from file, or create default
    pub fn load() -> Self {
        let path = Self::default_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(json) => match serde_json::from_str::<MemoryStore>(&json) {
                    Ok(mut store) => {
                        store.path = Some(path);
                        return store;
                    }
                    Err(e) => {
                        log::warn!("Failed to parse memory store: {e}, creating new");
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read memory store: {e}, creating new");
                }
            }
        }

        Self {
            path: Some(path),
            ..Self::default()
        }
    }

    /// Save memory to file
    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let json = serde_json::to_string_pretty(self)?;
            std::fs::write(path, json)?;
        }
        Ok(())
    }

    /// Update interaction patterns based on new input
    pub fn record_interaction(&mut self, input: &str, conv_type: &str) {
        self.patterns.interaction_count += 1;

        let word_count = input.split_whitespace().count() as f32;
        let count = self.patterns.interaction_count as f32;
        self.patterns.avg_input_length =
            (self.patterns.avg_input_length * (count - 1.0) + word_count) / count;

        // Track common conversation types
        if !self.patterns.common_types.contains(&conv_type.to_string()) {
            self.patterns.common_types.push(conv_type.to_string());
            if self.patterns.common_types.len() > 5 {
                self.patterns.common_types.remove(0);
            }
        }
    }

    fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pie")
            .join("memory.json")
    }
}
