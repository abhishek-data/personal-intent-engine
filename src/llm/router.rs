use super::openai::OpenAiClient;

/// User-provided LLM connection settings (Bring Your Own Key).
///
/// When `api_url` is empty the router falls back to environment variables
/// (`OPENAI_API_KEY` / `OPENAI_BASE_URL`), preserving the CLI/env path.
pub struct LlmConfig {
    /// OpenAI-compatible base URL, e.g. `https://api.openai.com/v1`.
    pub api_url: String,
    /// Bearer token; may be empty for local servers that need no key.
    pub api_key: String,
    /// Default model name; empty means "use the provider default".
    pub model: String,
}

/// Routes prompts to an LLM provider (OpenAI-compatible or the local `echo`
/// debug provider) and reports which providers are available.
pub struct LlmRouter {
    client: Option<OpenAiClient>,
    /// The user's configured default model. Used when a caller passes no
    /// explicit model — notably LLM-backed intent extraction inside the
    /// pipeline, which has no way to know the user's provider/model.
    default_model: Option<String>,
}

impl Default for LlmRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmRouter {
    /// Build a router, picking up an OpenAI-compatible client from the
    /// environment (`OPENAI_API_KEY`, optional `OPENAI_BASE_URL`) if present.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: OpenAiClient::from_env(),
            default_model: std::env::var("OPENAI_MODEL").ok().filter(|m| !m.is_empty()),
        }
    }

    /// Build a router from user settings (BYOK). When `config.api_url` is
    /// empty, falls back to environment variables so the CLI/env path keeps
    /// working.
    #[must_use]
    pub fn from_config(config: &LlmConfig) -> Self {
        let default_model = Some(config.model.trim())
            .filter(|m| !m.is_empty())
            .map(String::from);
        if config.api_url.trim().is_empty() {
            let mut router = Self::new();
            // An explicitly configured model still applies over the env path.
            if default_model.is_some() {
                router.default_model = default_model;
            }
            router
        } else {
            Self {
                client: Some(OpenAiClient::new(&config.api_url, &config.api_key)),
                default_model,
            }
        }
    }

    /// The user's configured default model, if any.
    #[must_use]
    pub fn default_model(&self) -> Option<&str> {
        self.default_model.as_deref()
    }

    /// Send a prompt to the specified provider
    pub async fn send(
        &self,
        prompt: &str,
        provider: &str,
        model: Option<&str>,
    ) -> anyhow::Result<String> {
        match provider {
            "openai" | "openrouter" => {
                let client = self.client.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("No LLM client configured. Set OPENAI_API_KEY.")
                })?;

                // Explicit caller model wins; otherwise the user's configured
                // default; only then the built-in fallback.
                let model_name = model
                    .or(self.default_model.as_deref())
                    .unwrap_or("gpt-4o-mini");
                client.chat(prompt, model_name).await
            }
            "echo" => {
                // Debug mode: echo back the prompt
                Ok(format!("[PIE Echo]\n{}", prompt))
            }
            _ => anyhow::bail!("Unknown provider: {}", provider),
        }
    }

    /// Check if a provider is available
    pub fn is_available(&self, provider: &str) -> bool {
        match provider {
            "openai" | "openrouter" => self.client.is_some(),
            "echo" => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_config_with_url_is_available() {
        let cfg = LlmConfig {
            api_url: "https://api.example.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "gpt-4o-mini".to_string(),
        };
        let router = LlmRouter::from_config(&cfg);
        assert!(router.is_available("openai"));
    }

    #[test]
    fn from_config_retains_the_configured_model() {
        // Regression: the configured model was dropped, so pipeline intent
        // extraction (which passes no explicit model) silently requested
        // "gpt-4o-mini" from every BYOK provider and failed.
        let cfg = LlmConfig {
            api_url: "https://api.example.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "mimo-v2.5-pro".to_string(),
        };
        let router = LlmRouter::from_config(&cfg);
        assert_eq!(router.default_model(), Some("mimo-v2.5-pro"));
    }

    #[test]
    fn from_config_empty_model_leaves_no_default() {
        let cfg = LlmConfig {
            api_url: "https://api.example.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: String::new(),
        };
        assert_eq!(LlmRouter::from_config(&cfg).default_model(), None);
    }

    #[test]
    fn from_config_empty_url_falls_back_to_env() {
        // No OPENAI_API_KEY in the test env => env fallback yields no client.
        std::env::remove_var("OPENAI_API_KEY");
        let cfg = LlmConfig {
            api_url: String::new(),
            api_key: String::new(),
            model: String::new(),
        };
        let router = LlmRouter::from_config(&cfg);
        assert!(!router.is_available("openai"));
    }
}
