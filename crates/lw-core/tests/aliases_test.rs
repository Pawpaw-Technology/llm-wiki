//! Integration tests for `lw_core::aliases` — see issue #100.
//!
//! Covers the acceptance bullets of #100 (term sources, normalization,
//! persistence, sentinel short-circuit, incremental updates) and the unicode
//! / multi-word edge cases called out in the implementation notes.

mod common;

use common::{make_page, TestWiki};
use lw_core::aliases::{
    build_index, ensure_index, normalize, rebuild_index, update_for_page, AliasIndex, PageRef,
    ALIASES_DIR,
};
use std::path::Path;

// ─── Normalization (lowercase + NFC) ─────────────────────────────────────────

#[test]
fn normalize_lowercases_ascii() {
    assert_eq!(normalize("Flash Attention"), "flash attention");
    assert_eq!(normalize("FOO"), "foo");
    assert_eq!(normalize("MixedCase"), "mixedcase");
}

#[test]
fn normalize_is_idempotent() {
    let once = normalize("Tantivy");
    let twice = normalize(&once);
    assert_eq!(once, twice, "double normalization must be a no-op");
}

#[test]
fn normalize_preserves_cjk_characters_unchanged() {
    // CJK has no case; lowercasing must leave the characters intact.
    let input = "创业指南";
    assert_eq!(normalize(input), input);
}

#[test]
fn normalize_collapses_decomposed_unicode_to_nfc() {
    // 'é' can be one precomposed code point (U+00E9) OR 'e' + U+0301 (combining
    // acute accent). NFC normalization collapses the second form to the first.
    let decomposed = "Cafe\u{0301}"; // C-a-f-e-´ (4 chars + combining)
    let nfc = "Café"; // C-a-f-é (precomposed)
    assert_eq!(
        normalize(decomposed),
        normalize(nfc),
        "decomposed and precomposed must normalize to the same string"
    );
    // The normalized output must be the precomposed (NFC) form.
    assert_eq!(normalize(decomposed), nfc.to_lowercase());
}

// ─── Build: term sources (title + aliases + slug) ────────────────────────────

#[test]
fn build_index_uses_title_as_term() {
    let wiki = TestWiki::new();
    let page = make_page("Transformer Architecture", &["arch"], "normal", "body");
    wiki.write_page("architecture/transformer-architecture.md", &page);

    let index = build_index(wiki.root()).expect("build");
    let hits = index.lookup("Transformer Architecture");
    assert_eq!(hits.len(), 1, "title must index the page: {hits:?}");
    assert_eq!(hits[0].slug, "transformer-architecture");
}

#[test]
fn build_index_uses_slug_from_filename_as_term() {
    let wiki = TestWiki::new();
    let page = make_page("Some Long Title", &["arch"], "normal", "body");
    wiki.write_page("architecture/short-slug.md", &page);

    let index = build_index(wiki.root()).expect("build");
    let hits = index.lookup("short-slug");
    assert_eq!(
        hits.len(),
        1,
        "slug derived from filename must be a term: {hits:?}"
    );
    assert_eq!(hits[0].title, "Some Long Title");
}

#[test]
fn build_index_uses_aliases_list_as_terms() {
    let wiki = TestWiki::new();
    let mut page = make_page("Real Title", &["arch"], "normal", "body");
    page.aliases = vec!["nickname".to_string(), "ALIAS-TWO".to_string()];
    wiki.write_page("architecture/real.md", &page);

    let index = build_index(wiki.root()).expect("build");
    assert_eq!(index.lookup("nickname").len(), 1, "first alias indexed");
    assert_eq!(
        index.lookup("alias-two").len(),
        1,
        "second alias indexed (case-folded)"
    );
}

#[test]
fn build_index_alias_field_missing_is_treated_as_empty() {
    // No `aliases:` key at all in frontmatter (the default for existing vaults).
    let wiki = TestWiki::new();
    let page = make_page("Plain Page", &["arch"], "normal", "body");
    // page.aliases is empty by default — this mimics absence of the field.
    wiki.write_page("architecture/plain.md", &page);

    let index = build_index(wiki.root()).expect("build");
    // Page is still indexed by title + slug.
    assert_eq!(
        index.lookup("Plain Page").len(),
        1,
        "title still indexed when aliases missing"
    );
    assert_eq!(
        index.lookup("plain").len(),
        1,
        "slug still indexed when aliases missing"
    );
}

