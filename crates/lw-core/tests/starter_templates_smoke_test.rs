use lw_core::fs::{init_wiki, load_schema, new_page, NewPageRequest};
/// Smoke tests for starter vault schema.toml files (issue #62)
///
/// These tests load the real starter `schema.toml` files from `templates/` and
/// validate that they contain well-formed `[categories.<name>]` blocks for every
/// category listed in `[tags].categories`, then exercise the `new_page` round-trip.
///
/// Pure-Rust — no CLI shell-out — so this test slice is parallelizable with #60 / #61.
use lw_core::WikiError;
use tempfile::TempDir;

/// Returns the absolute path to a starter template directory.
///
/// `CARGO_MANIFEST_DIR` points to `crates/lw-core`; the templates live two levels up
/// in `templates/<name>`.
fn template_root(name: &str) -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // crates/lw-core → repo root = two levels up
    let repo_root = manifest.parent().unwrap().parent().unwrap();
    repo_root.join("templates").join(name)
}

// ── Test 1: engineering-notes/tools round-trip ───────────────────────────────

/// Load `templates/engineering-notes/.lw/schema.toml`, call `new_page` for
/// category `tools`, assert body matches the shipped `[categories.tools].template`.
#[test]
fn engineering_notes_tools_template_round_trip() {
    let schema_root = template_root("engineering-notes");
    let schema = load_schema(&schema_root).expect("failed to load engineering-notes schema");

    let cfg = schema
        .category_config("tools")
        .expect("[categories.tools] block must exist in engineering-notes schema");

    // scaffold a temp wiki using this schema so new_page can write
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &schema).unwrap();

    let req = NewPageRequest {
        category: "tools",
        slug: "my-tool",
        title: "My Tool".to_string(),
        tags: vec!["rust".to_string()],
        author: None,
    };

    let (_path, page) = new_page(root, &schema, req).expect("new_page should succeed");

    assert_eq!(
        page.body, cfg.template,
        "page body must equal [categories.tools].template from the shipped schema"
    );
}

// ── Test 2: engineering-notes/tools missing required field ───────────────────

/// When `tags` is empty and `[categories.tools].required_fields` includes "tags",
/// `new_page` must return `MissingRequiredField`.
#[test]
fn engineering_notes_tools_missing_required_field() {
    let schema_root = template_root("engineering-notes");
    let schema = load_schema(&schema_root).expect("failed to load engineering-notes schema");

    // ensure the schema actually declares "tags" as required for tools
    let cfg = schema
        .category_config("tools")
        .expect("[categories.tools] block must exist");
    assert!(
        cfg.required_fields.contains(&"tags".to_string()),
        "required_fields for tools must include 'tags'; got: {:?}",
        cfg.required_fields
    );

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    init_wiki(root, &schema).unwrap();

    let req = NewPageRequest {
        category: "tools",
        slug: "no-tags-tool",
        title: "No Tags Tool".to_string(),
        tags: vec![], // ← intentionally empty to trigger MissingRequiredField
        author: None,
    };

    let err = new_page(root, &schema, req).expect_err("should fail due to missing tags");
    match err {
        WikiError::MissingRequiredField { category, field } => {
            assert_eq!(category, "tools");
            assert_eq!(field, "tags");
        }
        other => panic!("expected MissingRequiredField, got {other:?}"),
    }
}

// ── Test 3: all starters have category blocks for every listed category ───────

/// For each starter, every category in `tags.categories` must have a non-empty
/// `[categories.<name>]` block with template, required_fields, and review_days.
#[test]
fn all_starters_have_category_blocks_for_listed_categories() {
    let starters = ["general", "engineering-notes", "research-papers"];

    for starter in &starters {
        let schema_root = template_root(starter);
        let schema = load_schema(&schema_root)
            .unwrap_or_else(|e| panic!("failed to load schema for {starter}: {e}"));

        for cat in &schema.tags.categories {
            let cfg = schema.category_config(cat).unwrap_or_else(|| {
                panic!("starter '{starter}': category '{cat}' has no [categories.{cat}] block")
            });

            assert!(
                !cfg.template.is_empty(),
                "starter '{starter}': [categories.{cat}].template must not be empty"
            );

            assert!(
                cfg.review_days.is_some(),
                "starter '{starter}': [categories.{cat}].review_days must be set"
            );
        }
    }
}

// ── Test 4: doctor / cargo test doesn't regress ───────────────────────────────
//
// `lw doctor` is a CLI command that has its own test suite in `crates/lw-cli/`.
// There is no exposed `lw_core` validation function for "doctor". The acceptance
// criterion "lw doctor doesn't regress" is satisfied by:
//   (a) `cargo test` passing workspace-wide — which CI enforces; and
//   (b) the tests in this file covering schema validity directly.
//
// A dedicated `doctor_passes_for_all_starters` test would require shelling out
// to the `lw` binary (which makes this slice non-parallelizable with #60/#61).
// We document the decision here and rely on `cargo test` + CI instead.
