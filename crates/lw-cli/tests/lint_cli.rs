//! CLI-level integration tests for `lw lint` with the unlinked-mentions rule.
//! Issue #102: surfaces the mention matcher (#101) as a lint rule.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn lw() -> Command {
    Command::cargo_bin("lw").unwrap()
}

/// Create a minimal initialized wiki with two pages:
/// - tools/tantivy.md  (the target page)
/// - tools/comrak.md   (mentions "tantivy" without linking it)
///
/// Returns the TempDir so it stays alive for the duration of the test.
fn wiki_with_unlinked_mention() -> TempDir {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();

    std::fs::write(
        tmp.path().join("wiki/tools/tantivy.md"),
        "---\ntitle: Tantivy\ntags: [tools]\n---\n\nTantivy is a full-text search engine.\n",
    )
    .unwrap();

    std::fs::write(
        tmp.path().join("wiki/tools/comrak.md"),
        "---\ntitle: Comrak\ntags: [tools]\n---\n\nComrak is a CommonMark parser. We also use tantivy for search.\n",
    )
    .unwrap();

    tmp
}

/// Acceptance bullet 4 (CLI): text output format matches the spec.
/// Format: `<path>:<line> — "<term>" could link to [[<target>]]`
/// Uses em-dash (U+2014), quoted term, double-bracket target.
#[test]
fn lint_unlinked_mentions_text_format_cli() {
    let tmp = wiki_with_unlinked_mention();

    let output = lw()
        .args(["lint", "--root", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Must contain the em-dash character (U+2014)
    assert!(
        stdout.contains('\u{2014}'),
        "lint text output must contain em-dash (—): {stdout:?}"
    );
    // Must quote the term
    assert!(
        stdout.contains("\"tantivy\""),
        "lint text output must quote the term: {stdout:?}"
    );
    // Must use [[slug]] for target
    assert!(
        stdout.contains("[[tantivy]]"),
        "lint text output must use [[tantivy]] notation: {stdout:?}"
    );
    // Must contain a line number reference
    assert!(
        stdout.contains(":1") || stdout.contains(":2"),
        "lint text output must include a line number: {stdout:?}"
    );
}

/// Acceptance bullet 5 (CLI): exit code is 1 when there are findings.
#[test]
fn lint_exits_1_when_findings_present() {
    let tmp = wiki_with_unlinked_mention();

    let output = lw()
        .args(["lint", "--root", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "lw lint must exit 1 when findings are present; stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// Acceptance bullet 5 (CLI): exit code is 0 on a clean wiki.
#[test]
fn lint_exits_0_on_clean_wiki() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();

    // No pages — nothing to find
    let output = lw()
        .args(["lint", "--root", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(0),
        "lw lint must exit 0 on a clean wiki; stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// Acceptance bullet 3 (CLI): `--format json` output matches the spec exactly.
/// Shape: `{"rule": "unlinked-mentions", "path": "...", "line": N, "term": "...", "target": "..."}`
#[test]
fn lint_json_format_unlinked_mentions_shape() {
    let tmp = wiki_with_unlinked_mention();

    let output = lw()
        .args([
            "lint",
            "--format",
            "json",
            "--root",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("lint --format json must emit valid JSON");

    // The top-level report must have an "unlinked_mentions" array.
    let findings = json["unlinked_mentions"]
        .as_array()
        .expect("JSON report must contain unlinked_mentions array");

    assert!(
        !findings.is_empty(),
        "unlinked_mentions array must be non-empty when findings exist"
    );

    let f = &findings[0];
    assert_eq!(
        f["rule"], "unlinked-mentions",
        "rule field must be 'unlinked-mentions': {f}"
    );
    assert!(f["path"].is_string(), "path must be string: {f}");
    assert!(f["line"].is_number(), "line must be number: {f}");
    assert_eq!(f["term"], "tantivy", "term must match: {f}");
    assert_eq!(f["target"], "tantivy", "target must be slug: {f}");
}

/// `--rule unlinked-mentions` filters output to only that rule's findings.
/// With the flag, the report must contain unlinked-mentions findings and
/// suppress other rule sections (the output must be scoped to this rule only).
#[test]
fn lint_rule_filter_unlinked_mentions_json() {
    let tmp = wiki_with_unlinked_mention();

    let output = lw()
        .args([
            "lint",
            "--rule",
            "unlinked-mentions",
            "--format",
            "json",
            "--root",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("lint --rule json must emit valid JSON");

    // When --rule is applied, must include unlinked-mentions findings.
    let findings = json["unlinked_mentions"]
        .as_array()
        .expect("filtered report must contain unlinked_mentions array");
    assert!(
        !findings.is_empty(),
        "filtered report must include the rule's findings"
    );
    // Other rule sections must be empty (rule filter suppresses them).
    assert!(
        json["todo_pages"].as_array().is_none_or(|a| a.is_empty()),
        "todo_pages must be empty when filtering to unlinked-mentions"
    );
}

/// `--rule unlinked-mentions` also works in text/human mode.
#[test]
fn lint_rule_filter_unlinked_mentions_human() {
    let tmp = wiki_with_unlinked_mention();

    lw().args([
        "lint",
        "--rule",
        "unlinked-mentions",
        "--root",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .code(1)
    .stdout(predicate::str::contains("could link to"));
}