#[test]
fn build_index_alias_field_empty_list_adds_no_extra_terms() {
    let wiki = TestWiki::new();
    let mut page = make_page("Title Only", &["arch"], "normal", "body");
    page.aliases = vec![]; // explicit empty list
    wiki.write_page("architecture/title-only.md", &page);

    let index = build_index(wiki.root()).expect("build");
    // Only title + slug — no extras.
    let total: usize = index.terms.values().map(|v| v.len()).sum();
    assert_eq!(
        total, 2,
        "title + slug => exactly 2 PageRef entries: {:?}",
        index.terms
    );
}

// ─── Build: edge cases ──────────────────────────────────────────────────────

#[test]
fn build_index_multi_word_title_stored_verbatim_not_tokenized() {
    // Per implementation notes: "Multi-word titles stored verbatim in the
    // term index. Windowed scan happens in the matcher (next sub-issue)."
    let wiki = TestWiki::new();
    let page = make_page("Flash Attention 2", &["arch"], "normal", "body");
    wiki.write_page("architecture/flash-attention-2.md", &page);

    let index = build_index(wiki.root()).expect("build");
    // Whole multi-word title is one term:
    assert_eq!(index.lookup("flash attention 2").len(), 1);
    // Individual tokens are NOT terms here (matcher's job, not index's):
    assert!(
        index.lookup("attention").is_empty(),
        "individual tokens must not be split by the index: {:?}",
        index.terms
    );
}

#[test]
fn build_index_preserves_original_title_casing_in_pageref() {
    let wiki = TestWiki::new();
    let page = make_page("CamelCase Page", &["arch"], "normal", "body");
    wiki.write_page("architecture/camel.md", &page);

    let index = build_index(wiki.root()).expect("build");
    let hits = index.lookup("camelcase page");
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].title, "CamelCase Page",
        "PageRef.title preserves original casing"
    );
}

#[test]
fn build_index_pageref_path_uses_wiki_prefix() {
    let wiki = TestWiki::new();
    let page = make_page("X", &["arch"], "normal", "body");
    wiki.write_page("architecture/x.md", &page);

    let index = build_index(wiki.root()).expect("build");
    let hits = index.lookup("x");
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].path, "wiki/architecture/x.md",
        "path must include the `wiki/` prefix per project convention"
    );
}

#[test]
fn build_index_unicode_cjk_title_indexed_intact() {
    let wiki = TestWiki::new();
    let page = make_page("创业指南", &["startup"], "normal", "body");
    wiki.write_page("_uncategorized/startup-guide.md", &page);

    let index = build_index(wiki.root()).expect("build");
    let hits = index.lookup("创业指南");
    assert_eq!(
        hits.len(),
        1,
        "CJK title must be findable verbatim: {:?}",
        index.terms
    );
}

#[test]
fn build_index_empty_wiki_returns_empty_index() {
    let wiki = TestWiki::new();
    let index = build_index(wiki.root()).expect("build");
    assert!(index.terms.is_empty(), "empty wiki => empty index");
    assert!(index.lookup("anything").is_empty());
}

#[test]
fn build_index_ambiguous_term_lists_all_pages() {
    let wiki = TestWiki::new();
    let mut a = make_page("A", &["arch"], "normal", "body");
    a.aliases = vec!["common".to_string()];
    wiki.write_page("architecture/a.md", &a);
    let mut b = make_page("B", &["arch"], "normal", "body");
    b.aliases = vec!["common".to_string()];
    wiki.write_page("architecture/b.md", &b);

    let index = build_index(wiki.root()).expect("build");
    let hits = index.lookup("common");
    assert_eq!(
        hits.len(),
        2,
        "ambiguous term must list both pages — consumer decides how to surface: {hits:?}"
    );
    let slugs: Vec<&str> = hits.iter().map(|p| p.slug.as_str()).collect();
    assert!(slugs.contains(&"a"));
    assert!(slugs.contains(&"b"));
}

// ─── Lookup ──────────────────────────────────────────────────────────────────

#[test]
fn lookup_normalizes_input_so_callers_pass_raw_text() {
    let wiki = TestWiki::new();
    let page = make_page("Tantivy", &["tools"], "normal", "body");
    wiki.write_page("tools/tantivy.md", &page);

    let index = build_index(wiki.root()).expect("build");
    // Same page resolves whether caller passes uppercased, mixed, or lowercased.
    assert_eq!(index.lookup("TANTIVY").len(), 1);
    assert_eq!(index.lookup("Tantivy").len(), 1);
    assert_eq!(index.lookup("tantivy").len(), 1);
}

#[test]
fn lookup_returns_empty_for_unknown_term() {
    let wiki = TestWiki::new();
    let page = make_page("Real", &["tools"], "normal", "body");
    wiki.write_page("tools/real.md", &page);

    let index = build_index(wiki.root()).expect("build");
    assert!(index.lookup("nonexistent").is_empty());
}

