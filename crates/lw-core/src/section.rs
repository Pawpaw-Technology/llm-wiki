use comrak::nodes::NodeValue;
use comrak::{Arena, Options, parse_document};

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

/// Returns the byte offset of the start of a 1-based line number.
fn byte_offset_for_line(text: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut current_line = 1;
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            current_line += 1;
            if current_line == line {
                return i + 1;
            }
        }
    }
    text.len()
}

/// Returns the byte offset just past the `\n` ending the given 1-based line.
fn byte_offset_after_line(text: &str, line: usize) -> usize {
    let mut current_line = 1;
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            if current_line == line {
                return i + 1;
            }
            current_line += 1;
        }
    }
    // Last line has no trailing newline
    text.len()
}

/// Recursively collect plain text from an AST node's children.
fn collect_text<'a>(
    node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<comrak::nodes::Ast>>,
    out: &mut String,
) {
    for child in node.children() {
        let data = child.data.borrow();
        match &data.value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(c) => out.push_str(&c.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => {
                drop(data);
                collect_text(child, out);
            }
        }
    }
}

struct HeadingInfo {
    level: u8,
    start_byte: usize,
    end_byte: usize,
    text: String,
}

pub fn find_section(body: &str, section_name: &str) -> Option<SectionMatch> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.render.sourcepos = true;

    let root = parse_document(&arena, body, &options);

    // Collect all headings from the AST
    let mut headings: Vec<HeadingInfo> = Vec::new();
    for child in root.children() {
        let data = child.data.borrow();
        if let NodeValue::Heading(ref h) = data.value {
            let level = h.level;
            let sp = data.sourcepos;
            let start_byte = byte_offset_for_line(body, sp.start.line);
            let end_byte = byte_offset_after_line(body, sp.end.line);
            let mut text = String::new();
            drop(data);
            collect_text(child, &mut text);
            headings.push(HeadingInfo {
                level,
                start_byte,
                end_byte,
                text,
            });
        }
    }

    // Find matching headings by case-insensitive comparison
    let name_lower = section_name.to_lowercase();
    let matches: Vec<usize> = headings
        .iter()
        .enumerate()
        .filter(|(_, h)| h.text.trim().to_lowercase() == name_lower)
        .map(|(i, _)| i)
        .collect();

    if matches.is_empty() {
        return None;
    }

    let first_idx = matches[0];
    let h = &headings[first_idx];

    // Compute section_end: find next heading at same or higher (lower number) level
    let mut section_end = body.len();
    for next_h in &headings[first_idx + 1..] {
        if next_h.level <= h.level {
            section_end = next_h.start_byte;
            break;
        }
    }

    Some(SectionMatch {
        heading_start: h.start_byte,
        heading_end: h.end_byte,
        section_end,
        level: h.level,
        multiple_matches: matches.len() > 1,
    })
}

/// Returns None if content is empty (no-op).
/// Returns Some((new_body, section_found)).
pub fn apply_append(body: &str, section_name: &str, content: &str) -> Option<(String, bool)> {
    let content = content.trim_end();
    if content.is_empty() {
        return None;
    }

    match find_section(body, section_name) {
        Some(m) => {
            let before_insert = &body[..m.section_end];
            let trimmed_end = before_insert.trim_end_matches(|c: char| c.is_whitespace());
            let trim_point = trimmed_end.len();

            let mut result = String::with_capacity(body.len() + content.len() + 4);
            result.push_str(&body[..trim_point]);
            result.push('\n');
            result.push_str(content);
            result.push('\n');
            result.push_str(&body[m.section_end..]);

            Some((result, true))
        }
        None => {
            let trimmed = body.trim_end();
            let mut result =
                String::with_capacity(body.len() + section_name.len() + content.len() + 10);
            result.push_str(trimmed);
            result.push_str("\n\n## ");
            result.push_str(section_name);
            result.push('\n');
            result.push_str(content);
            result.push('\n');

            Some((result, false))
        }
    }
}

