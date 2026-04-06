use lw_core::page::Page;
use lw_core::tag::Taxonomy;

fn make_page(title: &str, tags: &[&str]) -> Page {
    Page {
        title: title.to_string(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        body: String::new(),
    }
}

#[test]
fn collect_tags_from_pages() {
    let pages = vec![
        make_page("A", &["transformer", "attention"]),
        make_page("B", &["attention", "optimization"]),
        make_page("C", &["transformer"]),
    ];
    let tax = Taxonomy::from_pages(&pages);
    assert_eq!(tax.tag_count("transformer"), 2);
    assert_eq!(tax.tag_count("attention"), 2);
    assert_eq!(tax.tag_count("optimization"), 1);
    assert_eq!(tax.tag_count("nonexistent"), 0);
}

#[test]
fn all_tags_sorted() {
    let pages = vec![
        make_page("A", &["z-tag", "a-tag"]),
        make_page("B", &["m-tag"]),
    ];
    let tax = Taxonomy::from_pages(&pages);
    let all = tax.all_tags();
    assert_eq!(all, vec!["a-tag", "m-tag", "z-tag"]);
}

#[test]
fn pages_with_tag() {
    let pages = vec![
        make_page("A", &["shared"]),
        make_page("B", &["other"]),
        make_page("C", &["shared", "other"]),
    ];
    let tax = Taxonomy::from_pages(&pages);
    assert_eq!(tax.pages_with_tag("shared"), vec!["A", "C"]);
}

#[test]
fn tag_counts_sorted_by_count() {
    let pages = vec![
        make_page("A", &["popular", "rare"]),
        make_page("B", &["popular", "medium"]),
        make_page("C", &["popular"]),
        make_page("D", &["medium"]),
    ];
    let tax = Taxonomy::from_pages(&pages);
    let counts = tax.tag_counts();
    assert_eq!(counts[0], ("popular".to_string(), 3));
    assert_eq!(counts[1], ("medium".to_string(), 2));
    assert_eq!(counts[2], ("rare".to_string(), 1));
}
