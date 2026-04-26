use crate::fs::{category_from_path, list_pages, load_schema, read_page};
use crate::git::{FreshnessLevel, compute_freshness, page_age_days};
use crate::link::{extract_wiki_links, resolve_link};
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

/// A single unlinked-mention finding produced by the `unlinked-mentions` rule.
/// JSON shape (per issue #102 spec):
/// `{"rule": "unlinked-mentions", "path": "...", "line": N, "term": "...", "target": "..."}`
#[derive(Debug, Clone, Serialize)]
pub struct UnlinkedMentionFinding {
    /// Always `"unlinked-mentions"` — present in every serialized record so
    /// a flat list of heterogeneous findings remains self-describing.
    pub rule: String,
    /// Wiki-relative path to the page that contains the mention
    /// (e.g. `wiki/tools/comrak.md`).
    pub path: String,
    /// 1-based line number in the page body where the mention occurs.
    pub line: u32,
    /// Verbatim text from the body that matched (preserves original casing).
    pub term: String,
    /// Slug of the page the term resolves to (not the full path).
    pub target: String,
}

impl UnlinkedMentionFinding {
    /// Format this finding as the canonical human-readable text line:
    /// `wiki/tools/comrak.md:12 — "tantivy" could link to [[tantivy]]`
    ///
    /// Uses an em-dash (U+2014) per the issue #102 text-format spec.
    pub fn to_text_line(&self) -> String {
        format!(
            "{}:{} \u{2014} \"{}\" could link to [[{}]]",
            self.path, self.line, self.term, self.target
        )
    }
}

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
    /// Journal pages whose last git commit is older than the threshold
    /// configured via `[journal] stale_after_days = N` (default 7 days).
    /// Issue #37: signals captures that haven't been triaged.
    #[serde(default)]
    pub stale_journal_pages: Vec<LintFinding>,
    /// Unlinked mentions found across all vault pages — issue #102.
    /// One entry per (page, term, target) triple; ambiguous matches produce
    /// multiple entries (one per matched page), following the one-finding-
    /// per-offense pattern used by the other rules above.
    #[serde(default)]
    pub unlinked_mentions: Vec<UnlinkedMentionFinding>,
}

impl LintReport {
    /// Returns `true` if any rule produced at least one finding.
    /// Used by the CLI to decide the exit code: 0 = clean, 1 = findings.
    pub fn has_findings(&self) -> bool {
        !self.todo_pages.is_empty()
            || !self.broken_related.is_empty()
            || !self.orphan_pages.is_empty()
            || !self.missing_concepts.is_empty()
            || !self.stale_journal_pages.is_empty()
            || self.freshness.stale > 0
            || !self.unlinked_mentions.is_empty()
    }
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

        // Skip special wiki files and _journal/* pages from orphan detection.
        // Journal pages are intentionally capture-not-linked (issue #39).
        let is_special = matches!(rel_str.as_str(), "index.md" | "log.md");
        let is_journal = rel_str.starts_with("_journal/") || rel_str.starts_with("_journal\\");
        if !is_special && !is_journal {
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

    // Journal triage check (issue #37): journal pages whose last git
    // commit is older than `[journal] stale_after_days` are flagged as
    // unprocessed captures awaiting promotion to permanent pages.
    let stale_threshold = schema.journal_stale_after_days();
    let stale_journal_pages: Vec<LintFinding> =
        crate::journal::find_stale_captures(root, stale_threshold)
            .unwrap_or_default()
            .into_iter()
            .filter(|finding| {
                // Honor the `--category` filter the same way other checks do.
                // The stale-finder returns wiki-relative paths starting with
                // `_journal/`; if the user filtered to a different category,
                // suppress these findings.
                match category {
                    Some(filter) => finding.path.starts_with(&format!("{filter}/")),
                    None => true,
                }
            })
            .map(|finding| LintFinding {
                path: finding.path,
                detail: format!(
                    "stale capture (age={}d, threshold={}d) — promote or archive",
                    finding.age_days, stale_threshold
                ),
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
        stale_journal_pages,
        // Stub: unlinked-mentions rule is not yet implemented.
        // Issue #102 implementation fills this in the GREEN step.
        unlinked_mentions: vec![],
    })
}
