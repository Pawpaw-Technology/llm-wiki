use lw_core::fs::{
    category_from_path, discover_wiki_root, init_wiki, list_pages, read_page, write_page,
};
use lw_core::page::Page;
use lw_core::schema::WikiSchema;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn init_creates_structure() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();
    assert!(root.join(".lw/schema.toml").exists());
    assert!(root.join("wiki/architecture").is_dir());
    assert!(root.join("wiki/training").is_dir());
    assert!(root.join("wiki/_uncategorized").is_dir());
    assert!(root.join("raw/papers").is_dir());
    assert!(root.join("raw/articles").is_dir());
    assert!(root.join("raw/assets").is_dir());
}

#[test]
fn write_and_read_page() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();
    let page = Page {
        title: "Test Page".to_string(),
        tags: vec!["architecture".to_string()],
        decay: None,
        sources: vec![],
        author: Some("alice".to_string()),
        generator: None,
        related: None,
        body: "Hello world.\n".to_string(),
    };
    let path = root.join("wiki/architecture/test-page.md");
    write_page(&path, &page).unwrap();
    assert!(path.exists());
    let loaded = read_page(&path).unwrap();
    assert_eq!(loaded.title, "Test Page");
    assert_eq!(loaded.body.trim(), "Hello world.");
}

#[test]
fn list_pages_finds_markdown() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();
    let p1 = Page {
        title: "A".into(),
        tags: vec![],
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        related: None,
        body: "A.\n".into(),
    };
    let p2 = Page {
        title: "B".into(),
        tags: vec![],
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        related: None,
        body: "B.\n".into(),
    };
    write_page(&root.join("wiki/architecture/a.md"), &p1).unwrap();
    write_page(&root.join("wiki/training/b.md"), &p2).unwrap();
    let pages = list_pages(&root.join("wiki")).unwrap();
    assert_eq!(pages.len(), 2);
}

#[test]
fn read_nonexistent_page_errors() {
    let result = read_page(Path::new("/nonexistent/page.md"));
    assert!(result.is_err());
}

#[test]
fn category_from_path_works() {
    let p = std::path::PathBuf::from("architecture/transformer.md");
    assert_eq!(category_from_path(&p), Some("architecture".to_string()));
    let p2 = std::path::PathBuf::from("test.md");
    assert_eq!(category_from_path(&p2), None);
}

#[test]
fn discover_wiki_root_from_subdir() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &WikiSchema::default()).unwrap();
    // Should find wiki root from a subdirectory
    let subdir = root.join("wiki/architecture");
    let discovered = discover_wiki_root(&subdir).unwrap();
    assert_eq!(discovered, root.to_path_buf());
}

#[test]
fn discover_wiki_root_not_found() {
    let tmp = TempDir::new().unwrap();
    // No wiki initialized here
    assert!(discover_wiki_root(tmp.path()).is_none());
}