// ─── Persistence: rebuild + sentinel ─────────────────────────────────────────

#[test]
fn rebuild_writes_index_json_under_aliases_dir() {
    let wiki = TestWiki::new();
    let page = make_page("Foo", &["tools"], "normal", "body");
    wiki.write_page("tools/foo.md", &page);

    rebuild_index(wiki.root()).expect("rebuild");
    let index_file = wiki.root().join(ALIASES_DIR).join("index.json");
    assert!(
        index_file.exists(),
        "index.json must exist after rebuild: {index_file:?}"
    );
    // The serialized blob round-trips into an AliasIndex with at least one term.
    let raw = std::fs::read_to_string(&index_file).unwrap();
    let parsed: AliasIndex = serde_json::from_str(&raw).expect("valid JSON shape");
    assert!(
        !parsed.terms.is_empty(),
        "persisted index must contain the page's terms"
    );
}

#[test]
fn rebuild_writes_built_sentinel() {
    let wiki = TestWiki::new();
    let page = make_page("Foo", &["tools"], "normal", "body");
    wiki.write_page("tools/foo.md", &page);

    rebuild_index(wiki.root()).expect("rebuild");
    let sentinel = wiki.root().join(ALIASES_DIR).join(".built");
    assert!(
        sentinel.exists(),
        "rebuild must write the .built sentinel: {sentinel:?}"
    );
}

#[test]
fn ensure_index_builds_when_missing() {
    let wiki = TestWiki::new();
    let page = make_page("Foo", &["tools"], "normal", "body");
    wiki.write_page("tools/foo.md", &page);

    let dir = wiki.root().join(ALIASES_DIR);
    assert!(!dir.exists(), "precondition: aliases dir absent");
    ensure_index(wiki.root()).expect("ensure ok");
    assert!(
        dir.exists(),
        "ensure_index must create the aliases directory"
    );
    let index_file = dir.join("index.json");
    assert!(index_file.exists(), "ensure_index must write index.json");
}

/// Mirrors the backlinks `ensure_index_skips_rebuild_when_already_built`
/// regression: after the first `ensure_index` builds the index, subsequent
/// calls must short-circuit on the `.built` sentinel rather than re-walking
/// `wiki/`. Detection: add a brand-new page after the first call; the second
/// `ensure_index` must NOT pick it up — only an explicit `rebuild_index` or
/// `update_for_page` should.
#[test]
fn ensure_index_skips_rebuild_when_already_built() {
    let wiki = TestWiki::new();
    let first = make_page("First", &["tools"], "normal", "body");
    wiki.write_page("tools/first.md", &first);

    ensure_index(wiki.root()).expect("first ensure ok");

    // Add a brand-new page without calling update_for_page.
    let stealth = make_page("Stealth", &["tools"], "normal", "body");
    wiki.write_page("tools/stealth.md", &stealth);

    ensure_index(wiki.root()).expect("second ensure ok");

    // Reload the persisted index and confirm Stealth is absent. The second
    // `ensure_index` must short-circuit; if it re-walked, Stealth would now
    // be in the persisted index.
    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let persisted: AliasIndex = serde_json::from_str(&raw).expect("parse");
    assert!(
        persisted.lookup("stealth").is_empty(),
        "ensure_index must short-circuit on .built — found Stealth in persisted index: {:?}",
        persisted.terms
    );
}

// ─── Incremental updates ─────────────────────────────────────────────────────

#[test]
fn update_for_page_indexes_a_brand_new_page() {
    let wiki = TestWiki::new();
    let page = make_page("Brand New", &["tools"], "normal", "body");
    wiki.write_page("tools/brand-new.md", &page);

    update_for_page(wiki.root(), Path::new("tools/brand-new.md")).expect("update");

    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let persisted: AliasIndex = serde_json::from_str(&raw).expect("parse");
    let hits = persisted.lookup("Brand New");
    assert_eq!(
        hits.len(),
        1,
        "update_for_page must persist the new page: {:?}",
        persisted.terms
    );
    assert_eq!(hits[0].slug, "brand-new");
}

