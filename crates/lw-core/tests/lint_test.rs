mod common;

use common::{TestWiki, make_page};
use lw_core::lint::run_lint;

#[test]
fn lint_empty_wiki_returns_clean_report() {
    let wiki = TestWiki::new();
    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert!(report.todo_pages.is_empty());
    assert!(report.broken_related.is_empty());
    assert!(report.orphan_pages.is_empty());
    assert!(report.missing_concepts.is_empty());
}

#[test]
fn lint_detects_todo_pages() {
    let wiki = TestWiki::new();
    let todo_page = make_page(
        "Draft Page",
        &["architecture"],
        "normal",
        "TODO: fill in details\nSome partial content here.",
    );
    wiki.write_page("architecture/draft.md", &todo_page);

    let normal_page = make_page(
        "Complete Page",
        &["architecture"],
        "normal",
        "This page is fully written and complete.",
    );
    wiki.write_page("architecture/complete.md", &normal_page);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert_eq!(report.todo_pages.len(), 1);
    assert!(report.todo_pages[0].path.contains("draft.md"));
}

#[test]
fn lint_detects_broken_related() {
    let wiki = TestWiki::new();
    let mut page = make_page(
        "Page With Broken Related",
        &["architecture"],
        "normal",
        "Some content.",
    );
    page.related = Some(vec!["architecture/nonexistent.md".to_string()]);
    wiki.write_page("architecture/page-a.md", &page);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert_eq!(report.broken_related.len(), 1);
    assert!(report.broken_related[0].detail.contains("nonexistent.md"));
}

#[test]
fn lint_valid_related_not_flagged() {
    let wiki = TestWiki::new();

    let page_b = make_page("Page B", &["architecture"], "normal", "Content of B.");
    wiki.write_page("architecture/page-b.md", &page_b);

    let mut page_a = make_page("Page A", &["architecture"], "normal", "Content of A.");
    page_a.related = Some(vec!["architecture/page-b.md".to_string()]);
    wiki.write_page("architecture/page-a.md", &page_a);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert!(report.broken_related.is_empty());
}

#[test]
fn lint_detects_orphan_pages() {
    let wiki = TestWiki::new();

    // Create two pages -- neither references the other, no index.md
    let page_a = make_page("Page A", &["architecture"], "normal", "Content A.");
    wiki.write_page("architecture/page-a.md", &page_a);

    let page_b = make_page("Page B", &["architecture"], "normal", "Content B.");
    wiki.write_page("architecture/page-b.md", &page_b);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert_eq!(
        report.orphan_pages.len(),
        2,
        "both unreferenced pages should be orphans"
    );
}

#[test]
fn lint_body_wikilink_prevents_orphan() {
    let wiki = TestWiki::new();

    // Page A references Page B via body wikilink only (NOT in related:)
    let page_a = make_page(
        "Page A",
        &["architecture"],
        "normal",
        "See [[page-b]] for details.",
    );
    wiki.write_page("architecture/page-a.md", &page_a);

    let page_b = make_page("Page B", &["architecture"], "normal", "Content of B.");
    wiki.write_page("architecture/page-b.md", &page_b);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");

    let orphan_paths: Vec<String> = report.orphan_pages.iter().map(|f| f.path.clone()).collect();

    // page-b should NOT be orphan (referenced via [[page-b]] in page-a's body)
    assert!(
        !orphan_paths.iter().any(|p| p.contains("page-b.md")),
        "page-b should not be orphan when referenced by wikilink, got: {:?}",
        orphan_paths
    );

    // page-a IS still an orphan (nothing references it)
    assert!(
        orphan_paths.iter().any(|p| p.contains("page-a.md")),
        "page-a should be orphan (nothing references it)"
    );
}

#[test]
fn lint_detects_missing_concepts() {
    let wiki = TestWiki::new();

    // Create 3 pages that all reference [[attention]]
    for i in 1..=3 {
        let page = make_page(
            &format!("Page {}", i),
            &["architecture"],
            "normal",
            &format!("Uses [[attention]] mechanism. Page {}.\n", i),
        );
        wiki.write_page(&format!("architecture/page-{}.md", i), &page);
    }

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert_eq!(report.missing_concepts.len(), 1);
    assert!(report.missing_concepts[0].path.contains("attention"));
    assert!(report.missing_concepts[0].detail.contains("3 pages"));
}

