use lw_core::page::Page;
use lw_core::search::{SearchQuery, Searcher, TantivySearcher};
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
        text: "attention".into(),
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
        text: "deep learning".into(),
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
        text: "content".into(),
        tags: vec!["ml".into()],
        category: None,
        limit: 10,
    };
    assert_eq!(searcher.search(&q1).unwrap().total, 1);

    let q2 = SearchQuery {
        text: "content".into(),
        tags: vec!["optimization".into()],
        category: None,
        limit: 10,
    };
    assert_eq!(searcher.search(&q2).unwrap().total, 1);

    let q3 = SearchQuery {
        text: "content".into(),
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
        text: "attention".into(),
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
        text: "removed".into(),
        tags: vec![],
        category: None,
        limit: 10,
    };
    assert_eq!(searcher.search(&query).unwrap().total, 0);
}
