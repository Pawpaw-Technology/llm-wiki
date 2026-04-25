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

/// Unix epoch seconds of the **first** git commit that introduced this file
/// (`git log --follow --reverse --format=%at -- <path> | head -1`).
/// Returns `None` if the file has no git history (untracked or not in a repo).
///
/// Used by `lw query --sort created_desc/created_asc` (issue #41) to order
/// hits by creation time. The search index doesn't have access to git, so
/// the CLI does this lookup post-hoc on the result set.
///
/// Anchors `git` to the file's parent directory via `-C`, so the lookup
/// works regardless of the calling process's cwd. Without that anchor,
/// `lw serve` (whose cwd is whatever the agent launched it from) and the
/// MCP unit tests (cwd = workspace root) would both see `git: not a git
/// repository` and silently return `None` for every page.
#[tracing::instrument]
pub fn page_first_commit_time(path: &Path) -> Result<Option<i64>> {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => return Ok(None),
    };
    // `-C <dir>` makes git locate the repo by walking up from `dir`. Use
    // the file's parent so a file in a sub-directory of the repo still
    // resolves correctly.
    let cwd = path.parent().and_then(|p| p.to_str()).unwrap_or(".");
    let output = Command::new("git")
        .args([
            "-C",
            cwd,
            "log",
            "--follow",
            "--reverse",
            "--format=%at",
            "--",
            path_str,
        ])
        .output()
        .map_err(|e| WikiError::Internal(format!("git log failed: {e}")))?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| WikiError::Internal(format!("git output not utf-8: {e}")))?;
    // `git log --reverse` lists oldest first; take the first non-empty line.
    let first = stdout.lines().find(|l| !l.trim().is_empty());
    let ts = match first {
        Some(l) => l.trim().parse::<i64>().ok(),
        None => None,
    };
    Ok(ts.filter(|&t| t > 0))
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
/// `paths` may be absolute or relative; `commit_paths` normalises them
/// against the actual git toplevel internally:
/// - absolute paths under the toplevel are stripped to toplevel-relative
///   form (handles wiki_root being a subdir of a larger repo);
/// - relative paths are passed through unchanged (interpreted by git as
///   relative to the toplevel, since we run all commands from there).
///
/// `author` is an optional `"Name <email>"` string. A bare name without
/// `<email>` is accepted — `commit_paths` synthesises a placeholder email
/// matching `parse_author` so git accepts the `--author` flag. When set,
/// both author and committer of the new commit are forced to that
/// identity; otherwise git uses the configured `user.name`/`user.email`.
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

    // Normalise every input path to toplevel-relative form. `git add` and
    // `git commit -- <paths>` run from the toplevel, so an absolute path
    // outside the toplevel would error and a wiki-relative path (which
    // assumes wiki_root == toplevel) would fail with "pathspec did not
    // match any files" when wiki_root is a subdir of the actual repo.
    let toplevel_paths: Vec<PathBuf> = paths
        .iter()
        .map(|p| normalize_against_toplevel(p, &toplevel))
        .collect();

    // Stage each requested path. Use `git add --` to defang any name that
    // happens to start with a dash.
    let mut add_cmd = Command::new("git");
    add_cmd.args(["add", "--"]).current_dir(&toplevel);
    for p in &toplevel_paths {
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
        //
        // Git rejects bare names like "Just A Name" with
        //   fatal: --author 'Just A Name' is not 'Name <email>'
        // so we always synthesize a complete `Name <email>` form using
        // the same placeholder email parse_author uses for the
        // committer envs. That way both headers stay in lock-step.
        let (name, email) = parse_author(a);
        let synthesized = format!("{name} <{email}>");
        commit_cmd.arg(format!("--author={synthesized}"));
        commit_cmd.env("GIT_COMMITTER_NAME", name);
        commit_cmd.env("GIT_COMMITTER_EMAIL", email);
    }

    commit_cmd.arg("--");
    for p in &toplevel_paths {
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

/// Normalise `p` to a toplevel-relative path. Absolute paths under the
/// toplevel are stripped to relative form; everything else (including
/// already-relative paths) passes through unchanged. Used by
/// `commit_paths` to handle the wiki_root != git_toplevel case (see
/// issue #38) — callers can hand over absolute paths and we make sure
/// `git add` / `git commit -- <paths>` see them in the toplevel-relative
/// form git expects.
fn normalize_against_toplevel(p: &Path, toplevel: &Path) -> PathBuf {
    if p.is_absolute()
        && let Ok(rel) = p.strip_prefix(toplevel)
    {
        return rel.to_path_buf();
    }
    p.to_path_buf()
}

/// Best-effort parse of `"Name <email>"` into `(name, email)`. If no `<` is
/// present, treat the whole string as the name and use a placeholder email
/// so git doesn't refuse the commit. We use this both for setting the
/// committer envs and for synthesising a complete `Name <email>` string
/// when the user supplies a bare name to `--author`.
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

// ─── High-level auto-commit policy (CLI / MCP shared) ───────────────────────

/// Action recorded in the conventional-commit subject. The string forms
/// (`"create"`, `"update"`, `"append"`, `"upsert"`, `"ingest"`, `"capture"`)
/// match the terms specified in issues #38 and #37.
#[derive(Debug, Clone, Copy)]
pub enum CommitAction {
    Create,
    Update,
    Append,
    Upsert,
    Ingest,
    /// Quick-capture journal entry (`lw capture` / `wiki_capture`, issue #37).
    Capture,
}

impl CommitAction {
    fn as_str(self) -> &'static str {
        match self {
            CommitAction::Create => "create",
            CommitAction::Update => "update",
            CommitAction::Append => "append",
            CommitAction::Upsert => "upsert",
            CommitAction::Ingest => "ingest",
            CommitAction::Capture => "capture",
        }
    }
}

