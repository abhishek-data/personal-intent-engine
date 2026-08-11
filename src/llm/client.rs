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

/// Default bound on an LLM call made from the pipeline. Intent extraction sits
/// on the interactive hotkey path, so a hung or unreachable provider must fail
/// fast and fall back to rule-based extraction rather than freeze the app.
pub const DEFAULT_LLM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Production adapter: satisfies the [`LlmClient`] seam by delegating to an
/// [`LlmRouter`] with a fixed provider/model.
///
/// Calls are bounded by [`DEFAULT_LLM_TIMEOUT`] unless overridden with
/// [`RouterLlmClient::with_timeout`].
pub struct RouterLlmClient<'a> {
    router: &'a LlmRouter,
    provider: &'a str,
    model: Option<&'a str>,
    timeout: std::time::Duration,
}

impl<'a> RouterLlmClient<'a> {
    #[must_use]
    pub fn new(router: &'a LlmRouter, provider: &'a str, model: Option<&'a str>) -> Self {
        Self {
            router,
            provider,
            model,
            timeout: DEFAULT_LLM_TIMEOUT,
        }
    }

    /// Override the per-call timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait]
impl LlmClient for RouterLlmClient<'_> {
    async fn complete(&self, prompt: &str) -> anyhow::Result<String> {
        let call = self.router.send(prompt, self.provider, self.model);
        match tokio::time::timeout(self.timeout, call).await {
            Ok(result) => result,
            Err(_) => anyhow::bail!("LLM call timed out after {}s", self.timeout.as_secs()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: an unresponsive provider must not hang the pipeline. The
    /// hotkey path calls this synchronously, so an unbounded wait froze the
    /// app until the OS gave up.
    #[tokio::test]
    async fn complete_times_out_instead_of_hanging() {
        // 127.0.0.1:1 refuses fast on most systems; use a non-routable address
        // so the connect attempt stalls rather than erroring immediately.
        let router = LlmRouter::from_config(&super::super::LlmConfig {
            api_url: "http://10.255.255.1:8080/v1".into(),
            api_key: "sk-x".into(),
            model: "m".into(),
        });
        let client = RouterLlmClient::new(&router, "openai", None)
            .with_timeout(std::time::Duration::from_millis(300));
        let started = std::time::Instant::now();
        let result = client.complete("hello").await;
        assert!(result.is_err(), "unreachable provider must error");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "must fail fast, took {:?}",
            started.elapsed()
        );
    }
}
