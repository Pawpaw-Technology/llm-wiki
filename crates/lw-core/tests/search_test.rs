use lw_core::page::Page;
use lw_core::search::{SearchQuery, Searcher, TantivySearcher};
use lw_core::WikiError;
use tempfile::TempDir;

fn make_page(title: &str, tags: &[&str], body: &str) -> (String, Page) {
    let slug = title.to_lowercase().replace(' ', "-");
    let page = Page {
        title: title.to_string(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        related: None,
        body: body.to_string(),
    };
    (format!("architecture/{slug}.md"), page)
}

#[test]
fn index_and_search() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();
    let (path, page) = make_page(
        "Transformer",
        &["architecture"],
        "The transformer architecture uses self-attention mechanisms.",
    );
    searcher.index_page(&path, &page).unwrap();
    searcher.commit().unwrap();

    let query = SearchQuery {
        text: Some("attention".into()),
        tags: vec![],
        category: None,
        limit: 10,
    };
    let results = searcher.search(&query).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].title, "Transformer");
}

#[test]
fn search_filters_by_tag() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();
    let (p1, page1) = make_page("A", &["ml"], "Deep learning fundamentals.");
    let (p2, page2) = make_page("B", &["infra"], "Deep learning infrastructure.");
    searcher.index_page(&p1, &page1).unwrap();
    searcher.index_page(&p2, &page2).unwrap();
    searcher.commit().unwrap();

    let query = SearchQuery {
        text: Some("deep learning".into()),
        tags: vec!["ml".into()],
        category: None,
        limit: 10,
    };
    let results = searcher.search(&query).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].title, "A");
}

#[test]
fn search_multi_tag_page() {
    // Verify multi-value tags work: page with ["ml", "optimization"] should match tag filter "ml"
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();
    let (p1, page1) = make_page("Multi", &["ml", "optimization"], "Multi-tag page content.");
    searcher.index_page(&p1, &page1).unwrap();
    searcher.commit().unwrap();

    let q1 = SearchQuery {
        text: Some("content".into()),
        tags: vec!["ml".into()],
        category: None,
        limit: 10,
    };
    assert_eq!(searcher.search(&q1).unwrap().total, 1);

    let q2 = SearchQuery {
        text: Some("content".into()),
        tags: vec!["optimization".into()],
        category: None,
        limit: 10,
    };
    assert_eq!(searcher.search(&q2).unwrap().total, 1);

    let q3 = SearchQuery {
        text: Some("content".into()),
        tags: vec!["nonexistent".into()],
        category: None,
        limit: 10,
    };
    assert_eq!(searcher.search(&q3).unwrap().total, 0);
}

#[test]
fn search_filters_by_category() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();
    let (p1, page1) = make_page("A", &[], "Attention paper.");
    let (_, page2) = make_page("B", &[], "Attention in training.");
    searcher.index_page(&p1, &page1).unwrap();
    searcher.index_page("training/b.md", &page2).unwrap();
    searcher.commit().unwrap();

    let query = SearchQuery {
        text: Some("attention".into()),
        tags: vec![],
        category: Some("training".into()),
        limit: 10,
    };
    let results = searcher.search(&query).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].title, "B");
}

#[test]
fn remove_page_from_index() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();
    let (path, page) = make_page("Gone", &[], "This will be removed.");
    searcher.index_page(&path, &page).unwrap();
    searcher.commit().unwrap();

    searcher.remove_page(&path).unwrap();
    searcher.commit().unwrap();

    let query = SearchQuery {
        text: Some("removed".into()),
        tags: vec![],
        category: None,
        limit: 10,
    };
    assert_eq!(searcher.search(&query).unwrap().total, 0);
}

#[test]
fn search_chinese_text() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();

    let page = Page {
        title: "创业指南".to_string(),
        tags: vec!["startup".to_string()],
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        related: None,
        body: "如果你在创业，陷入焦虑和负面情绪中无法自拔。".to_string(),
    };
    searcher
        .index_page("_uncategorized/startup-guide.md", &page)
        .unwrap();
    searcher.commit().unwrap();

    // Search Chinese
    let q = SearchQuery {
        text: Some("创业".into()),
        tags: vec![],
        category: None,
        limit: 10,
    };
    let results = searcher.search(&q).unwrap();
    assert!(
        results.total >= 1,
        "Chinese search should find results, got {}",
        results.total
    );
    assert_eq!(results.hits[0].title, "创业指南");
}

