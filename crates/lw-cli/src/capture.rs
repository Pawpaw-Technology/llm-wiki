//! `lw capture` subcommand (issue #37).
//!
//! Files a single timestamped entry into today's journal page at
//! `wiki/_journal/YYYY-MM-DD.md`, auto-creating the page on first use,
//! then runs the project's auto-commit policy with `CommitAction::Capture`.

use crate::git_commit::{AutoCommitFlags, run_auto_commit};
use lw_core::git::CommitAction;
use lw_core::journal::{append_capture, local_now};
use std::path::Path;

/// Auto-commit options forwarded from the CLI parser.
pub struct CommitOpts {
    pub no_commit: bool,
    pub push: bool,
    pub author: Option<String>,
}

pub fn run(
    root: &Path,
    content: &str,
    tags: Vec<String>,
    source: Option<String>,
    commit_opts: CommitOpts,
) -> anyhow::Result<()> {
    // Pre-flight: refuse empty captures with a clear error before we
    // touch the filesystem. `append_capture` also rejects this, but the
    // earlier exit produces a tidier error message in CLI mode.
    if content.trim().is_empty() {
        anyhow::bail!("capture content must not be empty (provide a non-blank message)");
    }

    let now = local_now();
    let date = now.date();
    let time = now.time();

    let outcome = append_capture(root, date, time, content, &tags, source.as_deref())?;

    eprintln!(
        "{} {} ({})",
        if outcome.created {
            "Created"
        } else {
            "Appended to"
        },
        outcome.display_path,
        outcome.line.trim_start_matches("- "),
    );

    // Auto-commit (issue #38). Page slug for the commit subject is the
    // vault-relative path, so reviewers can see the date at a glance.
    run_auto_commit(
        root,
        std::slice::from_ref(&outcome.path),
        CommitAction::Capture,
        &outcome.display_path,
        AutoCommitFlags {
            no_commit: commit_opts.no_commit,
            push: commit_opts.push,
            author: commit_opts.author.as_deref(),
            source: source.as_deref(),
        },
    )?;

    Ok(())
}
