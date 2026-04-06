//! MCP server for LLM Wiki.
//! Provides wiki_query, wiki_read, wiki_browse, wiki_tags, wiki_write, wiki_ingest, wiki_lint tools.

use lw_core::fs::{category_from_path, list_pages, read_page, validate_wiki_path, write_page};
use lw_core::git::{self, FreshnessLevel};
use lw_core::ingest;
use lw_core::llm::NoopLlm;
use lw_core::page::Page;
use lw_core::search::{SearchQuery, Searcher, TantivySearcher};
use lw_core::tag::Taxonomy;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

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
    /// Full markdown content including YAML frontmatter
    pub content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiIngestArgs {
    /// Absolute path to source file
    pub source_path: String,
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
}

fn default_raw_type() -> String {
    "articles".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiLintArgs {
    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,
}

// === Server ===

#[derive(Clone)]
pub struct WikiMcpServer {
    wiki_root: PathBuf,
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
                        let age = git::page_age_days(&abs_path);
                        let decay = page.decay.as_deref().unwrap_or("normal");
                        let level = match age {
                            Some(days) => {
                                git::compute_freshness(decay, days, self.default_review_days)
                            }
                            None => FreshnessLevel::Fresh,
                        };
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

    /// Write or update a wiki page. Content must include YAML frontmatter.
    #[tool(
        name = "wiki_write",
        description = "Write or update a wiki page. The content must include YAML frontmatter with at least a title field. After writing, the search index is updated incrementally."
    )]
    fn wiki_write(&self, Parameters(args): Parameters<WikiWriteArgs>) -> String {
        let page = match Page::parse(&args.content) {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({"error": format!("Invalid page content: {e}")})
                    .to_string();
            }
        };

        let abs_path = match validate_wiki_path(&self.wiki_root, &args.path) {
            Ok(p) => p,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };

        if let Err(e) = write_page(&abs_path, &page) {
            return serde_json::json!({"error": format!("Failed to write page: {e}")}).to_string();
        }

        // Incremental index update
        if let Err(e) = self.searcher.index_page(&args.path, &page) {
            tracing::warn!("Failed to index page {}: {}", args.path, e);
        }
        if let Err(e) = self.searcher.commit() {
            tracing::warn!("Failed to commit index: {}", e);
        }

        serde_json::json!({
            "status": "ok",
            "path": args.path,
            "title": page.title,
            "tags": page.tags,
        })
        .to_string()
    }

    /// Ingest source material into raw/. After ingesting, use wiki_write to create the corresponding wiki page.
    #[tool(
        name = "wiki_ingest",
        description = "Copy source material into the wiki's raw/ directory and return metadata. After ingesting, use wiki_write to create the corresponding wiki page."
    )]
    async fn wiki_ingest(&self, Parameters(args): Parameters<WikiIngestArgs>) -> String {
        let source = PathBuf::from(&args.source_path);
        if !source.exists() {
            return serde_json::json!({"error": format!("Source file not found: {}", args.source_path)}).to_string();
        }

        let llm = NoopLlm;
        match ingest::ingest_source(&self.wiki_root, &source, &args.raw_type, &llm).await {
            Ok(result) => serde_json::json!({
                "status": "ok",
                "raw_path": result.raw_path.to_string_lossy(),
                "suggested_title": args.title,
                "suggested_tags": args.tags,
                "suggested_category": args.category,
                "has_draft": result.draft.is_some(),
                "next_step": "Use wiki_write to create the wiki page from this source material.",
            })
            .to_string(),
            Err(e) => serde_json::json!({"error": format!("Ingest failed: {e}")}).to_string(),
        }
    }

    /// Freshness report for wiki pages using git log for page age.
    #[tool(
        name = "wiki_lint",
        description = "Generate a freshness report for wiki pages using git log to determine page age. Groups pages into fresh, suspect, and stale categories based on their decay settings. Optionally filter by category."
    )]
    fn wiki_lint(&self, Parameters(args): Parameters<WikiLintArgs>) -> String {
        let wiki_dir = self.wiki_root.join("wiki");
        match list_pages(&wiki_dir) {
            Ok(page_paths) => {
                let mut fresh: Vec<serde_json::Value> = Vec::new();
                let mut suspect: Vec<serde_json::Value> = Vec::new();
                let mut stale: Vec<serde_json::Value> = Vec::new();

                for rel_path in &page_paths {
                    let cat = category_from_path(rel_path).unwrap_or_default();

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

                    let decay = page.decay.as_deref().unwrap_or("normal");
                    let age_days = git::page_age_days(&abs_path);
                    let level = match age_days {
                        Some(days) => git::compute_freshness(decay, days, self.default_review_days),
                        None => FreshnessLevel::Fresh, // no git history = treat as fresh
                    };

                    let entry = serde_json::json!({
                        "path": rel_path.to_string_lossy(),
                        "title": page.title,
                        "category": cat,
                        "decay": decay,
                        "age_days": age_days,
                        "level": level.to_string(),
                    });

                    match level {
                        FreshnessLevel::Fresh => fresh.push(entry),
                        FreshnessLevel::Suspect => suspect.push(entry),
                        FreshnessLevel::Stale => stale.push(entry),
                    }
                }

                serde_json::json!({
                    "summary": {
                        "fresh": fresh.len(),
                        "suspect": suspect.len(),
                        "stale": stale.len(),
                        "total": fresh.len() + suspect.len() + stale.len(),
                    },
                    "stale": stale,
                    "suspect": suspect,
                    "fresh": fresh,
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
                 wiki_browse to list pages, wiki_tags to list tags, wiki_write to create/update pages, \
                 wiki_ingest to import source material, and wiki_lint to check freshness."
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
        if wiki_dir.exists()
            && let Err(e) = searcher.rebuild(&wiki_dir)
        {
            tracing::warn!("Failed to rebuild search index: {}", e);
        }

        let searcher = Arc::new(searcher);

        Ok(Self {
            wiki_root,
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
