use crate::page::Page;
use crate::{Result, WikiError};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, FAST, STORED, STRING,
};
use tantivy::snippet::SnippetGenerator;
use tantivy::tokenizer::{LowerCaser, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

/// Bumped whenever the tantivy schema layout changes. The first byte of the
/// `.schema_version` marker file in the index dir is compared against this
/// constant; a mismatch (or missing marker on a non-empty dir) triggers a
/// wipe-and-rebuild on `TantivySearcher::new` so old indexes don't surface as
/// cryptic tantivy errors.
///
/// History:
/// - `1` — added `status`, `author`, `generator` keyword fields and a fast
///   `title_keyword` field for sort-by-title (issue #41).
const SCHEMA_VERSION: u32 = 1;
const SCHEMA_VERSION_FILE: &str = ".schema_version";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single search hit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub category: String,
    pub snippet: String,
    pub score: f32,
}

/// Aggregated search results.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub total: usize,
}

/// Sort order for search results.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchSort {
    /// Tantivy BM25 relevance, descending (default).
    #[default]
    Relevance,
    /// Page title, ascending (case-insensitive).
    Title,
    /// First-commit date from `git log`, newest first.
    CreatedDesc,
    /// First-commit date from `git log`, oldest first.
    CreatedAsc,
}

impl SearchSort {
    /// Parse the CLI/MCP-facing string form. Accepts the documented
    /// `relevance`/`created_desc`/`created_asc`/`title` and returns
    /// [`WikiError::Internal`] for unknowns so callers (clap value parser,
    /// MCP handler) can surface a clear error.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "relevance" => Ok(SearchSort::Relevance),
            "title" => Ok(SearchSort::Title),
            "created_desc" => Ok(SearchSort::CreatedDesc),
            "created_asc" => Ok(SearchSort::CreatedAsc),
            other => Err(WikiError::Internal(format!(
                "invalid sort '{other}' (expected: relevance | created_desc | created_asc | title)"
            ))),
        }
    }
}

/// Parameters for a search query.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub tags: Vec<String>,
    pub category: Option<String>,
    /// Filter by frontmatter `status` field (e.g. `draft` / `published`).
    pub status: Option<String>,
    /// Filter by frontmatter `author` field.
    pub author: Option<String>,
    /// Result ordering. Date-based sorts apply only to the relevance-sorted
    /// hit set; the actual git-history lookup is the caller's responsibility
    /// (see `lw_cli::query`).
    pub sort: SearchSort,
    pub limit: usize,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            text: None,
            tags: Vec::new(),
            category: None,
            status: None,
            author: None,
            sort: SearchSort::default(),
            limit: 20,
        }
    }
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Full-text search over wiki pages.
pub trait Searcher: Send + Sync {
    fn index_page(&self, rel_path: &str, page: &Page) -> Result<()>;
    fn remove_page(&self, rel_path: &str) -> Result<()>;
    fn commit(&self) -> Result<()>;
    fn search(&self, query: &SearchQuery) -> Result<SearchResults>;
    fn rebuild(&self, wiki_dir: &Path) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Tantivy implementation
// ---------------------------------------------------------------------------

pub struct TantivySearcher {
    index: Index,
    index_dir: PathBuf,
    reader: IndexReader,
    /// Opened lazily: only writes need it, and holding it eagerly would
    /// lock out every other `lw` process pointed at the same vault.
    writer: Mutex<Option<IndexWriter>>,
    f_path: Field,
    f_title: Field,
    f_title_keyword: Field,
    f_body: Field,
    f_tags: Field,
    f_category: Field,
    f_status: Field,
    f_author: Field,
    f_generator: Field,
}

impl TantivySearcher {
    /// Read the on-disk schema marker. Returns `None` if absent or unparseable
    /// — both are treated by [`maybe_migrate`] as "rebuild me".
    fn read_schema_marker(index_dir: &Path) -> Option<u32> {
        let path = index_dir.join(SCHEMA_VERSION_FILE);
        let raw = std::fs::read_to_string(&path).ok()?;
        raw.trim().parse::<u32>().ok()
    }

    fn write_schema_marker(index_dir: &Path) -> Result<()> {
        let path = index_dir.join(SCHEMA_VERSION_FILE);
        std::fs::write(&path, format!("{SCHEMA_VERSION}\n"))
            .map_err(|e| WikiError::Internal(format!("write schema marker: {e}")))
    }

