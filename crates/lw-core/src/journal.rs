//! Journal-first quick capture (issue #37).
//!
//! Provides `append_capture` for filing a single timestamped entry into the
//! day's journal page (`wiki/_journal/YYYY-MM-DD.md`) and `find_stale_captures`
//! used by `lw lint` to flag unprocessed captures older than a threshold.
//!
//! Design rules (CLAUDE.md):
//! - Auto-creates the day's page (with frontmatter) if missing.
//! - All file writes go through `crate::fs::atomic_write` so concurrent
//!   readers (e.g. `lw serve`) never see a torn file.
//! - The function takes an injectable `(date, time)` pair so tests aren't
//!   wall-clock-dependent. Production callers pass the current local
//!   timestamp via `local_now()`.

use crate::fs::atomic_write;
use crate::{Result, WikiError};
use std::path::{Path, PathBuf};
use time::macros::format_description;
use time::{Date, OffsetDateTime, Time, UtcOffset};

/// Vault-relative directory where journal pages live.
pub const JOURNAL_DIR: &str = "_journal";

/// Default for `stale_after_days` when the schema does not configure
/// `[journal] stale_after_days = N`.
pub const DEFAULT_STALE_AFTER_DAYS: u32 = 7;

/// Outcome of `append_capture`.
#[derive(Debug)]
pub struct CaptureAppend {
    /// Absolute path to the journal page that received the capture.
    pub path: PathBuf,
    /// True iff the journal page was just auto-created.
    pub created: bool,
    /// The literal line appended (without the trailing newline). Useful for
    /// rendering the CLI/MCP success message.
    pub line: String,
    /// Vault-relative path (e.g. `wiki/_journal/2026-04-25.md`).
    pub display_path: String,
}

/// Append a capture entry to the journal page for `date`. Auto-creates the
/// page (with frontmatter) when it doesn't exist. The capture line carries
/// the `time` prefix in `HH:MM` form, the `content`, then optional tag
/// (`#rust`) and source (`([source](URL))`) suffixes.
///
/// `content` must not be empty (after trimming) — empty captures aren't
/// useful and would muddy the journal.
///
/// # Errors
///
/// - `WikiError::Internal` if `content.trim()` is empty.
/// - `WikiError::Io` for any filesystem failure.
#[tracing::instrument(skip(content, tags, source))]
pub fn append_capture(
    wiki_root: &Path,
    date: Date,
    time: Time,
    content: &str,
    tags: &[String],
    source: Option<&str>,
) -> Result<CaptureAppend> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(WikiError::Internal(
            "capture content must not be empty".to_string(),
        ));
    }

    let path = journal_path_for_date(wiki_root, date);
    let display_path = format!("wiki/{}/{}.md", JOURNAL_DIR, format_date_iso(date));

    let line = format_capture_line(time, trimmed, tags, source);

    // If the file doesn't exist, scaffold it with frontmatter +
    // ## Captures section + the new line. If it exists, append the line
    // to the end of the file (ensuring the file ends with a newline first).
    let (body, created) = if path.exists() {
        let existing = std::fs::read_to_string(&path)?;
        let mut out = existing;
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&line);
        out.push('\n');
        (out, false)
    } else {
        (scaffold_journal_page(date, &line), true)
    };

    atomic_write(&path, body.as_bytes())?;
    Ok(CaptureAppend {
        path,
        created,
        line,
        display_path,
    })
}

/// Format `date` as `YYYY-MM-DD`.
pub fn format_date_iso(date: Date) -> String {
    let fmt = format_description!("[year]-[month]-[day]");
    date.format(&fmt).unwrap_or_else(|_| {
        // Format string is statically valid; only OOM-class failures are
        // possible. Fall back to manual formatting if the impossible happens.
        format!(
            "{:04}-{:02}-{:02}",
            date.year(),
            u8::from(date.month()),
            date.day()
        )
    })
}

/// Format `time` as `HH:MM` (24h).
pub fn format_time_hm(time: Time) -> String {
    let fmt = format_description!("[hour]:[minute]");
    time.format(&fmt)
        .unwrap_or_else(|_| format!("{:02}:{:02}", time.hour(), time.minute()))
}

/// Compute the vault-absolute path to the journal page for `date`.
pub fn journal_path_for_date(wiki_root: &Path, date: Date) -> PathBuf {
    wiki_root
        .join("wiki")
        .join(JOURNAL_DIR)
        .join(format!("{}.md", format_date_iso(date)))
}

