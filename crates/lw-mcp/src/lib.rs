//! MCP server for LLM Wiki.
//! Provides wiki_query, wiki_read, wiki_browse, wiki_tags, wiki_write, wiki_ingest, wiki_lint, wiki_stats tools.

use lw_core::fs::{
    NewPageRequest, atomic_write, category_from_path, list_pages, new_page, read_page,
    validate_wiki_path, write_page,
};
use lw_core::git::{self, AutoCommitOpts, CommitAction, FreshnessLevel, auto_commit};
use lw_core::ingest;
use lw_core::page::Page;
use lw_core::schema::WikiSchema;
use lw_core::search::{SearchQuery, Searcher, TantivySearcher};
use lw_core::status::gather_status;
use lw_core::tag::Taxonomy;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Caller-supplied auto-commit fields, packed to keep `mcp_auto_commit`
/// from tripping the `clippy::too_many_arguments` lint.
struct McpCommitArgs<'a> {
    commit: Option<bool>,
    push: Option<bool>,
    author: Option<&'a str>,
    source: Option<&'a str>,
}

/// Outcome of the MCP-side auto-commit. Either an error response (tool
/// must propagate as-is) or success with an optional dirty-tree warning
/// the tool should surface to the agent in its JSON response.
enum McpCommitResult {
    Err(String),
    Ok { dirty_warning: Option<String> },
}

/// Append a `warnings: [...]` field to a tool's success JSON when the
/// auto-commit reported a dirty-tree warning. No-op when the warning
/// is `None`, so clean-tree responses don't grow a noisy empty array.
fn attach_warnings(response: &mut serde_json::Value, dirty_warning: Option<String>) {
    if let Some(w) = dirty_warning
        && let Some(obj) = response.as_object_mut()
    {
        obj.insert("warnings".to_string(), serde_json::json!([w]));
    }
}

/// Run the auto-commit policy for an MCP write tool.
///
/// Returns `McpCommitResult::Err(error_json)` if the commit/push failed
/// — the caller should propagate it verbatim. Otherwise returns
/// `McpCommitResult::Ok` with `dirty_warning` populated when the working
/// tree had uncommitted changes outside the page being committed.
/// Tools must surface that warning in the JSON they return so the agent
/// can show it to the user (issue #38: previously logged via tracing
/// only, never seen by the agent).
fn mcp_auto_commit(
    repo_root: &Path,
    paths: &[PathBuf],
    action: CommitAction,
    page_slug: &str,
    args: McpCommitArgs<'_>,
) -> McpCommitResult {
    let opts = AutoCommitOpts {
        commit: args.commit.unwrap_or(true),
        push: args.push.unwrap_or(false),
        author: args.author,
        source: args.source,
        generator_version: env!("CARGO_PKG_VERSION"),
    };
    match auto_commit(repo_root, paths, action, page_slug, opts) {
        Ok(outcome) => {
            if let Some(w) = &outcome.dirty_warning {
                // Keep the tracing log for telemetry alongside the
                // JSON-surfaced field.
                tracing::warn!("{w}");
            }
            McpCommitResult::Ok {
                dirty_warning: outcome.dirty_warning,
            }
        }
        Err(e) => McpCommitResult::Err(
            serde_json::json!({"error": format!("auto-commit failed: {e}")}).to_string(),
        ),
    }
}

