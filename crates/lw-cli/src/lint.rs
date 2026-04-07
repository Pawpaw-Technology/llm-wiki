use crate::output::Format;
use lw_core::fs::{category_from_path, list_pages, read_page};
use lw_core::git::{self, FreshnessLevel};
use lw_core::link::extract_wiki_links;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct LintReport {
    pub todo_pages: Vec<LintFinding>,
    pub broken_related: Vec<LintFinding>,
    pub orphan_pages: Vec<LintFinding>,
    pub missing_concepts: Vec<LintFinding>,
    pub freshness: FreshnessReport,
}

#[derive(Debug, Serialize)]
pub struct LintFinding {
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct FreshnessReport {
    pub fresh: usize,
    pub suspect: usize,
    pub stale: usize,
    pub stale_pages: Vec<LintFinding>,
}

pub fn run(root: &Path, category: &Option<String>, output_format: &Format) -> anyhow::Result<()> {
    let schema = lw_core::fs::load_schema(root)?;
    let report = lint_wiki(root, category.as_deref(), schema.wiki.default_review_days)?;

    match output_format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Format::Human | Format::Brief => {
            print_human_report(&report);
        }
    }
    Ok(())
}

pub fn lint_wiki(
    root: &Path,
    category_filter: Option<&str>,
    default_review_days: u32,
) -> anyhow::Result<LintReport> {
    let wiki_dir = root.join("wiki");
    let page_paths = list_pages(&wiki_dir)?;

    let mut todo_pages = Vec::new();
    let mut broken_related = Vec::new();
    let mut orphan_candidates: HashSet<String> = HashSet::new();
    let mut referenced_pages: HashSet<String> = HashSet::new();
    let mut wikilink_counts: HashMap<String, usize> = HashMap::new();
    let mut freshness_fresh = 0usize;
    let mut freshness_suspect = 0usize;
    let mut freshness_stale = 0usize;
    let mut stale_pages = Vec::new();

    // Read index.md to extract referenced pages
    let index_path = wiki_dir.join("index.md");
    if index_path.exists()
        && let Ok(index_content) = std::fs::read_to_string(&index_path)
    {
        // Extract markdown links like [title](path.md)
        for cap in regex::Regex::new(r"\]\(([^)]+\.md)\)")
            .unwrap()
            .captures_iter(&index_content)
        {
            referenced_pages.insert(cap[1].to_string());
        }
    }

    for rel_path in &page_paths {
        let cat = category_from_path(rel_path).unwrap_or_default();
        if let Some(filter) = category_filter
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

        // Check 2: Broken related
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

        // Track wikilink references for missing_concepts
        let links = extract_wiki_links(&page.body);
        for link in &links {
            *wikilink_counts.entry(link.clone()).or_insert(0) += 1;
        }

        // Track related references for orphan detection
        if let Some(ref related) = page.related {
            for rel in related {
                referenced_pages.insert(rel.clone());
            }
        }

        // Check freshness
        let decay = page.decay.as_deref().unwrap_or("normal");
        let age_days = git::page_age_days(&abs_path);
        let level = match age_days {
            Some(days) => git::compute_freshness(decay, days, default_review_days),
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

    // Check 3: Orphan pages — not referenced by any other page's related: or by index.md
    let orphan_pages: Vec<LintFinding> = orphan_candidates
        .into_iter()
        .filter(|p| !referenced_pages.contains(p))
        .map(|p| LintFinding {
            path: p.clone(),
            detail: "Not referenced by any page or index.md".to_string(),
        })
        .collect();

    // Check 4: Missing concepts — wikilinks with 3+ references and no concept page
    let missing_concepts: Vec<LintFinding> = wikilink_counts
        .into_iter()
        .filter(|(_, count)| *count >= 3)
        .filter(|(slug, _)| {
            let concept_path = wiki_dir.join(format!("concepts/{}.md", slug));
            !concept_path.exists()
        })
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

fn print_human_report(report: &LintReport) {
    let total_issues = report.todo_pages.len()
        + report.broken_related.len()
        + report.orphan_pages.len()
        + report.missing_concepts.len()
        + report.freshness.stale;

    println!("Wiki Lint Report");
    println!("================");
    println!();

    if !report.todo_pages.is_empty() {
        println!("TODO Pages ({}):", report.todo_pages.len());
        for f in &report.todo_pages {
            println!("  - {}: {}", f.path, f.detail);
        }
        println!();
    }

    if !report.broken_related.is_empty() {
        println!("Broken Related ({}):", report.broken_related.len());
        for f in &report.broken_related {
            println!("  - {}: {}", f.path, f.detail);
        }
        println!();
    }

    if !report.orphan_pages.is_empty() {
        println!("Orphan Pages ({}):", report.orphan_pages.len());
        for f in &report.orphan_pages {
            println!("  - {}", f.path);
        }
        println!();
    }

    if !report.missing_concepts.is_empty() {
        println!("Missing Concepts ({}):", report.missing_concepts.len());
        for f in &report.missing_concepts {
            println!("  - {}: {}", f.path, f.detail);
        }
        println!();
    }

    println!(
        "Freshness: {} fresh, {} suspect, {} stale",
        report.freshness.fresh, report.freshness.suspect, report.freshness.stale
    );

    if total_issues == 0 {
        println!("\nAll clear!");
    } else {
        println!("\n{} issue(s) found.", total_issues);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lw_core::fs::{init_wiki, write_page};
    use lw_core::page::Page;
    use lw_core::schema::WikiSchema;
    use tempfile::TempDir;

    fn setup_wiki() -> TempDir {
        let tmp = TempDir::new().unwrap();
        init_wiki(tmp.path(), &WikiSchema::default()).unwrap();
        tmp
    }

    fn make_page(title: &str, tags: &[&str], body: &str) -> Page {
        Page {
            title: title.to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            decay: None,
            sources: vec![],
            author: None,
            generator: None,
            related: None,
            body: body.to_string(),
        }
    }

    #[test]
    fn detects_todo_pages() {
        let tmp = setup_wiki();
        let page = make_page("Stub Page", &["test"], "TODO: summarize this content\n");
        write_page(&tmp.path().join("wiki/architecture/stub.md"), &page).unwrap();

        let report = lint_wiki(tmp.path(), None, 90).unwrap();
        assert_eq!(report.todo_pages.len(), 1);
        assert!(report.todo_pages[0].path.contains("stub.md"));
    }

    #[test]
    fn detects_broken_related() {
        let tmp = setup_wiki();
        let mut page = make_page("Page A", &["test"], "Content.\n");
        page.related = Some(vec!["architecture/nonexistent.md".to_string()]);
        write_page(&tmp.path().join("wiki/architecture/page-a.md"), &page).unwrap();

        let report = lint_wiki(tmp.path(), None, 90).unwrap();
        assert_eq!(report.broken_related.len(), 1);
        assert!(report.broken_related[0].detail.contains("nonexistent.md"));
    }

    #[test]
    fn detects_orphan_pages() {
        let tmp = setup_wiki();
        // Create two pages — neither references the other, no index.md
        let p1 = make_page("Orphan One", &["test"], "Content one.\n");
        let p2 = make_page("Orphan Two", &["test"], "Content two.\n");
        write_page(&tmp.path().join("wiki/architecture/orphan-one.md"), &p1).unwrap();
        write_page(&tmp.path().join("wiki/architecture/orphan-two.md"), &p2).unwrap();

        let report = lint_wiki(tmp.path(), None, 90).unwrap();
        assert_eq!(report.orphan_pages.len(), 2);
    }

    #[test]
    fn non_orphan_when_referenced() {
        let tmp = setup_wiki();
        let p1 = make_page("Referenced", &["test"], "Content.\n");
        let mut p2 = make_page("Referrer", &["test"], "See also.\n");
        p2.related = Some(vec!["architecture/referenced.md".to_string()]);
        write_page(&tmp.path().join("wiki/architecture/referenced.md"), &p1).unwrap();
        write_page(&tmp.path().join("wiki/architecture/referrer.md"), &p2).unwrap();

        let report = lint_wiki(tmp.path(), None, 90).unwrap();
        // "referenced.md" should NOT be an orphan; "referrer.md" is still orphan
        let orphan_paths: Vec<&str> = report
            .orphan_pages
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert!(
            !orphan_paths.iter().any(|p| p.contains("referenced")),
            "referenced page should not be orphan"
        );
        assert!(
            orphan_paths.iter().any(|p| p.contains("referrer")),
            "referrer page should be orphan (nothing references it)"
        );
    }

    #[test]
    fn detects_missing_concepts() {
        let tmp = setup_wiki();
        // Create 3 pages that all reference [[attention]]
        for i in 1..=3 {
            let page = make_page(
                &format!("Page {}", i),
                &["test"],
                &format!("Uses [[attention]] mechanism. Page {}.\n", i),
            );
            write_page(
                &tmp.path().join(format!("wiki/architecture/page-{}.md", i)),
                &page,
            )
            .unwrap();
        }

        let report = lint_wiki(tmp.path(), None, 90).unwrap();
        assert_eq!(report.missing_concepts.len(), 1);
        assert!(report.missing_concepts[0].path.contains("attention"));
        assert!(report.missing_concepts[0].detail.contains("3 pages"));
    }

    #[test]
    fn no_missing_concept_when_page_exists() {
        let tmp = setup_wiki();
        // Create concept page
        std::fs::create_dir_all(tmp.path().join("wiki/concepts")).unwrap();
        let concept = make_page("Attention", &["concepts"], "Attention mechanism.\n");
        write_page(&tmp.path().join("wiki/concepts/attention.md"), &concept).unwrap();

        // Create 3 pages that reference [[attention]]
        for i in 1..=3 {
            let page = make_page(
                &format!("Page {}", i),
                &["test"],
                &format!("Uses [[attention]]. Page {}.\n", i),
            );
            write_page(
                &tmp.path().join(format!("wiki/architecture/page-{}.md", i)),
                &page,
            )
            .unwrap();
        }

        let report = lint_wiki(tmp.path(), None, 90).unwrap();
        assert_eq!(report.missing_concepts.len(), 0);
    }

    #[test]
    fn category_filter_works() {
        let tmp = setup_wiki();
        let p1 = make_page("Arch Page", &["arch"], "TODO: fix this\n");
        let p2 = make_page("Train Page", &["train"], "TODO: fix that\n");
        write_page(&tmp.path().join("wiki/architecture/arch.md"), &p1).unwrap();
        write_page(&tmp.path().join("wiki/training/train.md"), &p2).unwrap();

        let report = lint_wiki(tmp.path(), Some("architecture"), 90).unwrap();
        assert_eq!(report.todo_pages.len(), 1);
        assert!(report.todo_pages[0].path.contains("arch"));
    }

    #[test]
    fn clean_wiki_has_no_issues() {
        let tmp = setup_wiki();
        let page = make_page("Clean Page", &["test"], "No issues here.\n");
        write_page(&tmp.path().join("wiki/architecture/clean.md"), &page).unwrap();

        // Add an index.md that references the page
        std::fs::write(
            tmp.path().join("wiki/index.md"),
            "# Index\n- [Clean Page](architecture/clean.md)\n",
        )
        .unwrap();

        let report = lint_wiki(tmp.path(), None, 90).unwrap();
        assert!(report.todo_pages.is_empty());
        assert!(report.broken_related.is_empty());
        assert!(report.orphan_pages.is_empty());
        assert!(report.missing_concepts.is_empty());
    }
}
