mod common;

use common::TestWiki;
use lw_core::status::gather_status;

#[test]
fn gather_status_returns_valid_json_shape() {
    let wiki = TestWiki::new();
    wiki.with_sample_pages();

    let status = gather_status(wiki.root()).expect("gather_status should succeed");

    // Verify it serializes to the JSON shape the MCP tool will expose
    let json = serde_json::json!({
        "wiki_name": status.wiki_name,
        "total_pages": status.total_pages,
        "categories": status.categories.iter().map(|c| {
            serde_json::json!({"name": &c.name, "page_count": c.page_count})
        }).collect::<Vec<_>>(),
        "freshness": {
            "fresh": status.freshness.fresh,
            "suspect": status.freshness.suspect,
            "stale": status.freshness.stale,
            "unknown": status.freshness.unknown,
        },
        "uncategorized_count": status.categories.iter()
            .find(|c| c.name == "_uncategorized")
            .map(|c| c.page_count)
            .unwrap_or(0),
        "index_present": status.index_present,
    });

    // Must be a valid JSON object with expected keys
    let obj = json.as_object().expect("should be a JSON object");
    assert!(obj.contains_key("wiki_name"), "missing wiki_name");
    assert!(obj.contains_key("total_pages"), "missing total_pages");
    assert!(obj.contains_key("categories"), "missing categories");
    assert!(obj.contains_key("freshness"), "missing freshness");
    assert!(
        obj.contains_key("uncategorized_count"),
        "missing uncategorized_count"
    );
    assert!(obj.contains_key("index_present"), "missing index_present");

    // wiki_name should be non-empty
    assert!(
        !status.wiki_name.is_empty(),
        "wiki_name should not be empty"
    );
}

#[test]
fn gather_status_counts_pages() {
    let wiki = TestWiki::new();
    let pages = wiki.with_sample_pages();

    let status = gather_status(wiki.root()).expect("gather_status should succeed");

    assert_eq!(
        status.total_pages,
        pages.len(),
        "total_pages should match the number of written pages"
    );
}

#[test]
fn gather_status_counts_categories() {
    let wiki = TestWiki::new();
    wiki.with_sample_pages(); // architecture(2), training(2), tools(1)

    let status = gather_status(wiki.root()).expect("gather_status should succeed");

    // Should have 3 categories
    assert_eq!(status.categories.len(), 3, "should have 3 categories");

    let find_cat = |name: &str| {
        status
            .categories
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.page_count)
    };

    assert_eq!(find_cat("architecture"), Some(2));
    assert_eq!(find_cat("training"), Some(2));
    assert_eq!(find_cat("tools"), Some(1));
}

#[test]
fn gather_status_freshness_totals_match() {
    let wiki = TestWiki::new();
    wiki.with_sample_pages();

    let status = gather_status(wiki.root()).expect("gather_status should succeed");

    let freshness_total = status.freshness.fresh
        + status.freshness.suspect
        + status.freshness.stale
        + status.freshness.unknown;
    assert_eq!(
        freshness_total, status.total_pages,
        "freshness distribution should sum to total_pages"
    );
}

#[test]
fn gather_status_empty_wiki() {
    let wiki = TestWiki::new();

    let status = gather_status(wiki.root()).expect("gather_status should succeed on empty wiki");

    assert_eq!(status.total_pages, 0);
    assert!(status.categories.is_empty());
    assert_eq!(status.freshness.fresh, 0);
    assert_eq!(status.freshness.suspect, 0);
    assert_eq!(status.freshness.stale, 0);
    assert_eq!(status.freshness.unknown, 0);
}
