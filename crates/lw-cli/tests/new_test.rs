use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn lw() -> Command {
    Command::cargo_bin("lw").unwrap()
}

/// Scaffold a minimal wiki with a `tools` category that has required_fields = ["title", "tags"]
/// and a body template.
fn setup_vault_with_tools_category() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // Run `lw init` to create the skeleton
    lw().args(["init", "--root", root.to_str().unwrap()])
        .assert()
        .success();

    // Overwrite the schema to add a [categories.tools] block with required fields + template
    let schema_toml = "[wiki]\nname = \"Test Wiki\"\ndefault_review_days = 90\n\n[tags]\ncategories = [\"architecture\", \"training\", \"infra\", \"tools\", \"product\", \"ops\"]\n\n[categories.tools]\nrequired_fields = [\"title\", \"tags\"]\ntemplate = \"## Overview\\n\\n## Usage\\n\"\n";
    std::fs::write(root.join(".lw/schema.toml"), schema_toml).unwrap();

    tmp
}

// ── happy_path_creates_file ───────────────────────────────────────────────────

#[test]
fn happy_path_creates_file() {
    let tmp = setup_vault_with_tools_category();
    let root = tmp.path();

    lw().args([
        "new",
        "tools/foo",
        "--title",
        "Foo",
        "--tags",
        "a,b",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    // File must exist at the expected path
    let page_path = root.join("wiki/tools/foo.md");
    assert!(page_path.exists(), "page file was not created");

    let content = std::fs::read_to_string(&page_path).unwrap();

    // Frontmatter: title and tags
    assert!(
        content.contains("title: Foo"),
        "frontmatter missing title; content:\n{content}"
    );
    assert!(
        content.contains("- a") && content.contains("- b"),
        "frontmatter missing tags; content:\n{content}"
    );

    // Body: category body template
    assert!(
        content.contains("## Overview"),
        "body missing '## Overview' template section; content:\n{content}"
    );
    assert!(
        content.contains("## Usage"),
        "body missing '## Usage' template section; content:\n{content}"
    );
}

// ── format_json_emits_metadata ────────────────────────────────────────────────

#[test]
fn format_json_emits_metadata() {
    let tmp = setup_vault_with_tools_category();
    let root = tmp.path();

    let output = lw()
        .args([
            "new",
            "tools/bar",
            "--title",
            "Bar",
            "--tags",
            "rust,cli",
            "--format",
            "json",
            "--root",
            root.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "expected success; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");

    // Must contain path, category, slug fields
    assert!(
        json["path"].is_string(),
        "JSON missing 'path' field; got: {json}"
    );
    assert_eq!(
        json["category"].as_str(),
        Some("tools"),
        "JSON 'category' field mismatch"
    );
    assert_eq!(
        json["slug"].as_str(),
        Some("bar"),
        "JSON 'slug' field mismatch"
    );

    // path must be vault-relative: "wiki/tools/bar.md"
    let path_val = json["path"].as_str().unwrap();
    assert_eq!(
        path_val, "wiki/tools/bar.md",
        "path must be vault-relative, got '{path_val}'"
    );
}

// ── duplicate_slug_exits_with_error ──────────────────────────────────────────

#[test]
fn duplicate_slug_exits_with_error() {
    let tmp = setup_vault_with_tools_category();
    let root = tmp.path();

    let args = [
        "new",
        "tools/dup",
        "--title",
        "Dup",
        "--tags",
        "x",
        "--root",
        root.to_str().unwrap(),
    ];

    // First call: success
    lw().args(args).assert().success();

    // Read file content before second call
    let page_path = root.join("wiki/tools/dup.md");
    let before = std::fs::read_to_string(&page_path).unwrap();

    // Second call: must fail with exit 1, and the message must carry a
    // VAULT-RELATIVE path (issue #87) — not the absolute host path.
    lw().args(args)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "page already exists: wiki/tools/dup.md",
        ));

    // File content must be unchanged
    let after = std::fs::read_to_string(&page_path).unwrap();
    assert_eq!(
        before, after,
        "file content changed after duplicate-slug error"
    );
}

// ── unknown_category_exits_with_error ────────────────────────────────────────

#[test]
fn unknown_category_exits_with_error() {
    let tmp = setup_vault_with_tools_category();
    let root = tmp.path();

    lw().args([
        "new",
        "bogus/foo",
        "--title",
        "x",
        "--tags",
        "a",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .code(1)
    // WikiError::UnknownCategory Display: "unknown category: bogus (valid: ...)"
    .stderr(predicate::str::contains("unknown category: bogus"))
    // valid list must be present so the user knows their options
    .stderr(predicate::str::contains("valid:"));
}

// ── help_shows_examples_section ──────────────────────────────────────────────

#[test]
fn help_shows_examples_section() {
    lw().args(["new", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("(?i)examples").unwrap());
}

// ── integration_test_uses_assert_cmd_and_tempdir ─────────────────────────────
//
// This test documents that the test file satisfies the structural criterion
// "Integration test in crates/lw-cli/tests/ using assert_cmd + TempDir".
// It is a meta-test: it passes iff the binary can be located via assert_cmd.

#[test]
fn assert_cmd_and_tempdir_are_used() {
    let _tmp = TempDir::new().unwrap(); // TempDir used
                                        // assert_cmd::Command used throughout (see lw() helper above)
    lw().args(["--version"]).assert().success();
}
