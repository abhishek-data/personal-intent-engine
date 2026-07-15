use super::openai::OpenAiClient;

/// Routes prompts to an LLM provider (OpenAI-compatible or the local `echo`
/// debug provider) and reports which providers are available.
pub struct LlmRouter {
    client: Option<OpenAiClient>,
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
        }
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

                let model_name = model.unwrap_or("gpt-4o-mini");
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
