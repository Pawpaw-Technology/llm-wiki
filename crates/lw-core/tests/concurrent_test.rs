//! Concurrent stress tests — run wiki operations in parallel threads/tasks
//! to surface shared-state bugs early.
//!
//! These tests are designed to be **inherently concurrent**: they don't just
//! tolerate parallelism, they *require* it to exercise the isolation boundaries.

mod common;

use common::{TestWiki, make_page};
use lw_core::fs::{list_pages, read_page};
use lw_core::link::{find_broken_links, resolve_link};
use lw_core::search::{SearchQuery, Searcher};
use lw_core::tag::Taxonomy;
use std::sync::Arc;
use std::thread;

/// N independent wikis running full CRUD in parallel threads.
/// If any global state leaks between wikis, this test will catch it.
#[test]
fn parallel_wikis_full_lifecycle() {
    let n = 16;
    let handles: Vec<_> = (0..n)
        .map(|i| {
            thread::spawn(move || {
                let wiki = TestWiki::new();
                let pages = wiki.with_sample_pages();

                // Verify page count is exactly 5 (no cross-wiki bleed)
                let listed = list_pages(&wiki.wiki_dir()).unwrap();
                assert_eq!(
                    listed.len(),
                    5,
                    "wiki {i}: expected 5 pages, got {}",
                    listed.len()
                );

                // Read back and verify content belongs to this wiki
                let p = read_page(
                    &wiki
                        .wiki_dir()
                        .join("architecture/transformer-architecture.md"),
                )
                .unwrap();
                assert_eq!(p.title, "Transformer Architecture");

                // Taxonomy is purely in-memory, but verify no cross-contamination
                let all: Vec<_> = pages.iter().map(|(_, p)| p.clone()).collect();
                let tax = Taxonomy::from_pages(&all);
                assert_eq!(
                    tax.tag_count("architecture"),
                    2,
                    "wiki {i}: tag count wrong"
                );
            })
        })
        .collect();

    for (i, h) in handles.into_iter().enumerate() {
        h.join()
            .unwrap_or_else(|e| panic!("wiki thread {i} panicked: {e:?}"));
    }
}

/// N independent search indexes built and queried in parallel.
/// Tantivy uses mmap — this catches any path/lock collision.
#[test]
fn parallel_search_indexes() {
    let n = 16;
    let handles: Vec<_> = (0..n)
        .map(|i| {
            thread::spawn(move || {
                let wiki = TestWiki::new();
                let pages = wiki.with_sample_pages();
                let searcher = wiki.searcher();

                for (rel, page) in &pages {
                    searcher.index_page(rel, page).unwrap();
                }
                searcher.commit().unwrap();

                let results = searcher
                    .search(&SearchQuery {
                        text: "attention".into(),
                        tags: vec![],
                        category: None,
                        limit: 10,
                    })
                    .unwrap();
                assert!(
                    results.total >= 2,
                    "wiki {i}: expected >=2 hits for 'attention', got {}",
                    results.total
                );

                // Tag-filtered search
                let filtered = searcher
                    .search(&SearchQuery {
                        text: "training".into(),
                        tags: vec!["training".into()],
                        category: None,
                        limit: 10,
                    })
                    .unwrap();
                for hit in &filtered.hits {
                    let page = pages.iter().find(|(_, p)| p.title == hit.title).unwrap();
                    assert!(
                        page.1.tags.contains(&"training".to_string()),
                        "wiki {i}: hit '{}' missing 'training' tag",
                        hit.title
                    );
                }
            })
        })
        .collect();

    for (i, h) in handles.into_iter().enumerate() {
        h.join()
            .unwrap_or_else(|e| panic!("search thread {i} panicked: {e:?}"));
    }
}

/// Parallel ingest operations — each wiki ingests its own source file.
#[tokio::test]
async fn parallel_ingest() {
    let n = 16;
    let mut tasks = Vec::with_capacity(n);

    for i in 0..n {
        tasks.push(tokio::spawn(async move {
            let wiki = TestWiki::new();
            let source = wiki.create_file(
                &format!("incoming/paper-{i}.md"),
                &format!("# Paper {i}\n\nContent for parallel ingest test."),
            );

            let llm = lw_core::llm::NoopLlm;
            let result = lw_core::ingest::ingest_source(wiki.root(), &source, "papers", &llm)
                .await
                .unwrap();

            assert!(result.raw_path.exists(), "task {i}: raw file missing");
            assert!(
                result.raw_path.starts_with(wiki.root().join("raw/papers")),
                "task {i}: raw file in wrong location"
            );
            assert_eq!(
                result.raw_path.file_name().unwrap().to_str().unwrap(),
                format!("paper-{i}.md"),
                "task {i}: wrong filename"
            );
        }));
    }

    for (i, t) in tasks.into_iter().enumerate() {
        t.await
            .unwrap_or_else(|e| panic!("ingest task {i} panicked: {e:?}"));
    }
}

