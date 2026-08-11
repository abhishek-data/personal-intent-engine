use async_trait::async_trait;

use super::router::LlmRouter;

/// Trait for LLM clients — allows testing with mocks.
///
/// This is a SEAM: behavior can be altered without editing callers.
/// Implementations: [`RouterLlmClient`] (production), mock clients in tests.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Complete a prompt and return the response.
    ///
    /// # Errors
    /// Returns error if the LLM call fails (network, auth, rate limit).
    async fn complete(&self, prompt: &str) -> anyhow::Result<String>;
}

/// Production adapter: satisfies the [`LlmClient`] seam by delegating to an
/// [`LlmRouter`] with a fixed provider/model.
pub struct RouterLlmClient<'a> {
    router: &'a LlmRouter,
    provider: &'a str,
    model: Option<&'a str>,
}

impl<'a> RouterLlmClient<'a> {
    #[must_use]
    pub fn new(router: &'a LlmRouter, provider: &'a str, model: Option<&'a str>) -> Self {
        Self {
            router,
            provider,
            model,
        }
    }
}

#[async_trait]
impl LlmClient for RouterLlmClient<'_> {
    async fn complete(&self, prompt: &str) -> anyhow::Result<String> {
        self.router.send(prompt, self.provider, self.model).await
    }
}
