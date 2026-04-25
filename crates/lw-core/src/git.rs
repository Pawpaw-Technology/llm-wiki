use crate::{Result, WikiError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Get the age in days of a file based on its last git commit.
/// Returns None if not in a git repo or file has no git history.
#[tracing::instrument]
pub fn page_age_days(path: &Path) -> Option<i64> {
    let output = Command::new("git")
        .args([
            "log",
            "--follow",
            "-1",
            "--format=%at",
            "--",
            path.to_str()?,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let ts: i64 = String::from_utf8(output.stdout).ok()?.trim().parse().ok()?;
    if ts == 0 {
        return None;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some((now - ts) / 86400)
}

/// Freshness level of a wiki page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshnessLevel {
    Fresh,
    Suspect,
    Stale,
}

impl std::fmt::Display for FreshnessLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FreshnessLevel::Fresh => write!(f, "fresh"),
            FreshnessLevel::Suspect => write!(f, "suspect"),
            FreshnessLevel::Stale => write!(f, "stale"),
        }
    }
}

impl FreshnessLevel {
    /// Returns a human-readable suffix like " [stale]" or empty for fresh.
    pub fn suffix(&self) -> &'static str {
        match self {
            FreshnessLevel::Fresh => "",
            FreshnessLevel::Suspect => " [suspect]",
            FreshnessLevel::Stale => " [stale]",
        }
    }
}

/// Compute a page's freshness from its file path.
/// Reads the page to get the decay field, then checks git history for age.
/// Returns `Fresh` if the file has no git history.
pub fn page_freshness(abs_path: &Path, default_review_days: u32) -> FreshnessLevel {
    let decay = crate::fs::read_page(abs_path)
        .ok()
        .and_then(|p| p.decay)
        .unwrap_or_else(|| "normal".to_string());
    match page_age_days(abs_path) {
        Some(days) => compute_freshness(&decay, days, default_review_days),
        None => FreshnessLevel::Fresh,
    }
}

/// Compute freshness from decay level and age.
/// - fast: stale after 30 days
/// - normal: stale after `default_days` (usually 90)
/// - evergreen: never stale by time
#[tracing::instrument]
pub fn compute_freshness(decay: &str, age_days: i64, default_days: u32) -> FreshnessLevel {
    let threshold = match decay {
        "fast" => 30,
        "evergreen" => return FreshnessLevel::Fresh,
        _ => default_days as i64,
    };

    if age_days > threshold {
        FreshnessLevel::Stale
    } else if age_days > threshold * 3 / 4 {
        FreshnessLevel::Suspect
    } else {
        FreshnessLevel::Fresh
    }
}

// ─── Write helpers (auto-commit support, issue #38) ──────────────────────────
//
// These wrap `git` via `std::process::Command` to match the existing
// read-only helper above (`page_age_days`). No `git2` or external crates.
//
// Naming convention: every helper takes the *repo root* (not a vault subdir),
// so callers must hand off a directory containing `.git/` (or one whose
// `git rev-parse --is-inside-work-tree` would say yes). The CLI/MCP layer
// is responsible for picking the right directory — usually the wiki root,
// but the wiki root is allowed to be a subdir of a larger repo.

/// Returns true if `path` (a directory) is inside a git working tree.
///
/// Uses `git rev-parse --is-inside-work-tree`. If git is missing, the path
/// is not a directory, or the command fails for any reason, returns false
/// — auto-commit then becomes a graceful no-op.
#[tracing::instrument]
pub fn is_git_repo(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.trim() == "true"
        }
        _ => false,
    }
}

/// Returns true if the working tree at `repo_root` has any uncommitted
/// changes (modified, added, deleted, or untracked files in `git status
/// --porcelain`). Returns false when the repo is clean, or when the path
/// is not a git repo (delegating that decision to `is_git_repo`).
#[tracing::instrument]
pub fn is_dirty(repo_root: &Path) -> bool {
    if !is_git_repo(repo_root) {
        return false;
    }
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output();
    match output {
        Ok(o) if o.status.success() => !o.stdout.is_empty(),
        _ => false,
    }
}

