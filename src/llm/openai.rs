use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// Shared HTTP client. `reqwest::Client` is internally reference-counted and
/// holds a connection pool, so cloning this shares one pool (and TLS session
/// reuse / HTTP/2 multiplexing) across every `OpenAiClient` instance.
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

/// OpenAI-compatible API client.
/// Works with OpenAI, Anthropic (via proxy), local models, etc.
pub struct OpenAiClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

impl OpenAiClient {
    /// Create a client for `base_url` (a trailing slash is trimmed) using
    /// `api_key` as the bearer token.
    #[must_use]
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            client: HTTP_CLIENT.clone(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        }
    }

    /// Create client from environment variables
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").ok()?;
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        Some(Self::new(&base_url, &api_key))
    }

    /// Send a prompt and get a response
    pub async fn chat(&self, prompt: &str, model: &str) -> anyhow::Result<String> {
        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: 0.7,
            max_tokens: 2048,
        };

        let url = format!("{}/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {}: {}", status, body);
        }

        let chat_response: ChatResponse = response.json().await?;
        chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| anyhow::anyhow!("No response from LLM"))
    }
}
