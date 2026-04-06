use std::future::Future;

use crate::{Result, WikiError};

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub system: Option<String>,
    pub prompt: String,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
}

/// LLM abstraction — the core decoupling point between tool layer and intelligence.
/// Implementations: Claude API, OpenAI, Kimi, local ollama, subprocess.
pub trait LlmBackend: Send + Sync {
    /// Generate a completion. Returns Err if the backend is unavailable or fails.
    fn complete(
        &self,
        req: &CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse>> + Send;

    /// Health check. Returns false if the LLM is not configured or unreachable.
    fn available(&self) -> bool;
}

/// Fallback when no LLM is configured. Always returns unavailable.
pub struct NoopLlm;

impl LlmBackend for NoopLlm {
    async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse> {
        Err(WikiError::LlmUnavailable)
    }

    fn available(&self) -> bool {
        false
    }
}
