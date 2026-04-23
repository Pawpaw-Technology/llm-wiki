use lw_core::fs::init_wiki;
use lw_core::ingest::{ingest_content, ingest_source};
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

    let result = ingest_source(root, &source, "papers").await.unwrap();

    assert!(result.raw_path.exists());
    assert!(result.raw_path.starts_with(root.join("raw/papers")));
}

#[tokio::test]
async fn ingest_binary_file_copies_to_raw() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    // Create a binary file (PDF-like)
    let source = tmp.path().join("external/test.pdf");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"\x25\x50\x44\x46binary content here").unwrap();

    let result = ingest_source(root, &source, "papers").await.unwrap();

    // Binary file should still be copied to raw/
    assert!(result.raw_path.exists());
}

#[tokio::test]
async fn ingest_content_writes_named_file_and_returns_path() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    let body = "# Attention\n\nSelf-attention replaced recurrence.";
    let result = ingest_content(root, "articles", "attention.md", body)
        .await
        .unwrap();

    assert_eq!(result.raw_path, root.join("raw/articles/attention.md"));
    assert!(result.raw_path.exists());
    assert_eq!(std::fs::read_to_string(&result.raw_path).unwrap(), body);
}

#[tokio::test]
async fn ingest_content_creates_missing_subdir() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    // `papers` subdir does not exist yet under raw/.
    let result = ingest_content(root, "papers", "note.md", "body")
        .await
        .unwrap();

    assert!(result.raw_path.starts_with(root.join("raw/papers")));
    assert!(result.raw_path.exists());
}

#[tokio::test]
async fn ingest_content_rejects_path_traversal_in_filename() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    // A filename that tries to escape raw/articles/ must be rejected so
    // callers can't smuggle content into arbitrary paths.
    let err = ingest_content(root, "articles", "../evil.md", "body")
        .await
        .expect_err("path traversal should be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("filename") || msg.contains("path"),
        "expected filename/path in error, got: {msg}"
    );
    assert!(!root.join("evil.md").exists());
}

#[tokio::test]
async fn ingest_content_rejects_raw_subdir_with_separator() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    // Same goes for the raw_subdir: it's a category name, not a path.
    let err = ingest_content(root, "../escape", "note.md", "body")
        .await
        .expect_err("path traversal in raw_subdir should be rejected");
    let msg = err.to_string();
    assert!(msg.contains("raw_subdir") || msg.contains("path"));
}

#[tokio::test]
async fn ingest_does_not_create_wiki_page() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();

    let source = tmp.path().join("external/paper.md");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "# My Paper\n\nContent here.").unwrap();

    let result = ingest_source(root, &source, "papers").await.unwrap();

    assert!(result.raw_path.exists());
    // No wiki page should exist — ingest is pure raw filing now
    let wiki_dir = root.join("wiki");
    let mut has_pages = false;
    for cat_entry in std::fs::read_dir(&wiki_dir).unwrap() {
        let cat_path = cat_entry.unwrap().path();
        if cat_path.is_dir() {
            for file_entry in std::fs::read_dir(&cat_path).unwrap() {
                let file_path = file_entry.unwrap().path();
                if file_path.extension().is_some_and(|ext| ext == "md") {
                    has_pages = true;
                }
            }
        }
    }
    assert!(!has_pages, "ingest should not create wiki pages");
}
