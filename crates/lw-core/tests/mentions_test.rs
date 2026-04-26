//! Integration tests for `lw_core::mentions::find_unlinked_mentions` — issue #101.
//!
//! Covers every acceptance bullet of #101 plus the performance contract lifted
//! from parent #42 (< 100ms for a 500-page wiki).

mod common;

use common::{TestWiki, make_page};
use lw_core::aliases::{AliasIndex, build_index};
use lw_core::mentions::{MAX_WINDOW_TOKENS, UnlinkedMention, find_unlinked_mentions};

// ─── Tiny helpers (reduce per-test boilerplate) ─────────────────────────────

/// Build an alias index containing exactly the given (rel_path, page) pairs.
fn index_with(pages: &[(&str, lw_core::page::Page)]) -> (TestWiki, AliasIndex) {
    let wiki = TestWiki::new();
    for (rel, page) in pages {
        wiki.write_page(rel, page);
    }
    let index = build_index(wiki.root()).expect("build index");
    (wiki, index)
}

/// Convenience: find mentions on an empty self_slug ("") so no self-ref guard
/// kicks in. Use the explicit API in tests that exercise the guard.
fn mentions_for(body: &str, index: &AliasIndex) -> Vec<UnlinkedMention> {
    find_unlinked_mentions(body, index, "")
}

// ─── Spec acceptance bullet: empty body ─────────────────────────────────────

#[test]
fn empty_body_returns_no_mentions() {
    let (_wiki, index) = index_with(&[(
        "tools/transformer.md",
        make_page("Transformer", &["arch"], "normal", "body"),
    )]);
    let hits = mentions_for("", &index);
    assert!(hits.is_empty(), "empty body must produce no mentions");
}

// ─── Spec acceptance bullet: empty index ────────────────────────────────────

#[test]
fn empty_index_returns_no_mentions() {
    let index = AliasIndex::default();
    let body = "Transformer architecture is widely used.";
    let hits = mentions_for(body, &index);
    assert!(
        hits.is_empty(),
        "empty index must produce no mentions: {hits:?}"
    );
}

// ─── Spec acceptance bullet: single-token match (case-insensitive) ──────────

#[test]
fn case_insensitive_single_token_match() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    // Body contains the page title with different casing.
    let body = "We use TANTIVY for full-text search.";
    let hits = mentions_for(body, &index);

    assert_eq!(
        hits.len(),
        1,
        "case-insensitive title hit expected: {hits:?}"
    );
    assert_eq!(hits[0].target_slug, "tantivy");
    assert_eq!(hits[0].line, 1);
    assert!(
        hits[0].context.contains("TANTIVY"),
        "context must include the matched text: {:?}",
        hits[0].context
    );
}

// ─── Spec acceptance bullet: multi-word title ───────────────────────────────

#[test]
fn multi_word_title_match_uses_token_window() {
    let (_wiki, index) = index_with(&[(
        "architecture/flash-attention-2.md",
        make_page("Flash Attention 2", &["arch"], "normal", "body"),
    )]);

    let body = "We benchmarked flash attention 2 against the baseline.";
    let hits = mentions_for(body, &index);

    assert_eq!(hits.len(), 1, "multi-word title must match: {hits:?}");
    assert_eq!(hits[0].target_slug, "flash-attention-2");
    // The matched term should reflect the body casing (not the indexed casing).
    assert!(
        hits[0].term.to_lowercase().contains("flash attention 2"),
        "matched term must reflect body text: {:?}",
        hits[0].term
    );
}

#[test]
fn multi_word_match_is_greedy_longest_wins() {
    // Both "Attention" and "Flash Attention 2" are pages. The body says
    // "flash attention 2" — the matcher must surface the longest match
    // (Flash Attention 2), not also "Attention" inside it.
    let (_wiki, index) = index_with(&[
        (
            "architecture/attention.md",
            make_page("Attention", &["arch"], "normal", "body"),
        ),
        (
            "architecture/flash-attention-2.md",
            make_page("Flash Attention 2", &["arch"], "normal", "body"),
        ),
    ]);

    let body = "Read about flash attention 2 here.";
    let hits = mentions_for(body, &index);

    let slugs: Vec<&str> = hits.iter().map(|h| h.target_slug.as_str()).collect();
    assert!(
        slugs.contains(&"flash-attention-2"),
        "longest greedy match (flash-attention-2) must be present: {hits:?}"
    );
    assert!(
        !slugs.contains(&"attention"),
        "shorter prefix (attention) must NOT also fire when consumed by the \
         longer match: {hits:?}"
    );
}

// ─── Spec acceptance bullet: alias match ────────────────────────────────────

