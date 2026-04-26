//! Unlinked-mention matcher — flag terms in a page body that match an indexed
//! page but are not already wrapped in `[[…]]`. Foundation for issue #42:
//! a `lw lint` rule and `wiki_write` response field that nudge authors toward
//! denser cross-linking.
//!
//! Algorithm overview
//! ──────────────────
//! 1. Skip the leading YAML frontmatter (`---\n…\n---\n`) so its keys never
//!    fire false matches and so reported line numbers stay aligned with the
//!    raw input text the caller hands us.
//! 2. Walk line-by-line, maintaining fenced-code state across lines (toggled
//!    by triple-backtick).
//! 3. Per line, compute byte ranges to *exclude* (inline code, URLs, existing
//!    `[[…]]` wikilinks). The matcher walks token windows of 1..=N words and
//!    skips any whose byte span overlaps an excluded range.
//! 4. Lookups go through `aliases::normalize` so casing + Unicode NFC are
//!    handled identically to the index side.
//! 5. Greedy longest-match per starting token: try the 4-, 3-, 2-, 1-token
//!    window in order and advance past the first hit so "Flash Attention 2"
//!    consumes its inner "Attention" without double-counting.
//!
//! Performance
//! ───────────
//! All regexes are compiled once via `LazyLock`. The hot loop allocates a
//! few `String`s per token window (the normalized lookup key); profiling on a
//! 500-page wiki + ~40-line body runs comfortably under the 100ms budget
//! lifted from parent issue #42.

use crate::aliases::{AliasIndex, normalize};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use unicode_segmentation::UnicodeSegmentation;

/// Maximum number of tokens we will join when looking up multi-word titles.
/// "Sensible cap; most page titles are well under this" — issue #101.
pub const MAX_WINDOW_TOKENS: usize = 4;

// ─── Regexes ────────────────────────────────────────────────────────────────

/// `[[target]]` or `[[target|display]]`. Used to mask already-linked spans
/// from the unlinked-mention scan. Same shape as `backlinks::WIKILINK_RE`.
static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\]").expect("WIKILINK_RE is a valid regex"));

/// Inline-code spans: paired single backticks. Greedy-non-greedy `[^`]+`
/// matches text up to the closing backtick. Unmatched single backticks (no
/// closing pair) are left alone — same heuristic CommonMark uses.
static INLINE_CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`[^`]+`").expect("INLINE_CODE_RE is a valid regex"));

/// HTTP/HTTPS URLs. The pattern stops at the first whitespace, matching the
/// issue note: "URL detection: regex `https?://\S+` is sufficient."
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://\S+").expect("URL_RE is a valid regex"));

// ─── Public API ─────────────────────────────────────────────────────────────

/// One unlinked mention found in a page body. Ambiguous lookups (a term that
/// hits multiple pages) emit one `UnlinkedMention` per matched page — the
/// consumer (`lw lint`, MCP) decides how to surface that to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnlinkedMention {
    /// The verbatim body text that matched (preserves original casing).
    pub term: String,
    /// Slug of the page the term resolves to.
    pub target_slug: String,
    /// 1-based line number in the input `body` where the match occurs.
    pub line: u32,
    /// A ~80-character snippet of the matched line, centred on the term.
    pub context: String,
}

/// Scan `body` for terms that resolve to entries in `index` but are not yet
/// wrapped in `[[…]]` wikilinks.
///
/// The matcher is read-only: it returns suggestions; auto-fix is out of scope
/// (issue #101). Excludes content inside fenced code blocks, inline code,
/// URLs, the leading frontmatter block, and existing wikilinks. Self-mentions
/// (`PageRef.slug == self_slug`) are dropped.
#[tracing::instrument(level = "debug", skip(body, index))]
pub fn find_unlinked_mentions(
    body: &str,
    index: &AliasIndex,
    self_slug: &str,
) -> Vec<UnlinkedMention> {
    if body.is_empty() || index.terms.is_empty() {
        return Vec::new();
    }

    let (scan_text, line_offset) = strip_frontmatter(body);

    let mut out: Vec<UnlinkedMention> = Vec::new();
    let mut in_fence = false;

    for (idx, line) in scan_text.lines().enumerate() {
        let line_no = (idx as u32) + 1 + line_offset;

        // Fenced-code state machine. A line whose trimmed-start prefix is
        // ``` (with optional language tag) toggles the fence state. The
        // toggle line itself is treated as part of the fence either way.
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }

        scan_line(line, line_no, index, self_slug, &mut out);
    }

    out
}

