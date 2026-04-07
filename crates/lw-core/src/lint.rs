use crate::fs::{category_from_path, list_pages, load_schema, read_page};
use crate::git::{FreshnessLevel, compute_freshness, page_age_days};
use crate::link::{extract_wiki_links, resolve_link};
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

static INDEX_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\]\(([^)]+\.md)\)").expect("INDEX_LINK_RE is a valid regex"));

/// A single lint finding with page path and detail message.
#[derive(Debug, Serialize)]
pub struct LintFinding {
    pub path: String,
    pub detail: String,
}

/// Freshness summary included in the lint report.
#[derive(Debug, Serialize)]
pub struct FreshnessReport {
    pub fresh: usize,
    pub suspect: usize,
    pub stale: usize,
    pub stale_pages: Vec<LintFinding>,
}

/// Full lint report produced by `run_lint`.
#[derive(Debug, Serialize)]
pub struct LintReport {
    pub todo_pages: Vec<LintFinding>,
    pub broken_related: Vec<LintFinding>,
    pub orphan_pages: Vec<LintFinding>,
    pub missing_concepts: Vec<LintFinding>,
    pub freshness: FreshnessReport,
}

/// Run all lint checks on the wiki at `root`.
/// If `category` is `Some`, only check pages in that category.
pub fn run_lint(root: &Path, category: Option<&str>) -> crate::Result<LintReport> {
    let schema = load_schema(root)?;
    let wiki_dir = root.join("wiki");
    let page_paths = list_pages(&wiki_dir)?;

    let mut todo_pages = Vec::new();
    let mut broken_related = Vec::new();
    let mut orphan_candidates: HashSet<String> = HashSet::new();
    let mut referenced_pages: HashSet<String> = HashSet::new();
    let mut wikilink_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut resolved_slugs: HashSet<String> = HashSet::new();
    let mut freshness_fresh = 0usize;
    let mut freshness_suspect = 0usize;
    let mut freshness_stale = 0usize;
    let mut stale_pages = Vec::new();

    // Read index.md to extract referenced pages
    let index_path = wiki_dir.join("index.md");
    if index_path.exists()
        && let Ok(index_content) = std::fs::read_to_string(&index_path)
    {
        for cap in INDEX_LINK_RE.captures_iter(&index_content) {
            referenced_pages.insert(cap[1].to_string());
        }
    }

    for rel_path in &page_paths {
        let cat = category_from_path(rel_path).unwrap_or_default();
        if let Some(filter) = category
            && cat != filter
        {
            continue;
        }

        let rel_str = rel_path.to_string_lossy().to_string();

        // Skip special wiki files from orphan detection
        let is_special = matches!(rel_str.as_str(), "index.md" | "log.md");
        if !is_special {
            orphan_candidates.insert(rel_str.clone());
        }

        let abs_path = wiki_dir.join(rel_path);
        let page = match read_page(&abs_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Check 1: TODO pages
        if page.body.contains("TODO:") {
            todo_pages.push(LintFinding {
                path: rel_str.clone(),
                detail: "Page body contains TODO:".to_string(),
            });
        }

        // Check 2: Broken related links (single pass, also tracks references)
        if let Some(ref related) = page.related {
            for rel in related {
                let target = wiki_dir.join(rel);
                if !target.exists() {
                    broken_related.push(LintFinding {
                        path: rel_str.clone(),
                        detail: format!("related entry not found: {}", rel),
                    });
                }
                // Track references for orphan detection
                referenced_pages.insert(rel.clone());
            }
        }

        // Check 3: Resolve body wikilinks for orphan detection + missing concepts
        let links = extract_wiki_links(&page.body);
        for link in &links {
            *wikilink_counts.entry(link.clone()).or_insert(0) += 1;
            if let Some(resolved) = resolve_link(link, &wiki_dir) {
                referenced_pages.insert(resolved.to_string_lossy().to_string());
                resolved_slugs.insert(link.clone());
            }
        }

        // Check 4: Freshness
        let decay = page.decay.as_deref().unwrap_or("normal");
        let age_days = page_age_days(&abs_path);
        let level = match age_days {
            Some(days) => compute_freshness(decay, days, schema.wiki.default_review_days),
            None => FreshnessLevel::Fresh,
        };
        match level {
            FreshnessLevel::Fresh => freshness_fresh += 1,
            FreshnessLevel::Suspect => freshness_suspect += 1,
            FreshnessLevel::Stale => {
                freshness_stale += 1;
                stale_pages.push(LintFinding {
                    path: rel_str.clone(),
                    detail: format!("stale (decay={}, age={}d)", decay, age_days.unwrap_or(0)),
                });
            }
        }
    }

    // Check 5: Orphan pages — not referenced by any page's related:, body wikilinks, or index.md
    let orphan_pages: Vec<LintFinding> = orphan_candidates
        .into_iter()
        .filter(|p| !referenced_pages.contains(p))
        .map(|p| LintFinding {
            path: p.clone(),
            detail: "Not referenced by any page or index.md".to_string(),
        })
        .collect();

    // Check 6: Missing concepts — wikilinks referenced 3+ times with no existing page anywhere
    let missing_concepts: Vec<LintFinding> = wikilink_counts
        .into_iter()
        .filter(|(_, count)| *count >= 3)
        .filter(|(slug, _)| !resolved_slugs.contains(slug))
        .map(|(slug, count)| LintFinding {
            path: format!("concepts/{}.md", slug),
            detail: format!("Referenced by {} pages but no concept page exists", count),
        })
        .collect();

    Ok(LintReport {
        todo_pages,
        broken_related,
        orphan_pages,
        missing_concepts,
        freshness: FreshnessReport {
            fresh: freshness_fresh,
            suspect: freshness_suspect,
            stale: freshness_stale,
            stale_pages,
        },
    })
}