#[test]
fn update_for_page_drops_aliases_removed_in_new_version() {
    let wiki = TestWiki::new();
    let mut v1 = make_page("Page", &["tools"], "normal", "body");
    v1.aliases = vec!["alias-keep".to_string(), "alias-drop".to_string()];
    wiki.write_page("tools/page.md", &v1);
    update_for_page(wiki.root(), Path::new("tools/page.md")).expect("v1 update");

    // Reload, verify v1 state.
    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let after_v1: AliasIndex = serde_json::from_str(&raw).unwrap();
    assert_eq!(after_v1.lookup("alias-keep").len(), 1);
    assert_eq!(after_v1.lookup("alias-drop").len(), 1);

    // v2: drop one alias.
    let mut v2 = make_page("Page", &["tools"], "normal", "body");
    v2.aliases = vec!["alias-keep".to_string()];
    wiki.write_page("tools/page.md", &v2);
    update_for_page(wiki.root(), Path::new("tools/page.md")).expect("v2 update");

    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let after_v2: AliasIndex = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        after_v2.lookup("alias-keep").len(),
        1,
        "kept alias still resolves"
    );
    assert!(
        after_v2.lookup("alias-drop").is_empty(),
        "dropped alias must be removed: {:?}",
        after_v2.terms
    );
}

#[test]
fn update_for_page_handles_title_rename() {
    let wiki = TestWiki::new();
    let v1 = make_page("Old Title", &["tools"], "normal", "body");
    wiki.write_page("tools/page.md", &v1);
    update_for_page(wiki.root(), Path::new("tools/page.md")).expect("v1 update");

    // Rename: same file, different title.
    let v2 = make_page("New Title", &["tools"], "normal", "body");
    wiki.write_page("tools/page.md", &v2);
    update_for_page(wiki.root(), Path::new("tools/page.md")).expect("v2 update");

    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let after: AliasIndex = serde_json::from_str(&raw).unwrap();
    assert!(
        after.lookup("old title").is_empty(),
        "old title must be dropped: {:?}",
        after.terms
    );
    assert_eq!(
        after.lookup("new title").len(),
        1,
        "new title must be present"
    );
    // Slug term stays (filename did not change).
    assert_eq!(after.lookup("page").len(), 1, "slug term still resolves");
}

#[test]
fn update_for_page_handles_deleted_page() {
    let wiki = TestWiki::new();
    let page = make_page("Doomed", &["tools"], "normal", "body");
    wiki.write_page("tools/doomed.md", &page);
    update_for_page(wiki.root(), Path::new("tools/doomed.md")).expect("create update");

    // Delete the page.
    std::fs::remove_file(wiki.root().join("wiki/tools/doomed.md")).unwrap();
    update_for_page(wiki.root(), Path::new("tools/doomed.md")).expect("delete update");

    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let after: AliasIndex = serde_json::from_str(&raw).unwrap();
    assert!(
        after.lookup("doomed").is_empty(),
        "deleted page must be stripped from the index: {:?}",
        after.terms
    );
}

#[test]
fn update_for_page_preserves_other_pages_terms() {
    let wiki = TestWiki::new();
    let a = make_page("Alpha", &["tools"], "normal", "body");
    wiki.write_page("tools/alpha.md", &a);
    let b = make_page("Beta", &["tools"], "normal", "body");
    wiki.write_page("tools/beta.md", &b);

    rebuild_index(wiki.root()).expect("rebuild");

    // Touch only Alpha.
    let mut a_v2 = make_page("Alpha", &["tools"], "normal", "body");
    a_v2.aliases = vec!["a-alias".to_string()];
    wiki.write_page("tools/alpha.md", &a_v2);
    update_for_page(wiki.root(), Path::new("tools/alpha.md")).expect("update alpha");

    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let after: AliasIndex = serde_json::from_str(&raw).unwrap();
    assert_eq!(after.lookup("alpha").len(), 1, "alpha still indexed");
    assert_eq!(after.lookup("a-alias").len(), 1, "alpha alias added");
    assert_eq!(after.lookup("beta").len(), 1, "beta untouched");
}

#[test]
fn update_for_page_returns_index_path_when_changed() {
    // Auto-commit (Option A) needs the sidecar paths so they land in the same
    // commit as the page. Mirrors backlinks::update_for_page's return shape.
    let wiki = TestWiki::new();
    let page = make_page("Foo", &["tools"], "normal", "body");
    wiki.write_page("tools/foo.md", &page);

    let written =
        update_for_page(wiki.root(), Path::new("tools/foo.md")).expect("update returns paths");
    assert!(
        !written.is_empty(),
        "writing a page must report at least one updated sidecar path"
    );
    let expected = wiki.root().join(ALIASES_DIR).join("index.json");
    assert!(
        written.iter().any(|p| p == &expected),
        "returned paths must include {expected:?}; got {written:?}"
    );
}

// ─── Regressions: PR #113 review ─────────────────────────────────────────────

