use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn lw() -> Command {
    Command::cargo_bin("lw").unwrap()
}

// === init ===

#[test]
fn init_creates_wiki() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized wiki"));
    assert!(tmp.path().join(".lw/schema.toml").exists());
    assert!(tmp.path().join("wiki/architecture").is_dir());
    assert!(tmp.path().join("wiki/_uncategorized").is_dir());
    assert!(tmp.path().join("raw/papers").is_dir());
}

#[test]
fn init_twice_fails() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

// === query ===

#[test]
fn query_on_empty_wiki() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    // Exit code 2 = no results (human format)
    lw().args(["query", "anything", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .code(2);
}

#[test]
fn query_json_on_empty_wiki() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    // JSON format always exits 0, returns empty results
    let output = lw()
        .args([
            "query",
            "anything",
            "--root",
            tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["total"], 0);
    assert_eq!(json["results"].as_array().unwrap().len(), 0);
}

#[test]
fn query_finds_page() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    // Write a test page directly
    std::fs::write(
        tmp.path().join("wiki/architecture/test.md"),
        "---\ntitle: Test Page\ntags: [test]\n---\n\nHello world of testing.\n",
    )
    .unwrap();
    let output = lw()
        .args([
            "query",
            "testing",
            "--root",
            tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["total"].as_u64().unwrap() >= 1);
    assert_eq!(json["results"][0]["title"], "Test Page");
}

#[test]
fn query_brief_format() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    std::fs::write(
        tmp.path().join("wiki/architecture/t.md"),
        "---\ntitle: Transformer\ntags: [arch]\n---\n\nAttention mechanism.\n",
    )
    .unwrap();
    lw().args([
        "query",
        "attention",
        "--root",
        tmp.path().to_str().unwrap(),
        "--format",
        "brief",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Transformer"));
}

// === ingest ===

#[test]
fn ingest_with_yes_flag() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    // Create a source file
    let source = tmp.path().join("external.md");
    std::fs::write(&source, "# Test Source\nContent.").unwrap();
    lw().args([
        "ingest",
        source.to_str().unwrap(),
        "--root",
        tmp.path().to_str().unwrap(),
        "--category",
        "architecture",
        "--yes",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("path: wiki/architecture/"));
    // Verify raw copy exists
    assert!(tmp.path().join("raw/articles/external.md").exists());
}

#[test]
fn ingest_without_source_fails() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    lw().args(["ingest", "--root", tmp.path().to_str().unwrap(), "--yes"])
        .assert()
        .failure();
}

// === help ===

#[test]
fn help_shows_all_commands() {
    lw().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("query"))
        .stdout(predicate::str::contains("ingest"))
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("Examples"));
}

#[test]
fn query_help_has_examples() {
    lw().args(["query", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Examples"))
        .stdout(predicate::str::contains("--format json"));
}

// === error messages ===

#[test]
fn query_without_wiki_shows_actionable_error() {
    let tmp = TempDir::new().unwrap();
    // No init — should fail with helpful message
    lw().args(["query", "test", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("lw init"));
}

// === env var ===

#[test]
fn lw_wiki_root_env_var() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    std::fs::write(
        tmp.path().join("wiki/architecture/envtest.md"),
        "---\ntitle: Env Test\ntags: [test]\n---\n\nEnvironment variable test.\n",
    )
    .unwrap();
    lw().env("LW_WIKI_ROOT", tmp.path().to_str().unwrap())
        .args(["query", "environment", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Env Test"));
}
