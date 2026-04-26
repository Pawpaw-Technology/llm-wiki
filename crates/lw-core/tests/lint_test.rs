mod common;

use common::{make_page, TestWiki};
use lw_core::lint::{run_lint, UnlinkedMentionFinding};

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

// ─── Issue #39: orphan rule must exclude `_journal/*` ─────────────────────────

#[test]
fn lint_excludes_journal_pages_from_orphans() {
    let wiki = TestWiki::new();

    // A non-journal orphan and a journal page (which is intentionally
    // captured-not-linked).
    let orphan = make_page("Orphan", &["tools"], "normal", "Body");
    wiki.write_page("tools/orphan-page.md", &orphan);

    let journal = make_page(
        "Daily 2026-04-25",
        &["journal"],
        "normal",
        "Notes for today",
    );
    wiki.write_page("_journal/2026-04-25.md", &journal);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");

    let orphan_paths: Vec<String> = report.orphan_pages.iter().map(|f| f.path.clone()).collect();
    assert!(
        orphan_paths.iter().any(|p| p.contains("orphan-page.md")),
        "regular page must still be flagged: {orphan_paths:?}"
    );
    assert!(
        !orphan_paths
            .iter()
            .any(|p| p.contains("2026-04-25.md") || p.starts_with("_journal/")),
        "_journal/* pages must be excluded: {orphan_paths:?}"
    );
}

// ─── Issue #102: unlinked-mentions lint rule ──────────────────────────────────

/// Acceptance bullet 1: rule fires on unlinked mention (positive case).
/// A page whose body contains "tantivy" (an unlinked mention of tools/tantivy.md)
/// must produce exactly one finding in `unlinked_mentions`.
#[test]
fn lint_unlinked_mentions_fires_on_unlinked_term() {
    let wiki = TestWiki::new();

    // The target page: tantivy
    let tantivy = make_page(
        "Tantivy",
        &["tools"],
        "normal",
        "Tantivy is a full-text search engine library.",
    );
    wiki.write_page("tools/tantivy.md", &tantivy);

    // A page that mentions tantivy but does NOT link it
    let comrak = make_page(
        "Comrak",
        &["tools"],
        "normal",
        "Comrak is a CommonMark parser. We also use tantivy for search.",
    );
    wiki.write_page("tools/comrak.md", &comrak);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert_eq!(
        report.unlinked_mentions.len(),
        1,
        "expected 1 unlinked-mention finding, got: {:?}",
        report.unlinked_mentions
    );
    let f = &report.unlinked_mentions[0];
    assert!(
        f.path.contains("comrak.md"),
        "finding path must point to comrak.md, got: {:?}",
        f.path
    );
    assert_eq!(f.term, "tantivy", "term must be verbatim: {:?}", f.term);
    assert_eq!(
        f.target, "tantivy",
        "target must be slug of tantivy page: {:?}",
        f.target
    );
    assert_eq!(
        f.line, 1,
        "line must be 1 (body has only one line): {:?}",
        f.line
    );
}

/// Acceptance bullet 2: silent on already-linked mention (negative case).
/// A page that uses `[[tantivy]]` must produce no unlinked-mention findings.
#[test]
fn lint_unlinked_mentions_silent_when_already_linked() {
    let wiki = TestWiki::new();

    let tantivy = make_page("Tantivy", &["tools"], "normal", "Full-text search library.");
    wiki.write_page("tools/tantivy.md", &tantivy);

    // Already linked with [[tantivy]]
    let comrak = make_page(
        "Comrak",
        &["tools"],
        "normal",
        "Comrak is a CommonMark parser. We also use [[tantivy]] for search.",
    );
    wiki.write_page("tools/comrak.md", &comrak);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert!(
        report.unlinked_mentions.is_empty(),
        "already-linked term must not produce findings: {:?}",
        report.unlinked_mentions
    );
}

/// Acceptance bullet 3: JSON output shape exactly matches the spec.
/// `{"rule": "unlinked-mentions", "path": "...", "line": N, "term": "...", "target": "..."}`
#[test]
fn lint_unlinked_mentions_json_shape() {
    let wiki = TestWiki::new();

    let tantivy = make_page("Tantivy", &["tools"], "normal", "Full-text search library.");
    wiki.write_page("tools/tantivy.md", &tantivy);

    let comrak = make_page(
        "Comrak",
        &["tools"],
        "normal",
        "We use tantivy for indexing.",
    );
    wiki.write_page("tools/comrak.md", &comrak);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert_eq!(report.unlinked_mentions.len(), 1);

    let f = &report.unlinked_mentions[0];
    // Serialize to JSON and verify exact field names and types.
    let json_val = serde_json::to_value(f).expect("must serialize");
    assert_eq!(
        json_val["rule"], "unlinked-mentions",
        "rule field must be 'unlinked-mentions': {json_val}"
    );
    assert!(
        json_val["path"].is_string(),
        "path must be a string: {json_val}"
    );
    assert!(
        json_val["line"].is_number(),
        "line must be a number: {json_val}"
    );
    assert_eq!(json_val["term"], "tantivy", "term must match: {json_val}");
    assert_eq!(
        json_val["target"], "tantivy",
        "target must be the slug: {json_val}"
    );
    // No extra fields not in the spec (path, rule, line, term, target only).
    let obj = json_val.as_object().unwrap();
    for key in obj.keys() {
        assert!(
            ["rule", "path", "line", "term", "target"].contains(&key.as_str()),
            "unexpected JSON field: {key}"
        );
    }
}

