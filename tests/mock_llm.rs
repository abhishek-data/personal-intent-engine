//! Mock LLM client for integration tests. Included via `mod mock_llm;` from
//! test crates; when compiled as its own test crate it exports no tests.
#![allow(dead_code)]

use async_trait::async_trait;
use pie_engine::llm::LlmClient;
use std::sync::Mutex;

/// Mock LLM client for testing.
/// Returns pre-configured responses in order; errors once exhausted.
pub struct MockLlmClient {
    responses: Mutex<Vec<String>>,
}

impl MockLlmClient {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, _prompt: &str) -> anyhow::Result<String> {
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            return Err(anyhow::anyhow!("No more mock responses"));
        }
        Ok(responses.remove(0))
    }
}
