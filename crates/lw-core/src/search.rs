use crate::page::Page;
use crate::{Result, WikiError};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::snippet::SnippetGenerator;
use tantivy::tokenizer::{LowerCaser, TextAnalyzer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

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

/// Parameters for a search query.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: Option<String>,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub limit: usize,
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
    f_body: Field,
    f_tags: Field,
    f_category: Field,
}

impl TantivySearcher {
    #[tracing::instrument]
    pub fn new(index_dir: &Path) -> Result<Self> {
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
        let f_body = schema_builder.add_text_field("body", text_options);
        let f_tags = schema_builder.add_text_field("tags", STRING | STORED);
        let f_category = schema_builder.add_text_field("category", STRING | STORED);
        let schema = schema_builder.build();

        let index =
            Index::open_or_create(tantivy::directory::MmapDirectory::open(index_dir)?, schema)?;

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
            f_body,
            f_tags,
            f_category,
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
        doc.add_text(self.f_body, &page.body);
        for tag in &page.tags {
            doc.add_text(self.f_tags, tag);
        }
        doc.add_text(self.f_category, &category);
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

        // Tag filters — each required tag must be present.
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

        let combined = BooleanQuery::new(subqueries);

        let top_docs = searcher.search(
            &combined,
            &TopDocs::with_limit(query.limit).order_by_score(),
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
                tags: vec![],
                category: None,
                limit: 10,
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
                tags: vec![],
                category: None,
                limit: 10,
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
