use crate::output::{self, Format};
use lw_core::WikiError;
use lw_core::fs::load_schema;
use lw_core::git::{FreshnessLevel, page_freshness};
use lw_core::search::{SearchHit, SearchQuery, SearchSort, Searcher, TantivySearcher};
use std::path::Path;

/// Argument bundle for [`run`] — keeps the call-site sane as we add filters
/// (status, author) and sort modes for issue #41.
pub struct RunArgs<'a> {
    pub root: &'a Path,
    pub text: &'a str,
    pub tags: &'a [String],
    pub category: &'a Option<String>,
    pub status: &'a Option<String>,
    pub author: &'a Option<String>,
    pub sort: &'a str,
    pub limit: usize,
    pub format: &'a Format,
    pub stale: bool,
}

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

pub fn run(args: RunArgs<'_>) -> anyhow::Result<()> {
    // Validate wiki exists (produces actionable error message)
    let schema = load_schema(args.root)?;
    let index_dir = args.root.join(lw_core::INDEX_DIR);
    std::fs::create_dir_all(&index_dir)?;
    let searcher = TantivySearcher::new(&index_dir)?;

    // CLI rebuilds on every query so stand-alone use sees on-disk edits.
    // If an MCP server (`lw serve`) is already holding the writer lock for
    // incremental updates, we can't rebuild in parallel — fall back to the
    // existing on-disk index instead of failing the whole command.
    let wiki_dir = args.root.join("wiki");
    match searcher.rebuild(&wiki_dir) {
        Ok(()) => {}
        Err(WikiError::IndexLocked { .. }) => {
            eprintln!(
                "note: index is locked by another lw process (likely `lw serve`); querying the existing index without rebuild"
            );
        }
        Err(e) => return Err(e.into()),
    }

    // Parse the sort string up front so we surface a clean error rather than
    // leaking a `WikiError::Internal` from inside SearchQuery construction.
    let sort =
        SearchSort::parse(args.sort).map_err(|e| anyhow::anyhow!("invalid --sort value: {e}"))?;

    let query = SearchQuery {
        text: if args.text.is_empty() {
            None
        } else {
            Some(args.text.to_string())
        },
        tags: args.tags.to_vec(),
        category: args.category.clone(),
        status: args.status.clone(),
        author: args.author.clone(),
        sort,
        limit: args.limit,
    };
    let results = searcher.search(&query)?;

    // Enrich hits with freshness from git.
    let enriched = enrich_with_freshness(&wiki_dir, &results.hits, schema.wiki.default_review_days);

    // Apply stale filter if requested
    let enriched = if args.stale {
        filter_stale(enriched)
    } else {
        enriched
    };

    // Date-based sort modes need git-history info, which the search layer
    // doesn't have. Apply them here, after freshness enrichment, via the
    // shared `lw_core::search::sort_by_created` helper so CLI and MCP agree
    // on what "newest" means. Title/Relevance sort already happened inside
    // the searcher.
    let mut enriched = enriched;
    lw_core::search::sort_by_created(&mut enriched, &wiki_dir, sort, |h| h.hit.path.as_str());

    let total = if args.stale {
        enriched.len()
    } else {
        results.total
    };
    output::print_query_results_with_freshness(args.text, &enriched, total, args.format);
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
