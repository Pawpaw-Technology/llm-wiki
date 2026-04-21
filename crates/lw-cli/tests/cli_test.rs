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
    .stdout(predicate::str::contains("path: raw/articles/"));
    // Verify raw copy exists
    assert!(tmp.path().join("raw/articles/external.md").exists());
}

#[test]
fn ingest_stdin_uses_title_derived_filename() {
    // Regression: v0.2.0-rc.2 smoke gate found stdin ingest writing
    // `.tmpRandomXXX` files instead of slug-derived names.
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    lw().args([
        "--root",
        tmp.path().to_str().unwrap(),
        "ingest",
        "--stdin",
        "--title",
        "Hello World",
        "--category",
        "notes",
        "--raw-type",
        "articles",
        "--yes",
    ])
    .write_stdin("# body\n")
    .assert()
    .success();

    let raw_dir = tmp.path().join("raw/articles");
    let entries: Vec<String> = std::fs::read_dir(&raw_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    assert_eq!(entries.len(), 1, "expected exactly 1 file in raw/articles");
    let name = &entries[0];
    assert!(
        !name.starts_with('.'),
        "filename should not start with '.' (got {name})"
    );
    assert!(
        !name.starts_with("tmp"),
        "filename should not start with 'tmp' (got {name})"
    );
    assert!(
        name.to_lowercase().contains("hello"),
        "filename should contain title slug (got {name})"
    );
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

// === stale flag ===

#[test]
fn query_help_shows_stale_flag() {
    lw().args(["query", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--stale"));
}

#[test]
fn query_json_includes_freshness_field() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    std::fs::write(
        tmp.path().join("wiki/architecture/fresh.md"),
        "---\ntitle: Fresh Page\ntags: [test]\n---\n\nSome content about freshness.\n",
    )
    .unwrap();
    let output = lw()
        .args([
            "query",
            "freshness",
            "--root",
            tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["total"].as_u64().unwrap() >= 1);
    // Every result must have a freshness field
    for result in json["results"].as_array().unwrap() {
        assert!(
            result.get("freshness").is_some(),
            "Missing freshness field in JSON result: {result}"
        );
        let f = result["freshness"].as_str().unwrap();
        assert!(
            f == "fresh" || f == "suspect" || f == "stale",
            "Invalid freshness value: {f}"
        );
    }
}

#[test]
fn query_brief_includes_freshness_column() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    std::fs::write(
        tmp.path().join("wiki/architecture/brief.md"),
        "---\ntitle: Brief Test\ntags: [test]\n---\n\nBrief content for freshness test.\n",
    )
    .unwrap();
    // Brief format should include freshness as a column
    lw().args([
        "query",
        "brief",
        "--root",
        tmp.path().to_str().unwrap(),
        "--format",
        "brief",
    ])
    .assert()
    .success()
    .stdout(
        predicate::str::contains("fresh")
            .or(predicate::str::contains("stale"))
            .or(predicate::str::contains("suspect")),
    );
}

#[test]
fn query_stale_flag_accepted() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    std::fs::write(
        tmp.path().join("wiki/architecture/staleflag.md"),
        "---\ntitle: Stale Flag Test\ntags: [test]\n---\n\nContent for stale flag test.\n",
    )
    .unwrap();
    // --stale on a newly written file (no git history) should return no stale results
    // The command should succeed (exit 0 for json format even with no results)
    let output = lw()
        .args([
            "query",
            "stale",
            "--stale",
            "--root",
            tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // No git history means pages are treated as fresh, so --stale should filter them out
    assert_eq!(json["total"], 0);
    assert_eq!(json["results"].as_array().unwrap().len(), 0);
}

// === ingest --dry-run (#21) ===

#[test]
fn ingest_dry_run_no_file_written() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    let source = tmp.path().join("dry-run-source.md");
    std::fs::write(&source, "# Dry Run Test\nContent for dry run.").unwrap();
    lw().args([
        "ingest",
        source.to_str().unwrap(),
        "--root",
        tmp.path().to_str().unwrap(),
        "--category",
        "architecture",
        "--yes",
        "--dry-run",
    ])
    .assert()
    .success();
    // No wiki page should have been created
    let wiki_dir = tmp.path().join("wiki/architecture");
    let pages: Vec<_> = std::fs::read_dir(&wiki_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("dry-run"))
        .collect();
    assert!(
        pages.is_empty(),
        "dry-run should not create wiki pages, but found: {:?}",
        pages
    );
}

#[test]
fn ingest_dry_run_shows_preview() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    let source = tmp.path().join("preview-test.md");
    std::fs::write(&source, "# Preview Test\nPreview content here.").unwrap();
    let output = lw()
        .args([
            "ingest",
            source.to_str().unwrap(),
            "--root",
            tmp.path().to_str().unwrap(),
            "--category",
            "architecture",
            "--yes",
            "--dry-run",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("dry-run json output should be valid JSON");
    // Must contain title, category, path, dry_run flag
    assert_eq!(json["dry_run"], true);
    assert!(
        json["title"].as_str().is_some(),
        "missing title in dry-run output"
    );
    assert_eq!(json["category"], "architecture");
    assert!(
        json["path"].as_str().is_some(),
        "missing path in dry-run output"
    );
}

// === ingest --output-format json (#20) ===

#[test]
fn ingest_json_output() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    let source = tmp.path().join("json-output-source.md");
    std::fs::write(&source, "# JSON Output Test\nContent.").unwrap();
    let output = lw()
        .args([
            "ingest",
            source.to_str().unwrap(),
            "--root",
            tmp.path().to_str().unwrap(),
            "--category",
            "architecture",
            "--yes",
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("ingest json output should be valid JSON");
    assert_eq!(json["dry_run"], false);
    assert!(json["title"].as_str().is_some(), "missing title");
    assert_eq!(json["category"], "architecture");
    assert!(
        json["path"].as_str().unwrap().starts_with("raw/articles/"),
        "path should start with raw/articles/"
    );
}

// === import --output-format json (#20) ===

#[test]
fn import_json_output() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    let tweets = tmp.path().join("tweets.json");
    std::fs::write(
        &tweets,
        r#"[
        {"id":"1","full_text":"This is a test tweet about AI agents and capabilities.","screen_name":"user1","name":"User One","created_at":"2025-01-01 00:00:00","url":"https://x.com/1","favorite_count":10,"bookmark_count":5,"views_count":100,"retweet_count":0,"quote_count":0,"reply_count":0},
        {"id":"2","full_text":"Another test tweet about transformer architecture design.","screen_name":"user2","name":"User Two","created_at":"2025-01-02 00:00:00","url":"https://x.com/2","favorite_count":20,"bookmark_count":10,"views_count":200,"retweet_count":0,"quote_count":0,"reply_count":0}
    ]"#,
    )
    .unwrap();
    let output = lw()
        .args([
            "import",
            tweets.to_str().unwrap(),
            "--format",
            "twitter-json",
            "--root",
            tmp.path().to_str().unwrap(),
            "-o",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("import json output should be valid JSON");
    assert_eq!(json["imported"], 2);
    assert_eq!(json["total"], 2);
    assert!(json["skipped"].is_number(), "missing skipped count");
    assert!(json["pages"].is_array(), "missing pages array");
    assert_eq!(json["pages"].as_array().unwrap().len(), 2);
}

// === ingest URL source (#22) ===

#[test]
fn ingest_url_unreachable_gives_error() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    // A URL pointing to a non-existent host should fail with a download error
    lw().args([
        "ingest",
        "https://this-host-does-not-exist-12345.invalid/paper.pdf",
        "--root",
        tmp.path().to_str().unwrap(),
        "--category",
        "architecture",
        "--yes",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("download").or(predicate::str::contains("URL")));
}

#[test]
fn ingest_url_dry_run_skips_download() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    // dry-run with a URL should NOT attempt to download — it derives
    // preview metadata from the URL alone, so even an unreachable host succeeds.
    let output = lw()
        .args([
            "ingest",
            "https://this-host-does-not-exist-12345.invalid/paper.pdf",
            "--root",
            tmp.path().to_str().unwrap(),
            "--category",
            "architecture",
            "--yes",
            "--dry-run",
            "-o",
            "json",
        ])
        .output()
        .expect("failed to run lw");
    assert!(
        output.status.success(),
        "dry-run + URL should succeed without network access"
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("dry-run json output should be valid JSON");
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["title"], "paper.pdf");
    assert_eq!(json["category"], "architecture");
}

#[test]
fn ingest_help_shows_url_example() {
    lw().args(["ingest", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("http"));
}

// === path traversal security (#13 class) ===

#[test]
fn ingest_category_traversal_rejected() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    let source = tmp.path().join("traversal-test.md");
    std::fs::write(&source, "# Traversal Test\nContent.").unwrap();
    lw().args([
        "ingest",
        source.to_str().unwrap(),
        "--root",
        tmp.path().to_str().unwrap(),
        "--category",
        "../../etc",
        "--yes",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("traversal").or(predicate::str::contains("path")));
}

// === read ===

fn setup_wiki_with_page(tmp: &TempDir) {
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    std::fs::write(
        tmp.path().join("wiki/architecture/transformer.md"),
        "---\ntitle: Flash Attention 2\ntags:\n  - architecture\n  - attention\ndecay: normal\n---\n\nFlash Attention 2 reduces memory usage.\n",
    )
    .unwrap();
}

#[test]
fn read_existing_page() {
    let tmp = TempDir::new().unwrap();
    setup_wiki_with_page(&tmp);
    lw().args([
        "read",
        "architecture/transformer.md",
        "--root",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Flash Attention 2"));
}

#[test]
fn read_nonexistent_page_fails() {
    let tmp = TempDir::new().unwrap();
    setup_wiki_with_page(&tmp);
    lw().args([
        "read",
        "nonexistent/page.md",
        "--root",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .failure();
}

#[test]
fn read_json_format() {
    let tmp = TempDir::new().unwrap();
    setup_wiki_with_page(&tmp);
    let output = lw()
        .args([
            "read",
            "architecture/transformer.md",
            "--root",
            tmp.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("read --format json should produce valid JSON");
    assert_eq!(json["path"], "architecture/transformer.md");
    assert_eq!(json["title"], "Flash Attention 2");
    assert!(json["tags"].is_array());
    assert!(
        json["body"]
            .as_str()
            .unwrap()
            .contains("reduces memory usage")
    );
}

#[test]
fn import_category_traversal_rejected() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    let tweets = tmp.path().join("traversal-tweets.json");
    std::fs::write(
        &tweets,
        r#"[{"id":"1","full_text":"Test tweet content about AI.","screen_name":"user1","name":"User One","created_at":"2025-01-01 00:00:00","url":"https://x.com/1","favorite_count":10,"bookmark_count":5,"views_count":100,"retweet_count":0,"quote_count":0,"reply_count":0}]"#,
    )
    .unwrap();
    lw().args([
        "import",
        tweets.to_str().unwrap(),
        "--format",
        "twitter-json",
        "--root",
        tmp.path().to_str().unwrap(),
        "--category",
        "../../etc",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("traversal").or(predicate::str::contains("path")));
}

#[test]
fn ingest_dry_run_category_traversal_rejected() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    let source = tmp.path().join("dry-traversal.md");
    std::fs::write(&source, "# Dry Traversal\nContent.").unwrap();
    lw().args([
        "ingest",
        source.to_str().unwrap(),
        "--root",
        tmp.path().to_str().unwrap(),
        "--category",
        "../../../tmp",
        "--yes",
        "--dry-run",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("traversal").or(predicate::str::contains("path")));
}

#[test]
fn read_path_traversal_rejected() {
    let tmp = TempDir::new().unwrap();
    setup_wiki_with_page(&tmp);
    lw().args([
        "read",
        "../../etc/passwd",
        "--root",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .failure();
}

#[test]
fn help_shows_read_command() {
    lw().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("read"));
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