// === Tool argument structs ===

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiQueryArgs {
    /// Full-text search query
    pub query: String,
    /// Filter by tags (comma-separated)
    #[serde(default)]
    pub tags: Option<String>,
    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,
    /// Max results (default: 20)
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiReadArgs {
    /// Relative path within wiki/ (e.g. "architecture/transformer.md")
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiBrowseArgs {
    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,
    /// Filter by tag
    #[serde(default)]
    pub tag: Option<String>,
    /// Only show stale/suspect pages
    #[serde(default)]
    pub stale_only: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiTagsArgs {
    /// Filter by category (optional)
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiWriteArgs {
    /// Relative path within wiki/ (e.g. "architecture/new-page.md")
    pub path: String,
    /// Content to write. For "overwrite" mode: full markdown with YAML frontmatter.
    /// For "append_section"/"upsert_section": body fragment without frontmatter.
    pub content: String,
    /// Write mode: "overwrite" (default), "append_section", or "upsert_section"
    #[serde(default = "default_write_mode")]
    pub mode: String,
    /// Section name for append/upsert modes (case-insensitive, no ## prefix needed)
    #[serde(default)]
    pub section: Option<String>,
    /// Auto-commit after write (default: true). Set to false to skip the commit.
    #[serde(default)]
    pub commit: Option<bool>,
    /// Push after commit (default: false). Requires `commit` to be true.
    #[serde(default)]
    pub push: Option<bool>,
    /// Override commit author as `"Name <email>"`.
    #[serde(default)]
    pub author: Option<String>,
    /// Optional source URL/identifier recorded in the commit body as `source: <…>`.
    #[serde(default)]
    pub source: Option<String>,
}

fn default_write_mode() -> String {
    "overwrite".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiIngestArgs {
    /// Absolute path to source file. Mutually exclusive with `content`.
    #[serde(default)]
    pub source_path: Option<String>,
    /// Pasted markdown to file directly into raw/ — the MCP-native replacement
    /// for the CLI's `--stdin` flag. Mutually exclusive with `source_path`.
    #[serde(default)]
    pub content: Option<String>,
    /// Explicit filename (e.g. `my-note.md`) used when `content` is provided.
    /// Must not contain path separators. When omitted, the filename is derived
    /// from `title`, the first H1 in `content`, or `untitled.md` in that order.
    #[serde(default)]
    pub filename: Option<String>,
    /// Target subdirectory under raw/ (papers, articles, assets)
    #[serde(default = "default_raw_type")]
    pub raw_type: String,
    /// Suggested title
    #[serde(default)]
    pub title: Option<String>,
    /// Suggested tags (comma-separated)
    #[serde(default)]
    pub tags: Option<String>,
    /// Target category
    #[serde(default)]
    pub category: Option<String>,
    /// Auto-commit after ingest (default: true).
    #[serde(default)]
    pub commit: Option<bool>,
    /// Push after commit (default: false).
    #[serde(default)]
    pub push: Option<bool>,
    /// Override commit author as `"Name <email>"`.
    #[serde(default)]
    pub author: Option<String>,
    /// Optional source URL/identifier recorded in the commit body.
    #[serde(default)]
    pub source: Option<String>,
}

fn default_raw_type() -> String {
    "articles".to_string()
}

/// Build the success response body shared by both ingest paths. Returns
/// `Value` so callers can mutate the payload (e.g. attach a `warnings`
/// field) before serialising.
fn ingest_ok_value(raw_path: &std::path::Path, args: &WikiIngestArgs) -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "raw_path": raw_path.to_string_lossy(),
        "suggested_title": args.title,
        "suggested_tags": args.tags,
        "suggested_category": args.category,
        "next_step": "Use wiki_write to create the wiki page from this source material.",
    })
}

/// Pick the filename for a content-mode ingest. Priority: explicit
/// `filename` > slug(title) > slug(first H1) > `untitled.md`.
fn derive_content_filename(
    filename: Option<&str>,
    title: Option<&str>,
    content: &str,
) -> std::result::Result<String, String> {
    if let Some(f) = filename {
        let f = f.trim();
        if f.is_empty() {
            return Err("`filename` must not be empty".to_string());
        }
        if f.contains('/') || f.contains('\\') || f == ".." || f == "." {
            return Err(format!(
                "invalid filename `{f}`: must not contain path separators"
            ));
        }
        return Ok(f.to_string());
    }
    Ok(format!(
        "{}.md",
        ingest::slug_from_title_or_h1(title, content)
    ))
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiLintArgs {
    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiNewArgs {
    /// Target category (must match a category in the schema, e.g. "tools")
    pub category: String,
    /// URL-safe slug for the page (e.g. "comrak-ast-parser"); must match [a-z0-9_-]+
    pub slug: String,
    /// Human-readable page title
    pub title: String,
    /// Tags to attach to the page
    #[serde(default)]
    pub tags: Vec<String>,
    /// Author name (optional). When set, used both as the page's `author`
    /// frontmatter field AND as the commit author (`Name <email>` form).
    pub author: Option<String>,
    /// Auto-commit after creation (default: true).
    #[serde(default)]
    pub commit: Option<bool>,
    /// Push after commit (default: false).
    #[serde(default)]
    pub push: Option<bool>,
    /// Optional source URL/identifier recorded in the commit body.
    #[serde(default)]
    pub source: Option<String>,
}

// === Server ===

#[derive(Clone)]
pub struct WikiMcpServer {
    wiki_root: PathBuf,
    schema: WikiSchema,
    searcher: Arc<TantivySearcher>,
    default_review_days: u32,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl WikiMcpServer {
    /// Full-text search across wiki pages with optional tag/category filters.
    #[tool(
        name = "wiki_query",
        description = "Full-text search across wiki pages with optional tag/category filters. Returns matching pages with titles, paths, scores, and text snippets."
    )]
    fn wiki_query(&self, Parameters(args): Parameters<WikiQueryArgs>) -> String {
        let tags: Vec<String> = args
            .tags
            .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        let sq = SearchQuery {
            text: if args.query.is_empty() {
                None
            } else {
                Some(args.query)
            },
            tags,
            category: args.category,
            limit: args.limit.unwrap_or(20),
        };

        match self.searcher.search(&sq) {
            Ok(results) => {
                let hits: Vec<serde_json::Value> = results
                    .hits
                    .iter()
                    .map(|h| {
                        serde_json::json!({
                            "path": h.path,
                            "title": h.title,
                            "tags": h.tags,
                            "category": h.category,
                            "score": h.score,
                            "snippet": h.snippet,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "total": results.total,
                    "hits": hits,
                })
                .to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    /// Read a single wiki page by its relative path within wiki/.
    #[tool(
        name = "wiki_read",
        description = "Read a wiki page by its relative path within wiki/. Returns the full markdown content including YAML frontmatter, title, tags, and body."
    )]
    fn wiki_read(&self, Parameters(args): Parameters<WikiReadArgs>) -> String {
        let abs_path = match validate_wiki_path(&self.wiki_root, &args.path) {
            Ok(p) => p,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };
        match read_page(&abs_path) {
            Ok(page) => serde_json::json!({
                "path": args.path,
                "title": page.title,
                "tags": page.tags,
                "decay": page.decay,
                "sources": page.sources,
                "author": page.author,
                "generator": page.generator,
                "related": page.related,
                "body": page.body,
                "markdown": page.to_markdown(),
            })
            .to_string(),
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    /// List wiki pages, optionally filtered by category or tag.
    #[tool(
        name = "wiki_browse",
        description = "List wiki pages, optionally filtered by category or tag. Returns page paths, titles, tags, and categories. Use stale_only to see only pages needing updates."
    )]
    fn wiki_browse(&self, Parameters(args): Parameters<WikiBrowseArgs>) -> String {
        let wiki_dir = self.wiki_root.join("wiki");
        match list_pages(&wiki_dir) {
            Ok(pages) => {
                let stale_only = args.stale_only.unwrap_or(false);
                let mut entries: Vec<serde_json::Value> = Vec::new();

                for rel_path in &pages {
                    let cat = category_from_path(rel_path).unwrap_or_default();

                    // Category filter
                    if let Some(ref filter_cat) = args.category
                        && cat != *filter_cat
                    {
                        continue;
                    }

                    let abs_path = wiki_dir.join(rel_path);
                    let page = match read_page(&abs_path) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    // Tag filter
                    if let Some(ref filter_tag) = args.tag
                        && !page.tags.iter().any(|t| t == filter_tag)
                    {
                        continue;
                    }

                    // Stale filter
                    if stale_only {
                        let level = git::page_freshness(&abs_path, self.default_review_days);
                        if level == FreshnessLevel::Fresh {
                            continue;
                        }
                    }

                    entries.push(serde_json::json!({
                        "path": rel_path.to_string_lossy(),
                        "title": page.title,
                        "tags": page.tags,
                        "category": cat,
                    }));
                }

                serde_json::json!({
                    "count": entries.len(),
                    "pages": entries,
                })
                .to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    /// List all tags with their page counts.
    #[tool(
        name = "wiki_tags",
        description = "List all tags used across wiki pages with their page counts. Optionally filter by category to see only tags used in that category."
    )]
    fn wiki_tags(&self, Parameters(args): Parameters<WikiTagsArgs>) -> String {
        let wiki_dir = self.wiki_root.join("wiki");
        match list_pages(&wiki_dir) {
            Ok(page_paths) => {
                let mut loaded_pages: Vec<Page> = Vec::new();

                for rel_path in &page_paths {
                    // Category filter
                    if let Some(ref filter_cat) = args.category {
                        let cat = category_from_path(rel_path).unwrap_or_default();
                        if cat != *filter_cat {
                            continue;
                        }
                    }

                    let abs_path = wiki_dir.join(rel_path);
                    if let Ok(page) = read_page(&abs_path) {
                        loaded_pages.push(page);
                    }
                }

                let taxonomy = Taxonomy::from_pages(&loaded_pages);
                let counts = taxonomy.tag_counts();
                let tags: Vec<serde_json::Value> = counts
                    .iter()
                    .map(|(tag, count)| {
                        serde_json::json!({
                            "tag": tag,
                            "count": count,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "total_tags": tags.len(),
                    "tags": tags,
                })
                .to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    /// Write or update a wiki page with support for section-level operations.
    #[tool(
        name = "wiki_write",
        description = "Write or update a wiki page. Modes: 'overwrite' (default) replaces the entire page (content must include YAML frontmatter). 'append_section' appends content to a named section. 'upsert_section' replaces or creates a named section. Section matching is case-insensitive."
    )]
    fn wiki_write(&self, Parameters(args): Parameters<WikiWriteArgs>) -> String {
        let abs_path = match validate_wiki_path(&self.wiki_root, &args.path) {
            Ok(p) => p,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };

        // Display path stays wiki-relative for the commit subject; the
        // absolute path is what we hand to the auto-commit so it works
        // when the wiki root is a subdir of a larger git repo.
        let display_path = match abs_path.strip_prefix(&self.wiki_root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => abs_path.to_string_lossy().to_string(),
        };

        match args.mode.as_str() {
            "overwrite" => {
                let page = match Page::parse(&args.content) {
                    Ok(p) => p,
                    Err(e) => {
                        return serde_json::json!({"error": format!("Invalid page content: {e}")})
                            .to_string();
                    }
                };

                if let Err(e) = write_page(&abs_path, &page) {
                    return serde_json::json!({"error": format!("Failed to write page: {e}")})
                        .to_string();
                }

                if let Err(e) = self.searcher.index_page(&args.path, &page) {
                    tracing::warn!("Failed to index page {}: {}", args.path, e);
                }
                if let Err(e) = self.searcher.commit() {
                    tracing::warn!("Failed to commit index: {}", e);
                }

                let dirty_warning = match mcp_auto_commit(
                    &self.wiki_root,
                    std::slice::from_ref(&abs_path),
                    CommitAction::Update,
                    &display_path,
                    McpCommitArgs {
                        commit: args.commit,
                        push: args.push,
                        author: args.author.as_deref(),
                        source: args.source.as_deref(),
                    },
                ) {
                    McpCommitResult::Err(err) => return err,
                    McpCommitResult::Ok { dirty_warning } => dirty_warning,
                };

                let mut response = serde_json::json!({
                    "status": "ok",
                    "path": args.path,
                    "title": page.title,
                    "tags": page.tags,
                });
                attach_warnings(&mut response, dirty_warning);
                response.to_string()
            }

            "append_section" | "upsert_section" => {
                let section_name = match &args.section {
                    Some(s) if !s.is_empty() => s.as_str(),
                    _ => {
                        return serde_json::json!({
                            "error": format!("'section' is required for {} mode", args.mode)
                        })
                        .to_string();
                    }
                };

                let raw = match std::fs::read_to_string(&abs_path) {
                    Ok(r) => r,
                    Err(_) => {
                        return serde_json::json!({
                            "error": "page not found; use overwrite mode to create"
                        })
                        .to_string();
                    }
                };

                let (frontmatter, body) = lw_core::section::split_frontmatter(&raw);

                let write_result = if args.mode == "append_section" {
                    match lw_core::section::apply_append(body, section_name, &args.content) {
                        Some(result) => result,
                        None => {
                            // Empty content — noop
                            return serde_json::json!({
                                "status": "ok",
                                "path": args.path,
                                "mode": args.mode,
                                "section": section_name,
                                "noop": true,
                            })
                            .to_string();
                        }
                    }
                } else {
                    lw_core::section::apply_upsert(body, section_name, &args.content)
                };

                let assembled = format!("{frontmatter}{}", write_result.body);
                if let Err(e) = atomic_write(&abs_path, assembled.as_bytes()) {
                    return serde_json::json!({
                        "error": format!("Failed to write page: {e}")
                    })
                    .to_string();
                }

                let action = if args.mode == "append_section" {
                    CommitAction::Append
                } else {
                    CommitAction::Upsert
                };
                let dirty_warning = match mcp_auto_commit(
                    &self.wiki_root,
                    std::slice::from_ref(&abs_path),
                    action,
                    &display_path,
                    McpCommitArgs {
                        commit: args.commit,
                        push: args.push,
                        author: args.author.as_deref(),
                        source: args.source.as_deref(),
                    },
                ) {
                    McpCommitResult::Err(err) => return err,
                    McpCommitResult::Ok { dirty_warning } => dirty_warning,
                };

                let mut response = serde_json::json!({
                    "status": "ok",
                    "path": args.path,
                    "mode": args.mode,
                    "section": section_name,
                    "section_found": write_result.section_found,
                });

                match Page::parse(&assembled) {
                    Ok(page) => {
                        if let Err(e) = self.searcher.index_page(&args.path, &page) {
                            tracing::warn!("Failed to index page {}: {}", args.path, e);
                        }
                        if let Err(e) = self.searcher.commit() {
                            tracing::warn!("Failed to commit index: {}", e);
                        }
                        response["title"] = serde_json::json!(page.title);
                        response["tags"] = serde_json::json!(page.tags);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Page written but failed to parse for indexing {}: {}",
                            args.path,
                            e
                        );
                    }
                }

                if write_result.multiple_matches {
                    response["warning"] = serde_json::json!(format!(
                        "Section '{}' matched multiple headings; operated on first occurrence",
                        section_name
                    ));
                } else if !write_result.section_found {
                    response["warning"] = serde_json::json!(format!(
                        "Section '{}' not found; created at end of page",
                        section_name
                    ));
                }

                attach_warnings(&mut response, dirty_warning);
                response.to_string()
            }

            other => serde_json::json!({
                "error": format!("Unknown write mode: '{other}'")
            })
            .to_string(),
        }
    }

    /// Ingest source material into raw/. After ingesting, use wiki_write to create the corresponding wiki page.
    #[tool(
        name = "wiki_ingest",
        description = "File source material into the wiki's raw/ directory. Pass either `source_path` (absolute path to a local file or URL) OR `content` (pasted markdown string). With `content`, the filename is derived from `filename` > slug(`title`) > first H1 > `untitled.md`. After ingesting, use wiki_write to create the corresponding wiki page."
    )]
    async fn wiki_ingest(&self, Parameters(args): Parameters<WikiIngestArgs>) -> String {
        match (args.source_path.as_deref(), args.content.as_deref()) {
            (Some(_), Some(_)) => serde_json::json!({
                "error": "pass either `source_path` or `content`, not both — they are mutually exclusive"
            })
            .to_string(),
            (None, None) => serde_json::json!({
                "error": "pass either `source_path` (file/URL) or `content` (pasted markdown)"
            })
            .to_string(),
            (Some(source_path), None) => self.ingest_from_path(source_path, &args).await,
            (None, Some(content)) => self.ingest_from_content(content, &args).await,
        }
    }

    async fn ingest_from_path(&self, source_path: &str, args: &WikiIngestArgs) -> String {
        let source = PathBuf::from(source_path);
        match ingest::ingest_source(&self.wiki_root, &source, &args.raw_type).await {
            Ok(result) => match self.ingest_auto_commit(&result.raw_path, args) {
                McpCommitResult::Err(err) => err,
                McpCommitResult::Ok { dirty_warning } => {
                    let mut payload = ingest_ok_value(&result.raw_path, args);
                    attach_warnings(&mut payload, dirty_warning);
                    payload.to_string()
                }
            },
            Err(e) => serde_json::json!({"error": format!("Ingest failed: {e}")}).to_string(),
        }
    }

    async fn ingest_from_content(&self, content: &str, args: &WikiIngestArgs) -> String {
        let filename =
            match derive_content_filename(args.filename.as_deref(), args.title.as_deref(), content)
            {
                Ok(f) => f,
                Err(e) => return serde_json::json!({"error": e}).to_string(),
            };
        match ingest::ingest_content(&self.wiki_root, &args.raw_type, &filename, content).await {
            Ok(result) => match self.ingest_auto_commit(&result.raw_path, args) {
                McpCommitResult::Err(err) => err,
                McpCommitResult::Ok { dirty_warning } => {
                    let mut payload = ingest_ok_value(&result.raw_path, args);
                    attach_warnings(&mut payload, dirty_warning);
                    payload.to_string()
                }
            },
            Err(e) => serde_json::json!({"error": format!("Ingest failed: {e}")}).to_string(),
        }
    }

    /// Run the auto-commit policy after a successful ingest. Returns the
    /// `McpCommitResult` so the caller can propagate either the error
    /// response or the optional dirty-tree warning into its JSON.
    fn ingest_auto_commit(&self, raw_path: &Path, args: &WikiIngestArgs) -> McpCommitResult {
        // Display path stays vault-relative for the commit subject; the
        // auto-commit itself receives the absolute raw path so it works
        // when the wiki root is a subdir of the git repo.
        let display = match raw_path.strip_prefix(&self.wiki_root) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => raw_path.to_string_lossy().to_string(),
        };
        let abs = raw_path.to_path_buf();
        mcp_auto_commit(
            &self.wiki_root,
            std::slice::from_ref(&abs),
            CommitAction::Ingest,
            &display,
            McpCommitArgs {
                commit: args.commit,
                push: args.push,
                author: args.author.as_deref(),
                source: args.source.as_deref(),
            },
        )
    }

    /// Lint report for wiki pages: freshness, TODOs, broken related links, orphans, missing concepts.
    #[tool(
        name = "wiki_lint",
        description = "Run lint checks on wiki pages: freshness (stale/suspect), TODO markers, broken related links, orphan pages, and missing concepts (broken wikilinks). Optionally filter by category."
    )]
    fn wiki_lint(&self, Parameters(args): Parameters<WikiLintArgs>) -> String {
        match lw_core::lint::run_lint(&self.wiki_root, args.category.as_deref()) {
            Ok(report) => {
                let total =
                    report.freshness.fresh + report.freshness.suspect + report.freshness.stale;
                serde_json::json!({
                    "summary": {
                        "fresh": report.freshness.fresh,
                        "suspect": report.freshness.suspect,
                        "stale": report.freshness.stale,
                        "total": total,
                        "todo_count": report.todo_pages.len(),
                        "broken_related_count": report.broken_related.len(),
                        "orphan_count": report.orphan_pages.len(),
                        "missing_concept_count": report.missing_concepts.len(),
                    },
                    "stale_pages": report.freshness.stale_pages,
                    "todo_pages": report.todo_pages,
                    "broken_related": report.broken_related,
                    "orphan_pages": report.orphan_pages,
                    "missing_concepts": report.missing_concepts,
                })
                .to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    /// Scaffold a new wiki page with schema-enforced frontmatter and body template.
    #[tool(
        name = "wiki_new",
        description = "Create a new wiki page with schema-enforced frontmatter and body template. Returns the full rendered page content so agents can immediately follow up with wiki_write section calls. Errors if the category is unknown or the slug already exists."
    )]
    fn wiki_new(&self, Parameters(args): Parameters<WikiNewArgs>) -> String {
        // Hold on to author so we can pass it to the auto-commit step too.
        let author_for_commit = args.author.clone();
        let req = NewPageRequest {
            category: &args.category,
            slug: &args.slug,
            title: args.title,
            tags: args.tags,
            author: args.author,
        };
        match new_page(&self.wiki_root, &self.schema, req) {
            Ok((abs_path, page)) => {
                let content = page.to_markdown();

                // Path for JSON response — vault-relative with "wiki/" prefix,
                // matching the spec example "wiki/tools/foo.md".
                let json_path = abs_path
                    .strip_prefix(&self.wiki_root)
                    .unwrap_or(&abs_path)
                    .to_string_lossy()
                    .to_string();

                // Path for the Tantivy index — relative to wiki_root/wiki/,
                // matching the convention used by rebuild() (e.g. "tools/foo.md").
                let wiki_dir = self.wiki_root.join("wiki");
                let index_path = abs_path
                    .strip_prefix(&wiki_dir)
                    .unwrap_or(&abs_path)
                    .to_string_lossy()
                    .to_string();

                if let Err(e) = self.searcher.index_page(&index_path, &page) {
                    tracing::warn!("Failed to index new page {}: {}", index_path, e);
                }
                if let Err(e) = self.searcher.commit() {
                    tracing::warn!("Failed to commit index after wiki_new: {}", e);
                }

                // Auto-commit the new page (issue #38). Pass the
                // *absolute* page path so `commit_paths` can re-resolve
                // it against the actual git toplevel — wiki_root is
                // allowed to be a subdir of a larger repo.
                let dirty_warning = match mcp_auto_commit(
                    &self.wiki_root,
                    std::slice::from_ref(&abs_path),
                    CommitAction::Create,
                    &json_path,
                    McpCommitArgs {
                        commit: args.commit,
                        push: args.push,
                        author: author_for_commit.as_deref(),
                        source: args.source.as_deref(),
                    },
                ) {
                    McpCommitResult::Err(err) => return err,
                    McpCommitResult::Ok { dirty_warning } => dirty_warning,
                };

                let mut response = serde_json::json!({
                    "path": json_path,
                    "category": args.category,
                    "slug": args.slug,
                    "content": content,
                });
                attach_warnings(&mut response, dirty_warning);
                response.to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }

    /// Get wiki health statistics: page count, category breakdown, freshness distribution.
    #[tool(
        name = "wiki_stats",
        description = "Get wiki statistics: page count, category breakdown, freshness distribution. Takes no arguments."
    )]
    fn wiki_stats(&self) -> String {
        match gather_status(&self.wiki_root) {
            Ok(status) => {
                let uncategorized_count = status
                    .categories
                    .iter()
                    .find(|c| c.name == "_uncategorized")
                    .map(|c| c.page_count)
                    .unwrap_or(0);

                serde_json::json!({
                    "wiki_name": status.wiki_name,
                    "total_pages": status.total_pages,
                    "categories": status.categories.iter().map(|c| {
                        serde_json::json!({
                            "name": c.name,
                            "page_count": c.page_count,
                        })
                    }).collect::<Vec<_>>(),
                    "freshness": {
                        "fresh": status.freshness.fresh,
                        "suspect": status.freshness.suspect,
                        "stale": status.freshness.stale,
                        "unknown": status.freshness.unknown,
                    },
                    "uncategorized_count": uncategorized_count,
                    "index_present": status.index_present,
                })
                .to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        }
    }
}

#[tool_handler]
impl ServerHandler for WikiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_03_26)
            .with_server_info(Implementation::new(
                "lw-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "LLM Wiki knowledge base server. Use wiki_query to search, wiki_read to read pages, \
                 wiki_browse to list pages, wiki_tags to list tags, wiki_new to scaffold a new page, \
                 wiki_write to create/update pages, wiki_ingest to import source material, \
                 wiki_lint to check freshness, and wiki_stats to get wiki health statistics."
            )
    }
}

impl WikiMcpServer {
    pub fn new(wiki_root: PathBuf) -> anyhow::Result<Self> {
        let schema = lw_core::fs::load_schema(&wiki_root)?;
        let default_review_days = schema.wiki.default_review_days;

        let index_dir = wiki_root.join(lw_core::INDEX_DIR);
        std::fs::create_dir_all(&index_dir)?;

        let searcher = TantivySearcher::new(&index_dir)?;
        let wiki_dir = wiki_root.join("wiki");
        // Rebuild only when the index is empty. A rebuild opens the
        // writer and holds the lock for this server's lifetime, which
        // would force any concurrent `lw query` onto the IndexLocked
        // fallback path.
        if wiki_dir.exists()
            && searcher.is_empty()
            && let Err(e) = searcher.rebuild(&wiki_dir)
        {
            tracing::warn!("Failed to build search index on first run: {}", e);
        }

        let searcher = Arc::new(searcher);

        Ok(Self {
            wiki_root,
            schema,
            searcher,
            default_review_days,
            tool_router: Self::tool_router(),
        })
    }
}

/// Start the MCP server on stdio.
pub async fn run_stdio(wiki_root: PathBuf) -> anyhow::Result<()> {
    let server = WikiMcpServer::new(wiki_root)?;
    let transport = rmcp::transport::io::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lw_core::fs::init_wiki;
    use lw_core::schema::WikiSchema;
    use rmcp::handler::server::wrapper::Parameters;
    use tempfile::TempDir;

    fn spawn_server() -> (TempDir, WikiMcpServer) {
        let tmp = TempDir::new().unwrap();
        init_wiki(tmp.path(), &WikiSchema::default()).unwrap();
        let server = WikiMcpServer::new(tmp.path().to_path_buf()).unwrap();
        (tmp, server)
    }

    fn ingest_args(
        source_path: Option<&str>,
        content: Option<&str>,
        filename: Option<&str>,
        title: Option<&str>,
    ) -> WikiIngestArgs {
        WikiIngestArgs {
            source_path: source_path.map(String::from),
            content: content.map(String::from),
            filename: filename.map(String::from),
            raw_type: "articles".to_string(),
            title: title.map(String::from),
            tags: None,
            category: None,
            commit: Some(false),
            push: None,
            author: None,
            source: None,
        }
    }

    /// Parse the tool's JSON string response. Panics on bad JSON so the
    /// assertion site shows the raw body in the failure message.
    fn parse(resp: &str) -> serde_json::Value {
        serde_json::from_str(resp)
            .unwrap_or_else(|e| panic!("tool returned invalid JSON ({e}): {resp}"))
    }

    #[tokio::test]
    async fn ingest_with_content_and_title_writes_slug_derived_file() {
        let (tmp, server) = spawn_server();
        let body = "# Attention\n\nSelf-attention replaced recurrence.";
        let args = ingest_args(None, Some(body), None, Some("My Article"));

        let resp = server.wiki_ingest(Parameters(args)).await;
        let v = parse(&resp);

        assert_eq!(v["status"], "ok", "expected ok, got: {resp}");
        let raw_path = v["raw_path"].as_str().unwrap();
        let expected = tmp.path().join("raw/articles/my-article.md");
        assert_eq!(std::path::Path::new(raw_path), expected);
        assert_eq!(std::fs::read_to_string(&expected).unwrap(), body);
    }

    #[tokio::test]
    async fn ingest_with_content_derives_filename_from_h1_when_no_title() {
        let (tmp, server) = spawn_server();
        let body = "# Transformer Basics\n\nBody here.";
        let args = ingest_args(None, Some(body), None, None);

        let resp = server.wiki_ingest(Parameters(args)).await;
        let v = parse(&resp);

        assert_eq!(v["status"], "ok", "expected ok, got: {resp}");
        let raw_path = v["raw_path"].as_str().unwrap();
        assert_eq!(
            std::path::Path::new(raw_path),
            tmp.path().join("raw/articles/transformer-basics.md")
        );
    }

    #[tokio::test]
    async fn ingest_with_content_explicit_filename_wins() {
        let (tmp, server) = spawn_server();
        let body = "# Whatever\n\nBody.";
        let args = ingest_args(None, Some(body), Some("custom.md"), Some("ignored title"));

        let resp = server.wiki_ingest(Parameters(args)).await;
        let v = parse(&resp);

        assert_eq!(v["status"], "ok", "expected ok, got: {resp}");
        assert_eq!(
            std::path::Path::new(v["raw_path"].as_str().unwrap()),
            tmp.path().join("raw/articles/custom.md")
        );
    }

    #[tokio::test]
    async fn ingest_without_source_or_content_returns_error() {
        let (_tmp, server) = spawn_server();
        let args = ingest_args(None, None, None, None);

        let resp = server.wiki_ingest(Parameters(args)).await;
        let v = parse(&resp);

        assert!(
            v.get("error").is_some(),
            "expected error field, got: {resp}"
        );
        let msg = v["error"].as_str().unwrap();
        assert!(
            msg.contains("source_path") || msg.contains("content"),
            "error should mention source_path/content: {msg}"
        );
    }

    #[tokio::test]
    async fn ingest_rejects_both_source_path_and_content() {
        let (tmp, server) = spawn_server();
        // Stage a real file so the "source not found" error doesn't shadow
        // the mutual-exclusion check.
        let staged = tmp.path().join("staged.md");
        std::fs::write(&staged, "stub").unwrap();
        let args = ingest_args(
            Some(staged.to_str().unwrap()),
            Some("pasted body"),
            None,
            None,
        );

        let resp = server.wiki_ingest(Parameters(args)).await;
        let v = parse(&resp);

        assert!(v.get("error").is_some(), "expected error, got: {resp}");
        let msg = v["error"].as_str().unwrap();
        assert!(
            msg.to_lowercase().contains("both")
                || msg.contains("either")
                || msg.contains("exclusive"),
            "error should name the conflict: {msg}"
        );
    }

    // ── wiki_new tests ────────────────────────────────────────────────────────

    fn new_args(
        category: &str,
        slug: &str,
        title: &str,
        tags: Vec<&str>,
        author: Option<&str>,
    ) -> WikiNewArgs {
        WikiNewArgs {
            category: category.to_string(),
            slug: slug.to_string(),
            title: title.to_string(),
            tags: tags.into_iter().map(String::from).collect(),
            author: author.map(String::from),
            // Existing tests run against a non-git tempdir so commit must be
            // disabled (or it would be a no-op anyway). Setting it explicitly
            // to false keeps the original assertions stable.
            commit: Some(false),
            push: None,
            source: None,
        }
    }

    #[tokio::test]
    async fn wiki_new_happy_path_returns_content() {
        let (tmp, server) = spawn_server();
        let args = new_args(
            "tools",
            "comrak-ast-parser",
            "Comrak AST Parser",
            vec!["rust", "markdown"],
            Some("claude"),
        );

        let resp = server.wiki_new(Parameters(args));
        let v = parse(&resp);

        // No error field
        assert!(v.get("error").is_none(), "unexpected error: {resp}");

        // Required fields present
        assert_eq!(v["category"], "tools", "category mismatch: {resp}");
        assert_eq!(v["slug"], "comrak-ast-parser", "slug mismatch: {resp}");
        assert_eq!(
            v["path"].as_str(),
            Some("wiki/tools/comrak-ast-parser.md"),
            "JSON path must be vault-relative with wiki/ prefix: {resp}"
        );
        let content = v["content"].as_str().expect("missing content field");
        assert!(
            content.starts_with("---\ntitle:"),
            "content should start with frontmatter, got: {content}"
        );

        // File exists on disk
        let expected = tmp.path().join("wiki/tools/comrak-ast-parser.md");
        assert!(expected.exists(), "file not created at {expected:?}");
    }

    #[tokio::test]
    async fn wiki_new_duplicate_slug_returns_error_json() {
        let (_tmp, server) = spawn_server();
        let args = new_args("tools", "dup-slug", "Dup Slug", vec![], None);

        // First call succeeds
        let resp1 = server.wiki_new(Parameters(args));
        let v1 = parse(&resp1);
        assert!(
            v1.get("error").is_none(),
            "first call should succeed: {resp1}"
        );

        // Second call with same args
        let args2 = new_args("tools", "dup-slug", "Dup Slug", vec![], None);
        let resp2 = server.wiki_new(Parameters(args2));
        let v2 = parse(&resp2);

        let msg = v2["error"]
            .as_str()
            .expect("expected error field on duplicate");
        assert!(
            msg.starts_with("page already exists:"),
            "error should be 'page already exists: ...', got: {msg}"
        );
    }

    #[tokio::test]
    async fn wiki_new_unknown_category_returns_error_json() {
        let (_tmp, server) = spawn_server();
        let args = new_args("bogus", "some-slug", "Some Title", vec![], None);

        let resp = server.wiki_new(Parameters(args));
        let v = parse(&resp);

        let msg = v["error"]
            .as_str()
            .expect("expected error field for unknown category");
        assert!(
            msg.starts_with("unknown category: bogus"),
            "error should start with 'unknown category: bogus', got: {msg}"
        );
    }

    #[test]
    fn with_instructions_mentions_wiki_new() {
        // The instructions string is embedded in get_info(); we verify it at the
        // source level by checking the hardcoded string constant in the binary.
        // Construct a server and call get_info() to retrieve the instructions.
        let tmp = TempDir::new().unwrap();
        lw_core::fs::init_wiki(tmp.path(), &WikiSchema::default()).unwrap();
        let server = WikiMcpServer::new(tmp.path().to_path_buf()).unwrap();
        let info = server.get_info();
        let instructions = info.instructions.as_deref().unwrap_or("");
        assert!(
            instructions.contains("wiki_new"),
            "with_instructions should mention wiki_new, got: {instructions}"
        );
    }

    #[tokio::test]
    async fn wiki_query_finds_page_after_wiki_new() {
        let (_tmp, server) = spawn_server();
        let args = new_args(
            "tools",
            "unique-foo-title",
            "A Unique Foo Title",
            vec!["search-test"],
            None,
        );

        let resp = server.wiki_new(Parameters(args));
        let v = parse(&resp);
        assert!(v.get("error").is_none(), "wiki_new failed: {resp}");

        // Commit the index so wiki_query sees the new page
        // (wiki_new triggers index_page + commit)
        let query_args = WikiQueryArgs {
            query: "Unique Foo".to_string(),
            tags: None,
            category: None,
            limit: Some(10),
        };
        let qresp = server.wiki_query(Parameters(query_args));
        let qv = parse(&qresp);

        let hits = qv["hits"].as_array().expect("expected hits array");
        assert!(
            !hits.is_empty(),
            "wiki_query should find the new page, got: {qresp}"
        );
        assert!(
            hits.iter()
                .any(|h| h["path"].as_str() == Some("tools/unique-foo-title.md")),
            "expected hit path 'tools/unique-foo-title.md', got {qresp}"
        );
    }

    // ── git auto-commit tests (issue #38) ────────────────────────────────────

    /// Spawn a server whose wiki root is *also* a fresh git repo with sane
    /// identity config. Commits are seeded with the .lw scaffold so HEAD
    /// exists and subsequent auto-commits aren't initial-commit edge cases.
    fn spawn_server_in_git() -> (TempDir, WikiMcpServer) {
        use std::process::Command as StdCommand;
        let tmp = TempDir::new().unwrap();
        init_wiki(tmp.path(), &WikiSchema::default()).unwrap();

        StdCommand::new("git")
            .args(["init", "--initial-branch=main"])
            .current_dir(tmp.path())
            .output()
            .expect("git init");
        StdCommand::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        StdCommand::new("git")
            .args(["commit", "-m", "seed"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let server = WikiMcpServer::new(tmp.path().to_path_buf()).unwrap();
        (tmp, server)
    }

    fn head_subject(repo: &std::path::Path) -> String {
        use std::process::Command as StdCommand;
        let out = StdCommand::new("git")
            .args(["log", "-1", "--format=%s"])
            .current_dir(repo)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn commit_count(repo: &std::path::Path) -> u32 {
        use std::process::Command as StdCommand;
        let out = StdCommand::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn wiki_write_auto_commits_by_default() {
        let (tmp, server) = spawn_server_in_git();
        let before = commit_count(tmp.path());

        let args = WikiWriteArgs {
            path: "architecture/auto.md".to_string(),
            content: "---\ntitle: Auto\ntags: [t]\n---\n\nbody\n".to_string(),
            mode: "overwrite".to_string(),
            section: None,
            commit: None,
            push: None,
            author: None,
            source: None,
        };
        let resp = server.wiki_write(Parameters(args));
        let v = parse(&resp);
        assert_eq!(v["status"], "ok", "expected ok; got: {resp}");

        assert_eq!(
            commit_count(tmp.path()),
            before + 1,
            "wiki_write should auto-commit"
        );
        assert!(head_subject(tmp.path()).starts_with("docs(wiki): update"));
    }

    #[tokio::test]
    async fn wiki_write_commit_false_skips_commit() {
        let (tmp, server) = spawn_server_in_git();
        let before = commit_count(tmp.path());

        let args = WikiWriteArgs {
            path: "architecture/no.md".to_string(),
            content: "---\ntitle: No\ntags: [t]\n---\n\nbody\n".to_string(),
            mode: "overwrite".to_string(),
            section: None,
            commit: Some(false),
            push: None,
            author: None,
            source: None,
        };
        let resp = server.wiki_write(Parameters(args));
        let v = parse(&resp);
        assert_eq!(v["status"], "ok", "expected ok; got: {resp}");

        assert_eq!(commit_count(tmp.path()), before, "commit=false should skip");
    }

    #[tokio::test]
    async fn wiki_new_auto_commits_with_author() {
        let (tmp, server) = spawn_server_in_git();
        let before = commit_count(tmp.path());

        let args = WikiNewArgs {
            category: "tools".to_string(),
            slug: "mcp-page".to_string(),
            title: "MCP Page".to_string(),
            tags: vec!["x".to_string()],
            author: Some("Carol <carol@example.com>".to_string()),
            commit: None,
            push: None,
            source: None,
        };
        let resp = server.wiki_new(Parameters(args));
        let v = parse(&resp);
        assert!(v.get("error").is_none(), "wiki_new error: {resp}");

        assert_eq!(commit_count(tmp.path()), before + 1);
        assert!(head_subject(tmp.path()).starts_with("docs(wiki): create"));

        // Author should have been honored.
        use std::process::Command as StdCommand;
        let out = StdCommand::new("git")
            .args(["log", "-1", "--format=%an <%ae>"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let line = String::from_utf8_lossy(&out.stdout);
        assert_eq!(line.trim(), "Carol <carol@example.com>");
    }

    #[tokio::test]
    async fn wiki_ingest_auto_commits() {
        let (tmp, server) = spawn_server_in_git();
        let before = commit_count(tmp.path());

        let args = WikiIngestArgs {
            source_path: None,
            content: Some("# Pasted\n\nbody\n".to_string()),
            filename: Some("pasted.md".to_string()),
            raw_type: "articles".to_string(),
            title: None,
            tags: None,
            category: None,
            commit: None,
            push: None,
            author: None,
            source: None,
        };
        let resp = server.wiki_ingest(Parameters(args)).await;
        let v = parse(&resp);
        assert_eq!(v["status"], "ok", "ingest error: {resp}");

        assert_eq!(commit_count(tmp.path()), before + 1);
        assert!(head_subject(tmp.path()).starts_with("docs(wiki): ingest"));
    }

    #[tokio::test]
    async fn wiki_write_outside_git_repo_succeeds_without_commit() {
        // Plain wiki, NOT inside a git repo. Auto-commit must skip.
        let (tmp, server) = spawn_server();
        let args = WikiWriteArgs {
            path: "architecture/no-git.md".to_string(),
            content: "---\ntitle: NoGit\ntags: [t]\n---\n\nbody\n".to_string(),
            mode: "overwrite".to_string(),
            section: None,
            commit: None,
            push: None,
            author: None,
            source: None,
        };
        let resp = server.wiki_write(Parameters(args));
        let v = parse(&resp);
        assert_eq!(v["status"], "ok", "wiki_write should succeed; got: {resp}");
        assert!(tmp.path().join("wiki/architecture/no-git.md").exists());
        assert!(
            !tmp.path().join(".git").exists(),
            "must not init git on the user's behalf"
        );
    }

    // ─── Reviewer-flagged fix: dirty-tree warning must surface in JSON ────────
    //
    // The dirty-tree warning was previously fired only via `tracing::warn!`,
    // which goes to stderr where the agent never sees it. The JSON response
    // must carry a `warnings` field so the agent can show it to the user.

    #[tokio::test]
    async fn wiki_write_dirty_tree_returns_warnings_field_in_json() {
        let (tmp, server) = spawn_server_in_git();

        // Create an unrelated dirty file before the wiki write.
        std::fs::write(tmp.path().join("dirty.txt"), "junk").unwrap();

        let args = WikiWriteArgs {
            path: "architecture/dirty-warn.md".to_string(),
            content: "---\ntitle: DirtyWarn\ntags: [t]\n---\n\nbody\n".to_string(),
            mode: "overwrite".to_string(),
            section: None,
            commit: None,
            push: None,
            author: None,
            source: None,
        };
        let resp = server.wiki_write(Parameters(args));
        let v = parse(&resp);
        assert_eq!(v["status"], "ok", "wiki_write should succeed; got: {resp}");

        // The response must include a warnings array surfacing the dirty
        // working tree to the agent.
        let warnings = v["warnings"]
            .as_array()
            .unwrap_or_else(|| panic!("expected warnings array in response: {resp}"));
        assert!(
            !warnings.is_empty(),
            "warnings should be populated when working tree is dirty: {resp}"
        );
        let joined = warnings
            .iter()
            .map(|w| w.as_str().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            joined.to_lowercase().contains("dirty")
                || joined.to_lowercase().contains("uncommitted"),
            "warning should mention dirty/uncommitted; got: {joined}"
        );
    }

    // ─── Reviewer-flagged fix: assert generator metadata in MCP test ─────────
    //
    // Strengthen the existing wiki_write auto-commit test by asserting the
    // commit body contains the `generator: lw v…` line.

    #[tokio::test]
    async fn wiki_write_auto_commit_body_records_generator_metadata() {
        let (tmp, server) = spawn_server_in_git();

        let args = WikiWriteArgs {
            path: "architecture/gen.md".to_string(),
            content: "---\ntitle: Gen\ntags: [t]\n---\n\nbody\n".to_string(),
            mode: "overwrite".to_string(),
            section: None,
            commit: None,
            push: None,
            author: None,
            source: None,
        };
        let resp = server.wiki_write(Parameters(args));
        let v = parse(&resp);
        assert_eq!(v["status"], "ok", "wiki_write should succeed; got: {resp}");

        use std::process::Command as StdCommand;
        let body = StdCommand::new("git")
            .args(["log", "-1", "--format=%B"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let body_str = String::from_utf8_lossy(&body.stdout);
        assert!(
            body_str.contains("generator: lw v"),
            "commit body must contain 'generator: lw v…'; got: {body_str}"
        );
    }
}
