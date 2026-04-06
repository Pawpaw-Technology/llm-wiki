use crate::page::Page;
use crate::{Result, WikiError};
use std::path::Path;
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
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
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

        let writer = index.writer(50_000_000)?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            f_path,
            f_title,
            f_body,
            f_tags,
            f_category,
        })
    }

    fn category_from_path(rel_path: &str) -> String {
        crate::fs::category_from_path(std::path::Path::new(rel_path)).unwrap_or_default()
    }
}

impl Searcher for TantivySearcher {
    #[tracing::instrument(skip(self, page))]
    fn index_page(&self, rel_path: &str, page: &Page) -> Result<()> {
        let writer = self
            .writer
            .lock()
            .map_err(|e| WikiError::Internal(e.to_string()))?;

        // Remove any previous version of this page.
        let path_term = Term::from_field_text(self.f_path, rel_path);
        writer.delete_term(path_term);

        let category = Self::category_from_path(rel_path);

        let mut doc = TantivyDocument::new();
        doc.add_text(self.f_path, rel_path);
        doc.add_text(self.f_title, &page.title);
        doc.add_text(self.f_body, &page.body);
        // Multi-value tags: each tag is a separate field value.
        for tag in &page.tags {
            doc.add_text(self.f_tags, tag);
        }
        doc.add_text(self.f_category, &category);

        writer.add_document(doc)?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn remove_page(&self, rel_path: &str) -> Result<()> {
        let writer = self
            .writer
            .lock()
            .map_err(|e| WikiError::Internal(e.to_string()))?;
        let path_term = Term::from_field_text(self.f_path, rel_path);
        writer.delete_term(path_term);
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn commit(&self) -> Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| WikiError::Internal(e.to_string()))?;
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn search(&self, query: &SearchQuery) -> Result<SearchResults> {
        use tantivy::query::AllQuery;

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
                snippet.to_html()
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
        // Clear all documents.
        {
            let writer = self
                .writer
                .lock()
                .map_err(|e| WikiError::Internal(e.to_string()))?;
            writer.delete_all_documents()?;
        }
        self.commit()?;

        // Re-index all pages from disk.
        let pages = crate::fs::list_pages(wiki_dir)?;
        for rel_path in &pages {
            let abs_path = wiki_dir.join(rel_path);
            match crate::fs::read_page(&abs_path) {
                Ok(page) => {
                    let rel_str = rel_path.to_string_lossy();
                    self.index_page(&rel_str, &page)?;
                }
                Err(_) => continue, // skip unparseable files
            }
        }
        self.commit()
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
}
