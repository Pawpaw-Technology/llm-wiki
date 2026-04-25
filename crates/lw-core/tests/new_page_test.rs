use lw_core::fs::{init_wiki, new_page, read_page, NewPageRequest};
use lw_core::schema::{CategoryConfig, WikiSchema};
/// Tests for `lw_core::fs::new_page` (issue #59)
///
/// These tests cover all acceptance criteria from the spec:
///   1. Happy path — file created, frontmatter correct, body == category template
///   2. Duplicate slug → `PageAlreadyExists`, file unchanged
///   3. Missing required field → `MissingRequiredField`
///   4. Unknown category → `UnknownCategory`
///   5. `_uncategorized` accepted with empty template (no crash)
///   6. Invalid slugs (`/`, `..`, empty, uppercase) → `InvalidSlug`
///   7. Category without `[categories.<name>]` block uses empty template (no crash)
///   8. Display strings for all four new error variants match canonical spec wording
use lw_core::WikiError;
use std::collections::HashMap;
use tempfile::TempDir;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Build a `WikiSchema` with one category that has a template + required field.
fn schema_with_tools_category() -> WikiSchema {
    let mut schema = WikiSchema::default();
    // "tools" is already in default categories; give it a template + required field.
    schema.categories.insert(
        "tools".to_string(),
        CategoryConfig {
            review_days: None,
            required_fields: vec!["title".to_string()],
            template: "## Overview\n\nDescribe the tool here.\n".to_string(),
        },
    );
    schema
}

fn req<'a>(category: &'a str, slug: &'a str, title: &str) -> NewPageRequest<'a> {
    NewPageRequest {
        category,
        slug,
        title: title.to_string(),
        tags: vec!["rust".to_string()],
        author: Some("alice".to_string()),
    }
}

// ── Acceptance criterion 1: happy path ───────────────────────────────────────

/// Happy path: file is created, frontmatter matches request, body == template.
#[test]
fn happy_path_creates_file_with_correct_content() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = schema_with_tools_category();
    init_wiki(root, &schema).unwrap();

    let request = NewPageRequest {
        category: "tools",
        slug: "my-tool",
        title: "My Tool".to_string(),
        tags: vec!["rust".to_string(), "cli".to_string()],
        author: Some("alice".to_string()),
    };

    let (path, page) = new_page(root, &schema, request).unwrap();

    // Path must be under wiki_root/wiki/tools/my-tool.md
    assert_eq!(path, root.join("wiki/tools/my-tool.md"));
    assert!(path.exists(), "file should have been written to disk");

    // Frontmatter
    assert_eq!(page.title, "My Tool");
    assert_eq!(page.tags, vec!["rust".to_string(), "cli".to_string()]);
    assert_eq!(page.author, Some("alice".to_string()));

    // Body == category template
    assert_eq!(
        page.body, "## Overview\n\nDescribe the tool here.\n",
        "body must equal the category template"
    );

    // Round-trip: read back from disk and verify
    let loaded = read_page(&path).unwrap();
    assert_eq!(loaded.title, "My Tool");
    assert_eq!(loaded.tags, vec!["rust".to_string(), "cli".to_string()]);
}

// ── Acceptance criterion 2: duplicate slug → PageAlreadyExists ───────────────

/// Writing the same slug twice must return `PageAlreadyExists` on the second call,
/// and must NOT overwrite the first file.
#[test]
fn duplicate_slug_returns_page_already_exists() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = schema_with_tools_category();
    init_wiki(root, &schema).unwrap();

    let make_req = || NewPageRequest {
        category: "tools",
        slug: "duplicate",
        title: "Duplicate Tool".to_string(),
        tags: vec![],
        author: None,
    };

    // First write succeeds
    let (path, _) = new_page(root, &schema, make_req()).unwrap();
    let original_content = std::fs::read_to_string(&path).unwrap();

    // Second write must fail
    let err = new_page(root, &schema, make_req()).unwrap_err();
    match err {
        WikiError::PageAlreadyExists { path: err_path } => {
            assert_eq!(err_path, root.join("wiki/tools/duplicate.md"));
        }
        other => panic!("expected PageAlreadyExists, got {other:?}"),
    }

    // File must be unchanged
    let after_content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        original_content, after_content,
        "duplicate write must not modify the existing file"
    );
}

