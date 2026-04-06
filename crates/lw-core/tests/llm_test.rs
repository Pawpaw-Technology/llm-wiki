use lw_core::llm::{CompletionRequest, LlmBackend, NoopLlm};

#[tokio::test]
async fn noop_llm_is_unavailable() {
    let llm = NoopLlm;
    assert!(!llm.available());
}

#[tokio::test]
async fn noop_llm_returns_error() {
    let llm = NoopLlm;
    let req = CompletionRequest {
        system: None,
        prompt: "Summarize this paper.".to_string(),
        max_tokens: None,
    };
    let result = llm.complete(&req).await;
    assert!(result.is_err());
}
