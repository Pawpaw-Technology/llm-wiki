use crate::git_commit::{AutoCommitFlags, run_auto_commit};
use crate::output::Format;
use lw_core::fs::{NewPageRequest, load_schema, new_page};
use lw_core::git::CommitAction;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct NewPageOutput {
    path: String,
    category: String,
    slug: String,
}

/// Auto-commit options forwarded from the CLI parser.
pub struct CommitOpts {
    pub no_commit: bool,
    pub push: bool,
    pub source: Option<String>,
}

/// Create a new wiki page with schema-enforced frontmatter and body template.
///
/// # Errors
///
/// Propagates `WikiError` variants as `anyhow::Error`. The caller in `main.rs`
/// prints the error message to stderr and exits with code 1.
pub fn run(
    root: &Path,
    path_arg: &str,
    title: Option<String>,
    tags: Option<String>,
    author: Option<String>,
    format: &Format,
    commit_opts: CommitOpts,
) -> anyhow::Result<()> {
    // Split "<category>/<slug>" on the first '/'
    let (category, slug) = match path_arg.split_once('/') {
        Some(pair) => pair,
        None => {
            anyhow::bail!("path must be '<category>/<slug>' (e.g. tools/my-page), got: {path_arg}")
        }
    };

    // Load wiki schema
    let schema = load_schema(root)?;

    // Parse comma-separated tags
    let parsed_tags: Vec<String> = match &tags {
        Some(t) if !t.trim().is_empty() => t
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    };

    // Keep author around for the commit step too — `req` consumes it.
    let author_for_commit = author.clone();

    let req = NewPageRequest {
        category,
        slug,
        title: title.unwrap_or_default(),
        tags: parsed_tags,
        author,
    };

    let (abs_path, _page) = new_page(root, &schema, req)?;

    // Compute a wiki-relative display path: strip the wiki_root prefix
    let display_path = abs_path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| abs_path.to_string_lossy().into_owned());

    // Auto-commit (issue #38). Hand `commit_paths` the *absolute* page
    // path so it can re-resolve against the actual git toplevel — the
    // wiki root is allowed to be a subdir of a larger repo.
    run_auto_commit(
        root,
        std::slice::from_ref(&abs_path),
        CommitAction::Create,
        &display_path,
        AutoCommitFlags {
            no_commit: commit_opts.no_commit,
            push: commit_opts.push,
            author: author_for_commit.as_deref(),
            source: commit_opts.source.as_deref(),
        },
    )?;

    match format {
        Format::Json => {
            let out = NewPageOutput {
                path: display_path,
                category: category.to_string(),
                slug: slug.to_string(),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&out).expect("serialization cannot fail")
            );
        }
        Format::Human | Format::Brief => {
            println!("wrote {display_path}");
        }
    }

    Ok(())
}
