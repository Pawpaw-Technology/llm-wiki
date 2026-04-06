use crate::output::{self, Format};
use lw_core::fs::load_schema;
use lw_core::search::{SearchQuery, Searcher, TantivySearcher};
use std::path::Path;

pub fn run(
    root: &Path,
    text: &str,
    tags: &[String],
    category: &Option<String>,
    limit: usize,
    format: &Format,
) -> anyhow::Result<()> {
    let _schema = load_schema(root)?;
    let index_dir = root.join(".lw/search");
    std::fs::create_dir_all(&index_dir)?;
    let searcher = TantivySearcher::new(&index_dir)?;

    // Rebuild index (Phase 2: incremental)
    let wiki_dir = root.join("wiki");
    searcher.rebuild(&wiki_dir)?;

    let query = SearchQuery {
        text: if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        },
        tags: tags.to_vec(),
        category: category.clone(),
        limit,
    };
    let results = searcher.search(&query)?;
    output::print_query_results(text, &results.hits, results.total, format);
    Ok(())
}
