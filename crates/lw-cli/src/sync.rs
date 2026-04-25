//! `lw sync` — `git pull --rebase` then `git push`.
//!
//! When `force == true`, the push uses `--force-with-lease` (safer than
//! `--force` — refuses to overwrite remote work the local hasn't seen).
//!
//! Bails out early with a clear message when the wiki root isn't inside
//! a git repo, since "sync" only makes sense for tracked vaults.

use lw_core::git::{is_git_repo, pull_rebase, push};
use std::path::Path;

pub fn run(root: &Path, force: bool) -> anyhow::Result<()> {
    if !is_git_repo(root) {
        anyhow::bail!(
            "not a git repository: {}\n  `lw sync` requires the vault to be inside a git repo.\n  Run: git init  (then add a remote, e.g. `git remote add origin <url>`)",
            root.display()
        );
    }

    eprintln!("Pulling with rebase…");
    pull_rebase(root)?;

    eprintln!("Pushing{}…", if force { " (force-with-lease)" } else { "" });
    push(root, force)?;

    eprintln!("Sync complete.");
    Ok(())
}