/// Regression for PR #113 review item 1: `update_for_page` must not write the
/// `.built` sentinel — a partial incremental update is not a substitute for
/// a full rebuild. If the sentinel were written by an incremental update on
/// a fresh vault, a later `ensure_index` would short-circuit and leave every
/// untouched page absent from alias lookups.
///
/// Detection: in a vault with two pages, call `update_for_page` on one
/// (skipping `ensure_index`) and then call `ensure_index`. If the sentinel
/// was incorrectly written, `ensure_index` short-circuits and the untouched
/// page never makes it into the index.
#[test]
fn update_for_page_does_not_mark_index_as_fully_built() {
    let wiki = TestWiki::new();
    let touched = make_page("Touched", &["tools"], "normal", "body");
    wiki.write_page("tools/touched.md", &touched);
    let untouched = make_page("Untouched", &["tools"], "normal", "body");
    wiki.write_page("tools/untouched.md", &untouched);

    // Incremental update on the first page only — no ensure_index beforehand.
    update_for_page(wiki.root(), Path::new("tools/touched.md")).expect("update");

    // The next ensure_index MUST do a full rebuild (since we never built the
    // index from scratch). If the incremental update wrote `.built`, this
    // call short-circuits and leaves the untouched page out forever.
    ensure_index(wiki.root()).expect("ensure");

    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let persisted: AliasIndex = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        persisted.lookup("untouched").len(),
        1,
        "ensure_index after a partial incremental update must still rebuild \
         the full vault — Untouched is missing: {:?}",
        persisted.terms
    );
}

/// Regression for PR #113 review item 2: page identity in the index is the
/// full `wiki/<category>/<file>.md` path, not the filename slug. `lw new`
/// allows two pages with the same filename in different categories (only the
/// exact path is uniqueness-checked), so `update_for_page` must strip stale
/// entries by path — otherwise editing one of two same-slug pages wipes the
/// other from the index until a full rebuild.
#[test]
fn update_for_page_does_not_strip_same_slug_pages_in_other_categories() {
    let wiki = TestWiki::new();
    let tools_foo = make_page("Tools Foo", &["tools"], "normal", "body");
    wiki.write_page("tools/foo.md", &tools_foo);
    let arch_foo = make_page("Arch Foo", &["architecture"], "normal", "body");
    wiki.write_page("architecture/foo.md", &arch_foo);

    rebuild_index(wiki.root()).expect("rebuild");

    // Sanity: term "foo" (the shared slug) resolves to BOTH pages before any
    // incremental update runs.
    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let before: AliasIndex = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        before.lookup("foo").len(),
        2,
        "precondition: both same-slug pages indexed: {:?}",
        before.terms
    );

    // Edit only tools/foo.md.
    let tools_foo_v2 = make_page("Tools Foo v2", &["tools"], "normal", "body");
    wiki.write_page("tools/foo.md", &tools_foo_v2);
    update_for_page(wiki.root(), Path::new("tools/foo.md")).expect("update tools/foo");

    let raw = std::fs::read_to_string(wiki.root().join(ALIASES_DIR).join("index.json")).unwrap();
    let after: AliasIndex = serde_json::from_str(&raw).unwrap();

    // The architecture page must still be in the index — its title and slug
    // entries must survive an unrelated edit on a same-slug sibling.
    let arch_paths: Vec<&str> = after
        .lookup("foo")
        .iter()
        .filter(|p| p.path == "wiki/architecture/foo.md")
        .map(|p| p.path.as_str())
        .collect();
    assert_eq!(
        arch_paths.len(),
        1,
        "architecture/foo.md must survive an edit on tools/foo.md: {:?}",
        after.terms
    );
    assert_eq!(
        after.lookup("arch foo").len(),
        1,
        "architecture/foo.md's title term must still resolve: {:?}",
        after.terms
    );
    // And the just-edited page is reflected with the new title.
    assert_eq!(after.lookup("tools foo v2").len(), 1, "new title indexed");
    assert!(
        after.lookup("tools foo").is_empty(),
        "old title dropped for the edited page: {:?}",
        after.terms
    );
}

// ─── Public API surface check ────────────────────────────────────────────────

/// Compile-time assertion: PageRef must be Clone + serde-serializable so it
/// can be embedded into MCP responses and CLI JSON output downstream (#102/#103).
#[test]
fn pageref_is_serializable() {
    let p = PageRef {
        slug: "x".into(),
        title: "X".into(),
        path: "wiki/x.md".into(),
    };
    let json = serde_json::to_string(&p).expect("serialize");
    let back: PageRef = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(p, back);
}