/// Run the absolute path of `repo_root` through `git rev-parse --show-toplevel`
/// to find the actual repo root, in case `repo_root` is a subdirectory.
fn resolve_toplevel(repo_root: &Path) -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(repo_root)
        .output()
        .map_err(|e| WikiError::Git(format!("failed to spawn git: {e}")))?;
    if !output.status.success() {
        return Err(WikiError::Git(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(s))
}

/// Stage `paths` and create a commit with `message`. The commit only
/// includes the supplied paths — other dirty files in the working tree
/// stay uncommitted, so a "dirty tree elsewhere" warning never accidentally
/// captures unrelated edits.
///
/// `paths` are interpreted relative to the *repo root*, not `repo_root` —
/// callers must pass paths already rooted at the toplevel (typically by
/// stripping the repo-root prefix from an absolute path first).
///
/// `author` is an optional `"Name <email>"` string. When `Some`, both the
/// author and committer of the new commit are set to it; otherwise git
/// uses the configured `user.name`/`user.email`.
///
/// Errors:
/// - `WikiError::Git("not a git repository: …")` if `repo_root` isn't tracked
/// - `WikiError::Git(stderr)` for any failed `git add` / `git commit`
#[tracing::instrument(skip(paths, message))]
pub fn commit_paths(
    repo_root: &Path,
    paths: &[PathBuf],
    message: &str,
    author: Option<&str>,
) -> Result<()> {
    if !is_git_repo(repo_root) {
        return Err(WikiError::Git(format!(
            "not a git repository: {}",
            repo_root.display()
        )));
    }
    if paths.is_empty() {
        return Err(WikiError::Git(
            "commit_paths requires at least one path".to_string(),
        ));
    }

    let toplevel = resolve_toplevel(repo_root)?;

    // Stage each requested path. Use `git add --` to defang any name that
    // happens to start with a dash. Paths are interpreted relative to the
    // git toplevel, so a caller can either hand us repo-relative paths or
    // absolute paths — git accepts both as long as they resolve under the
    // toplevel.
    let mut add_cmd = Command::new("git");
    add_cmd.args(["add", "--"]).current_dir(&toplevel);
    for p in paths {
        add_cmd.arg(p);
    }
    let add = add_cmd
        .output()
        .map_err(|e| WikiError::Git(format!("git add failed to spawn: {e}")))?;
    if !add.status.success() {
        return Err(WikiError::Git(format!(
            "git add failed: {}",
            String::from_utf8_lossy(&add.stderr)
        )));
    }

    // Build a `git commit` that ONLY commits the staged paths we just added.
    // Using `git commit -- <paths>` (with paths after `--`) ensures we don't
    // accidentally pull in unrelated staged changes.
    let mut commit_cmd = Command::new("git");
    commit_cmd.current_dir(&toplevel);
    commit_cmd.args(["commit", "-m", message]);

    if let Some(a) = author {
        // `--author` sets the author header; the committer is taken from
        // user.name / user.email. To make the *committer* match too,
        // override env for this child.
        commit_cmd.arg(format!("--author={a}"));
        let (name, email) = parse_author(a);
        commit_cmd.env("GIT_COMMITTER_NAME", name);
        commit_cmd.env("GIT_COMMITTER_EMAIL", email);
    }

    commit_cmd.arg("--");
    for p in paths {
        commit_cmd.arg(p);
    }

    let commit = commit_cmd
        .output()
        .map_err(|e| WikiError::Git(format!("git commit failed to spawn: {e}")))?;
    if !commit.status.success() {
        let err = String::from_utf8_lossy(&commit.stderr);
        let out = String::from_utf8_lossy(&commit.stdout);
        return Err(WikiError::Git(format!(
            "git commit failed: stdout={out} stderr={err}"
        )));
    }
    Ok(())
}

/// Best-effort parse of `"Name <email>"` into `(name, email)`. If no `<` is
/// present, treat the whole string as the name and use a placeholder email
/// so git doesn't refuse the commit. We only need this for setting the
/// committer envs — the author header itself is passed through unchanged.
fn parse_author(s: &str) -> (String, String) {
    if let Some(open) = s.find('<')
        && let Some(close) = s.find('>')
        && open < close
    {
        let name = s[..open].trim().to_string();
        let email = s[open + 1..close].trim().to_string();
        return (name, email);
    }
    (s.trim().to_string(), "noreply@local".to_string())
}

/// Run `git push` from `repo_root`. With `force_with_lease == true`,
/// passes `--force-with-lease` (safer than `--force` — refuses to overwrite
/// remote work the local hasn't seen).
#[tracing::instrument]
pub fn push(repo_root: &Path, force_with_lease: bool) -> Result<()> {
    if !is_git_repo(repo_root) {
        return Err(WikiError::Git(format!(
            "not a git repository: {}",
            repo_root.display()
        )));
    }
    let toplevel = resolve_toplevel(repo_root)?;
    let mut cmd = Command::new("git");
    cmd.current_dir(&toplevel);
    if force_with_lease {
        cmd.args(["push", "--force-with-lease"]);
    } else {
        cmd.arg("push");
    }
    let out = cmd
        .output()
        .map_err(|e| WikiError::Git(format!("git push failed to spawn: {e}")))?;
    if !out.status.success() {
        return Err(WikiError::Git(format!(
            "git push failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

/// Run `git pull --rebase` from `repo_root`.
///
/// Used by `lw sync` (issue #38). Returns Err on failure (e.g. conflicts,
/// no upstream configured) so the CLI can surface the underlying message.
#[tracing::instrument]
pub fn pull_rebase(repo_root: &Path) -> Result<()> {
    if !is_git_repo(repo_root) {
        return Err(WikiError::Git(format!(
            "not a git repository: {}",
            repo_root.display()
        )));
    }
    let toplevel = resolve_toplevel(repo_root)?;
    let out = Command::new("git")
        .args(["pull", "--rebase"])
        .current_dir(&toplevel)
        .output()
        .map_err(|e| WikiError::Git(format!("git pull --rebase failed to spawn: {e}")))?;
    if !out.status.success() {
        return Err(WikiError::Git(format!(
            "git pull --rebase failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Initialise a fresh repo at `path` with a known identity and a default
    /// branch. We set identity via `-c` flags on every command rather than
    /// touching the user's global git config.
    fn init_repo(path: &Path) {
        let out = Command::new("git")
            .args(["init", "--initial-branch=main"])
            .current_dir(path)
            .output()
            .expect("git init must run");
        assert!(out.status.success(), "git init failed: {:?}", out);
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output()
            .expect("git config user.name");
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(path)
            .output()
            .expect("git config user.email");
        // GPG signing must be off — CI runners may not have keys configured.
        Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(path)
            .output()
            .expect("git config commit.gpgsign");
    }

    #[test]
    fn is_git_repo_returns_true_inside_initialized_repo() {
        let tmp = TempDir::new().unwrap();
        init_repo(tmp.path());
        assert!(is_git_repo(tmp.path()));
    }

    #[test]
    fn is_git_repo_returns_false_for_plain_directory() {
        let tmp = TempDir::new().unwrap();
        assert!(!is_git_repo(tmp.path()));
    }

    #[test]
    fn is_git_repo_returns_false_for_nonexistent_path() {
        let p = PathBuf::from("/tmp/this/does/not/exist/in/practice/lw-test");
        assert!(!is_git_repo(&p));
    }

    #[test]
    fn is_dirty_false_on_clean_repo() {
        let tmp = TempDir::new().unwrap();
        init_repo(tmp.path());
        assert!(!is_dirty(tmp.path()));
    }

    #[test]
    fn is_dirty_true_with_untracked_file() {
        let tmp = TempDir::new().unwrap();
        init_repo(tmp.path());
        fs::write(tmp.path().join("hello.txt"), "hi").unwrap();
        assert!(is_dirty(tmp.path()));
    }

    #[test]
    fn is_dirty_false_for_non_git_dir() {
        let tmp = TempDir::new().unwrap();
        // No init_repo — should still return false (graceful)
        assert!(!is_dirty(tmp.path()));
    }

    #[test]
    fn commit_paths_creates_commit_with_message() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);

        let target = root.join("note.md");
        fs::write(&target, "hello world").unwrap();

        commit_paths(
            root,
            &[PathBuf::from("note.md")],
            "docs(wiki): create note.md\n\ngenerator: lw v0.0.0",
            None,
        )
        .expect("commit_paths must succeed");

        // The commit should now appear in `git log`.
        let log = Command::new("git")
            .args(["log", "-1", "--format=%s"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(log.status.success(), "git log failed: {:?}", log);
        let subject = String::from_utf8_lossy(&log.stdout);
        assert_eq!(subject.trim(), "docs(wiki): create note.md");

        // Body must include the generator metadata.
        let body = Command::new("git")
            .args(["log", "-1", "--format=%b"])
            .current_dir(root)
            .output()
            .unwrap();
        let body_str = String::from_utf8_lossy(&body.stdout);
        assert!(
            body_str.contains("generator: lw v0.0.0"),
            "commit body should contain generator line; got: {body_str}"
        );
    }

    #[test]
    fn commit_paths_only_commits_specified_paths() {
        // Regression for the "dirty tree elsewhere" warning: when a wiki write
        // happens in a repo that has unrelated dirty files, our auto-commit
        // must NOT capture them — only the path we asked for.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);

        // Seed a baseline commit so HEAD exists for diff checks.
        fs::write(root.join("seed.txt"), "seed").unwrap();
        Command::new("git")
            .args(["add", "seed.txt"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "seed"])
            .current_dir(root)
            .output()
            .unwrap();

        // Now have an unrelated dirty file plus the file we will commit.
        fs::write(root.join("unrelated.txt"), "junk").unwrap();
        fs::write(root.join("page.md"), "page body").unwrap();

        commit_paths(
            root,
            &[PathBuf::from("page.md")],
            "docs(wiki): create page.md",
            None,
        )
        .expect("commit must succeed");

        // `unrelated.txt` should still be dirty (untracked).
        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(root)
            .output()
            .unwrap();
        let s = String::from_utf8_lossy(&status.stdout);
        assert!(
            s.contains("unrelated.txt"),
            "unrelated dirty file must NOT be swept into the commit; got: {s}"
        );
        assert!(
            !s.contains("page.md"),
            "page.md should be committed cleanly; got: {s}"
        );
    }

    #[test]
    fn commit_paths_with_author_sets_author_header() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);

        fs::write(root.join("auth.md"), "x").unwrap();

        commit_paths(
            root,
            &[PathBuf::from("auth.md")],
            "docs(wiki): create auth.md",
            Some("Alice <alice@example.com>"),
        )
        .expect("commit must succeed");

        let log = Command::new("git")
            .args(["log", "-1", "--format=%an <%ae>"])
            .current_dir(root)
            .output()
            .unwrap();
        let line = String::from_utf8_lossy(&log.stdout);
        assert!(
            line.trim() == "Alice <alice@example.com>",
            "author should be Alice, got: {line}"
        );
    }

    #[test]
    fn commit_paths_errors_when_not_a_repo() {
        let tmp = TempDir::new().unwrap();
        // Don't init_repo — this is a plain dir.
        fs::write(tmp.path().join("foo.md"), "x").unwrap();

        let res = commit_paths(
            tmp.path(),
            &[PathBuf::from("foo.md")],
            "docs(wiki): create foo.md",
            None,
        );
        assert!(matches!(res, Err(WikiError::Git(_))));
    }

    #[test]
    fn push_succeeds_against_local_bare_remote() {
        // Use a local bare repo as the remote. Mirrors how integration tests
        // can validate `--push` without ever talking to a real GitHub.
        let work = TempDir::new().unwrap();
        let bare = TempDir::new().unwrap();

        // Init the bare remote.
        let bare_init = Command::new("git")
            .args(["init", "--bare", "--initial-branch=main"])
            .current_dir(bare.path())
            .output()
            .unwrap();
        assert!(bare_init.status.success());

        // Init the working repo and wire up the bare remote.
        init_repo(work.path());
        let add_remote = Command::new("git")
            .args(["remote", "add", "origin", bare.path().to_str().unwrap()])
            .current_dir(work.path())
            .output()
            .unwrap();
        assert!(add_remote.status.success());

        // Make a commit so push has something to send.
        fs::write(work.path().join("a.txt"), "hi").unwrap();
        commit_paths(
            work.path(),
            &[PathBuf::from("a.txt")],
            "docs(wiki): seed",
            None,
        )
        .unwrap();

        // First push needs an upstream — set it via `git push -u origin main`
        // ourselves, then `lw_core::git::push` should be a plain `git push`.
        let setup_push = Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(work.path())
            .output()
            .unwrap();
        assert!(
            setup_push.status.success(),
            "initial upstream push failed: {:?}",
            setup_push
        );

        // Make another commit and call our push helper.
        fs::write(work.path().join("b.txt"), "again").unwrap();
        commit_paths(
            work.path(),
            &[PathBuf::from("b.txt")],
            "docs(wiki): again",
            None,
        )
        .unwrap();

        push(work.path(), false).expect("push must succeed against local bare remote");
    }

    #[test]
    fn push_errors_when_not_a_repo() {
        let tmp = TempDir::new().unwrap();
        let res = push(tmp.path(), false);
        assert!(matches!(res, Err(WikiError::Git(_))));
    }

    #[test]
    fn pull_rebase_errors_when_not_a_repo() {
        let tmp = TempDir::new().unwrap();
        let res = pull_rebase(tmp.path());
        assert!(matches!(res, Err(WikiError::Git(_))));
    }

    #[test]
    fn pull_rebase_succeeds_with_remote_changes() {
        // Stand up: bare remote, working clone A, working clone B.
        // B commits and pushes; A pulls — must succeed (fast-forward rebase).
        let bare = TempDir::new().unwrap();
        let clone_a = TempDir::new().unwrap();
        let clone_b = TempDir::new().unwrap();

        // Bare remote.
        Command::new("git")
            .args(["init", "--bare", "--initial-branch=main"])
            .current_dir(bare.path())
            .output()
            .unwrap();

        // Seed clone B with content + push to bare.
        init_repo(clone_b.path());
        Command::new("git")
            .args(["remote", "add", "origin", bare.path().to_str().unwrap()])
            .current_dir(clone_b.path())
            .output()
            .unwrap();
        fs::write(clone_b.path().join("seed.txt"), "seed").unwrap();
        commit_paths(
            clone_b.path(),
            &[PathBuf::from("seed.txt")],
            "docs(wiki): seed",
            None,
        )
        .unwrap();
        Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(clone_b.path())
            .output()
            .unwrap();

        // Clone A from the bare remote.
        let clone_a_status = Command::new("git")
            .args(["clone", bare.path().to_str().unwrap(), "."])
            .current_dir(clone_a.path())
            .output()
            .unwrap();
        assert!(clone_a_status.status.success());
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(clone_a.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(clone_a.path())
            .output()
            .unwrap();

        // Push a new commit from B.
        fs::write(clone_b.path().join("two.txt"), "two").unwrap();
        commit_paths(
            clone_b.path(),
            &[PathBuf::from("two.txt")],
            "docs(wiki): two",
            None,
        )
        .unwrap();
        push(clone_b.path(), false).unwrap();

        // Now A pull-rebases — should succeed cleanly.
        pull_rebase(clone_a.path()).expect("pull --rebase must succeed");

        assert!(clone_a.path().join("two.txt").exists());
    }
}
