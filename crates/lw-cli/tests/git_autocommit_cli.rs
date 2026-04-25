//! CLI integration tests for git auto-commit (issue #38).
//!
//! These tests stand up a real git repo in a TempDir for each scenario,
//! run `lw write/new/ingest` via assert_cmd, then verify the resulting
//! git history. A bare repo is wired in for `--push` and `lw sync`
//! coverage, so no network access is required.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn lw() -> Command {
    Command::cargo_bin("lw").unwrap()
}

/// Init a git repo with sane defaults so commits don't fail on missing
/// identity / GPG signing config.
fn init_repo(path: &Path) {
    StdCommand::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(path)
        .output()
        .expect("git init");
    StdCommand::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(path)
        .output()
        .expect("git config user.name");
    StdCommand::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(path)
        .output()
        .expect("git config user.email");
    StdCommand::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(path)
        .output()
        .expect("git config commit.gpgsign");
}

/// `lw init` + `git init` + a baseline empty commit. Returns the wiki root.
fn setup_wiki_in_git_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    lw().args(["init", "--root", root.to_str().unwrap()])
        .assert()
        .success();
    init_repo(root);
    // Seed: commit the .lw/ scaffold so HEAD exists and subsequent
    // commits aren't initial commits (initial commit semantics differ).
    StdCommand::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .expect("git add scaffold");
    StdCommand::new("git")
        .args(["commit", "-m", "seed"])
        .current_dir(root)
        .output()
        .expect("git commit seed");
    tmp
}

/// Set up wiki + git + a `tools` category with required fields, suitable
/// for `lw new tools/<slug>` flows.
fn setup_wiki_with_tools_category() -> TempDir {
    let tmp = setup_wiki_in_git_repo();
    let schema_toml = "[wiki]\nname = \"Test Wiki\"\ndefault_review_days = 90\n\n[tags]\ncategories = [\"architecture\", \"training\", \"infra\", \"tools\", \"product\", \"ops\"]\n\n[categories.tools]\nrequired_fields = [\"title\", \"tags\"]\ntemplate = \"## Overview\\n\\n## Usage\\n\"\n";
    fs::write(tmp.path().join(".lw/schema.toml"), schema_toml).unwrap();
    // Re-stage the schema change so HEAD is clean for the test.
    StdCommand::new("git")
        .args(["add", "-A"])
        .current_dir(tmp.path())
        .output()
        .expect("git add");
    StdCommand::new("git")
        .args(["commit", "-m", "schema"])
        .current_dir(tmp.path())
        .output()
        .expect("git commit");
    tmp
}

