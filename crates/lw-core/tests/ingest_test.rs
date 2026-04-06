use lw_core::fs::init_wiki;
use lw_core::ingest::ingest_source;
use lw_core::llm::{CompletionRequest, CompletionResponse, LlmBackend, NoopLlm};
use lw_core::schema::WikiSchema;
use tempfile::TempDir;

#[tokio::test]
async fn ingest_copies_to_raw() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    let source = tmp.path().join("external/paper.md");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "# My Paper\n\nContent here.").unwrap();

    let llm = NoopLlm;
    let result = ingest_source(root, &source, "papers", &llm).await.unwrap();

    assert!(result.raw_path.exists());
    assert!(result.raw_path.starts_with(root.join("raw/papers")));
    assert!(result.draft.is_none()); // NoopLlm -> no draft
}

#[tokio::test]
async fn ingest_with_mock_llm_generates_draft() {
    struct MockLlm;
    impl LlmBackend for MockLlm {
        async fn complete(&self, _req: &CompletionRequest) -> lw_core::Result<CompletionResponse> {
            Ok(CompletionResponse {
                text: "---\ntitle: My Paper\ntags: [architecture, attention]\ndecay: normal\n---\n\nSummary of the paper content.".to_string(),
            })
        }
        fn available(&self) -> bool {
            true
        }
    }

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    let source = tmp.path().join("external/paper.md");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "# My Paper\n\nSome content.").unwrap();

    let llm = MockLlm;
    let result = ingest_source(root, &source, "papers", &llm).await.unwrap();

    assert!(result.draft.is_some());
    let draft = result.draft.unwrap();
    assert_eq!(draft.title, "My Paper");
    assert_eq!(draft.tags, vec!["architecture", "attention"]);
}
