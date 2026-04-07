use crate::output::Format;
use lw_core::lint::{self, LintReport};
use std::path::Path;

pub fn run(root: &Path, category: &Option<String>, output_format: &Format) -> anyhow::Result<()> {
    let report = lint::run_lint(root, category.as_deref())?;

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
    use lw_core::fs::{init_wiki, write_page};
    use lw_core::lint::run_lint;
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

        let report = run_lint(tmp.path(), None).unwrap();
        assert_eq!(report.todo_pages.len(), 1);
        assert!(report.todo_pages[0].path.contains("stub.md"));
    }

    #[test]
    fn detects_broken_related() {
        let tmp = setup_wiki();
        let mut page = make_page("Page A", &["test"], "Content.\n");
        page.related = Some(vec!["architecture/nonexistent.md".to_string()]);
        write_page(&tmp.path().join("wiki/architecture/page-a.md"), &page).unwrap();

        let report = run_lint(tmp.path(), None).unwrap();
        assert_eq!(report.broken_related.len(), 1);
        assert!(report.broken_related[0].detail.contains("nonexistent.md"));
    }

    #[test]
    fn detects_orphan_pages() {
        let tmp = setup_wiki();
        // Create two pages -- neither references the other, no index.md
        let p1 = make_page("Orphan One", &["test"], "Content one.\n");
        let p2 = make_page("Orphan Two", &["test"], "Content two.\n");
        write_page(&tmp.path().join("wiki/architecture/orphan-one.md"), &p1).unwrap();
        write_page(&tmp.path().join("wiki/architecture/orphan-two.md"), &p2).unwrap();

        let report = run_lint(tmp.path(), None).unwrap();
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

        let report = run_lint(tmp.path(), None).unwrap();
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

        let report = run_lint(tmp.path(), None).unwrap();
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

        let report = run_lint(tmp.path(), None).unwrap();
        assert_eq!(report.missing_concepts.len(), 0);
    }

    #[test]
    fn category_filter_works() {
        let tmp = setup_wiki();
        let p1 = make_page("Arch Page", &["arch"], "TODO: fix this\n");
        let p2 = make_page("Train Page", &["train"], "TODO: fix that\n");
        write_page(&tmp.path().join("wiki/architecture/arch.md"), &p1).unwrap();
        write_page(&tmp.path().join("wiki/training/train.md"), &p2).unwrap();

        let report = run_lint(tmp.path(), Some("architecture")).unwrap();
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

        let report = run_lint(tmp.path(), None).unwrap();
        assert!(report.todo_pages.is_empty());
        assert!(report.broken_related.is_empty());
        assert!(report.orphan_pages.is_empty());
        assert!(report.missing_concepts.is_empty());
    }
}