#[test]
fn alias_match_resolves_via_index() {
    let mut page = make_page("Tantivy", &["tools"], "normal", "body");
    page.aliases = vec!["tantivy-search".to_string()];
    let (_wiki, index) = index_with(&[("tools/tantivy.md", page)]);

    let body = "We migrated from Lucene to tantivy-search last year.";
    let hits = mentions_for(body, &index);

    assert_eq!(hits.len(), 1, "alias must resolve to its page: {hits:?}");
    assert_eq!(hits[0].target_slug, "tantivy");
}

// ─── Spec acceptance bullet: already-linked exclusion ───────────────────────

#[test]
fn already_linked_term_is_excluded() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    let body = "We use [[tantivy]] for full-text search.";
    let hits = mentions_for(body, &index);

    assert!(
        hits.is_empty(),
        "term inside [[…]] must not be flagged: {hits:?}"
    );
}

#[test]
fn already_linked_with_pipe_alias_is_excluded() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    let body = "We use [[tantivy|the search engine]] here.";
    let hits = mentions_for(body, &index);

    assert!(
        hits.is_empty(),
        "term inside [[a|b]] must not be flagged: {hits:?}"
    );
}

#[test]
fn unlinked_term_on_same_line_as_a_link_is_still_flagged() {
    // "[[Foo]]" should not gate every other token on that line.
    let (_wiki, index) = index_with(&[
        (
            "tools/foo.md",
            make_page("Foo", &["tools"], "normal", "body"),
        ),
        (
            "tools/bar.md",
            make_page("Bar", &["tools"], "normal", "body"),
        ),
    ]);

    let body = "See [[foo]] and also bar for details.";
    let hits = mentions_for(body, &index);

    let slugs: Vec<&str> = hits.iter().map(|h| h.target_slug.as_str()).collect();
    assert!(slugs.contains(&"bar"), "bar (unlinked) must fire: {hits:?}");
    assert!(
        !slugs.contains(&"foo"),
        "foo (already linked) must not fire: {hits:?}"
    );
}

// ─── Spec acceptance bullet: code-block exclusion ───────────────────────────

#[test]
fn fenced_code_block_content_is_excluded() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    let body = "Some prose.\n```rust\nlet x = tantivy::Index::create();\n```\nMore prose.";
    let hits = mentions_for(body, &index);

    assert!(
        hits.is_empty(),
        "tantivy inside ``` fence must be excluded: {hits:?}"
    );
}

#[test]
fn text_after_closing_fence_is_re_enabled() {
    // Make sure the fence state machine flips back on the closing ```.
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    let body = "Intro.\n```\nignored tantivy here\n```\nUse tantivy outside.";
    let hits = mentions_for(body, &index);

    assert_eq!(
        hits.len(),
        1,
        "exactly one match (post-fence) expected: {hits:?}"
    );
    assert_eq!(hits[0].target_slug, "tantivy");
    // Line 5 of the body (1-based).
    assert_eq!(
        hits[0].line, 5,
        "match must come from the post-fence line, not inside the fence: {hits:?}"
    );
}

// ─── Spec acceptance bullet: inline-code exclusion ──────────────────────────

#[test]
fn inline_code_content_is_excluded() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    let body = "Look at the `tantivy` API for details.";
    let hits = mentions_for(body, &index);

    assert!(
        hits.is_empty(),
        "term inside `inline code` must be excluded: {hits:?}"
    );
}

#[test]
fn inline_code_does_not_swallow_rest_of_line() {
    let (_wiki, index) = index_with(&[
        (
            "tools/tantivy.md",
            make_page("Tantivy", &["tools"], "normal", "body"),
        ),
        (
            "tools/lucene.md",
            make_page("Lucene", &["tools"], "normal", "body"),
        ),
    ]);

    let body = "Compare `tantivy` to lucene now.";
    let hits = mentions_for(body, &index);

    let slugs: Vec<&str> = hits.iter().map(|h| h.target_slug.as_str()).collect();
    assert!(
        slugs.contains(&"lucene"),
        "lucene (outside backticks) must fire: {hits:?}"
    );
    assert!(
        !slugs.contains(&"tantivy"),
        "tantivy (inside backticks) must not fire: {hits:?}"
    );
}

// ─── Spec acceptance bullet: URL exclusion ──────────────────────────────────

#[test]
fn url_content_is_excluded() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    let body = "See https://example.com/tantivy/docs for the manual.";
    let hits = mentions_for(body, &index);

    assert!(
        hits.is_empty(),
        "term inside a URL must be excluded: {hits:?}"
    );
}