/// Last commit subject (`git log -1 --format=%s`), trimmed.
fn head_subject(repo: &Path) -> String {
    let out = StdCommand::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Last commit body (`git log -1 --format=%b`).
fn head_body(repo: &Path) -> String {
    let out = StdCommand::new("git")
        .args(["log", "-1", "--format=%b"])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Number of commits on the current branch.
fn commit_count(repo: &Path) -> u32 {
    let out = StdCommand::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim().parse().unwrap_or(0)
}

// ─── lw new — auto-commits by default ────────────────────────────────────────

#[test]
fn lw_new_auto_commits_by_default() {
    let tmp = setup_wiki_with_tools_category();
    let root = tmp.path();
    let before = commit_count(root);

    lw().args([
        "new",
        "tools/auto-foo",
        "--title",
        "Auto Foo",
        "--tags",
        "rust,cli",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    assert!(root.join("wiki/tools/auto-foo.md").exists());
    assert_eq!(
        commit_count(root),
        before + 1,
        "lw new must produce exactly one new commit"
    );

    // Conventional commits subject + generator metadata in the body.
    let subject = head_subject(root);
    assert!(
        subject.starts_with("docs(wiki): create"),
        "subject should start with 'docs(wiki): create', got: {subject}"
    );
    assert!(
        subject.contains("tools/auto-foo.md"),
        "subject should contain page slug; got: {subject}"
    );

    let body = head_body(root);
    assert!(
        body.contains("generator: lw v"),
        "body should contain 'generator: lw v…'; got: {body}"
    );
}

// ─── lw new --no-commit → write succeeds, no commit ──────────────────────────

#[test]
fn lw_new_no_commit_skips_commit() {
    let tmp = setup_wiki_with_tools_category();
    let root = tmp.path();
    let before = commit_count(root);

    lw().args([
        "new",
        "tools/no-commit",
        "--title",
        "Plain",
        "--tags",
        "x",
        "--no-commit",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    assert!(root.join("wiki/tools/no-commit.md").exists());
    assert_eq!(
        commit_count(root),
        before,
        "with --no-commit, no new commit must be created"
    );
}

// ─── lw new --author overrides commit author ─────────────────────────────────

#[test]
fn lw_new_author_flag_sets_commit_author() {
    let tmp = setup_wiki_with_tools_category();
    let root = tmp.path();

    lw().args([
        "new",
        "tools/who-wrote",
        "--title",
        "Who",
        "--tags",
        "x",
        "--author",
        "Bob <bob@example.com>",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    let out = StdCommand::new("git")
        .args(["log", "-1", "--format=%an <%ae>"])
        .current_dir(root)
        .output()
        .unwrap();
    let line = String::from_utf8_lossy(&out.stdout);
    assert_eq!(line.trim(), "Bob <bob@example.com>");

    // Body should include author: line.
    let body = head_body(root);
    assert!(
        body.contains("author: Bob <bob@example.com>"),
        "body should record author; got: {body}"
    );
}

// ─── lw write — overwrite mode auto-commits ──────────────────────────────────

#[test]
fn lw_write_overwrite_auto_commits() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    let before = commit_count(root);
    // Use `--content=…` (single arg) so clap doesn't see the `---` frontmatter
    // marker as a separator.
    lw().args([
        "write",
        "architecture/edited.md",
        "--mode",
        "overwrite",
        "--content=---\ntitle: Edited\ntags: [t]\n---\n\nbody\n",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    assert_eq!(commit_count(root), before + 1);
    let subject = head_subject(root);
    assert!(
        subject.starts_with("docs(wiki): update"),
        "subject should start with 'docs(wiki): update', got: {subject}"
    );
    assert!(
        subject.contains("architecture/edited.md"),
        "subject should contain page slug, got: {subject}"
    );
}

// ─── lw write --mode append uses 'append' action ─────────────────────────────

#[test]
fn lw_write_append_uses_append_action() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    // First create the page so we have a section to append to.
    fs::write(
        root.join("wiki/architecture/section.md"),
        "---\ntitle: Section\ntags: [t]\n---\n\n## Notes\n\noriginal\n",
    )
    .unwrap();
    StdCommand::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "stage"])
        .current_dir(root)
        .output()
        .unwrap();

    let before = commit_count(root);

    lw().args([
        "write",
        "architecture/section.md",
        "--mode",
        "append",
        "--section",
        "Notes",
        "--content",
        "appended line",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    assert_eq!(commit_count(root), before + 1);
    let subject = head_subject(root);
    assert!(
        subject.starts_with("docs(wiki): append"),
        "subject should start with 'docs(wiki): append', got: {subject}"
    );
}

// ─── lw write --no-commit ────────────────────────────────────────────────────

#[test]
fn lw_write_no_commit_skips_commit() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();
    let before = commit_count(root);

    lw().args([
        "write",
        "architecture/uncommitted.md",
        "--mode",
        "overwrite",
        "--content=---\ntitle: U\ntags: [t]\n---\n\nbody\n",
        "--no-commit",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    assert!(root.join("wiki/architecture/uncommitted.md").exists());
    assert_eq!(commit_count(root), before, "no new commit expected");
}

// ─── lw ingest auto-commits the raw/ file ────────────────────────────────────

#[test]
fn lw_ingest_auto_commits() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();
    let source = root.join("source.md");
    fs::write(&source, "# Source\n\nbody\n").unwrap();

    let before = commit_count(root);

    lw().args([
        "ingest",
        source.to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--category",
        "architecture",
        "--yes",
    ])
    .assert()
    .success();

    assert!(root.join("raw/articles/source.md").exists());
    assert_eq!(commit_count(root), before + 1, "ingest should auto-commit");

    let subject = head_subject(root);
    assert!(
        subject.starts_with("docs(wiki): ingest"),
        "subject should start with 'docs(wiki): ingest', got: {subject}"
    );
}

// ─── Auto-commit is a no-op outside a git repo (no error, no commit) ─────────

#[test]
fn lw_new_outside_git_repo_skips_silently() {
    // A wiki that's not inside any git repo. We don't init_repo here.
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    let schema_toml = "[wiki]\nname = \"Test Wiki\"\ndefault_review_days = 90\n\n[tags]\ncategories = [\"architecture\", \"tools\"]\n\n[categories.tools]\nrequired_fields = [\"title\"]\ntemplate = \"\"\n";
    fs::write(tmp.path().join(".lw/schema.toml"), schema_toml).unwrap();

    lw().args([
        "new",
        "tools/no-git",
        "--title",
        "No Git",
        "--root",
        tmp.path().to_str().unwrap(),
    ])
    .assert()
    .success();

    assert!(tmp.path().join("wiki/tools/no-git.md").exists());
    // No .git dir should have been created.
    assert!(
        !tmp.path().join(".git").exists(),
        "must not init a git repo on the user's behalf"
    );
}

// ─── Dirty tree elsewhere → warning to stderr, not an error ──────────────────

#[test]
fn lw_new_with_dirty_tree_warns_but_succeeds() {
    let tmp = setup_wiki_with_tools_category();
    let root = tmp.path();

    // Create unrelated dirty file (not staged).
    fs::write(root.join("dirty.txt"), "junk").unwrap();

    let assert = lw()
        .args([
            "new",
            "tools/with-dirt",
            "--title",
            "With Dirt",
            "--tags",
            "x",
            "--root",
            root.to_str().unwrap(),
        ])
        .assert()
        .success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("dirty") || stderr.to_lowercase().contains("uncommitted"),
        "stderr should warn about dirty tree; got: {stderr}"
    );

    // The unrelated file must still be untracked — auto-commit must have
    // limited itself to the wiki page.
    let status = StdCommand::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&status.stdout);
    assert!(
        s.contains("dirty.txt"),
        "dirty.txt must remain dirty after lw new; got: {s}"
    );
}

// ─── lw new --push pushes to remote ──────────────────────────────────────────

#[test]
fn lw_new_push_flag_pushes_to_remote() {
    // Stand up a bare repo as the remote.
    let bare = TempDir::new().unwrap();
    StdCommand::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .current_dir(bare.path())
        .output()
        .unwrap();

    // Init the wiki + git, wire up the remote.
    let tmp = setup_wiki_with_tools_category();
    let root = tmp.path();
    StdCommand::new("git")
        .args(["remote", "add", "origin", bare.path().to_str().unwrap()])
        .current_dir(root)
        .output()
        .unwrap();
    // Seed an initial push so we have an upstream-tracked branch.
    StdCommand::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(root)
        .output()
        .unwrap();

    lw().args([
        "new",
        "tools/pushed",
        "--title",
        "Pushed",
        "--tags",
        "x",
        "--push",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    // Verify the new commit is on the remote (bare repo).
    let log = StdCommand::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(bare.path())
        .output()
        .unwrap();
    let subject = String::from_utf8_lossy(&log.stdout);
    assert!(
        subject.contains("create") && subject.contains("tools/pushed.md"),
        "remote HEAD should be the new wiki commit; got: {subject}"
    );
}

// ─── lw sync — pull-rebase + push ────────────────────────────────────────────

#[test]
fn lw_sync_pull_rebase_then_push() {
    // Bare remote + two clones.
    let bare = TempDir::new().unwrap();
    StdCommand::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .current_dir(bare.path())
        .output()
        .unwrap();

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    lw().args(["init", "--root", root.to_str().unwrap()])
        .assert()
        .success();
    init_repo(root);
    StdCommand::new("git")
        .args(["remote", "add", "origin", bare.path().to_str().unwrap()])
        .current_dir(root)
        .output()
        .unwrap();
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
    StdCommand::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(root)
        .output()
        .unwrap();

    // Make a local-only commit.
    fs::write(root.join("wiki/architecture/sync.md"), "x").unwrap();
    StdCommand::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "local"])
        .current_dir(root)
        .output()
        .unwrap();

    // `lw sync` should push the local commit upstream.
    lw().args(["sync", "--root", root.to_str().unwrap()])
        .assert()
        .success();

    // Bare HEAD should now show the local commit.
    let log = StdCommand::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(bare.path())
        .output()
        .unwrap();
    let subject = String::from_utf8_lossy(&log.stdout);
    assert!(
        subject.trim() == "local",
        "remote HEAD should be the local commit after lw sync; got: {subject}"
    );
}

// ─── lw sync errors when not a git repo ──────────────────────────────────────

#[test]
fn lw_sync_errors_when_not_a_git_repo() {
    let tmp = TempDir::new().unwrap();
    lw().args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    // No git init.
    lw().args(["sync", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not a git repository")
                .or(predicate::str::contains("not a git repo")),
        );
}

// ─── lw sync --force uses --force-with-lease ─────────────────────────────────

#[test]
fn lw_sync_force_succeeds() {
    // Force push with lease should still work in a happy-path setup.
    let bare = TempDir::new().unwrap();
    StdCommand::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .current_dir(bare.path())
        .output()
        .unwrap();

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    lw().args(["init", "--root", root.to_str().unwrap()])
        .assert()
        .success();
    init_repo(root);
    StdCommand::new("git")
        .args(["remote", "add", "origin", bare.path().to_str().unwrap()])
        .current_dir(root)
        .output()
        .unwrap();
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
    StdCommand::new("git")
        .args(["push", "-u", "origin", "main"])
        .current_dir(root)
        .output()
        .unwrap();

    fs::write(root.join("wiki/architecture/forced.md"), "x").unwrap();
    StdCommand::new("git")
        .args(["add", "-A"])
        .current_dir(root)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "force me"])
        .current_dir(root)
        .output()
        .unwrap();

    lw().args(["sync", "--force", "--root", root.to_str().unwrap()])
        .assert()
        .success();
}

// ─── --help advertises new commands and flags ────────────────────────────────

#[test]
fn write_help_lists_no_commit_push_author() {
    let out = lw().args(["write", "--help"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("--no-commit"), "help should list --no-commit");
    assert!(text.contains("--push"), "help should list --push");
    assert!(text.contains("--author"), "help should list --author");
}

#[test]
fn sync_help_present() {
    lw().args(["sync", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Examples"))
        .stdout(predicate::str::contains("--force"));
}

#[test]
fn top_level_help_lists_sync() {
    lw().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("sync"));
}