/// Build the formatted capture line (no trailing newline).
///
/// Examples:
/// - `- **10:23** comrak can round-trip markdown via arena AST`
/// - `- **10:25** see docs.rs/comrak \`#rust\` \`#markdown\``
/// - `- **10:30** key insight ([source](https://example.com))`
pub fn format_capture_line(
    time: Time,
    content: &str,
    tags: &[String],
    source: Option<&str>,
) -> String {
    let mut line = format!("- **{}** {}", format_time_hm(time), content.trim());
    for tag in tags {
        let t = tag.trim().trim_start_matches('#');
        if !t.is_empty() {
            line.push_str(&format!(" `#{t}`"));
        }
    }
    if let Some(url) = source {
        let url = url.trim();
        if !url.is_empty() {
            line.push_str(&format!(" ([source]({url}))"));
        }
    }
    line
}

/// Scaffold a journal page with frontmatter and a `## Captures` section
/// containing the first entry.
///
/// We hand-build the YAML rather than going through `Page::to_markdown`
/// because the spec requires a `created: YYYY-MM-DD` field that the `Page`
/// struct doesn't carry (the project rule keeps time data in git, but the
/// spec example shows `created` literally in the journal frontmatter so
/// agents can read the date without filename parsing).
fn scaffold_journal_page(date: Date, first_line: &str) -> String {
    let date_str = format_date_iso(date);
    // Quote the title so YAML parsers treat the colons in the date string
    // as part of the value rather than nested mapping syntax.
    let yaml = format!(
        "title: \"{date}\"\ntags: [journal]\ncreated: {date}\n",
        date = date_str
    );
    format!("---\n{yaml}---\n\n## Captures\n\n{first_line}\n")
}

/// Best-effort: get the current local time. Falls back to UTC if the
/// system rejects the offset lookup (some sandboxed test runners).
pub fn local_now() -> OffsetDateTime {
    let utc = OffsetDateTime::now_utc();
    match UtcOffset::current_local_offset() {
        Ok(off) => utc.to_offset(off),
        Err(_) => utc,
    }
}

/// One stale-journal lint finding.
#[derive(Debug, Clone)]
pub struct StaleJournalFinding {
    /// Wiki-relative path (e.g. `_journal/2026-04-15.md`).
    pub path: String,
    /// Age in days at lint time (since the journal page's last commit).
    pub age_days: i64,
}

/// Find journal pages whose last git commit is older than `threshold_days`.
/// Pages without git history are silently skipped (matches the rule used in
/// `lint::run_lint`).
///
/// Returns the findings sorted oldest-first.
pub fn find_stale_captures(
    wiki_root: &Path,
    threshold_days: u32,
) -> Result<Vec<StaleJournalFinding>> {
    let journal_dir = wiki_root.join("wiki").join(JOURNAL_DIR);
    if !journal_dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&journal_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file()
            || path.extension().is_none_or(|e| e != "md")
            || path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        {
            continue;
        }
        let age = match age_days_in_repo(wiki_root, &path) {
            Some(a) => a,
            None => continue, // no git history: not actionable
        };
        if age > threshold_days as i64 {
            let rel = path
                .strip_prefix(wiki_root.join("wiki"))
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string());
            out.push(StaleJournalFinding {
                path: rel,
                age_days: age,
            });
        }
    }
    out.sort_by(|a, b| b.age_days.cmp(&a.age_days));
    Ok(out)
}

/// `crate::git::page_age_days` doesn't pin its working directory, so it
/// queries the cwd's git repo — useless when callers (CLI, MCP, lint)
/// are running in some other directory. This helper sets `current_dir`
/// to `wiki_root` so the lookup hits the right repo even when the
/// process cwd points elsewhere (e.g. tests running from the worktree).
fn age_days_in_repo(wiki_root: &Path, abs_path: &Path) -> Option<i64> {
    let out = std::process::Command::new("git")
        .args([
            "log",
            "--follow",
            "-1",
            "--format=%at",
            "--",
            abs_path.to_str()?,
        ])
        .current_dir(wiki_root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let ts: i64 = String::from_utf8(out.stdout).ok()?.trim().parse().ok()?;
    if ts == 0 {
        return None;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some((now - ts) / 86400)
}