    /// If the on-disk index dir was built with an older schema version, wipe
    /// its contents (but not the directory itself) so [`Index::open_or_create`]
    /// can build a fresh index. A directory that's empty (no marker, no
    /// tantivy files) is left alone — the caller's `open_or_create` will set
    /// it up and we'll write the marker after.
    ///
    /// We deliberately keep this logic simple: any non-empty index dir
    /// without the **current** marker version is reset. Tantivy doesn't
    /// support online schema migration, and a stale schema would surface
    /// as a hard-to-diagnose `SchemaError` at first read.
    fn maybe_migrate(index_dir: &Path) -> Result<()> {
        if !index_dir.exists() {
            return Ok(());
        }
        let entries: Vec<_> = match std::fs::read_dir(index_dir) {
            Ok(it) => it.filter_map(|e| e.ok()).collect(),
            Err(_) => return Ok(()),
        };
        // Empty dir: nothing to migrate.
        if entries.is_empty() {
            return Ok(());
        }
        let marker = Self::read_schema_marker(index_dir);
        if marker == Some(SCHEMA_VERSION) {
            return Ok(());
        }
        // Either no marker (pre-#41 index) or an older/newer one — wipe.
        // Remove all entries in the dir (files + subdirs) without unlinking
        // the dir itself, so callers holding the path still work.
        for entry in entries {
            let p = entry.path();
            let res = if p.is_dir() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            };
            if let Err(e) = res {
                return Err(WikiError::Internal(format!(
                    "failed to wipe stale index entry {}: {e}",
                    p.display()
                )));
            }
        }
        Ok(())
    }

    #[tracing::instrument]
    pub fn new(index_dir: &Path) -> Result<Self> {
        // Wipe any incompatible old-schema index contents up front so the
        // tantivy `open_or_create` call below sees a clean slate.
        Self::maybe_migrate(index_dir)?;

        // Build text options with jieba tokenizer for CJK support.
        let text_indexing = TextFieldIndexing::default()
            .set_tokenizer("jieba")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let text_options = TextOptions::default()
            .set_indexing_options(text_indexing)
            .set_stored();

        let mut schema_builder = Schema::builder();
        let f_path = schema_builder.add_text_field("path", STRING | STORED);
        let f_title = schema_builder.add_text_field("title", text_options.clone());
        // Separate keyword + fast field for sort-by-title. STRING is whole-
        // value untokenized; FAST gives us order_by_u64_field-compatible
        // ordinal access. We lowercase the value before indexing so the sort
        // is case-insensitive.
        let f_title_keyword = schema_builder.add_text_field("title_kw", STRING | STORED | FAST);
        let f_body = schema_builder.add_text_field("body", text_options);
        let f_tags = schema_builder.add_text_field("tags", STRING | STORED);
        let f_category = schema_builder.add_text_field("category", STRING | STORED);
        let f_status = schema_builder.add_text_field("status", STRING | STORED);
        let f_author = schema_builder.add_text_field("author", STRING | STORED);
        let f_generator = schema_builder.add_text_field("generator", STRING | STORED);
        let schema = schema_builder.build();

        let index =
            Index::open_or_create(tantivy::directory::MmapDirectory::open(index_dir)?, schema)?;

        // Persist the marker so future opens of this dir know which schema it
        // was built against. Best-effort: a write failure shouldn't abort the
        // open (the user may not have write permission for some reason), but
        // we'd rather know about it.
        if let Err(e) = Self::write_schema_marker(index_dir) {
            tracing::warn!("failed to write schema version marker: {e}");
        }

        // Register jieba tokenizer with lowercase filter for CJK + English support.
        let jieba_analyzer = TextAnalyzer::builder(tantivy_jieba::JiebaTokenizer::default())
            .filter(LowerCaser)
            .build();
        index.tokenizers().register("jieba", jieba_analyzer);

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;

        Ok(Self {
            index,
            index_dir: index_dir.to_path_buf(),
            reader,
            writer: Mutex::new(None),
            f_path,
            f_title,
            f_title_keyword,
            f_body,
            f_tags,
            f_category,
            f_status,
            f_author,
            f_generator,
        })
    }

