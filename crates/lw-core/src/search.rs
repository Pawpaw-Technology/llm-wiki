use crate::page::Page;
use crate::{Result, WikiError};
use std::path::Path;
use std::sync::Mutex;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, Schema, Value, STORED, STRING, TEXT};
use tantivy::snippet::SnippetGenerator;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single search hit.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

/// Aggregated search results.
#[derive(Debug, Clone)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub total: usize,
}

/// Parameters for a search query.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: String,
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
    pub fn new(index_dir: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let f_path = schema_builder.add_text_field("path", STRING | STORED);
        let f_title = schema_builder.add_text_field("title", TEXT | STORED);
        let f_body = schema_builder.add_text_field("body", TEXT | STORED);
        let f_tags = schema_builder.add_text_field("tags", STRING | STORED);
        let f_category = schema_builder.add_text_field("category", STRING | STORED);
        let schema = schema_builder.build();

        let index =
            Index::open_or_create(tantivy::directory::MmapDirectory::open(index_dir)?, schema)?;

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

    /// Extract category from a relative path (first path component).
    fn category_from_path(rel_path: &str) -> String {
        let p = std::path::Path::new(rel_path);
        p.iter()
            .next()
            .filter(|_| p.components().count() > 1)
            .map(|c| c.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

impl Searcher for TantivySearcher {
    fn index_page(&self, rel_path: &str, page: &Page) -> Result<()> {
        let writer = self.writer.lock().unwrap();

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

    fn remove_page(&self, rel_path: &str) -> Result<()> {
        let writer = self.writer.lock().unwrap();
        let path_term = Term::from_field_text(self.f_path, rel_path);
        writer.delete_term(path_term);
        Ok(())
    }

    fn commit(&self) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    fn search(&self, query: &SearchQuery) -> Result<SearchResults> {
        let searcher = self.reader.searcher();

        let query_parser = QueryParser::for_index(&self.index, vec![self.f_title, self.f_body]);
        let text_query = query_parser.parse_query(&query.text).map_err(|e| {
            WikiError::Tantivy(tantivy::TantivyError::InvalidArgument(e.to_string()))
        })?;

        let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
        subqueries.push((Occur::Must, text_query));

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

            let snippet = snippet_gen.snippet_from_doc(&doc);
            let snippet_text = if snippet.is_empty() {
                String::new()
            } else {
                snippet.to_html()
            };

            hits.push(SearchHit {
                path,
                title,
                snippet: snippet_text,
                score: *score,
            });
        }

        let total = hits.len();
        Ok(SearchResults { hits, total })
    }

    fn rebuild(&self, wiki_dir: &Path) -> Result<()> {
        // Clear all documents.
        {
            let writer = self.writer.lock().unwrap();
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
