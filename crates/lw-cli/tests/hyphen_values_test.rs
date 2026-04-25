//! CLI integration tests for issue #96: leading-dash values in --content args.
//!
//! Both `lw write --content "- [[bar]]"` (space form) and
//! `lw write --content=- [[bar]]` (equals form) were rejected by clap's
//! default parser because it treats leading-dash values as unknown flags.
//!
//! Fix: `#[arg(allow_hyphen_values = true)]` on WriteArgs::content,
//! IngestArgs::content (new field), and Capture::content (positional).

use assert_cmd::Command;
use std::path::Path;
use std::process::Command as StdCommand;
use tempfile::TempDir;

fn lw() -> Command {
    Command::cargo_bin("lw").unwrap()
}

/// Init a git repo with sane defaults so commits don't fail.
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

/// `lw init` + `git init` + baseline commit.
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
        .expect("git add scaffold");
    StdCommand::new("git")
        .args(["commit", "-m", "seed"])
        .current_dir(root)
        .output()
        .expect("git commit seed");
    tmp
}

// ─── lw write: space form (primary regression) ───────────────────────────────

/// Regression #96: `lw write --content "- [[bar]]"` (space-separated form)
/// must succeed. Before the fix clap rejected it with exit 2
/// "error: unexpected argument '- ' found".
#[test]
fn write_content_leading_dash_space_form() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    // Create the page first.
    std::fs::write(
        root.join("wiki/architecture/target.md"),
        "---\ntitle: Target\ntags: [t]\n---\n\n## Related\n\noriginal\n",
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

    // Space form: --content "- [[bar]]"
    lw().args([
        "write",
        "architecture/target.md",
        "--mode",
        "upsert_section",
        "--section",
        "Related",
        "--content",
        "- [[bar]]",
        "--no-commit",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    // Verify the content was actually written.
    let contents = std::fs::read_to_string(root.join("wiki/architecture/target.md")).unwrap();
    assert!(
        contents.contains("- [[bar]]"),
        "page should contain '- [[bar]]', got:\n{contents}"
    );
}

// ─── lw write: equals form ────────────────────────────────────────────────────

/// Regression #96: `lw write --content=- [[bar]]` (equals form, leading dash)
/// must succeed. Equals form already works with clap even without
/// allow_hyphen_values, but we test it to ensure both forms are stable.
#[test]
fn write_content_leading_dash_equals_form() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    // Create the page first.
    std::fs::write(
        root.join("wiki/architecture/equals-target.md"),
        "---\ntitle: Equals Target\ntags: [t]\n---\n\n## Related\n\noriginal\n",
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

    // Equals form: --content=- [[bar]]
    lw().args([
        "write",
        "architecture/equals-target.md",
        "--mode",
        "upsert_section",
        "--section",
        "Related",
        "--content=- [[bar]]",
        "--no-commit",
        "--root",
        root.to_str().unwrap(),
    ])
    .assert()
    .success();

    let contents =
        std::fs::read_to_string(root.join("wiki/architecture/equals-target.md")).unwrap();
    assert!(
        contents.contains("- [[bar]]"),
        "page should contain '- [[bar]]', got:\n{contents}"
    );
}

// ─── lw ingest --content: space form ─────────────────────────────────────────

/// Regression #96: `lw ingest --content "# Heading\n- bullet"` (space form)
/// must not be rejected by clap. Before the fix --content didn't exist on
/// ingest at all, so the test also documents the new --content flag.
///
/// Also verifies the content actually lands on disk in `raw/articles/<slug>.md`.
#[test]
fn ingest_content_space_form_with_markdown() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    // Space form with inline markdown content (starts with '#').
    lw().args([
        "--root",
        root.to_str().unwrap(),
        "ingest",
        "--content",
        "# Inline Article\n\n- bullet point\n",
        "--title",
        "Inline Article",
        "--category",
        "architecture",
        "--raw-type",
        "articles",
        "--yes",
        "--no-commit",
    ])
    .assert()
    .success();

    // Read back the produced raw file and verify the heading landed on disk.
    // `lw ingest` writes raw files to `raw/<raw-type>/<slug>.md`.
    // slug_from_title_or_h1("Inline Article", ...) → slugify("Inline Article") → "inline-article"
    let raw_path = root.join("raw/articles/inline-article.md");
    let contents = std::fs::read_to_string(&raw_path).unwrap_or_else(|e| {
        panic!(
            "raw file not found at {}: {e}\n(ingest --content did not write the file)",
            raw_path.display()
        )
    });
    assert!(
        contents.contains("# Inline Article"),
        "raw file should contain '# Inline Article', got:\n{contents}"
    );
    assert!(
        contents.contains("- bullet point"),
        "raw file should contain '- bullet point', got:\n{contents}"
    );
}

// ─── lw ingest --content: equals form (leading dash) ─────────────────────────

/// Regression #96: `lw ingest --content=- list item` (equals form, leading dash)
/// must not be rejected by clap.
///
/// Also verifies the leading-dash content actually lands on disk in
/// `raw/articles/<slug>.md`.
#[test]
fn ingest_content_leading_dash_equals_form() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    // Equals form with leading dash in content.
    lw().args([
        "--root",
        root.to_str().unwrap(),
        "ingest",
        "--content=- first bullet\n- second bullet\n",
        "--title",
        "Bullet List",
        "--category",
        "architecture",
        "--raw-type",
        "articles",
        "--yes",
        "--no-commit",
    ])
    .assert()
    .success();

    // Read back the produced raw file and verify the leading-dash content landed.
    // slug_from_title_or_h1("Bullet List", ...) → slugify("Bullet List") → "bullet-list"
    let raw_path = root.join("raw/articles/bullet-list.md");
    let contents = std::fs::read_to_string(&raw_path).unwrap_or_else(|e| {
        panic!(
            "raw file not found at {}: {e}\n(ingest --content did not write the file)",
            raw_path.display()
        )
    });
    assert!(
        contents.contains("- first bullet"),
        "raw file should contain '- first bullet', got:\n{contents}"
    );
    assert!(
        contents.contains("- second bullet"),
        "raw file should contain '- second bullet', got:\n{contents}"
    );
}

// ─── lw ingest: --stdin and --content are mutually exclusive ─────────────────

/// Regression guard: passing both --stdin and --content must be rejected by
/// clap with a conflict error (exit 2), not silently drop --content.
#[test]
fn ingest_stdin_and_content_conflict_is_rejected() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    let output = lw()
        .args([
            "--root",
            root.to_str().unwrap(),
            "ingest",
            "--stdin",
            "--content",
            "x",
            "--raw-type",
            "articles",
        ])
        .output()
        .unwrap();

    // clap exits 2 for argument errors.
    assert_eq!(
        output.status.code(),
        Some(2),
        "--stdin + --content should exit with code 2 (clap conflict error), got: {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("content") || stderr.contains("stdin"),
        "clap error message should mention the conflicting args, got:\n{stderr}"
    );
}

// ─── lw capture: positional arg with leading dash ────────────────────────────

/// Regression #96: `lw capture "- a note"` (positional arg, leading dash)
/// was also rejected by clap's default parser. The fix adds
/// `allow_hyphen_values = true` to the capture content positional arg.
#[test]
fn capture_content_with_leading_dash() {
    let tmp = setup_wiki_in_git_repo();
    let root = tmp.path();

    lw().args([
        "--root",
        root.to_str().unwrap(),
        "capture",
        "- a note with leading dash",
        "--no-commit",
    ])
    .assert()
    .success();
}