/// Caller-supplied options for `auto_commit`.
///
/// `commit` and `push` use bare `bool` (not `Option<bool>`) — the CLI / MCP
/// layer is responsible for converting their CLI flags / MCP args into a
/// concrete decision before calling here.
#[derive(Debug)]
pub struct AutoCommitOpts<'a> {
    pub commit: bool,
    pub push: bool,
    pub author: Option<&'a str>,
    pub source: Option<&'a str>,
    /// Generator string injected into the commit body, e.g.
    /// `env!("CARGO_PKG_VERSION")` from the call site. Stored verbatim
    /// after `generator: lw v`.
    pub generator_version: &'a str,
}

/// Outcome surfaced back to the CLI/MCP layer for user-facing messaging.
#[derive(Debug, Default)]
pub struct AutoCommitOutcome {
    /// True iff a new commit was created.
    pub committed: bool,
    /// True iff `git push` was invoked successfully.
    pub pushed: bool,
    /// `Some(_)` when the working tree had uncommitted changes outside
    /// the paths that were just written. The string is suitable for
    /// printing to stderr.
    pub dirty_warning: Option<String>,
}

/// Build the conventional-commit subject + body for an auto-commit.
///
/// Subject: `docs(wiki): <action> <page-slug>`.
/// Body lines: `generator: lw v<X.Y.Z>`, optional `author:`, optional
/// `source:`. Per the issue spec these go in the *trailer* of the body.
pub fn build_commit_message(
    action: CommitAction,
    page_slug: &str,
    opts: &AutoCommitOpts<'_>,
) -> String {
    let mut msg = format!("docs(wiki): {} {}\n\n", action.as_str(), page_slug);
    msg.push_str(&format!("generator: lw v{}\n", opts.generator_version));
    if let Some(a) = opts.author {
        msg.push_str(&format!("author: {a}\n"));
    }
    if let Some(s) = opts.source {
        msg.push_str(&format!("source: {s}\n"));
    }
    msg
}