/// Parallel link resolution — verify no cross-wiki path leakage.
#[test]
fn parallel_link_resolution() {
    let n = 16;
    let handles: Vec<_> = (0..n)
        .map(|i| {
            thread::spawn(move || {
                let wiki = TestWiki::new();
                wiki.with_sample_pages();

                let wiki_dir = wiki.wiki_dir();

                // Resolve valid links
                let resolved = resolve_link("transformer-architecture", &wiki_dir);
                assert!(resolved.is_some(), "wiki {i}: failed to resolve valid link");

                // Detect broken links
                let broken = find_broken_links(
                    "See [[transformer-architecture]] and [[nonexistent-page]]",
                    &wiki_dir,
                );
                assert_eq!(
                    broken,
                    vec!["nonexistent-page"],
                    "wiki {i}: broken link detection wrong"
                );
            })
        })
        .collect();

    for (i, h) in handles.into_iter().enumerate() {
        h.join()
            .unwrap_or_else(|e| panic!("link thread {i} panicked: {e:?}"));
    }
}

/// Interleaved read-write: one thread writes pages while another reads.
/// Catches filesystem-level race conditions.
#[test]
fn concurrent_read_write() {
    let wiki = TestWiki::new();
    let root = wiki.root().to_path_buf();
    let wiki_dir = wiki.wiki_dir();

    // Writer thread: write 50 pages sequentially
    let writer_root = root.clone();
    let writer = thread::spawn(move || {
        for i in 0..50 {
            let page = make_page(
                &format!("Page {i}"),
                &["concurrent"],
                "normal",
                &format!("Body for concurrent page {i}"),
            );
            let abs = writer_root
                .join("wiki/architecture")
                .join(format!("page-{i}.md"));
            lw_core::fs::write_page(&abs, &page).unwrap();
        }
    });

    // Reader thread: continuously list pages (may see partial state — that's OK)
    let reader_dir = wiki_dir.clone();
    let reader = thread::spawn(move || {
        let mut max_seen = 0;
        for _ in 0..100 {
            if let Ok(pages) = list_pages(&reader_dir) {
                max_seen = max_seen.max(pages.len());
            }
            thread::yield_now();
        }
        max_seen
    });

    writer.join().expect("writer panicked");
    let max_seen = reader.join().expect("reader panicked");

    // After writer completes, all 50 should be visible
    let final_count = list_pages(&wiki_dir).unwrap().len();
    assert_eq!(final_count, 50, "expected 50 pages after writer done");
    // Reader should have seen *something* being written
    assert!(max_seen > 0, "reader never saw any pages");
}

/// Build and query the same search index from an Arc across threads.
/// This tests TantivySearcher's thread safety (it uses interior Mutex).
#[test]
fn shared_searcher_across_threads() {
    let wiki = TestWiki::new();
    let pages = wiki.with_sample_pages();
    let searcher = Arc::new(wiki.searcher());

    // Index from main thread
    for (rel, page) in &pages {
        searcher.index_page(rel, page).unwrap();
    }
    searcher.commit().unwrap();

    // Query from N threads simultaneously
    let n = 16;
    let handles: Vec<_> = (0..n)
        .map(|i| {
            let s = Arc::clone(&searcher);
            thread::spawn(move || {
                let results = s
                    .search(&SearchQuery {
                        text: "attention".into(),
                        tags: vec![],
                        category: None,
                        limit: 10,
                    })
                    .unwrap();
                assert!(
                    results.total >= 2,
                    "thread {i}: expected >=2 results, got {}",
                    results.total
                );
            })
        })
        .collect();

    for (i, h) in handles.into_iter().enumerate() {
        h.join()
            .unwrap_or_else(|e| panic!("query thread {i} panicked: {e:?}"));
    }
}