// ─── Internal helpers ───────────────────────────────────────────────────────

/// Strip a leading YAML frontmatter block (`---\n…\n---\n`) from `body`.
/// Returns the remaining text plus the number of lines consumed so the caller
/// can keep reported line numbers aligned with the *original* input text.
///
/// If the body has no frontmatter the input is returned unchanged with a 0
/// line offset.
fn strip_frontmatter(body: &str) -> (&str, u32) {
    // Frontmatter must begin at byte 0 — anything else is body content.
    if !body.starts_with("---\n") && body != "---" && !body.starts_with("---\r\n") {
        return (body, 0);
    }

    // Walk lines, counting bytes until we find the closing `---` line.
    let mut consumed_bytes = 0usize;
    let mut consumed_lines = 0u32;
    let mut found_close = false;

    for (i, line) in body.lines().enumerate() {
        // `lines()` strips the line terminator entirely. Detect whether the
        // original byte sequence uses CRLF (\r\n = 2 bytes) or LF (\n = 1
        // byte) so `consumed_bytes` stays aligned with `body`.
        let line_bytes = line.len();
        consumed_bytes += line_bytes;
        // Peek at the next byte after the line content: if it is '\r' the
        // terminator is \r\n (2 bytes), otherwise just \n (1 byte). Only add
        // the terminator when there is more content after this line.
        if consumed_bytes < body.len() {
            let next_byte = body.as_bytes()[consumed_bytes];
            if next_byte == b'\r' {
                consumed_bytes += 2; // \r\n
            } else {
                consumed_bytes += 1; // \n
            }
        }
        consumed_lines += 1;

        if i > 0 && line.trim_end() == "---" {
            found_close = true;
            break;
        }
    }

    if !found_close {
        return (body, 0);
    }

    // `consumed_bytes` is now the byte index of the first body byte. Clamp to
    // the body length to be defensive against pathological inputs.
    let split = consumed_bytes.min(body.len());
    (&body[split..], consumed_lines)
}

/// Compute byte ranges (start..end, half-open) on `line` that should be
/// invisible to the matcher: inside `[[…]]`, inline `…`, or a URL.
fn excluded_ranges(line: &str) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for cap in WIKILINK_RE.find_iter(line) {
        ranges.push((cap.start(), cap.end()));
    }
    for cap in INLINE_CODE_RE.find_iter(line) {
        ranges.push((cap.start(), cap.end()));
    }
    for cap in URL_RE.find_iter(line) {
        ranges.push((cap.start(), cap.end()));
    }
    // Sort + leave overlaps in place — the overlap check downstream is O(N)
    // anyway and ranges per line are tiny.
    ranges.sort_by_key(|&(s, _)| s);
    ranges
}

/// True if `[start, end)` overlaps any excluded range.
fn span_excluded(start: usize, end: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|&(s, e)| start < e && s < end)
}