/// Run the auto-commit policy: optionally commit `paths`, optionally push.
///
/// Behavior at a glance (matches issue #38 acceptance criteria):
/// - If `repo_root` is not a git repo → returns Ok(default) with
///   `committed = false`. No error.
/// - If `opts.commit == false` → also Ok(default), no commit.
/// - If the working tree has dirty files OUTSIDE the supplied paths,
///   `dirty_warning` is set. The commit still happens (issue spec: warn,
///   don't error) and only includes the supplied paths.
/// - If `opts.push == true` and the commit succeeded, also runs
///   `git push`. A push failure surfaces as `WikiError::Git` so the
///   caller can show it; the commit is *not* rolled back.
///
/// `paths` are interpreted by `commit_paths` — see that function for the
/// path-resolution rules.
#[tracing::instrument(skip(paths, opts))]
pub fn auto_commit(
    repo_root: &Path,
    paths: &[PathBuf],
    action: CommitAction,
    page_slug: &str,
    opts: AutoCommitOpts<'_>,
) -> Result<AutoCommitOutcome> {
    let mut outcome = AutoCommitOutcome::default();

    // Non-git directories are a graceful no-op (acceptance criterion 7).
    if !is_git_repo(repo_root) {
        return Ok(outcome);
    }

    // Caller asked us not to commit (acceptance criterion 3).
    if !opts.commit {
        return Ok(outcome);
    }

    // Detect "dirty elsewhere" before staging our paths. This is a
    // best-effort check — the commit still proceeds either way.
    if let Some(warning) = dirty_elsewhere_warning(repo_root, paths) {
        outcome.dirty_warning = Some(warning);
    }

    let message = build_commit_message(action, page_slug, &opts);
    commit_paths(repo_root, paths, &message, opts.author)?;
    outcome.committed = true;

    if opts.push {
        push(repo_root, false)?;
        outcome.pushed = true;
    }

    Ok(outcome)
}

/// Returns true if `path_part` (a porcelain-output path fragment) is an
/// ephemeral `.lw/` artifact that should be silently excluded from the
/// dirty-elsewhere warning.
///
/// Ephemeral paths (issue #97):
/// - `.lw/search/*`   — Tantivy index files; fully regenerable, never user content.
/// - `.lw/backlinks/.built` — sentinel written by `rebuild_index`; local-only.
///
/// Note: `.lw/backlinks/*.json` sidecar files are NOT ephemeral — they carry
/// the link-evolution audit trail and are auto-committed alongside the page
/// (Option A per issue #97). They are intentionally NOT filtered here.
fn is_lw_ephemeral(path_part: &str) -> bool {
    // Tantivy index files: anything under .lw/search/
    // The constant crate::INDEX_DIR == ".lw/search".
    let index_prefix = format!("{}/", crate::INDEX_DIR);
    if path_part.starts_with(&index_prefix) || path_part.contains(&format!("/{index_prefix}")) {
        return true;
    }
    // Backlinks built sentinel: .lw/backlinks/.built
    // Matches regardless of leading directory, to handle wiki-root-in-subdir.
    // crate::backlinks::BACKLINKS_DIR == ".lw/backlinks"
    let sentinel_suffix = format!("{}/{}", crate::backlinks::BACKLINKS_DIR, ".built");
    if path_part == sentinel_suffix || path_part.ends_with(&format!("/{sentinel_suffix}")) {
        return true;
    }
    false
}