#[test]
fn search_tag_only_no_text() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();
    let (p1, page1) = make_page("A", &["ml"], "Machine learning basics.");
    let (p2, page2) = make_page("B", &["infra"], "Infrastructure setup.");
    searcher.index_page(&p1, &page1).unwrap();
    searcher.index_page(&p2, &page2).unwrap();
    searcher.commit().unwrap();

    // Tag-only query with no text
    let query = SearchQuery {
        text: None,
        tags: vec!["ml".into()],
        category: None,
        limit: 10,
    };
    let results = searcher.search(&query).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].title, "A");
}

#[test]
fn search_category_only_no_text() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();
    let (p1, page1) = make_page("A", &[], "Content A.");
    let (_, page2) = make_page("B", &[], "Content B.");
    searcher.index_page(&p1, &page1).unwrap();
    searcher.index_page("training/b.md", &page2).unwrap();
    searcher.commit().unwrap();

    let query = SearchQuery {
        text: None,
        tags: vec![],
        category: Some("training".into()),
        limit: 10,
    };
    let results = searcher.search(&query).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].title, "B");
}

#[test]
fn search_mixed_chinese_english() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();

    let page = Page {
        title: "AI Agent 开发实践".to_string(),
        tags: vec!["ai".to_string(), "agent".to_string()],
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        related: None,
        body: "使用 Claude Code 进行 AI Agent 开发的最佳实践。".to_string(),
    };
    searcher.index_page("tools/ai-agent-dev.md", &page).unwrap();
    searcher.commit().unwrap();

    // English term in mixed content
    let q1 = SearchQuery {
        text: Some("Claude".into()),
        tags: vec![],
        category: None,
        limit: 10,
    };
    assert!(searcher.search(&q1).unwrap().total >= 1);

    // Chinese term in mixed content
    let q2 = SearchQuery {
        text: Some("开发".into()),
        tags: vec![],
        category: None,
        limit: 10,
    };
    assert!(searcher.search(&q2).unwrap().total >= 1);
}

// ---------------------------------------------------------------------------
// Concurrent-reader / writer-lock tests
//
// Tantivy's index writer holds an exclusive lockfile on the index directory.
// `lw serve` opens a writer for its lifetime (incremental MCP writes); the
// CLI's `lw query` used to also open one eagerly for its startup rebuild,
// so both contended on the lock and the CLI failed with LockBusy any time
// an MCP was up. The fix: writers lazy, reads never acquire one, rebuild
// surfaces a distinct IndexLocked error so the query path can degrade to
// read-only instead of bailing out.
// ---------------------------------------------------------------------------

#[test]
fn new_does_not_hold_writer_lock() {
    // Two searchers pointing at the same dir must both construct without
    // either touching the writer lock. Under the old eager-writer impl the
    // second `new` call fails with LockBusy.
    let tmp = TempDir::new().unwrap();
    let _first = TantivySearcher::new(tmp.path()).unwrap();
    let _second = TantivySearcher::new(tmp.path())
        .expect("second searcher must open without writer contention");
}

#[test]
fn read_path_works_while_another_searcher_holds_writer() {
    let tmp = TempDir::new().unwrap();

    // Searcher A seeds the index and keeps the writer open (simulates
    // `lw serve` after startup).
    let searcher_a = TantivySearcher::new(tmp.path()).unwrap();
    let (path, page) = make_page("Attention", &[], "Self-attention mechanism.");
    searcher_a.index_page(&path, &page).unwrap();
    searcher_a.commit().unwrap();

    // Searcher B opens the same dir and searches without ever writing.
    // Must not hit LockBusy.
    let searcher_b = TantivySearcher::new(tmp.path())
        .expect("read-only searcher must open while writer is held elsewhere");
    let q = SearchQuery {
        text: Some("attention".into()),
        tags: vec![],
        category: None,
        limit: 10,
    };
    let results = searcher_b.search(&q).unwrap();
    assert_eq!(results.total, 1);
}

#[test]
fn rebuild_returns_index_locked_when_writer_held_elsewhere() {
    let tmp = TempDir::new().unwrap();

    // Searcher A takes the writer by doing any write.
    let searcher_a = TantivySearcher::new(tmp.path()).unwrap();
    let (path, page) = make_page("A", &[], "body");
    searcher_a.index_page(&path, &page).unwrap();
    searcher_a.commit().unwrap();

    // Searcher B tries to rebuild — it needs the writer, so it should
    // report IndexLocked (distinct from a generic TantivyError) so the
    // CLI can catch it and fall back to query-without-rebuild.
    let searcher_b = TantivySearcher::new(tmp.path()).unwrap();
    let err = searcher_b
        .rebuild(tmp.path())
        .expect_err("rebuild must fail when writer is busy");
    assert!(
        matches!(err, WikiError::IndexLocked { .. }),
        "expected IndexLocked, got {err:?}"
    );
}