#[test]
fn http_url_is_also_excluded() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    let body = "Mirror at http://example.org/tantivy/index.html for offline use.";
    let hits = mentions_for(body, &index);

    assert!(hits.is_empty(), "http:// URL exclusion: {hits:?}");
}

// ─── Spec acceptance bullet: frontmatter exclusion ──────────────────────────

#[test]
fn frontmatter_block_is_excluded_and_lines_after_it_count_correctly() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    // The matcher must accept a raw markdown source (with its own frontmatter)
    // and skip the leading `---\n…\n---\n` block. Line counting starts at the
    // first body line after the closing `---`.
    let body = "---\ntitle: Doc with frontmatter\naliases: [tantivy]\n---\n\nWe use tantivy here.";
    let hits = mentions_for(body, &index);

    assert_eq!(
        hits.len(),
        1,
        "exactly one match (in body, not frontmatter): {hits:?}"
    );
    assert_eq!(
        hits[0].target_slug, "tantivy",
        "match must resolve to tantivy: {hits:?}"
    );
    // The body line is the 6th line of the raw input — but matchers may report
    // either the raw-input line or the body-relative line. Whichever convention
    // is chosen, the `tantivy` token alone must NOT show up on a frontmatter
    // line (so a reported line number of 3 or 4 is wrong; 5 or 6 is fine).
    assert!(
        hits[0].line >= 5,
        "matched line must be after the closing --- (>= 5): {hits:?}"
    );
}

// ─── Spec acceptance bullet: self-reference guard ───────────────────────────

#[test]
fn self_reference_is_not_flagged() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    // The body of tantivy.md mentions its own title. With self_slug = "tantivy"
    // the matcher must drop that mention entirely.
    let body = "Tantivy is a Rust search library.";
    let hits = find_unlinked_mentions(body, &index, "tantivy");

    assert!(
        hits.is_empty(),
        "page must not flag mentions of itself: {hits:?}"
    );
}

#[test]
fn other_pages_still_flag_when_self_slug_set() {
    let (_wiki, index) = index_with(&[
        (
            "tools/tantivy.md",
            make_page("Tantivy", &["tools"], "normal", "body"),
        ),
        (
            "tools/lucene.md",
            make_page("Lucene", &["tools"], "normal", "body"),
        ),
    ]);

    let body = "Tantivy is faster than Lucene.";
    let hits = find_unlinked_mentions(body, &index, "tantivy");

    let slugs: Vec<&str> = hits.iter().map(|h| h.target_slug.as_str()).collect();
    assert!(
        !slugs.contains(&"tantivy"),
        "self-mention must be guarded: {hits:?}"
    );
    assert!(
        slugs.contains(&"lucene"),
        "non-self mentions must still fire: {hits:?}"
    );
}

// ─── Spec acceptance bullet: ambiguous matches surfaced ─────────────────────

#[test]
fn ambiguous_term_emits_one_mention_per_page() {
    let mut a = make_page("A", &["arch"], "normal", "body");
    a.aliases = vec!["common".to_string()];
    let mut b = make_page("B", &["arch"], "normal", "body");
    b.aliases = vec!["common".to_string()];
    let (_wiki, index) = index_with(&[("architecture/a.md", a), ("architecture/b.md", b)]);

    let body = "The common term shows up here.";
    let hits = mentions_for(body, &index);

    assert_eq!(
        hits.len(),
        2,
        "ambiguous term must emit one mention per matched page: {hits:?}"
    );
    let slugs: Vec<&str> = hits.iter().map(|h| h.target_slug.as_str()).collect();
    assert!(slugs.contains(&"a"));
    assert!(slugs.contains(&"b"));
    // Same line + same matched term, different target slug.
    assert_eq!(hits[0].line, hits[1].line);
    assert_eq!(hits[0].term, hits[1].term);
}

// ─── Spec acceptance bullet: unicode (CJK + accented) ───────────────────────

#[test]
fn unicode_cjk_title_match() {
    let (_wiki, index) = index_with(&[(
        "_uncategorized/startup-guide.md",
        make_page("创业指南", &["startup"], "normal", "body"),
    )]);

    let body = "今天读了创业指南这本书。";
    let hits = mentions_for(body, &index);

    assert_eq!(hits.len(), 1, "CJK title match: {hits:?}");
    assert_eq!(hits[0].target_slug, "startup-guide");
}

#[test]
fn unicode_accented_title_match_with_decomposed_input() {
    // Title indexed in NFC form, body uses decomposed form. The matcher
    // normalizes both via aliases::normalize so these collide.
    let (_wiki, index) = index_with(&[(
        "_uncategorized/cafe.md",
        make_page("Café", &["food"], "normal", "body"),
    )]);

    let decomposed_body = "Visited the Cafe\u{0301} downtown today.";
    let hits = mentions_for(decomposed_body, &index);

    assert_eq!(
        hits.len(),
        1,
        "accented title (decomposed) must still match: {hits:?}"
    );
    assert_eq!(hits[0].target_slug, "cafe");
}