/// Acceptance bullet 4: text output shape matches the spec exactly.
/// Format: `wiki/tools/comrak.md:12 — "tantivy" could link to [[tantivy]]`
/// (em-dash U+2014, double quotes around term, double brackets around target)
#[test]
fn lint_unlinked_mentions_text_format() {
    let wiki = TestWiki::new();

    let tantivy = make_page("Tantivy", &["tools"], "normal", "Full-text search library.");
    wiki.write_page("tools/tantivy.md", &tantivy);

    let comrak = make_page(
        "Comrak",
        &["tools"],
        "normal",
        "We use tantivy for indexing.",
    );
    wiki.write_page("tools/comrak.md", &comrak);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert_eq!(report.unlinked_mentions.len(), 1);

    let f = &report.unlinked_mentions[0];
    let text = f.to_text_line();
    // Must contain the em-dash (U+2014), quoted term, and [[slug]] brackets.
    assert!(
        text.contains('\u{2014}'),
        "text line must contain em-dash (—): {text:?}"
    );
    assert!(
        text.contains("\"tantivy\""),
        "text line must quote the term: {text:?}"
    );
    assert!(
        text.contains("[[tantivy]]"),
        "text line must use [[slug]] for target: {text:?}"
    );
    assert!(
        text.contains(":1"),
        "text line must contain line number ':1': {text:?}"
    );
    // Path portion before the colon must contain the page path.
    assert!(
        text.starts_with("wiki/") || text.contains("comrak.md"),
        "text line must start with wiki-relative path: {text:?}"
    );
}

/// Acceptance bullet 5: aggregate exit code — `lw lint` exits 1 when
/// unlinked-mentions findings exist. Tested at the library level by checking
/// that `has_findings()` returns true when `unlinked_mentions` is non-empty.
#[test]
fn lint_report_has_findings_when_unlinked_mentions_present() {
    let wiki = TestWiki::new();

    let tantivy = make_page("Tantivy", &["tools"], "normal", "Full-text search library.");
    wiki.write_page("tools/tantivy.md", &tantivy);

    let comrak = make_page(
        "Comrak",
        &["tools"],
        "normal",
        "We use tantivy for indexing.",
    );
    wiki.write_page("tools/comrak.md", &comrak);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    assert!(
        report.has_findings(),
        "has_findings() must return true when unlinked_mentions is non-empty: {:?}",
        report
    );

    // Clean wiki (no pages) must not have findings.
    let wiki2 = TestWiki::new();
    let clean = run_lint(wiki2.root(), None).expect("lint should succeed");
    assert!(
        !clean.has_findings(),
        "has_findings() must return false on a clean wiki: {:?}",
        clean
    );
}

/// Acceptance bullet 6: ambiguous match — a term that matches multiple pages
/// produces one finding per matched page (one-finding-per-offense pattern).
#[test]
fn lint_unlinked_mentions_ambiguous_match_produces_one_finding_per_target() {
    let wiki = TestWiki::new();

    // Two pages with the same alias "search"
    let tantivy = make_page("Tantivy", &["tools"], "normal", "Full-text search library.");
    wiki.write_page("tools/tantivy.md", &tantivy);

    // A second page that also uses "tantivy" as an alias
    let mut meilisearch = make_page(
        "Meilisearch",
        &["tools"],
        "normal",
        "Another search engine.",
    );
    meilisearch.aliases = vec!["tantivy".to_string()];
    wiki.write_page("tools/meilisearch.md", &meilisearch);

    // A page that mentions "tantivy" without linking
    let page = make_page(
        "Comrak",
        &["tools"],
        "normal",
        "We use tantivy for full-text search.",
    );
    wiki.write_page("tools/comrak.md", &page);

    let report = run_lint(wiki.root(), None).expect("lint should succeed");
    // The mention "tantivy" is ambiguous (resolves to tantivy + meilisearch),
    // so we expect 2 findings — one per matched page.
    let findings_for_comrak: Vec<&UnlinkedMentionFinding> = report
        .unlinked_mentions
        .iter()
        .filter(|f| f.path.contains("comrak.md"))
        .collect();
    assert_eq!(
        findings_for_comrak.len(),
        2,
        "ambiguous term must produce one finding per matched page: {:?}",
        findings_for_comrak
    );
    let targets: Vec<&str> = findings_for_comrak
        .iter()
        .map(|f| f.target.as_str())
        .collect();
    assert!(
        targets.contains(&"tantivy"),
        "findings must include tantivy slug: {targets:?}"
    );
    assert!(
        targets.contains(&"meilisearch"),
        "findings must include meilisearch slug: {targets:?}"
    );
}
