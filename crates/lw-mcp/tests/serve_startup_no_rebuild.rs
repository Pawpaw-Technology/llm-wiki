//! `WikiMcpServer::new` must not open the writer lock when the index
//! is already populated; otherwise concurrent `lw query` rebuilds are
//! forced onto the IndexLocked fallback for the server's lifetime.

use lw_core::WikiError;
use lw_core::fs::{init_wiki, validate_wiki_path, write_page};
use lw_core::page::Page;
use lw_core::schema::WikiSchema;
use lw_core::search::{Searcher, TantivySearcher};
use lw_mcp::WikiMcpServer;
use tempfile::TempDir;

fn seed_vault_and_index(root: &std::path::Path) {
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    let page_content = "---\ntitle: Seed\ntags: [seed]\n---\n\nseed body\n";
    let rel = "architecture/seed.md";
    let abs = validate_wiki_path(&root.join("wiki"), rel).unwrap();
    let page = Page::parse(page_content).unwrap();
    write_page(&abs, &page).unwrap();

    // Pre-populate the on-disk tantivy index so WikiMcpServer::new sees a
    // "current" index. Drop the searcher so the writer lock is fully
    // released before the server starts.
    let index_dir = root.join(lw_core::INDEX_DIR);
    std::fs::create_dir_all(&index_dir).unwrap();
    {
        let s = TantivySearcher::new(&index_dir).unwrap();
        s.index_page(rel, &page).unwrap();
        s.commit().unwrap();
    }
}

#[test]
fn mcp_new_skips_rebuild_when_index_is_populated() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    seed_vault_and_index(&root);

    // Start the MCP server. With the fix, it must NOT open the writer
    // because the index is already populated.
    let _server = WikiMcpServer::new(root.clone()).expect("server must start");

    // A separate searcher (simulating `lw query`) must now be able to
    // rebuild without hitting IndexLocked.
    let index_dir = root.join(lw_core::INDEX_DIR);
    let cli_searcher = TantivySearcher::new(&index_dir).unwrap();
    match cli_searcher.rebuild(&root.join("wiki")) {
        Ok(_) => {} // expected
        Err(WikiError::IndexLocked { .. }) => {
            panic!(
                "lw serve still holds the writer lock after startup — \
                 `lw query` rebuild should have succeeded"
            );
        }
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[test]
fn mcp_new_still_rebuilds_when_index_is_empty() {
    // Fresh install: no prior index. The server must build one so
    // `wiki_query` returns results on first use.
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    let schema = WikiSchema::default();
    init_wiki(&root, &schema).unwrap();

    let rel = "architecture/fresh.md";
    let abs = validate_wiki_path(&root.join("wiki"), rel).unwrap();
    let page = Page::parse("---\ntitle: Fresh\ntags: []\n---\n\nfresh body\n").unwrap();
    write_page(&abs, &page).unwrap();

    let _server = WikiMcpServer::new(root.clone()).expect("server must start");

    // The index should have been built — searching should find the page.
    let index_dir = root.join(lw_core::INDEX_DIR);
    let reader = TantivySearcher::new(&index_dir).unwrap();
    let q = lw_core::search::SearchQuery {
        text: Some("fresh".into()),
        tags: vec![],
        category: None,
        status: None,
        author: None,
        sort: lw_core::search::SearchSort::Relevance,
        limit: 10,
    };
    let results = reader.search(&q).unwrap();
    assert_eq!(
        results.total, 1,
        "empty-index startup must rebuild so queries return hits"
    );
}