    /// Returns true if the on-disk index contains no committed documents.
    ///
    /// Callers use this to skip work that would otherwise open the
    /// writer — e.g. a `lw serve` startup rebuild, which would hold
    /// the writer lock for the server's lifetime.
    ///
    /// Reloads the reader before checking so long-lived instances see
    /// commits made by other processes (ReloadPolicy::Manual requires
    /// explicit reload — it does not auto-refresh).
    pub fn is_empty(&self) -> bool {
        // Ignore reload errors: is_empty() returns bool, and a stale
        // snapshot is better than panicking. On success the reader sees
        // the current on-disk state; on error it falls back to whatever
        // snapshot it already holds.
        let _ = self.reader.reload();
        self.reader.searcher().num_docs() == 0
    }

    fn category_from_path(rel_path: &str) -> String {
        crate::fs::category_from_path(std::path::Path::new(rel_path)).unwrap_or_default()
    }

    fn page_document(&self, rel_path: &str, page: &Page) -> TantivyDocument {
        let category = Self::category_from_path(rel_path);
        let mut doc = TantivyDocument::new();
        doc.add_text(self.f_path, rel_path);
        doc.add_text(self.f_title, &page.title);
        // Lowercased keyword copy of the title for case-insensitive sort.
        doc.add_text(self.f_title_keyword, page.title.to_lowercase());
        doc.add_text(self.f_body, &page.body);
        for tag in &page.tags {
            doc.add_text(self.f_tags, tag);
        }
        doc.add_text(self.f_category, &category);
        if let Some(s) = &page.status {
            doc.add_text(self.f_status, s);
        }
        if let Some(a) = &page.author {
            doc.add_text(self.f_author, a);
        }
        if let Some(g) = &page.generator {
            doc.add_text(self.f_generator, g);
        }
        doc
    }

    /// Run `op` with the lazily-opened writer. Translates tantivy's
    /// `LockFailure` into the typed `WikiError::IndexLocked` so callers
    /// (e.g. CLI query) can fall back to read-only mode instead of
    /// bubbling a generic index error to the user.
    fn with_writer<T>(&self, op: impl FnOnce(&mut IndexWriter) -> Result<T>) -> Result<T> {
        let mut guard = self
            .writer
            .lock()
            .map_err(|e| WikiError::Internal(e.to_string()))?;
        if guard.is_none() {
            match self.index.writer(50_000_000) {
                Ok(w) => *guard = Some(w),
                Err(tantivy::TantivyError::LockFailure(..)) => {
                    return Err(WikiError::IndexLocked {
                        path: self.index_dir.clone(),
                    });
                }
                Err(e) => return Err(e.into()),
            }
        }
        op(guard.as_mut().expect("writer opened above"))
    }

    fn check_writer_available(&self) -> Result<()> {
        let guard = self
            .writer
            .lock()
            .map_err(|e| WikiError::Internal(e.to_string()))?;
        if guard.is_some() {
            return Ok(());
        }
        match self.index.writer::<TantivyDocument>(50_000_000) {
            Ok(_writer) => Ok(()),
            Err(tantivy::TantivyError::LockFailure(..)) => Err(WikiError::IndexLocked {
                path: self.index_dir.clone(),
            }),
            Err(e) => Err(e.into()),
        }
    }

    fn rollback_writer(&self) -> Result<()> {
        let mut guard = self
            .writer
            .lock()
            .map_err(|e| WikiError::Internal(e.to_string()))?;
        if let Some(writer) = guard.as_mut() {
            writer.rollback()?;
        }
        Ok(())
    }
}

impl Searcher for TantivySearcher {
    #[tracing::instrument(skip(self, page))]
    fn index_page(&self, rel_path: &str, page: &Page) -> Result<()> {
        let f_path = self.f_path;
        let rel_path = rel_path.to_string();
        let doc = self.page_document(&rel_path, page);

        self.with_writer(|writer| {
            let path_term = Term::from_field_text(f_path, &rel_path);
            writer.delete_term(path_term);
            writer.add_document(doc)?;
            Ok(())
        })
    }