#[test]
fn lint_missing_concepts_not_flagged_when_target_exists_same_category() {
    let wiki = TestWiki::new();

    // Create the target page
    let page_b = make_page("Page B", &["architecture"], "normal", "Content of B.");
    wiki.write_page("architecture/page-b.md", &page_b);

    // Create 3 pages referencing [[page-b]] — exceeds threshold
    for i in 1..=3 {
        let page = make_page(
            &format!("Ref {}", i),
            &["architecture"],
            "normal",
            &format!("See [[page-b]] for details. Ref {}.", i),
        );
        wiki.write_page(&format!("architecture/ref-{}.md", i), &page);
    }

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert!(
        report.missing_concepts.is_empty(),
        "page-b exists in same category, should not be flagged: {:?}",
        report.missing_concepts
    );
}

#[test]
fn lint_category_filter_only_checks_matching() {
    let wiki = TestWiki::new();

    let arch_page = make_page(
        "Arch Draft",
        &["architecture"],
        "normal",
        "TODO: write this",
    );
    wiki.write_page("architecture/arch-draft.md", &arch_page);

    let train_page = make_page(
        "Train Draft",
        &["training"],
        "normal",
        "TODO: write this too",
    );
    wiki.write_page("training/train-draft.md", &train_page);

    let report = run_lint(wiki.root(), Some("architecture")).expect("lint should succeed");
    assert_eq!(
        report.todo_pages.len(),
        1,
        "only architecture pages should appear"
    );
}

#[test]
fn lint_missing_concepts_not_flagged_when_page_exists_in_other_category() {
    let wiki = TestWiki::new();

    // Create a page in ops/ (not concepts/)
    let target = make_page("BDD Testing", &["ops"], "normal", "BDD testing guide.\n");
    wiki.write_page("ops/bdd-testing.md", &target);

    // Create 3 pages that all reference [[bdd-testing]] — exceeds 3-ref threshold
    for i in 1..=3 {
        let page = make_page(
            &format!("Referrer {}", i),
            &["architecture"],
            "normal",
            &format!("We use [[bdd-testing]] extensively. Page {}.\n", i),
        );
        wiki.write_page(&format!("architecture/referrer-{}.md", i), &page);
    }

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    // bdd-testing exists in ops/ — should NOT be flagged as missing concept
    assert!(
        report.missing_concepts.is_empty(),
        "wikilink resolving to existing page in another category should not be flagged as missing concept, got: {:?}",
        report.missing_concepts
    );
}

#[test]
fn lint_freshness_included() {
    let wiki = TestWiki::new();
    wiki.with_sample_pages();

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    // Freshness info should be present (all fresh since no git history in temp dir)
    let total = report.freshness.fresh + report.freshness.suspect + report.freshness.stale;
    assert!(total > 0);
}

// ── Stale journal entries (issue #37) ────────────────────────────────────────

/// `lw lint` must surface a `stale_journal_pages` field listing journal pages
/// older than the schema's `[journal] stale_after_days` threshold.
#[test]
fn lint_reports_stale_journal_entries_older_than_threshold() {
    use lw_core::journal::append_capture;
    use std::process::Command;
    use time::macros::{date, time};

    let wiki = TestWiki::new();
    let root = wiki.root();

    // Init a real git repo so age-via-git-log works.
    Command::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "T"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "t@example.com"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "seed"])
        .current_dir(root)
        .output()
        .unwrap();

    // Append a capture, then commit it with a 30-day-old date.
    append_capture(
        root,
        date!(2026 - 04 - 25),
        time!(10:00),
        "stale capture",
        &[],
        None,
    )
    .unwrap();
    Command::new("git")
        .args(["add", "wiki/_journal"])
        .current_dir(root)
        .output()
        .unwrap();
    // RFC 2822 explicit form so git accepts it regardless of natural-date support.
    let backdate = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        let unix = now.as_secs() as i64 - 30 * 86400;
        let dt = time::OffsetDateTime::from_unix_timestamp(unix).unwrap();
        let fmt = time::macros::format_description!(
            "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] +0000"
        );
        dt.format(&fmt).unwrap()
    };
    let out = Command::new("git")
        .args(["commit", "-m", "old"])
        .env("GIT_AUTHOR_DATE", &backdate)
        .env("GIT_COMMITTER_DATE", &backdate)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success(), "backdated commit failed: {out:?}");

    let report = run_lint(root, None).expect("lint should succeed");
    assert!(
        !report.stale_journal_pages.is_empty(),
        "lint must surface stale_journal_pages list when entries are old; report: {report:?}"
    );
    let finding = &report.stale_journal_pages[0];
    assert!(
        finding.path.contains("_journal"),
        "stale_journal_pages.path must include `_journal`: {:?}",
        finding.path
    );
    assert!(
        finding.detail.contains("days") || finding.detail.contains("stale"),
        "stale_journal detail should mention age/stale: {:?}",
        finding.detail
    );
}
