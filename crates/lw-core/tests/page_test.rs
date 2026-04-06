use lw_core::page::Page;

#[test]
fn page_new_convenience() {
    let page = Page::new("My Title", &["tag1", "tag2"], "Body content.");
    assert_eq!(page.title, "My Title");
    assert_eq!(page.tags, vec!["tag1", "tag2"]);
    assert_eq!(page.body, "Body content.");
    assert_eq!(page.decay, None);
    assert!(page.sources.is_empty());
    assert_eq!(page.author, None);
    assert_eq!(page.generator, None);
}

#[test]
fn slugify_basic() {
    use lw_core::page::slugify;
    assert_eq!(slugify("Hello World"), "hello-world");
    assert_eq!(slugify("Flash Attention 2"), "flash-attention-2");
    assert_eq!(slugify("  Multiple   Spaces  "), "multiple-spaces");
    assert_eq!(slugify("Special!@#Characters"), "special-characters");
}

#[test]
fn slugify_unicode() {
    use lw_core::page::slugify;
    // Chinese characters should be preserved
    let slug = slugify("创业指南 2024");
    assert!(slug.contains("创业指南"));
}

#[test]
fn parse_full_frontmatter() {
    let md = r#"---
title: Flash Attention 2
tags: [architecture, attention, optimization]
decay: normal
sources: [raw/papers/flash-attention-2.pdf]
author: vergil
generator: kimi
---

Flash Attention 2 reduces memory usage from O(N^2) to O(N).

See also [[transformer]] and [[scaling-laws]].
"#;
    let page = Page::parse(md).unwrap();
    assert_eq!(page.title, "Flash Attention 2");
    assert_eq!(page.tags, vec!["architecture", "attention", "optimization"]);
    assert_eq!(page.decay, Some("normal".to_string()));
    assert_eq!(page.sources, vec!["raw/papers/flash-attention-2.pdf"]);
    assert_eq!(page.author, Some("vergil".to_string()));
    assert_eq!(page.generator, Some("kimi".to_string()));
    assert!(page.body.contains("Flash Attention 2 reduces"));
    assert!(page.body.contains("[[transformer]]"));
}

#[test]
fn parse_minimal_frontmatter() {
    let md = r#"---
title: Backpropagation
---

The chain rule applied to computational graphs.
"#;
    let page = Page::parse(md).unwrap();
    assert_eq!(page.title, "Backpropagation");
    assert!(page.tags.is_empty());
    assert_eq!(page.decay, None);
    assert!(page.body.contains("chain rule"));
}

#[test]
fn parse_missing_title_fails() {
    let md = r#"---
tags: [test]
---

No title here.
"#;
    assert!(Page::parse(md).is_err());
}

#[test]
fn round_trip() {
    let md = r#"---
title: Test Page
tags: [a, b]
decay: fast
sources: [raw/test.pdf]
author: alice
generator: claude
---

Body content here.
"#;
    let page = Page::parse(md).unwrap();
    let rendered = page.to_markdown();
    let reparsed = Page::parse(&rendered).unwrap();
    assert_eq!(page.title, reparsed.title);
    assert_eq!(page.tags, reparsed.tags);
    assert_eq!(page.decay, reparsed.decay);
    assert_eq!(page.sources, reparsed.sources);
    assert_eq!(page.author, reparsed.author);
    assert_eq!(page.generator, reparsed.generator);
    assert_eq!(page.body.trim(), reparsed.body.trim());
}
