use lw_core::link::{extract_wiki_links, find_broken_links, resolve_link};
use std::path::PathBuf;

#[test]
fn extract_links_from_body() {
    let body = "See [[transformer]] for details. Also related: [[scaling-laws]] and [[attention-mechanism]].";
    let links = extract_wiki_links(body);
    assert_eq!(
        links,
        vec!["transformer", "scaling-laws", "attention-mechanism"]
    );
}

#[test]
fn extract_no_links() {
    assert!(extract_wiki_links("No links here.").is_empty());
}

#[test]
fn extract_deduplicates() {
    let body = "See [[foo]] and then [[foo]] again.";
    assert_eq!(extract_wiki_links(body), vec!["foo"]);
}

#[test]
fn resolve_link_finds_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let wiki_dir = tmp.path().join("wiki");
    std::fs::create_dir_all(wiki_dir.join("architecture")).unwrap();
    std::fs::write(
        wiki_dir.join("architecture/transformer.md"),
        "---\ntitle: T\n---\n",
    )
    .unwrap();
    let result = resolve_link("transformer", &wiki_dir).unwrap();
    assert_eq!(result, PathBuf::from("architecture/transformer.md"));
}

#[test]
fn resolve_link_returns_none_for_missing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let wiki_dir = tmp.path().join("wiki");
    std::fs::create_dir_all(&wiki_dir).unwrap();
    assert!(resolve_link("nonexistent", &wiki_dir).is_none());
}

#[test]
fn extract_pipe_syntax_takes_slug_only() {
    let body = "See [[slug|display text]] for details.";
    let links = extract_wiki_links(body);
    assert_eq!(links, vec!["slug"]);
}

#[test]
fn extract_plain_and_pipe_mixed() {
    let body = "Compare [[transformer]] with [[attention|Attention Mechanism]] and [[scaling-laws|Scaling Laws]].";
    let links = extract_wiki_links(body);
    assert_eq!(links, vec!["transformer", "attention", "scaling-laws"]);
}

#[test]
fn extract_pipe_syntax_regression_plain_still_works() {
    let body = "See [[simple-slug]] and [[another-one]].";
    let links = extract_wiki_links(body);
    assert_eq!(links, vec!["simple-slug", "another-one"]);
}

#[test]
fn find_broken_links_detects_missing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let wiki_dir = tmp.path().join("wiki");
    std::fs::create_dir_all(wiki_dir.join("architecture")).unwrap();
    std::fs::write(wiki_dir.join("architecture/transformer.md"), "x").unwrap();
    let body = "See [[transformer]] and [[nonexistent]].";
    let broken = find_broken_links(body, &wiki_dir);
    assert_eq!(broken, vec!["nonexistent"]);
}