/// Returns (new_body, section_found).
pub fn apply_upsert(body: &str, section_name: &str, content: &str) -> (String, bool) {
    let content = content.trim_end();

    match find_section(body, section_name) {
        Some(m) => {
            let mut result = String::with_capacity(body.len() + content.len());
            result.push_str(&body[..m.heading_end]);
            if !content.is_empty() {
                result.push_str(content);
                result.push('\n');
            }
            result.push_str(&body[m.section_end..]);

            (result, true)
        }
        None => {
            let trimmed = body.trim_end();
            let mut result =
                String::with_capacity(body.len() + section_name.len() + content.len() + 10);
            result.push_str(trimmed);
            result.push_str("\n\n## ");
            result.push_str(section_name);
            result.push('\n');
            if !content.is_empty() {
                result.push_str(content);
                result.push('\n');
            }

            (result, false)
        }
    }
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

    #[test]
    fn find_section_by_name() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        let m = find_section(body, "References").unwrap();
        let section_text = &body[m.heading_start..m.section_end];
        assert!(section_text.starts_with("## References"));
        assert!(section_text.contains("- existing ref"));
        assert!(!section_text.contains("## See Also"));
        assert_eq!(m.level, 2);
        assert!(!m.multiple_matches);
    }

    #[test]
    fn find_section_case_insensitive() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        let m = find_section(body, "references").unwrap();
        assert!(body[m.heading_start..].starts_with("## References"));
    }

    #[test]
    fn find_section_any_level() {
        let body = "### Deep Heading\ncontent\n## Next\n";
        let m = find_section(body, "Deep Heading").unwrap();
        assert_eq!(m.level, 3);
        assert!(body[m.heading_start..m.section_end].contains("content"));
    }

    #[test]
    fn find_section_respects_nesting() {
        let body = "## Parent\ntext\n### Child\nchild text\n## Next\nafter\n";
        let m = find_section(body, "Parent").unwrap();
        let section = &body[m.heading_start..m.section_end];
        assert!(section.contains("### Child"));
        assert!(section.contains("child text"));
        assert!(!section.contains("## Next"));
    }

    #[test]
    fn find_section_not_found() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        assert!(find_section(body, "Nonexistent").is_none());
    }

    #[test]
    fn find_section_skips_code_fences() {
        let body = "## Real\ncontent\n```\n## Fake\ninside code\n```\n## Next\n";
        assert!(find_section(body, "Fake").is_none());
        assert!(find_section(body, "Real").is_some());
    }

    #[test]
    fn find_section_multiple_matches() {
        let body = "## Notes\nfirst\n## Notes\nsecond\n";
        let m = find_section(body, "Notes").unwrap();
        assert!(m.multiple_matches);
        let section = &body[m.heading_start..m.section_end];
        assert!(section.contains("first"));
        assert!(!section.contains("second"));
    }

    #[test]
    fn find_section_setext_heading() {
        let body = "Overview\n========\nsome content\n\n# Next\n";
        let m = find_section(body, "Overview").unwrap();
        assert_eq!(m.level, 1);
        let section = &body[m.heading_start..m.section_end];
        assert!(section.contains("some content"));
        assert!(!section.contains("# Next"));
    }

    #[test]
    fn apply_append_existing() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        let (result, found) = apply_append(body, "References", "- new ref").unwrap();
        assert!(found);
        assert!(result.contains("- existing ref"));
        assert!(result.contains("- new ref"));
        let existing_pos = result.find("- existing ref").unwrap();
        let new_pos = result.find("- new ref").unwrap();
        assert!(new_pos > existing_pos);
        assert!(result.contains("## See Also"));
        assert!(result.contains("- [[other]]"));
    }

    #[test]
    fn apply_append_missing() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        let (result, found) = apply_append(body, "Notes", "some notes").unwrap();
        assert!(!found);
        assert!(result.contains("## Notes"));
        assert!(result.contains("some notes"));
        let notes_pos = result.find("## Notes").unwrap();
        let see_also_pos = result.find("## See Also").unwrap();
        assert!(notes_pos > see_also_pos);
    }

    #[test]
    fn apply_append_empty_is_noop() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        let result = apply_append(body, "References", "");
        assert!(result.is_none());
    }

    #[test]
    fn apply_upsert_existing() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        let (result, found) = apply_upsert(body, "References", "- replaced");
        assert!(found);
        assert!(result.contains("## References"));
        assert!(result.contains("- replaced"));
        assert!(!result.contains("- existing ref"));
        assert!(result.contains("## Overview"));
        assert!(result.contains("## See Also"));
    }

    #[test]
    fn apply_upsert_missing() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        let (result, found) = apply_upsert(body, "Notes", "new notes");
        assert!(!found);
        assert!(result.contains("## Notes"));
        assert!(result.contains("new notes"));
    }

    #[test]
    fn apply_upsert_empty_clears_body() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        let (result, found) = apply_upsert(body, "References", "");
        assert!(found);
        assert!(result.contains("## References"));
        assert!(!result.contains("- existing ref"));
        assert!(result.contains("## See Also"));
    }

    #[test]
    fn tight_list_preservation() {
        let body = "## Refs\n- a\n\n## Next\n";
        let (result, _) = apply_append(body, "Refs", "- b").unwrap();
        assert!(result.contains("- a\n- b\n"));
        assert!(!result.contains("- a\n\n- b"));
    }

    #[test]
    fn frontmatter_preservation() {
        let raw = "---\ntitle: \"Quoted\"\ntags: [a, b]\n---\n\n## Sec\ntext\n";
        let (fm, body) = split_frontmatter(raw);
        let (new_body, _) = apply_append(body, "Sec", "appended").unwrap();
        let reassembled = format!("{}{}", fm, new_body);
        assert!(reassembled.starts_with("---\ntitle: \"Quoted\"\ntags: [a, b]\n---\n"));
    }
}
