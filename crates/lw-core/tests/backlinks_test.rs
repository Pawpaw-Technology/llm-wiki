mod common;

use common::{TestWiki, make_page};
use lw_core::backlinks::{
    BACKLINKS_DIR, BacklinkKind, BacklinkRecord, build_index, ensure_index, extract_link_lines,
    query, rebuild_index, sidecar_path, slug_from_wiki_path, snippet_for, update_for_page,
};
use std::path::Path;

// ─── Pure helpers ─────────────────────────────────────────────────────────────

#[test]
fn extract_link_lines_finds_wikilinks_per_line() {
    let body = "first line has [[foo]]\nsecond line links [[bar]] and [[baz]]\nthird is plain";
    let pairs = extract_link_lines(body);
    let slugs: Vec<&str> = pairs.iter().map(|(s, _)| s.as_str()).collect();
    assert!(slugs.contains(&"foo"), "should find foo: {slugs:?}");
    assert!(slugs.contains(&"bar"), "should find bar: {slugs:?}");
    assert!(slugs.contains(&"baz"), "should find baz: {slugs:?}");
    assert_eq!(slugs.len(), 3, "no extra slugs: {slugs:?}");

    // Each pair must carry the surrounding line so callers can build a snippet.
    for (slug, line) in &pairs {
        assert!(
            line.contains(&format!("[[{slug}")),
            "line for slug {slug} must contain the wikilink: {line}"
        );
    }
}

#[test]
fn extract_link_lines_handles_pipe_display() {
    let body = "ref [[slug-x|some display]] inline";
    let pairs = extract_link_lines(body);
    assert_eq!(pairs.len(), 1, "one wikilink: {pairs:?}");
    assert_eq!(pairs[0].0, "slug-x", "slug must be the pre-pipe portion");
}

#[test]
fn extract_link_lines_includes_code_fenced_links() {
    // Per spec we follow Obsidian's convention: links inside code fences DO count.
    // Documented in the PR description.
    let body = "```\nfn x() { let _ = \"[[code-link]]\"; }\n```";
    let pairs = extract_link_lines(body);
    let slugs: Vec<&str> = pairs.iter().map(|(s, _)| s.as_str()).collect();
    assert!(
        slugs.contains(&"code-link"),
        "code-fenced link counts: {slugs:?}"
    );
}

#[test]
fn snippet_for_centers_around_match() {
    let line =
        "Lorem ipsum dolor sit amet consectetur adipiscing elit, [[transformer]] is great today";
    let snip = snippet_for(line, "transformer");
    assert!(
        snip.contains("[[transformer]]"),
        "must contain match: {snip}"
    );
    // Snippet must be shorter than the full line for long inputs (default radius ~80).
    assert!(snip.len() < line.len() + 8, "should be trimmed: {snip}");
}

#[test]
fn snippet_for_empty_line_returns_empty_string() {
    assert!(snippet_for("", "foo").is_empty());
}

#[test]
fn slug_from_wiki_path_strips_ext_and_dirs() {
    assert_eq!(
        slug_from_wiki_path(Path::new("architecture/transformer.md")),
        Some("transformer".to_string())
    );
    assert_eq!(
        slug_from_wiki_path(Path::new("foo.md")),
        Some("foo".to_string())
    );
    assert_eq!(slug_from_wiki_path(Path::new("")), None);
}

// ─── Index build + query ─────────────────────────────────────────────────────

#[test]
fn build_index_picks_up_wikilinks_and_related() {
    let wiki = TestWiki::new();

    // target page: tools/bar
    let bar = make_page("Bar", &["tools"], "normal", "I am bar.");
    wiki.write_page("tools/bar.md", &bar);

    // source via body wikilink
    let foo = make_page("Foo", &["tools"], "normal", "Use [[bar]] for parsing.");
    wiki.write_page("tools/foo.md", &foo);

    // source via related: frontmatter
    let mut baz = make_page("Baz", &["tools"], "normal", "No body link here.");
    baz.related = Some(vec!["tools/bar.md".to_string()]);
    wiki.write_page("tools/baz.md", &baz);

    let map = build_index(wiki.root()).expect("index must build");
    let sources = map.get("bar").expect("bar must be indexed");
    assert_eq!(
        sources.len(),
        2,
        "two sources: wikilink + related: {sources:?}"
    );

    let kinds: Vec<&BacklinkKind> = sources.iter().map(|s| &s.kind).collect();
    assert!(kinds.contains(&&BacklinkKind::Wikilink));
    assert!(kinds.contains(&&BacklinkKind::Related));

    // Source paths use `wiki/` prefix per spec.
    let paths: Vec<&str> = sources.iter().map(|s| s.path.as_str()).collect();
    assert!(paths.contains(&"wiki/tools/foo.md"));
    assert!(paths.contains(&"wiki/tools/baz.md"));
}

#[test]
fn build_index_pipe_wikilinks_match_by_slug() {
    let wiki = TestWiki::new();
    let target = make_page("Target", &["tools"], "normal", "target body");
    wiki.write_page("tools/target.md", &target);

    let src = make_page(
        "Src",
        &["tools"],
        "normal",
        "Read [[target|the awesome target]] now",
    );
    wiki.write_page("tools/src.md", &src);

    let map = build_index(wiki.root()).expect("build");
    let sources = map.get("target").expect("target must be indexed");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].path, "wiki/tools/src.md");
    let ctx = sources[0].context.as_deref().unwrap_or("");
    assert!(
        ctx.contains("[[target"),
        "context should include wikilink: {ctx}"
    );
}

