//! Thin CLI shim around `lw_core::git::auto_commit`.
//!
//! Centralises the "wiki write succeeded → maybe commit + push" policy
//! so the `new`, `write`, and `ingest` subcommands all behave identically:
//! same default (commit on, push off), same commit message format, same
//! handling of dirty trees and non-git directories.

use lw_core::git::{AutoCommitOpts, CommitAction, auto_commit};
use std::path::{Path, PathBuf};

/// Convenience options bag the CLI subcommands populate from clap args.
pub struct AutoCommitFlags<'a> {
    pub no_commit: bool,
    pub push: bool,
    pub author: Option<&'a str>,
    pub source: Option<&'a str>,
}

/// Run the auto-commit policy for a CLI subcommand and surface any
/// stderr-worthy output. Returns Err iff the commit or push *failed*.
/// Non-git directories produce Ok(()) silently — that's the spec.
pub fn run_auto_commit(
    repo_root: &Path,
    paths: &[PathBuf],
    action: CommitAction,
    page_slug: &str,
    flags: AutoCommitFlags<'_>,
) -> anyhow::Result<()> {
    let opts = AutoCommitOpts {
        commit: !flags.no_commit,
        push: flags.push,
        author: flags.author,
        source: flags.source,
        generator_version: env!("CARGO_PKG_VERSION"),
    };
    let outcome = auto_commit(repo_root, paths, action, page_slug, opts)?;

    // Surface any dirty-elsewhere warning before the result message so
    // the user sees the warning even when the commit succeeded.
    if let Some(w) = &outcome.dirty_warning {
        eprintln!("{w}");
    }

    if outcome.committed {
        eprintln!("Committed {} ({})", page_slug, action_str(action));
    }
    if outcome.pushed {
        eprintln!("Pushed to remote.");
    }
    Ok(())
}

fn action_str(a: CommitAction) -> &'static str {
    match a {
        CommitAction::Create => "create",
        CommitAction::Update => "update",
        CommitAction::Append => "append",
        CommitAction::Upsert => "upsert",
        CommitAction::Ingest => "ingest",
    }
}
