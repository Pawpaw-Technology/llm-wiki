use crate::output::{self, Format};
use lw_core::fs::load_schema;
use lw_core::git::{page_freshness, FreshnessLevel};
use lw_core::search::{SearchHit, SearchQuery, Searcher, TantivySearcher};
use lw_core::WikiError;
use std::path::Path;

/// A search hit enriched with freshness information.
#[derive(Debug, Clone)]
pub struct HitWithFreshness {
    pub hit: SearchHit,
    pub freshness: FreshnessLevel,
}

/// Compute freshness for each search hit by consulting git history.
pub fn enrich_with_freshness(
    wiki_dir: &Path,
    hits: &[SearchHit],
    default_review_days: u32,
) -> Vec<HitWithFreshness> {
    hits.iter()
        .map(|hit| {
            let abs_path = wiki_dir.join(&hit.path);
            let freshness = page_freshness(&abs_path, default_review_days);
            HitWithFreshness {
                hit: hit.clone(),
                freshness,
            }
        })
        .collect()
}

/// Filter to only stale hits.
pub fn filter_stale(hits: Vec<HitWithFreshness>) -> Vec<HitWithFreshness> {
    hits.into_iter()
        .filter(|h| h.freshness == FreshnessLevel::Stale)
        .collect()
}

pub fn run(
    root: &Path,
    text: &str,
    tags: &[String],
    category: &Option<String>,
    limit: usize,
    format: &Format,
    stale: bool,
) -> anyhow::Result<()> {
    // Validate wiki exists (produces actionable error message)
    let schema = load_schema(root)?;
    let index_dir = root.join(lw_core::INDEX_DIR);
    std::fs::create_dir_all(&index_dir)?;
    let searcher = TantivySearcher::new(&index_dir)?;

    // CLI rebuilds on every query so stand-alone use sees on-disk edits.
    // If an MCP server (`lw serve`) is already holding the writer lock for
    // incremental updates, we can't rebuild in parallel — fall back to the
    // existing on-disk index instead of failing the whole command.
    let wiki_dir = root.join("wiki");
    match searcher.rebuild(&wiki_dir) {
        Ok(()) => {}
        Err(WikiError::IndexLocked { .. }) => {
            eprintln!(
                "note: index is locked by another lw process (likely `lw serve`); querying the existing index without rebuild"
            );
        }
        Err(e) => return Err(e.into()),
    }

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

    // Enrich hits with freshness from git
    let enriched = enrich_with_freshness(&wiki_dir, &results.hits, schema.wiki.default_review_days);

    // Apply stale filter if requested
    let enriched = if stale {
        filter_stale(enriched)
    } else {
        enriched
    };

    let total = if stale { enriched.len() } else { results.total };
    output::print_query_results_with_freshness(text, &enriched, total, format);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hit(path: &str, title: &str) -> SearchHit {
        SearchHit {
            path: path.to_string(),
            title: title.to_string(),
            tags: vec![],
            category: "test".to_string(),
            snippet: String::new(),
            score: 1.0,
        }
    }

    #[test]
    fn filter_stale_keeps_only_stale() {
        let hits = vec![
            HitWithFreshness {
                hit: make_hit("a.md", "Fresh Page"),
                freshness: FreshnessLevel::Fresh,
            },
            HitWithFreshness {
                hit: make_hit("b.md", "Stale Page"),
                freshness: FreshnessLevel::Stale,
            },
            HitWithFreshness {
                hit: make_hit("c.md", "Suspect Page"),
                freshness: FreshnessLevel::Suspect,
            },
        ];

        let result = filter_stale(hits);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].hit.title, "Stale Page");
        assert_eq!(result[0].freshness, FreshnessLevel::Stale);
    }

    #[test]
    fn filter_stale_empty_when_none_stale() {
        let hits = vec![
            HitWithFreshness {
                hit: make_hit("a.md", "Fresh Page"),
                freshness: FreshnessLevel::Fresh,
            },
            HitWithFreshness {
                hit: make_hit("b.md", "Suspect Page"),
                freshness: FreshnessLevel::Suspect,
            },
        ];

        let result = filter_stale(hits);
        assert!(result.is_empty());
    }

    #[test]
    fn filter_stale_all_stale() {
        let hits = vec![
            HitWithFreshness {
                hit: make_hit("a.md", "Old Page 1"),
                freshness: FreshnessLevel::Stale,
            },
            HitWithFreshness {
                hit: make_hit("b.md", "Old Page 2"),
                freshness: FreshnessLevel::Stale,
            },
        ];

        let result = filter_stale(hits);
        assert_eq!(result.len(), 2);
    }
}