// ─── Window cap (N=4 tokens) ────────────────────────────────────────────────

#[test]
fn window_cap_n_equals_four_documented() {
    // Just compile-time / public-API check on the constant.
    assert_eq!(MAX_WINDOW_TOKENS, 4);
}

#[test]
fn five_token_title_is_not_matched_by_window_scan() {
    // Five-token title — beyond the N=4 cap. This isn't a hard requirement
    // (the issue says "sensible cap; most page titles are well under this"),
    // but the cap MUST exist; this regression catches accidental removal.
    let (_wiki, index) = index_with(&[(
        "_uncategorized/long.md",
        make_page("alpha beta gamma delta epsilon", &["x"], "normal", "body"),
    )]);

    let body = "Discuss alpha beta gamma delta epsilon today.";
    let hits = mentions_for(body, &index);

    assert!(
        hits.is_empty(),
        "5-token title must not be matched (window cap = 4): {hits:?}"
    );
}

// ─── Context snippet sanity check ───────────────────────────────────────────

#[test]
fn context_snippet_centered_on_match_for_long_lines() {
    let (_wiki, index) = index_with(&[(
        "tools/tantivy.md",
        make_page("Tantivy", &["tools"], "normal", "body"),
    )]);

    // 200+ char line so the snippet helper truncates with ellipsis.
    let prefix = "lorem ipsum dolor sit amet ".repeat(8);
    let suffix = " consectetur adipiscing elit sed do eiusmod tempor".to_string();
    let body = format!("{prefix}tantivy{suffix}");
    let hits = mentions_for(&body, &index);

    assert_eq!(hits.len(), 1, "single hit on a long line: {hits:?}");
    let ctx = &hits[0].context;
    assert!(
        ctx.contains("tantivy"),
        "context must include the match: {ctx:?}"
    );
    assert!(
        ctx.len() < body.len(),
        "context must be shorter than the full line: ctx_len={}, body_len={}",
        ctx.len(),
        body.len()
    );
}

// ─── Performance contract: < 100ms on a 500-page wiki ───────────────────────

/// The parent issue (#42) sets a 100ms budget for the matcher on a
/// 500-page wiki. Run only in `--release` so debug-mode noise (regex
/// compilation, no inlining, integer-overflow checks) doesn't make this
/// flaky on slow CI runners. In debug mode the assertion is skipped.
#[test]
fn perf_under_100ms_on_500_page_wiki() {
    use std::time::Instant;

    let wiki = TestWiki::new();
    for i in 0..500 {
        let title = format!("Page Title {i}");
        let page = make_page(&title, &["bench"], "normal", "body");
        let rel = format!("bench/page-{i:03}.md");
        wiki.write_page(&rel, &page);
    }
    let index = build_index(wiki.root()).expect("build index");

    // Build a typical body with ~40 lines, lots of plain text, a few
    // already-linked references, a code fence, and a URL — exercising every
    // exclusion path on every line.
    let mut body = String::new();
    body.push_str("This page references [[page-001]] and discusses Page Title 7\n");
    body.push_str("along with Page Title 13 and a fenced block:\n");
    body.push_str("```\nlet x = page_title_99();\n```\n");
    body.push_str("plus a URL https://example.com/page-200/index.html.\n");
    for i in 0..30 {
        body.push_str(&format!(
            "Line {i}: discussing Page Title {} in some prose.\n",
            (i + 50) % 500
        ));
    }

    let start = Instant::now();
    let hits = find_unlinked_mentions(&body, &index, "");
    let elapsed = start.elapsed();

    // Sanity: should have found a bunch of matches.
    assert!(
        !hits.is_empty(),
        "expected at least one match in the perf body"
    );

    // Only enforce the budget in release builds (issue spec).
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed.as_millis() < 100,
        "find_unlinked_mentions took {elapsed:?} on 500-page wiki — exceeds 100ms budget"
    );
    // In debug mode just print the timing for human review.
    eprintln!("find_unlinked_mentions on 500-page wiki: {elapsed:?}");
}

// ─── Compile-time API surface ───────────────────────────────────────────────

#[test]
fn unlinked_mention_is_serializable() {
    let m = UnlinkedMention {
        term: "Tantivy".into(),
        target_slug: "tantivy".into(),
        line: 1,
        context: "We use Tantivy.".into(),
    };
    let json = serde_json::to_string(&m).expect("serialize");
    let back: UnlinkedMention = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(m, back);
}
