//! CLI surface for `lw backlinks`. Exercises happy path, missing page, JSON
//! format, and slug-vs-path argument acceptance.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn lw() -> Command {
    Command::cargo_bin("lw").unwrap()
}

/// Set up a minimal wiki with a `tools` category, then create a target page
/// (`tools/bar.md`) and a source page (`tools/foo.md`) that wikilinks to it.
fn setup_with_link() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    lw().args(["init", "--root", root.to_str().unwrap()])
        .assert()
        .success();

    // Schema: minimal `tools` block (no required_fields so we can easily author).
    let schema_toml = "[wiki]\nname = \"Test Wiki\"\ndefault_review_days = 90\n\n[tags]\ncategories = [\"architecture\", \"training\", \"infra\", \"tools\", \"product\", \"ops\"]\n\n[categories.tools]\ntemplate = \"\"\n";
    std::fs::write(root.join(".lw/schema.toml"), schema_toml).unwrap();

    std::fs::create_dir_all(root.join("wiki/tools")).unwrap();
    // Target page
    std::fs::write(
        root.join("wiki/tools/bar.md"),
        "---\ntitle: Bar\ntags: [tools]\n---\n\nbar body\n",
    )
    .unwrap();
    // Source page wikilinks to bar
    std::fs::write(
        root.join("wiki/tools/foo.md"),
        "---\ntitle: Foo\ntags: [tools]\n---\n\nSee [[bar]] for details.\n",
    )
    .unwrap();

    tmp
}

#[test]
fn backlinks_happy_path_lists_inbound_links() {
    let tmp = setup_with_link();
    let root = tmp.path();

    lw().args(["backlinks", "bar", "--root", root.to_str().unwrap()])
        .assert()
        .success()
        // Human format names the source page so a user can jump to it.
        .stdout(predicate::str::contains("tools/foo.md"));
}

#[test]
fn backlinks_accepts_full_path_arg() {
    let tmp = setup_with_link();
    let root = tmp.path();

    // Same call but using the wiki-relative path form.
    lw().args([
        "backlinks",
        "tools/bar.md",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("tools/foo.md"));
}

#[test]
fn backlinks_format_json_emits_structured_payload() {
    let tmp = setup_with_link();
    let root = tmp.path();

    let output = lw()
        .args([
            "backlinks",
            "bar",
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

    let backlinks = json["backlinks"]
        .as_array()
        .expect("backlinks array required");
    assert_eq!(backlinks.len(), 1, "one inbound link: {json}");
    assert_eq!(
        backlinks[0]["source"].as_str(),
        Some("wiki/tools/foo.md"),
        "source path must use wiki/ prefix: {json}"
    );
}

#[test]
fn backlinks_missing_page_exits_with_error() {
    let tmp = setup_with_link();
    let root = tmp.path();

    lw().args([
        "backlinks",
        "definitely-not-a-page",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .code(2)
    .stderr(predicate::str::contains("no backlinks").or(predicate::str::contains("not found")));
}

#[test]
fn backlinks_help_shows_examples() {
    lw().args(["backlinks", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("(?i)examples").unwrap());
}
