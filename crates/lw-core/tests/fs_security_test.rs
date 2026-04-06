use lw_core::fs::validate_wiki_path;
use lw_core::WikiError;
use std::path::Path;

#[test]
fn test_path_traversal_dotdot_rejected() {
    let wiki_root = Path::new("/tmp/test-wiki");
    let result = validate_wiki_path(wiki_root, "../etc/passwd");
    assert!(result.is_err(), "path with .. should be rejected");
    match result.unwrap_err() {
        WikiError::PathTraversal(_) => {} // expected
        other => panic!("expected PathTraversal error, got: {other}"),
    }
}

#[test]
fn test_path_traversal_absolute_path_rejected() {
    let wiki_root = Path::new("/tmp/test-wiki");
    let result = validate_wiki_path(wiki_root, "/etc/passwd");
    assert!(result.is_err(), "absolute path should be rejected");
    match result.unwrap_err() {
        WikiError::PathTraversal(_) => {} // expected
        other => panic!("expected PathTraversal error, got: {other}"),
    }
}

#[test]
fn test_valid_relative_path_accepted() {
    let wiki_root = Path::new("/tmp/test-wiki");
    let result = validate_wiki_path(wiki_root, "architecture/foo.md");
    assert!(result.is_ok(), "valid relative path should be accepted");
    let path = result.unwrap();
    assert_eq!(path, Path::new("/tmp/test-wiki/wiki/architecture/foo.md"));
}

#[test]
fn test_path_traversal_nested_dotdot() {
    let wiki_root = Path::new("/tmp/test-wiki");
    let result = validate_wiki_path(wiki_root, "architecture/../../etc/passwd");
    assert!(result.is_err(), "nested .. path should be rejected");
    match result.unwrap_err() {
        WikiError::PathTraversal(_) => {} // expected
        other => panic!("expected PathTraversal error, got: {other}"),
    }
}

#[test]
fn test_path_traversal_raw_papers_escape() {
    // The specific attack vector from the issue
    let wiki_root = Path::new("/tmp/test-wiki");
    let result = validate_wiki_path(wiki_root, "../raw/papers/secret.pdf");
    assert!(result.is_err(), "path escaping to raw/ should be rejected");
    match result.unwrap_err() {
        WikiError::PathTraversal(_) => {} // expected
        other => panic!("expected PathTraversal error, got: {other}"),
    }
}

#[test]
fn test_valid_simple_path_accepted() {
    let wiki_root = Path::new("/tmp/test-wiki");
    let result = validate_wiki_path(wiki_root, "test.md");
    assert!(result.is_ok(), "simple filename should be accepted");
    let path = result.unwrap();
    assert_eq!(path, Path::new("/tmp/test-wiki/wiki/test.md"));
}

#[test]
fn test_path_traversal_encoded_dot_dot() {
    // Even if someone tries component-level traversal
    let wiki_root = Path::new("/tmp/test-wiki");
    let result = validate_wiki_path(wiki_root, "architecture/../../../etc/passwd");
    assert!(result.is_err(), "deep nested .. should be rejected");
    match result.unwrap_err() {
        WikiError::PathTraversal(_) => {} // expected
        other => panic!("expected PathTraversal error, got: {other}"),
    }
}
