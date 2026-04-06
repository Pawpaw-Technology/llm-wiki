//! Dogfood integration test: exercises lw-core end-to-end from an agent's perspective.
//!
//! Uses the shared TestWiki harness for consistent isolation.

mod common;

use common::{TestWiki, sample_pages};
use lw_core::fs::{list_pages, read_page};
use lw_core::ingest::ingest_source;
use lw_core::link::{extract_wiki_links, find_broken_links, resolve_link};
use lw_core::llm::NoopLlm;
use lw_core::search::{SearchQuery, Searcher};
use lw_core::tag::Taxonomy;

#[test]
fn step1_init_wiki_and_write_pages() {
    let wiki = TestWiki::new();
    assert!(wiki.root().join(".lw/schema.toml").exists());
    assert!(wiki.root().join("wiki/architecture").is_dir());
    assert!(wiki.root().join("raw/papers").is_dir());

    wiki.with_sample_pages();
    let listed = list_pages(&wiki.wiki_dir()).unwrap();
    assert_eq!(listed.len(), 5, "Expected 5 pages, got: {listed:?}");

    let read_back = read_page(
        &wiki
            .wiki_dir()
            .join("architecture/transformer-architecture.md"),
    )
    .unwrap();
    assert_eq!(read_back.title, "Transformer Architecture");
    assert!(read_back.tags.contains(&"attention".to_string()));
    assert_eq!(read_back.decay.as_deref(), Some("evergreen"));
}

#[test]
fn step3_search_text_and_tag_filter() {
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
        "Expected >=2 hits, got {}",
        results.total
    );
    let titles: Vec<&str> = results.hits.iter().map(|h| h.title.as_str()).collect();
    assert!(titles.contains(&"Transformer Architecture"));
    assert!(titles.contains(&"Flash Attention 2"));

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
        assert!(page.1.tags.contains(&"training".to_string()));
    }
}

#[test]
fn step4_tag_taxonomy() {
    let pages = sample_pages();
    let all: Vec<_> = pages.into_iter().map(|(_, p)| p).collect();
    let taxonomy = Taxonomy::from_pages(&all);

    assert_eq!(taxonomy.tag_count("architecture"), 2);
    assert_eq!(taxonomy.tag_count("training"), 2);
    assert_eq!(taxonomy.tag_count("attention"), 2);
    assert_eq!(taxonomy.tag_count("tools"), 1);

    let arch = taxonomy.pages_with_tag("architecture");
    assert!(arch.contains(&"Transformer Architecture".to_string()));
    assert!(arch.contains(&"Flash Attention 2".to_string()));

    let all_tags = taxonomy.all_tags();
    assert!(all_tags.contains(&"rlhf".to_string()));
    assert!(all_tags.contains(&"pytorch".to_string()));
}

#[test]
fn step5_wiki_links_extract_and_resolve() {
    let wiki = TestWiki::new();
    let pages = wiki.with_sample_pages();

    let rlhf = pages
        .iter()
        .find(|(_, p)| p.title == "RLHF Training")
        .unwrap();
    let links = extract_wiki_links(&rlhf.1.body);
    assert!(links.contains(&"transformer-architecture".to_string()));
    assert!(links.contains(&"flash-attention-2".to_string()));

    let resolved = resolve_link("transformer-architecture", &wiki.wiki_dir());
    assert_eq!(
        resolved,
        Some(std::path::PathBuf::from(
            "architecture/transformer-architecture.md"
        ))
    );
}

#[tokio::test]
async fn step6_ingest_source_file() {
    let wiki = TestWiki::new();
    let source = wiki.create_file(
        "incoming/llm-survey-2024.md",
        "# A Survey of Large Language Models (2024)\n\nComprehensive survey covering...",
    );

    let result = ingest_source(wiki.root(), &source, "papers", &NoopLlm)
        .await
        .unwrap();
    assert!(result.raw_path.exists());
    assert!(result.raw_path.starts_with(wiki.root().join("raw/papers")));
    assert!(result.draft.is_none());
}

#[test]
fn step7_broken_link_detection() {
    let wiki = TestWiki::new();
    wiki.with_sample_pages();

    let broken = find_broken_links(
        "See [[transformer-architecture]] and [[nonexistent-page]]",
        &wiki.wiki_dir(),
    );
    assert_eq!(broken, vec!["nonexistent-page"]);
    assert!(!broken.contains(&"transformer-architecture".to_string()));

    let no_broken = find_broken_links(
        "[[transformer-architecture]] and [[flash-attention-2]]",
        &wiki.wiki_dir(),
    );
    assert!(no_broken.is_empty());
}

#[tokio::test]
async fn full_agent_workflow() {
    let wiki = TestWiki::new();
    let loaded = lw_core::fs::load_schema(wiki.root()).unwrap();
    assert_eq!(loaded.wiki.name, "LLM Wiki");

    let pages = wiki.with_sample_pages();

    // Search
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
    assert!(results.total >= 2);

    // Taxonomy
    let all: Vec<_> = pages.iter().map(|(_, p)| p.clone()).collect();
    let tax = Taxonomy::from_pages(&all);
    assert_eq!(tax.tag_count("architecture"), 2);

    // Links
    let rlhf = pages
        .iter()
        .find(|(_, p)| p.title == "RLHF Training")
        .unwrap();
    let links = extract_wiki_links(&rlhf.1.body);
    assert!(links.contains(&"transformer-architecture".to_string()));
    assert!(resolve_link("transformer-architecture", &wiki.wiki_dir()).is_some());

    // Ingest
    let source = wiki.create_file("incoming/paper.txt", "Raw paper content.");
    let ingest = ingest_source(wiki.root(), &source, "papers", &NoopLlm)
        .await
        .unwrap();
    assert!(ingest.raw_path.exists());

    // Broken links
    let broken = find_broken_links(
        "[[transformer-architecture]] and [[ghost-page]]",
        &wiki.wiki_dir(),
    );
    assert_eq!(broken, vec!["ghost-page"]);
}
