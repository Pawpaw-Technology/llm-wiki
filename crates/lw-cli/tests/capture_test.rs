//! CLI integration tests for `lw capture` (issue #37).
//!
//! Coverage map:
//!   AC1 — `lw capture "text"` appends to today's journal: `appends_to_today_journal`
//!   AC2 — Auto-creates frontmatter:                       `auto_creates_frontmatter`
//!   AC3 — HH:MM timestamp prefix:                         `appends_to_today_journal`
//!   AC4 — `--tag` and `--source` flags:                   `tag_flag_renders`, `source_flag_renders`
//!   AC6 — `_journal` scaffolded by `lw init`:             `init_creates_journal_dir`
//!
//! Auto-commit interaction is also covered: a `lw capture` inside a git
//! repo produces a `docs(wiki): capture _journal/<DATE>` commit.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn lw() -> Command {
    Command::cargo_bin("lw").unwrap()
}

/// `lw init` + git init inside `tmp.path()`. Returns the wiki root TempDir.
fn setup_wiki_in_git_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    lw().args(["init", "--root", root.to_str().unwrap()])
        .assert()
        .success();
    init_repo(root);
    StdCommand::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "seed"])
        .current_dir(root)
        .output()
        .unwrap();
    tmp
}

fn init_repo(path: &Path) {
    StdCommand::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(path)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["config", "user.name", "T"])
        .current_dir(path)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["config", "user.email", "t@example.com"])
        .current_dir(path)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(path)
        .output()
        .unwrap();
}

fn today_iso() -> String {
    use lw_core::journal::{format_date_iso, local_now};
    format_date_iso(local_now().date())
}

fn read_today_journal(root: &Path) -> String {
    let path = root
        .join("wiki/_journal")
        .join(format!("{}.md", today_iso()));
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("journal not found at {path:?}"))
}

// ── AC6: `lw init` scaffolds `_journal/` ──────────────────────────────────────

#[test]
fn init_creates_journal_dir() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    lw().args(["init", "--root", root.to_str().unwrap()])
        .assert()
        .success();
    assert!(
        root.join("wiki/_journal").is_dir(),
        "lw init must scaffold wiki/_journal/ directory"
    );
}

// ── AC1 + AC3: lw capture writes the timestamped line ─────────────────────────

#[test]
fn appends_to_today_journal() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    lw().args([
        "--root",
        root.to_str().unwrap(),
        "capture",
        "comrak can round-trip markdown via arena AST",
    ])
    .assert()
    .success();

    let content = read_today_journal(root);
    assert!(
        content.contains("comrak can round-trip markdown via arena AST"),
        "captured text missing; content:\n{content}"
    );
    // Timestamp prefix shape: `**HH:MM**`. Use a regex predicate.
    let re = predicates::str::is_match(r"\*\*\d{2}:\d{2}\*\* comrak can round-trip").unwrap();
    assert!(
        re.eval(&content),
        "expected `**HH:MM** comrak can …` in journal; got:\n{content}"
    );
}

// ── AC2: auto-created frontmatter ─────────────────────────────────────────────

#[test]
fn auto_creates_frontmatter() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    lw().args([
        "--root",
        root.to_str().unwrap(),
        "capture",
        "first ever capture",
    ])
    .assert()
    .success();

    let content = read_today_journal(root);
    assert!(
        content.starts_with("---\n"),
        "page must start with frontmatter; got:\n{content}"
    );
    assert!(
        content.contains("tags: [journal]") || content.contains("- journal"),
        "frontmatter must include `journal` tag; got:\n{content}"
    );
    assert!(
        content.contains(&format!("created: {}", today_iso())),
        "frontmatter must include `created: <today>`; got:\n{content}"
    );
    assert!(
        content.contains("## Captures"),
        "page must include `## Captures` heading; got:\n{content}"
    );
}

// ── AC4: --tag + --source flags ───────────────────────────────────────────────

#[test]
fn tag_flag_renders() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    lw().args([
        "--root",
        root.to_str().unwrap(),
        "capture",
        "with tags",
        "--tag",
        "rust",
        "--tag",
        "markdown",
    ])
    .assert()
    .success();

    let content = read_today_journal(root);
    assert!(
        content.contains("with tags `#rust` `#markdown`"),
        "tag flags must render after content; got:\n{content}"
    );
}

#[test]
fn source_flag_renders() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    lw().args([
        "--root",
        root.to_str().unwrap(),
        "capture",
        "with source",
        "--source",
        "https://example.com/article",
    ])
    .assert()
    .success();

    let content = read_today_journal(root);
    assert!(
        content.contains("with source ([source](https://example.com/article))"),
        "source flag must render at end of line; got:\n{content}"
    );
}

// ── Auto-commit interaction ───────────────────────────────────────────────────

#[test]
fn capture_inside_git_repo_auto_commits_with_capture_action() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    let before = commit_count(root);
    lw().args([
        "--root",
        root.to_str().unwrap(),
        "capture",
        "auto-commit me",
    ])
    .assert()
    .success();

    let after = commit_count(root);
    assert_eq!(
        after,
        before + 1,
        "lw capture inside a git repo must auto-commit"
    );
    let subj = head_subject(root);
    assert!(
        subj.starts_with("docs(wiki): capture"),
        "commit subject must be 'docs(wiki): capture <slug>'; got: {subj}"
    );
}

#[test]
fn capture_no_commit_skips_commit() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    let before = commit_count(root);
    lw().args([
        "--root",
        root.to_str().unwrap(),
        "capture",
        "no-commit thought",
        "--no-commit",
    ])
    .assert()
    .success();

    assert_eq!(
        commit_count(root),
        before,
        "--no-commit must suppress the auto-commit"
    );
}

// ── Help string ───────────────────────────────────────────────────────────────

#[test]
fn capture_help_shows_examples() {
    lw().args(["capture", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::is_match("(?i)examples").unwrap());
}

// ── Empty content rejected ────────────────────────────────────────────────────

#[test]
fn empty_capture_exits_with_error() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();
    lw().args(["--root", root.to_str().unwrap(), "capture", "   "])
        .assert()
        .failure();
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn head_subject(repo: &Path) -> String {
    let out = StdCommand::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit_count(repo: &Path) -> u32 {
    let out = StdCommand::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0)
}
