use crate::output::Format;
use std::path::Path;

/// Create a new wiki page with schema-enforced frontmatter and body template.
///
/// # Arguments
///
/// - `path_arg`: `"<category>/<slug>"` (split on first `/`)
/// - `title`: page title (required when schema demands it)
/// - `tags`: comma-separated tag list
/// - `author`: optional author name
/// - `format`: output format
/// - `root`: resolved wiki root directory
///
/// # Errors
///
/// Propagates `WikiError` variants as `anyhow::Error`, printing to stderr and
/// exiting with code 1 via the standard error path in `main.rs`.
///
/// # Examples
///
/// ```bash
/// lw new tools/comrak-ast-parser --title "Comrak AST Parser" --tags rust,markdown,parsing
/// # => wrote wiki/tools/comrak-ast-parser.md
///
/// lw new tools/comrak-ast-parser --title "..." --format json
/// # => {"path": "wiki/tools/comrak-ast-parser.md", "category": "tools", "slug": "comrak-ast-parser"}
///
/// lw new tools/foo --title "Foo"
/// # error: category tools requires field: tags
///
/// lw new tools/comrak-ast-parser --title "..." --tags rust
/// # error: page already exists: wiki/tools/comrak-ast-parser.md
/// ```
pub fn run(
    root: &Path,
    path_arg: &str,
    title: Option<String>,
    tags: Option<String>,
    author: Option<String>,
    format: &Format,
) -> anyhow::Result<()> {
    // RED stub — always errors
    let _ = (root, path_arg, title, tags, author, format);
    anyhow::bail!("new_page CLI not implemented yet (RED stub)")
}
