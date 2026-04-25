//! `lw capture` subcommand stub (issue #37).
//!
//! Real implementation lands in the GREEN step.

use std::path::Path;

pub struct CommitOpts {
    pub no_commit: bool,
    pub push: bool,
    pub author: Option<String>,
}

pub fn run(
    _root: &Path,
    _content: &str,
    _tags: Vec<String>,
    _source: Option<String>,
    _commit_opts: CommitOpts,
) -> anyhow::Result<()> {
    unimplemented!("`lw capture` not yet implemented (#37)")
}