/// Compose the dirty-elsewhere warning, if any. Compares
/// `git status --porcelain` against the supplied paths and returns
/// `Some(message)` when there are dirty files that aren't being committed.
///
/// Ephemeral `.lw/` artifacts (Tantivy index files under `.lw/search/` and
/// the backlinks built sentinel `.lw/backlinks/.built`) are silently excluded
/// from the warning — see `is_lw_ephemeral`. This covers both fresh vaults
/// (where `.gitignore` excludes them) and existing vaults that may have
/// accidentally tracked these paths before the fix.
fn dirty_elsewhere_warning(repo_root: &Path, paths: &[PathBuf]) -> Option<String> {
    let toplevel = resolve_toplevel(repo_root).ok()?;
    // `--untracked-files=all` forces individual file listings; without it
    // git collapses fully-untracked directories (e.g. `?? wiki/`) and our
    // suffix-matching can't tell whether the targeted page lives inside
    // that collapse — see the dirty_warning_suppresses_… test below.
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(&toplevel)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.is_empty() {
        return None;
    }

    // Normalise our supplied paths to plain string suffixes for matching
    // against `git status` output.
    let our_paths: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    let mut other_dirty: Vec<String> = Vec::new();
    for line in stdout.lines() {
        // `git status --porcelain` lines look like `XY <path>` (rename uses
        // `XY <old> -> <new>`). Strip the 2 status chars + 1 space.
        let path_part = line.get(3..).unwrap_or("").trim();
        if path_part.is_empty() {
            continue;
        }
        // Silently skip ephemeral .lw/ artifacts (Tantivy index, built sentinel).
        if is_lw_ephemeral(path_part) {
            continue;
        }
        // Naive check: skip if any of our supplied paths matches the trailing
        // segment of the dirty path (or vice versa). Covers both repo-relative
        // and toplevel-relative names.
        let ours = our_paths.iter().any(|p| {
            path_part == p
                || path_part.ends_with(&format!("/{p}"))
                || p.ends_with(&format!("/{path_part}"))
        });
        if !ours {
            other_dirty.push(path_part.to_string());
        }
    }

    if other_dirty.is_empty() {
        return None;
    }
    let preview = other_dirty
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let extra = if other_dirty.len() > 3 {
        format!(" (+{} more)", other_dirty.len() - 3)
    } else {
        String::new()
    };
    Some(format!(
        "warning: working tree has uncommitted changes elsewhere ({preview}{extra}); only the wiki page was committed"
    ))
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

    // ─── Reviewer-flagged fix: --author "Name Only" (no <email>) ─────────────
    //
    // Git rejects `--author "Just A Name"` with
    //   fatal: --author 'Just A Name' is not 'Name <email>' …
    // and exits 128. `commit_paths` was passing the user's literal
    // `Name Only` straight through. The fix is to synthesize a
    // `Name <email>` form (matching `parse_author`'s placeholder email)
    // before handing the string to `git commit --author=`.

    #[test]
    fn commit_paths_with_name_only_author_synthesizes_email() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);

        fs::write(root.join("auth.md"), "x").unwrap();

        // Caller supplied a bare name — no `<email>`. Must NOT fail.
        commit_paths(
            root,
            &[PathBuf::from("auth.md")],
            "docs(wiki): create auth.md",
            Some("Just A Name"),
        )
        .expect("commit must succeed even when --author has no <email>");

        // The committed author should be the synthesized full form
        // (name + placeholder email matching parse_author).
        let log = Command::new("git")
            .args(["log", "-1", "--format=%an <%ae>"])
            .current_dir(root)
            .output()
            .unwrap();
        let line = String::from_utf8_lossy(&log.stdout);
        let trimmed = line.trim();
        assert!(
            trimmed.starts_with("Just A Name <") && trimmed.ends_with('>'),
            "synthesized author should be 'Just A Name <some-email>'; got: {trimmed}"
        );
    }

    // ─── Reviewer-flagged fix: wiki_root != git_toplevel path resolution ─────
    //
    // When the wiki root is a subdir of a larger repo, the CLI/MCP layer
    // strips `wiki_root` from the absolute page path, producing a
    // *wiki-relative* path. `commit_paths` was running `git add` from the
    // git toplevel with that relative path, which never resolves. The fix
    // is to accept absolute paths and re-strip them against the actual
    // git toplevel inside `commit_paths` itself.

    #[test]
    fn commit_paths_handles_wiki_subdir_of_outer_repo() {
        // Outer git repo has `vault/` as a subdir; wiki lives at vault/.
        let tmp = TempDir::new().unwrap();
        let outer = tmp.path();
        init_repo(outer);

        // Seed an outer-root commit so HEAD exists for diff checks.
        fs::write(outer.join("seed.md"), "seed").unwrap();
        Command::new("git")
            .args(["add", "seed.md"])
            .current_dir(outer)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "seed"])
            .current_dir(outer)
            .output()
            .unwrap();

        // Wiki root inside the outer repo.
        let vault = outer.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let page_abs = vault.join("page.md");
        fs::write(&page_abs, "page body").unwrap();

        // Pass the ABSOLUTE page path. commit_paths must figure out
        // the toplevel-relative form internally.
        commit_paths(
            &vault,
            std::slice::from_ref(&page_abs),
            "docs(wiki): create vault/page.md",
            None,
        )
        .expect("commit_paths must succeed when wiki root is a subdir");

        // Verify a new commit exists at the OUTER toplevel and contains
        // the toplevel-relative path `vault/page.md`.
        let log_files = Command::new("git")
            .args(["log", "-1", "--name-only", "--format="])
            .current_dir(outer)
            .output()
            .unwrap();
        let names = String::from_utf8_lossy(&log_files.stdout);
        assert!(
            names.lines().any(|l| l.trim() == "vault/page.md"),
            "commit should include toplevel-relative path vault/page.md; got: {names}"
        );
    }

    #[test]
    fn commit_paths_accepts_repo_relative_path_when_root_is_toplevel() {
        // Regression guard: existing repo-relative callers must still work.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);

        fs::write(root.join("note.md"), "n").unwrap();

        commit_paths(
            root,
            &[PathBuf::from("note.md")],
            "docs(wiki): create note.md",
            None,
        )
        .expect("repo-relative path at toplevel must still work");

        let log = Command::new("git")
            .args(["log", "-1", "--name-only", "--format="])
            .current_dir(root)
            .output()
            .unwrap();
        let names = String::from_utf8_lossy(&log.stdout);
        assert!(
            names.lines().any(|l| l.trim() == "note.md"),
            "expected note.md in commit; got: {names}"
        );
    }

    /// Build a repo with `README.md` tracked + the target page only, so the
    /// dirty-warning function sees `?? wiki/` (collapsed) — the case that
    /// previously false-positive-warned on a fresh `lw init` + `lw new`.
    fn repo_with_only_target_dirty(root: &Path) {
        init_repo(root);
        fs::write(root.join("README.md"), "x").unwrap();
        let out = Command::new("git")
            .args(["add", "README.md"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(out.status.success());
        let out = Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(out.status.success(), "commit init: {out:?}");
        fs::create_dir_all(root.join("wiki/tools")).unwrap();
        fs::write(root.join("wiki/tools/foo.md"), "page").unwrap();
    }

    #[test]
    fn dirty_warning_suppresses_when_only_dirty_thing_is_target_in_collapsed_dir() {
        // Regression for #38 reviewer-noted false positive: `git status --porcelain`
        // collapses an untracked dir (`?? wiki/`) when no tracked files live
        // under it. Without `--untracked-files=all`, suffix-matching couldn't
        // see that `wiki/tools/foo.md` lives under that collapse and the
        // function would warn even though no OTHER file was dirty.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        repo_with_only_target_dirty(root);

        let warning = dirty_elsewhere_warning(root, &[PathBuf::from("wiki/tools/foo.md")]);
        assert!(
            warning.is_none(),
            "false-positive warning when only the target is dirty: {warning:?}"
        );
    }

    #[test]
    fn dirty_warning_fires_when_truly_other_dirty_files_present() {
        // Regression guard: the false-positive fix must not silence the
        // warning when there really are unrelated dirty files.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        repo_with_only_target_dirty(root);
        fs::write(root.join("scratch.txt"), "draft").unwrap();

        let warning = dirty_elsewhere_warning(root, &[PathBuf::from("wiki/tools/foo.md")]);
        let w = warning.expect("warning expected when scratch.txt is also dirty");
        assert!(
            w.contains("scratch.txt"),
            "warning must mention the other dirty file; got: {w}"
        );
    }

    // ─── Issue #97: .lw/ ephemeral paths must not contribute to dirty-warning ──
    //
    // `.lw/search/*` (Tantivy index) and `.lw/backlinks/.built` (sentinel) are
    // regenerable artifacts of the wiki tooling itself, not user content.  The
    // dirty-elsewhere warning must silently skip them regardless of git-ignore
    // state — this covers existing vaults that may already have these tracked.

    #[test]
    fn dirty_warning_ignores_lw_search_files() {
        // Set up a repo where `.lw/search/segment.idx` appears as untracked
        // alongside the wiki page being committed. The warning must be None
        // because the ONLY other dirty entry is an ephemeral .lw/search/ file.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);

        // Seed a baseline commit so HEAD exists.
        fs::write(root.join("README.md"), "x").unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        // Simulate the Tantivy index artifacts under .lw/search/
        fs::create_dir_all(root.join(".lw/search")).unwrap();
        fs::write(root.join(".lw/search/segment.idx"), "tantivy").unwrap();
        fs::write(root.join(".lw/search/meta.json"), "{}").unwrap();

        // Also create the wiki page being committed.
        fs::create_dir_all(root.join("wiki/tools")).unwrap();
        fs::write(root.join("wiki/tools/bar.md"), "page").unwrap();

        let warning = dirty_elsewhere_warning(root, &[PathBuf::from("wiki/tools/bar.md")]);
        assert!(
            warning.is_none(),
            ".lw/search/* must not trigger dirty-warning; got: {warning:?}"
        );
    }

    #[test]
    fn dirty_warning_ignores_lw_backlinks_built_sentinel() {
        // `.lw/backlinks/.built` is the sentinel for the backlink index.
        // It must be silently ignored in the dirty-warning.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);

        fs::write(root.join("README.md"), "x").unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        // Simulate the backlinks built sentinel.
        fs::create_dir_all(root.join(".lw/backlinks")).unwrap();
        fs::write(root.join(".lw/backlinks/.built"), "").unwrap();

        // Wiki page being committed.
        fs::create_dir_all(root.join("wiki/tools")).unwrap();
        fs::write(root.join("wiki/tools/baz.md"), "page").unwrap();

        let warning = dirty_elsewhere_warning(root, &[PathBuf::from("wiki/tools/baz.md")]);
        assert!(
            warning.is_none(),
            ".lw/backlinks/.built must not trigger dirty-warning; got: {warning:?}"
        );
    }

    #[test]
    fn dirty_warning_fires_for_non_lw_dirty_file_positive_control() {
        // Positive control: even when .lw/ ephemeral paths are present, an
        // unrelated dirty file outside .lw/ must STILL trigger the warning.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        init_repo(root);

        fs::write(root.join("README.md"), "x").unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .unwrap();

        // Ephemeral .lw/ paths — these must be ignored.
        fs::create_dir_all(root.join(".lw/search")).unwrap();
        fs::write(root.join(".lw/search/segment.idx"), "tantivy").unwrap();
        fs::create_dir_all(root.join(".lw/backlinks")).unwrap();
        fs::write(root.join(".lw/backlinks/.built"), "").unwrap();

        // The unrelated dirty file that SHOULD trigger the warning.
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/other-page.md"), "draft").unwrap();

        // Wiki page being committed.
        fs::create_dir_all(root.join("wiki/tools")).unwrap();
        fs::write(root.join("wiki/tools/qux.md"), "page").unwrap();

        let warning = dirty_elsewhere_warning(root, &[PathBuf::from("wiki/tools/qux.md")]);
        let w = warning.expect("warning expected for wiki/other-page.md");
        assert!(
            w.contains("other-page.md"),
            "warning must mention the non-.lw dirty file; got: {w}"
        );
        // The ephemeral .lw/ paths must NOT appear in the warning.
        assert!(
            !w.contains(".lw/search"),
            "warning must NOT mention .lw/search; got: {w}"
        );
        assert!(
            !w.contains(".built"),
            "warning must NOT mention .lw/backlinks/.built; got: {w}"
        );
    }
}