#[test]
fn rebuild_index_writes_sidecar_files() {
    let wiki = TestWiki::new();
    let target = make_page("T", &["tools"], "normal", "");
    wiki.write_page("tools/t.md", &target);
    let src = make_page("S", &["tools"], "normal", "See [[t]].");
    wiki.write_page("tools/s.md", &src);

    rebuild_index(wiki.root()).expect("rebuild");
    let path = sidecar_path(wiki.root(), "t");
    assert!(path.exists(), "sidecar must be written: {path:?}");

    let raw = std::fs::read_to_string(&path).unwrap();
    let record: BacklinkRecord = serde_json::from_str(&raw).unwrap();
    assert_eq!(record.target, "t");
    assert_eq!(record.sources.len(), 1);
    assert_eq!(record.sources[0].path, "wiki/tools/s.md");
}

#[test]
fn query_returns_none_when_no_inbound_links() {
    let wiki = TestWiki::new();
    let res = query(wiki.root(), "nonexistent").expect("query ok");
    assert!(res.is_none(), "no sidecar => None: {res:?}");
}

#[test]
fn query_returns_record_after_rebuild() {
    let wiki = TestWiki::new();
    let target = make_page("Target", &["tools"], "normal", "");
    wiki.write_page("tools/target.md", &target);
    let src = make_page("Src", &["tools"], "normal", "Use [[target]].");
    wiki.write_page("tools/src.md", &src);

    rebuild_index(wiki.root()).expect("rebuild");
    let record = query(wiki.root(), "target")
        .expect("query ok")
        .expect("Some record");
    assert_eq!(record.target, "target");
    assert_eq!(record.sources.len(), 1);
    let snippet = record.sources[0]
        .context
        .as_deref()
        .expect("wikilink source carries snippet");
    assert!(
        snippet.contains("[[target]]"),
        "snippet should center on the link: {snippet}"
    );
}

// ─── Incremental updates ─────────────────────────────────────────────────────

#[test]
fn update_for_page_creates_sidecar_for_new_links() {
    let wiki = TestWiki::new();
    let target = make_page("T", &["tools"], "normal", "t");
    wiki.write_page("tools/t.md", &target);
    let src = make_page("S", &["tools"], "normal", "See [[t]] here.");
    wiki.write_page("tools/s.md", &src);

    update_for_page(wiki.root(), Path::new("tools/s.md")).expect("update");
    let record = query(wiki.root(), "t")
        .expect("query ok")
        .expect("sidecar created");
    assert_eq!(record.sources[0].path, "wiki/tools/s.md");
}

#[test]
fn update_for_page_removes_dropped_links() {
    let wiki = TestWiki::new();
    let target = make_page("T", &["tools"], "normal", "t");
    wiki.write_page("tools/t.md", &target);

    // Initial: source links to T
    let src_v1 = make_page("S", &["tools"], "normal", "Use [[t]] often.");
    wiki.write_page("tools/s.md", &src_v1);
    update_for_page(wiki.root(), Path::new("tools/s.md")).expect("v1 update");
    assert!(query(wiki.root(), "t").unwrap().is_some());

    // Updated: source no longer mentions T
    let src_v2 = make_page("S", &["tools"], "normal", "All references removed.");
    wiki.write_page("tools/s.md", &src_v2);
    update_for_page(wiki.root(), Path::new("tools/s.md")).expect("v2 update");

    let after = query(wiki.root(), "t").expect("query ok");
    assert!(
        after.is_none(),
        "sidecar should be removed when the only source drops the link: {after:?}"
    );
}

#[test]
fn update_for_page_preserves_other_sources() {
    let wiki = TestWiki::new();
    let target = make_page("T", &["tools"], "normal", "t");
    wiki.write_page("tools/t.md", &target);
    let a = make_page("A", &["tools"], "normal", "From A: [[t]].");
    wiki.write_page("tools/a.md", &a);
    let b = make_page("B", &["tools"], "normal", "From B: [[t]] also.");
    wiki.write_page("tools/b.md", &b);

    rebuild_index(wiki.root()).expect("rebuild");
    let before = query(wiki.root(), "t").unwrap().unwrap();
    assert_eq!(before.sources.len(), 2);

    // Drop B's link only.
    let b_v2 = make_page("B", &["tools"], "normal", "B no longer references it.");
    wiki.write_page("tools/b.md", &b_v2);
    update_for_page(wiki.root(), Path::new("tools/b.md")).expect("update b");

    let after = query(wiki.root(), "t").unwrap().unwrap();
    let paths: Vec<&str> = after.sources.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, vec!["wiki/tools/a.md"], "only A remains: {paths:?}");
}

#[test]
fn ensure_index_builds_when_missing() {
    let wiki = TestWiki::new();
    let t = make_page("T", &["tools"], "normal", "t");
    wiki.write_page("tools/t.md", &t);
    let s = make_page("S", &["tools"], "normal", "See [[t]].");
    wiki.write_page("tools/s.md", &s);

    let dir = wiki.root().join(BACKLINKS_DIR);
    assert!(!dir.exists(), "precondition: dir absent");
    ensure_index(wiki.root()).expect("ensure ok");
    assert!(dir.exists(), "ensure_index must create dir");
    assert!(query(wiki.root(), "t").unwrap().is_some());
}