// ── Acceptance criterion 3: missing required field → MissingRequiredField ────

/// When a category declares `required_fields = ["title"]` and the request has
/// an empty title, `MissingRequiredField` must be returned.
#[test]
fn missing_required_title_returns_error() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = schema_with_tools_category();
    init_wiki(root, &schema).unwrap();

    let request = NewPageRequest {
        category: "tools",
        slug: "empty-title",
        title: String::new(), // ← empty title
        tags: vec![],
        author: None,
    };

    let err = new_page(root, &schema, request).unwrap_err();
    match err {
        WikiError::MissingRequiredField { category, field } => {
            assert_eq!(category, "tools");
            assert_eq!(field, "title");
        }
        other => panic!("expected MissingRequiredField, got {other:?}"),
    }
}

/// When a category declares `required_fields = ["tags"]` and the request has
/// empty tags, `MissingRequiredField` must be returned.
#[test]
fn missing_required_tags_returns_error() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let mut schema = WikiSchema::default();
    schema.categories.insert(
        "tools".to_string(),
        CategoryConfig {
            review_days: None,
            required_fields: vec!["tags".to_string()],
            template: String::new(),
        },
    );
    init_wiki(root, &schema).unwrap();

    let request = NewPageRequest {
        category: "tools",
        slug: "notags",
        title: "No Tags".to_string(),
        tags: vec![], // ← empty tags
        author: None,
    };

    let err = new_page(root, &schema, request).unwrap_err();
    match err {
        WikiError::MissingRequiredField { category, field } => {
            assert_eq!(category, "tools");
            assert_eq!(field, "tags");
        }
        other => panic!("expected MissingRequiredField, got {other:?}"),
    }
}

// ── Acceptance criterion 4: unknown category → UnknownCategory ───────────────

/// A category that is not in `schema.tags.categories` (and not `_uncategorized`)
/// must return `UnknownCategory`.
#[test]
fn unknown_category_returns_error() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    let request = req("nonexistent-category", "some-slug", "Title");
    let err = new_page(root, &schema, request).unwrap_err();
    match err {
        WikiError::UnknownCategory { name, valid } => {
            assert_eq!(name, "nonexistent-category");
            // valid must be a comma-separated list of schema categories
            for cat in &schema.tags.categories {
                assert!(
                    valid.contains(cat.as_str()),
                    "valid list '{valid}' must contain '{cat}'"
                );
            }
        }
        other => panic!("expected UnknownCategory, got {other:?}"),
    }
}

// ── Acceptance criterion 5: _uncategorized accepted with empty template ───────

/// `_uncategorized` is always a valid category and must succeed with an empty body.
#[test]
fn uncategorized_accepted_with_empty_template() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    let request = NewPageRequest {
        category: "_uncategorized",
        slug: "misc-note",
        title: "Misc Note".to_string(),
        tags: vec![],
        author: None,
    };

    let (path, page) = new_page(root, &schema, request).unwrap();
    assert_eq!(path, root.join("wiki/_uncategorized/misc-note.md"));
    assert!(path.exists());
    // No template configured → body must be empty (or at most whitespace)
    assert!(
        page.body.trim().is_empty(),
        "uncategorized page body must be empty when no template configured"
    );
}

// ── Acceptance criterion 6: invalid slugs → InvalidSlug ─────────────────────

fn assert_invalid_slug(slug: &str) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    let request = NewPageRequest {
        category: "tools",
        slug,
        title: "Title".to_string(),
        tags: vec![],
        author: None,
    };

    let err = new_page(root, &schema, request).unwrap_err();
    match err {
        WikiError::InvalidSlug { slug: bad } => {
            assert_eq!(bad, slug, "error slug must echo the input");
        }
        other => panic!("expected InvalidSlug for slug={slug:?}, got {other:?}"),
    }
}

#[test]
fn empty_slug_is_invalid() {
    assert_invalid_slug("");
}

#[test]
fn slug_with_slash_is_invalid() {
    assert_invalid_slug("foo/bar");
}

