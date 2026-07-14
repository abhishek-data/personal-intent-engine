use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Desktop app settings, persisted as JSON at ~/.config/pie/settings.json
/// (next to the engine's memory.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Path to the whisper GGML/GGUF model
    pub whisper_model: String,
    /// Path to the Silero VAD ONNX model (empty = record without VAD)
    pub silero_model: String,
    /// Spoken language code or "auto"
    pub language: String,
    /// Prompt optimization mode
    pub mode: String,
    /// LLM provider ("echo", "openai", "openrouter")
    pub provider: String,
    /// LLM model name (empty = provider default)
    pub llm_model: String,
    /// Global shortcut that toggles recording from any app
    /// (tauri-plugin-global-shortcut syntax, e.g. "CmdOrCtrl+Shift+Space")
    pub hotkey: String,
    /// What the hotkey flow pastes into the active app:
    /// "transcript" (raw speech-to-text) or "prompt" (PIE-optimized prompt)
    pub paste_output: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            whisper_model: existing_cache_model("ggml-tiny.en.bin"),
            silero_model: existing_cache_model("silero_vad_v4.onnx"),
            language: "auto".to_string(),
            mode: "balanced".to_string(),
            provider: "echo".to_string(),
            llm_model: String::new(),
            hotkey: "CmdOrCtrl+Shift+Space".to_string(),
            paste_output: "transcript".to_string(),
        }
    }
}

/// Default to a model already present in ~/.cache/pie/models, else empty.
fn existing_cache_model(filename: &str) -> String {
    let Some(home) = dirs::home_dir() else {
        return String::new();
    };
    let path = home.join(".cache/pie/models").join(filename);
    if path.exists() {
        path.to_string_lossy().into_owned()
    } else {
        String::new()
    }
}

fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pie")
        .join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                log::warn!("Failed to parse settings ({e}); using defaults");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Expand a leading `~/` so paths pasted from the shell work.
    pub fn expand(path: &str) -> PathBuf {
        if let Some(rest) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest);
            }
        }
        PathBuf::from(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_json() {
        let settings = Settings {
            whisper_model: "/tmp/model.bin".into(),
            mode: "enhanced".into(),
            ..Settings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let loaded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.whisper_model, "/tmp/model.bin");
        assert_eq!(loaded.mode, "enhanced");
    }

    #[test]
    fn partial_settings_fill_defaults() {
        let loaded: Settings = serde_json::from_str(r#"{"mode":"compact"}"#).unwrap();
        assert_eq!(loaded.mode, "compact");
        assert_eq!(loaded.language, "auto");
    }

    #[test]
    fn expand_tilde() {
        let expanded = Settings::expand("~/models/x.bin");
        assert!(!expanded.to_string_lossy().starts_with('~'));
        assert!(expanded.to_string_lossy().ends_with("models/x.bin"));
    }
}