/// Walk `line` and append every unlinked mention into `out`.
fn scan_line(
    line: &str,
    line_no: u32,
    index: &AliasIndex,
    self_slug: &str,
    out: &mut Vec<UnlinkedMention>,
) {
    let exclusions = excluded_ranges(line);

    // Tokenize with Unicode word boundaries. Each entry is (byte_start, word).
    // For CJK text, where there are no whitespace boundaries, this still
    // yields the contiguous CJK run as a single "word" — which is what we
    // want for verbatim multi-character title matching.
    let tokens: Vec<(usize, &str)> = line.unicode_word_indices().collect();
    if tokens.is_empty() {
        return;
    }

    // Greedy: at each starting token, try the longest window first.
    let mut i = 0usize;
    while i < tokens.len() {
        let mut matched_end_token: Option<usize> = None;

        for window in (1..=MAX_WINDOW_TOKENS).rev() {
            let end = i + window;
            if end > tokens.len() {
                continue;
            }
            let (start_byte, _) = tokens[i];
            let (last_start, last_word) = tokens[end - 1];
            let end_byte = last_start + last_word.len();

            // Skip windows that touch any excluded byte range.
            if span_excluded(start_byte, end_byte, &exclusions) {
                continue;
            }

            // Build the lookup key from the verbatim byte slice between the
            // first and last tokens. This naturally handles two cases:
            //
            // - ASCII: "Flash Attention 2" — tokens separated by spaces in
            //   the body text, which `normalize` collapses to lowercase NFC.
            // - CJK: "创业指南" — `unicode_word_indices` splits each
            //   character into its own token, but the byte slice between the
            //   first and last tokens is the contiguous run, with no
            //   spurious separators inserted.
            //
            // We also collapse internal whitespace runs to a single space so
            // that "Flash  Attention" (double-space) still resolves.
            let raw_slice = &line[start_byte..end_byte];
            let normalized_key = normalize(&collapse_whitespace(raw_slice));
            let pages = index.terms.get(&normalized_key);

            let Some(pages) = pages else {
                continue;
            };
            if pages.is_empty() {
                continue;
            }

            // Self-reference guard: filter out any PageRef whose slug is
            // the current page (spec criterion #6). If the term resolves
            // ambiguously to [self, page_b], we keep page_b and emit one
            // mention for it (spec criterion #7 — consumer decides how to
            // surface ambiguous matches). Only when ALL resolved pages are
            // self do we suppress entirely and advance past the window.
            let non_self_pages: Vec<_> = if self_slug.is_empty() {
                pages.iter().collect()
            } else {
                pages.iter().filter(|p| p.slug != self_slug).collect()
            };

            if non_self_pages.is_empty() {
                // Every resolution was self — consume the window without
                // emitting, same as the original single-self behaviour.
                matched_end_token = Some(end);
                break;
            }

            // Use the verbatim body slice as the `term` so the user sees
            // their own casing in lint output.
            let verbatim_term = line[start_byte..end_byte].to_string();
            let context = snippet_centered(line, start_byte, end_byte);

            for page in non_self_pages {
                out.push(UnlinkedMention {
                    term: verbatim_term.clone(),
                    target_slug: page.slug.clone(),
                    line: line_no,
                    context: context.clone(),
                });
            }

            matched_end_token = Some(end);
            break;
        }

        // Advance past the matched window (greedy) or by one token if
        // nothing matched at this position.
        i = matched_end_token.unwrap_or(i + 1);
    }
}

/// Collapse runs of whitespace inside `s` to a single space and trim ends.
/// Used so that double-space or tab-separated multi-word titles in body text
/// still match the canonicalized index keys.
fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_was_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Build a context snippet of `line` centred on the byte range `[start, end)`.
/// Mirrors `backlinks::snippet_for` but works on byte offsets so it can be
/// reused without reconstructing the `[[slug]]` needle.
fn snippet_centered(line: &str, start: usize, end: usize) -> String {
    if line.is_empty() {
        return String::new();
    }
    const RADIUS: usize = 80;
    if line.len() <= RADIUS * 2 {
        return line.to_string();
    }

    let want_start = start.saturating_sub(RADIUS / 2);
    let want_end = (end + RADIUS / 2).min(line.len());

    // Clamp to char boundaries so we never split a multi-byte UTF-8 sequence.
    let char_start = line
        .char_indices()
        .map(|(i, _)| i)
        .rfind(|&i| i <= want_start)
        .unwrap_or(0);
    let char_end = line
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= want_end)
        .unwrap_or(line.len());

    let mut snip = line[char_start..char_end].to_string();
    if char_start > 0 {
        snip = format!("…{snip}");
    }
    if char_end < line.len() {
        snip = format!("{snip}…");
    }
    snip
}