#[test]
fn slug_double_dot_is_invalid() {
    assert_invalid_slug("..");
}

#[test]
fn slug_leading_dot_is_invalid() {
    assert_invalid_slug(".hidden");
}

#[test]
fn slug_uppercase_is_invalid() {
    assert_invalid_slug("FooBar");
}

#[test]
fn slug_with_space_is_invalid() {
    assert_invalid_slug("foo bar");
}

/// Valid slugs must not return an error.
#[test]
fn valid_slugs_are_accepted() {
    for valid in &["foo", "foo-bar", "foo_bar", "foo123", "123", "a-b-c"] {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let schema = WikiSchema::default();
        init_wiki(root, &schema).unwrap();

        let request = NewPageRequest {
            category: "tools",
            slug: valid,
            title: "T".to_string(),
            tags: vec![],
            author: None,
        };

        new_page(root, &schema, request)
            .unwrap_or_else(|e| panic!("slug {valid:?} should be valid but got {e:?}"));
    }
}

// ── Acceptance criterion 7: category without config block uses empty template ─

/// A category that is listed in `schema.tags.categories` but has no
/// `[categories.<name>]` block must produce an empty-body page (no crash).
#[test]
fn category_without_config_block_uses_empty_template() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    // Default schema has "architecture" with no CategoryConfig entry
    let schema = WikiSchema::default();
    assert!(
        schema.category_config("architecture").is_none(),
        "test setup: architecture must have no config block in default schema"
    );
    init_wiki(root, &schema).unwrap();

    let request = NewPageRequest {
        category: "architecture",
        slug: "transformer",
        title: "Transformer".to_string(),
        tags: vec!["ml".to_string()],
        author: None,
    };

    let (path, page) = new_page(root, &schema, request).unwrap();
    assert_eq!(path, root.join("wiki/architecture/transformer.md"));
    assert!(path.exists());
    assert!(
        page.body.trim().is_empty(),
        "category without config block must produce empty body"
    );
}

// ── Acceptance criterion 8: Display strings match canonical spec wording ──────

/// Verify that the four new `WikiError` variants produce exactly the strings
/// specified in issue #59. These strings are the contract reused by #60 and #61.
#[test]
fn display_strings_match_spec() {
    // PageAlreadyExists
    let e = WikiError::PageAlreadyExists {
        path: std::path::PathBuf::from("/wiki/tools/foo.md"),
    };
    assert_eq!(
        e.to_string(),
        "page already exists: /wiki/tools/foo.md",
        "PageAlreadyExists display must match spec"
    );

    // UnknownCategory
    let e = WikiError::UnknownCategory {
        name: "bogus".to_string(),
        valid: "tools, infra".to_string(),
    };
    assert_eq!(
        e.to_string(),
        "unknown category: bogus (valid: tools, infra)",
        "UnknownCategory display must match spec"
    );

    // MissingRequiredField
    let e = WikiError::MissingRequiredField {
        category: "tools".to_string(),
        field: "title".to_string(),
    };
    assert_eq!(
        e.to_string(),
        "category tools requires field: title",
        "MissingRequiredField display must match spec"
    );

    // InvalidSlug
    let e = WikiError::InvalidSlug {
        slug: "Bad/Slug".to_string(),
    };
    assert_eq!(
        e.to_string(),
        "invalid slug: Bad/Slug (must match [a-z0-9_-]+, no path separators)",
        "InvalidSlug display must match spec"
    );
}

// ── Extra: author is optional ─────────────────────────────────────────────────

/// Ensure new_page works when author is None.
#[test]
fn no_author_is_accepted() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    let request = NewPageRequest {
        category: "tools",
        slug: "no-author",
        title: "No Author".to_string(),
        tags: vec![],
        author: None,
    };

    let (path, page) = new_page(root, &schema, request).unwrap();
    assert!(path.exists());
    assert_eq!(page.author, None);
}

// ── Unused import suppression ─────────────────────────────────────────────────
// HashMap is used only to construct schemas inside helpers; keep lint quiet.
#[allow(unused_imports)]
use std::collections::HashMap as _HashMap;
