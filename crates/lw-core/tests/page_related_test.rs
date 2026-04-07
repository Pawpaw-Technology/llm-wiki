use lw_core::page::Page;

#[test]
fn parse_preserves_related_field() {
    let md = r#"---
title: "Test Page"
tags: [test]
related:
  - ops/adr-011.md
  - ops/adr-017.md
---

Body content here.
"#;
    let page = Page::parse(md).unwrap();
    assert_eq!(
        page.related,
        Some(vec![
            "ops/adr-011.md".to_string(),
            "ops/adr-017.md".to_string(),
        ])
    );
}

#[test]
fn roundtrip_preserves_related() {
    let md = r#"---
title: "Test Page"
tags: [test]
related:
  - ops/adr-011.md
---

Body.
"#;
    let page = Page::parse(md).unwrap();
    let output = page.to_markdown();
    let reparsed = Page::parse(&output).unwrap();
    assert_eq!(reparsed.related, Some(vec!["ops/adr-011.md".to_string()]));
}

#[test]
fn parse_without_related_gives_none() {
    let md = r#"---
title: "No Related"
tags: []
---

Body.
"#;
    let page = Page::parse(md).unwrap();
    assert_eq!(page.related, None);
}

#[test]
fn empty_related_serializes_as_none() {
    let page = Page::new("Test", &[], "Body");
    let md = page.to_markdown();
    assert!(!md.contains("related"));
}
