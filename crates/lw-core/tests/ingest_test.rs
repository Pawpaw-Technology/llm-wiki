use lw_core::fs::init_wiki;
use lw_core::ingest::ingest_source;
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
