/// Result of finding a section in a markdown body.
#[derive(Debug, Clone)]
pub struct SectionMatch {
    /// Byte offset of the heading line start in the body string.
    pub heading_start: usize,
    /// Byte offset immediately after the heading span's last `\n`.
    pub heading_end: usize,
    /// Byte offset where the section ends (next same-or-higher heading, or body end).
    pub section_end: usize,
    /// The heading level (1-6).
    pub level: u8,
    /// True if there were multiple matches (we used the first).
    pub multiple_matches: bool,
}

pub fn split_frontmatter(raw: &str) -> (&str, &str) {
    if !raw.starts_with("---\n") && !raw.starts_with("---\r\n") {
        return ("", raw);
    }
    let search_start = if raw.starts_with("---\r\n") { 5 } else { 4 };
    let closing_patterns: &[&str] = &["\n---\n", "\n---\r\n"];
    for pattern in closing_patterns {
        if let Some(pos) = raw[search_start..].find(pattern) {
            let split_at = search_start + pos + pattern.len();
            return (&raw[..split_at], &raw[split_at..]);
        }
    }
    ("", raw)
}

pub fn find_section(_body: &str, _section_name: &str) -> Option<SectionMatch> {
    todo!()
}

/// Returns None if content is empty (no-op).
/// Returns Some((new_body, section_found)).
pub fn apply_append(_body: &str, _section_name: &str, _content: &str) -> Option<(String, bool)> {
    todo!()
}

/// Returns (new_body, section_found).
pub fn apply_upsert(_body: &str, _section_name: &str, _content: &str) -> (String, bool) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PAGE: &str = "\
---
title: Example
tags: [test]
---

## Overview
This is the overview.

## References
- existing ref

## See Also
- [[other]]
";

    #[test]
    fn split_frontmatter_basic() {
        let (fm, body) = split_frontmatter(SAMPLE_PAGE);
        assert!(fm.starts_with("---\n"));
        assert!(fm.ends_with("---\n"));
        assert!(fm.contains("title: Example"));
        assert!(body.starts_with("\n## Overview"));
    }

    #[test]
    fn split_frontmatter_no_frontmatter() {
        let raw = "## Just a heading\nSome text\n";
        let (fm, body) = split_frontmatter(raw);
        assert_eq!(fm, "");
        assert_eq!(body, raw);
    }

    #[test]
    fn split_frontmatter_preserves_bytes() {
        let raw = "---\ntitle: \"Quoted Title\"\ntags: [a, b]\n---\n\n## Body\n";
        let (fm, body) = split_frontmatter(raw);
        assert_eq!(fm, "---\ntitle: \"Quoted Title\"\ntags: [a, b]\n---\n");
        assert_eq!(body, "\n## Body\n");
    }
}
