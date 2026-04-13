use lw_core::fs::{init_wiki, read_page, validate_wiki_path, write_page};
use lw_core::page::Page;
use lw_core::schema::WikiSchema;
use lw_core::section;
use tempfile::TempDir;

fn setup_wiki() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    let schema = WikiSchema::default();
    init_wiki(&root, &schema).unwrap();
    (tmp, root)
}

fn write_test_page(root: &std::path::Path, rel_path: &str, content: &str) {
    let abs = validate_wiki_path(root, rel_path).unwrap();
    let page = Page::parse(content).unwrap();
    write_page(&abs, &page).unwrap();
}

const TEST_PAGE: &str = "\
---
title: Test Page
tags: [test]
---

## Overview
This is the overview.

## References
- existing ref

## See Also
- [[other]]
";

#[test]
fn integration_append_section() {
    let (_tmp, root) = setup_wiki();
    write_test_page(&root, "tools/test.md", TEST_PAGE);

    let abs = validate_wiki_path(&root, "tools/test.md").unwrap();
    let raw = std::fs::read_to_string(&abs).unwrap();
    let (fm, body) = section::split_frontmatter(&raw);
    let (new_body, found) = section::apply_append(body, "References", "- new ref").unwrap();
    assert!(found);
    let output = format!("{}{}", fm, new_body);
    std::fs::write(&abs, &output).unwrap();

    let result = std::fs::read_to_string(&abs).unwrap();
    assert!(result.contains("- existing ref"));
    assert!(result.contains("- new ref"));
    assert!(result.contains("## See Also"));

    // Verify page is still parseable
    let page = Page::parse(&result).unwrap();
    assert_eq!(page.title, "Test Page");
}

#[test]
fn integration_upsert_section() {
    let (_tmp, root) = setup_wiki();
    write_test_page(&root, "tools/test.md", TEST_PAGE);

    let abs = validate_wiki_path(&root, "tools/test.md").unwrap();
    let raw = std::fs::read_to_string(&abs).unwrap();
    let (fm, body) = section::split_frontmatter(&raw);
    let (new_body, found) = section::apply_upsert(body, "References", "- replaced");
    assert!(found);
    let output = format!("{}{}", fm, new_body);
    std::fs::write(&abs, &output).unwrap();

    let result = std::fs::read_to_string(&abs).unwrap();
    assert!(result.contains("## References"));
    assert!(result.contains("- replaced"));
    assert!(!result.contains("- existing ref"));
}

#[test]
fn integration_overwrite_unchanged() {
    let (_tmp, root) = setup_wiki();
    write_test_page(&root, "tools/test.md", TEST_PAGE);

    let abs = validate_wiki_path(&root, "tools/test.md").unwrap();
    let new_content = "---\ntitle: Replaced\ntags: [new]\n---\n\n## New Content\nHello\n";
    let page = Page::parse(new_content).unwrap();
    write_page(&abs, &page).unwrap();

    let result = read_page(&abs).unwrap();
    assert_eq!(result.title, "Replaced");
}

#[test]
fn integration_page_not_found() {
    let (_tmp, root) = setup_wiki();
    let abs = validate_wiki_path(&root, "tools/nonexistent.md").unwrap();
    let result = std::fs::read_to_string(&abs);
    assert!(result.is_err());
}

#[test]
fn integration_append_empty_noop() {
    let (_tmp, root) = setup_wiki();
    write_test_page(&root, "tools/test.md", TEST_PAGE);

    let abs = validate_wiki_path(&root, "tools/test.md").unwrap();
    let raw = std::fs::read_to_string(&abs).unwrap();
    let (_, body) = section::split_frontmatter(&raw);
    let result = section::apply_append(body, "References", "");
    assert!(result.is_none()); // No-op, don't write
}

#[test]
fn integration_frontmatter_preserved_after_section_op() {
    let (_tmp, root) = setup_wiki();
    // Use frontmatter with specific quoting that serde would change
    let content = "---\ntitle: \"Quoted Title\"\ntags: [a, b]\n---\n\n## Section\ncontent\n";
    write_test_page(&root, "tools/test.md", content);

    let abs = validate_wiki_path(&root, "tools/test.md").unwrap();
    let raw = std::fs::read_to_string(&abs).unwrap();
    let (fm, body) = section::split_frontmatter(&raw);
    let (new_body, _) = section::apply_append(body, "Section", "- appended").unwrap();
    let output = format!("{}{}", fm, new_body);
    std::fs::write(&abs, &output).unwrap();

    let result = std::fs::read_to_string(&abs).unwrap();
    // Check the frontmatter portion is unchanged
    // Note: write_page uses Page::to_markdown which round-trips through serde
    // But our section op path preserves raw frontmatter bytes
    let (result_fm, _) = section::split_frontmatter(&result);
    assert_eq!(result_fm, fm);
}