    #[tracing::instrument(skip(self))]
    fn remove_page(&self, rel_path: &str) -> Result<()> {
        let f_path = self.f_path;
        let rel_path = rel_path.to_string();
        self.with_writer(|writer| {
            let path_term = Term::from_field_text(f_path, &rel_path);
            writer.delete_term(path_term);
            Ok(())
        })
    }

    #[tracing::instrument(skip(self))]
    fn commit(&self) -> Result<()> {
        self.with_writer(|writer| {
            writer.commit()?;
            Ok(())
        })?;
        self.reader.reload()?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn search(&self, query: &SearchQuery) -> Result<SearchResults> {
        use tantivy::query::AllQuery;

        // Reload before querying so a long-lived instance (e.g. `lw serve`)
        // sees commits made by other processes. ReloadPolicy::Manual never
        // auto-refreshes; without this call external writes are invisible.
        self.reader.reload()?;

        let searcher = self.reader.searcher();

        let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

        // Text query: if provided, parse and require; otherwise match all.
        match &query.text {
            Some(text) if !text.is_empty() => {
                let query_parser =
                    QueryParser::for_index(&self.index, vec![self.f_title, self.f_body]);
                let text_query = query_parser.parse_query(text).map_err(|e| {
                    WikiError::Tantivy(tantivy::TantivyError::InvalidArgument(e.to_string()))
                })?;
                subqueries.push((Occur::Must, text_query));
            }
            _ => {
                subqueries.push((Occur::Must, Box::new(AllQuery)));
            }
        }

        // Tag filters — each required tag must be present (AND).
        for tag in &query.tags {
            let term = Term::from_field_text(self.f_tags, tag);
            subqueries.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        // Category filter.
        if let Some(ref cat) = query.category {
            let term = Term::from_field_text(self.f_category, cat);
            subqueries.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        // Status filter (frontmatter `status` field).
        if let Some(ref status) = query.status {
            let term = Term::from_field_text(self.f_status, status);
            subqueries.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        // Author filter (frontmatter `author` field).
        if let Some(ref author) = query.author {
            let term = Term::from_field_text(self.f_author, author);
            subqueries.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        let combined = BooleanQuery::new(subqueries);

        // Pull a generous page so post-sort (Title) still has the right top-N.
        // For Relevance we honour query.limit directly.
        let collector_limit = match query.sort {
            SearchSort::Relevance => query.limit,
            // Date sorts and title sort run as a fixed top-N then sort in
            // memory. 1000 is enough for any wiki we'd index in v1; if that
            // ever stops being true, pivot to Tantivy's order_by_fast_field.
            SearchSort::Title | SearchSort::CreatedDesc | SearchSort::CreatedAsc => {
                query.limit.max(1000)
            }
        };
        let top_docs = searcher.search(
            &combined,
            &TopDocs::with_limit(collector_limit).order_by_score(),
        )?;

        // Snippet generator for the body field.
        let snippet_gen = SnippetGenerator::create(&searcher, &combined, self.f_body)?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in &top_docs {
            let doc: TantivyDocument = searcher.doc(*doc_address)?;

            let path = doc
                .get_first(self.f_path)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let title = doc
                .get_first(self.f_title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let tags: Vec<String> = doc
                .get_all(self.f_tags)
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();

            let category = doc
                .get_first(self.f_category)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let snippet = snippet_gen.snippet_from_doc(&doc);
            let snippet_text = if snippet.is_empty() {
                String::new()
            } else {
                // Strip Tantivy's HTML highlight tags so consumers (MCP, CLI)
                // receive plain text.  See GitHub issue #25.
                snippet.to_html().replace("<b>", "").replace("</b>", "")
            };

            hits.push(SearchHit {
                path,
                title,
                tags,
                category,
                snippet: snippet_text,
                score: *score,
            });
        }

        // Apply non-Relevance sort modes. Date sorts (`CreatedDesc` /
        // `CreatedAsc`) are intentionally a no-op at the search layer: git
        // history isn't visible to Tantivy. The CLI/MCP wrappers that have
        // a wiki_dir handy resolve those after enrichment. Leaving them as
        // pass-through keeps the searcher pure and avoids embedding git
        // logic in the index layer.
        match query.sort {
            SearchSort::Title => {
                hits.sort_by_key(|h| h.title.to_lowercase());
            }
            SearchSort::Relevance | SearchSort::CreatedDesc | SearchSort::CreatedAsc => {}
        }

        // Apply user-facing limit after sort.
        if hits.len() > query.limit {
            hits.truncate(query.limit);
        }

        let total = hits.len();
        Ok(SearchResults { hits, total })
    }

    #[tracing::instrument(skip(self))]
    fn rebuild(&self, wiki_dir: &Path) -> Result<()> {
        self.check_writer_available()?;

        let mut docs = Vec::new();
        let pages = crate::fs::list_pages(wiki_dir)?;
        for rel_path in &pages {
            let abs_path = wiki_dir.join(rel_path);
            match crate::fs::read_page(&abs_path) {
                Ok(page) => {
                    let rel_path = rel_path.to_string_lossy();
                    docs.push(self.page_document(&rel_path, &page));
                }
                Err(_) => continue, // skip unparseable files
            }
        }

        let rebuild_result = self.with_writer(|writer| {
            writer.delete_all_documents()?;
            for doc in docs {
                writer.add_document(doc)?;
            }
            Ok(())
        });
        if let Err(err) = rebuild_result {
            let _ = self.rollback_writer();
            return Err(err);
        }
        match self.commit() {
            Ok(()) => Ok(()),
            Err(err) => {
                let _ = self.rollback_writer();
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_extraction() {
        assert_eq!(
            TantivySearcher::category_from_path("training/a.md"),
            "training"
        );
        assert_eq!(TantivySearcher::category_from_path("a.md"), "");
        assert_eq!(
            TantivySearcher::category_from_path("architecture/transformers/a.md"),
            "architecture"
        );
    }

    /// Helper: create a TantivySearcher in a temp dir, index a page, commit, return searcher.
    fn setup_searcher_with_page(
        dir: &std::path::Path,
        rel_path: &str,
        page: &crate::page::Page,
    ) -> TantivySearcher {
        let searcher = TantivySearcher::new(dir).expect("create searcher");
        searcher.index_page(rel_path, page).expect("index page");
        searcher.commit().expect("commit");
        searcher
    }

    #[test]
    fn test_snippet_no_html_tags() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let page = crate::page::Page::new(
            "Attention Is All You Need",
            &["transformer", "attention"],
            "The dominant sequence transduction models are based on complex recurrent \
             or convolutional neural networks. The best performing models also connect \
             the encoder and decoder through an attention mechanism. We propose a new \
             simple network architecture, the Transformer, based solely on attention \
             mechanisms, dispensing with recurrence and convolutions entirely.",
        );

        let searcher = setup_searcher_with_page(tmp.path(), "architecture/attention.md", &page);

        let results = searcher
            .search(&SearchQuery {
                text: Some("attention mechanism".to_string()),
                limit: 10,
                ..SearchQuery::default()
            })
            .expect("search");

        assert!(!results.hits.is_empty(), "should have at least one hit");

        for hit in &results.hits {
            assert!(
                !hit.snippet.contains("<b>"),
                "snippet must not contain <b> tag, got: {}",
                hit.snippet
            );
            assert!(
                !hit.snippet.contains("</b>"),
                "snippet must not contain </b> tag, got: {}",
                hit.snippet
            );
        }
    }

    #[test]
    fn test_snippet_preserves_highlighted_text() {
        let tmp = tempfile::tempdir().expect("create tempdir");
        let page = crate::page::Page::new(
            "Transformer Architecture",
            &["transformer"],
            "The transformer architecture uses self-attention to process input sequences \
             in parallel. Multi-head attention allows the model to jointly attend to \
             information from different representation subspaces.",
        );

        let searcher = setup_searcher_with_page(tmp.path(), "architecture/transformer.md", &page);

        let results = searcher
            .search(&SearchQuery {
                text: Some("attention".to_string()),
                limit: 10,
                ..SearchQuery::default()
            })
            .expect("search");

        assert!(!results.hits.is_empty(), "should have at least one hit");

        let snippet = &results.hits[0].snippet;
        // The word "attention" should still appear in the snippet text (just not wrapped in HTML).
        assert!(
            snippet.to_lowercase().contains("attention"),
            "snippet should contain the search term 'attention', got: {}",
            snippet
        );
        // And no HTML tags.
        assert!(
            !snippet.contains('<'),
            "snippet must not contain any HTML tags, got: {}",
            snippet
        );
    }
}
