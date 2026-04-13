use comrak::nodes::NodeValue;
use comrak::{parse_document, Arena, Options};

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
    todo!()
}

pub fn find_section(body: &str, section_name: &str) -> Option<SectionMatch> {
    todo!()
}

/// Returns None if content is empty (no-op).
/// Returns Some((new_body, section_found)).
pub fn apply_append(body: &str, section_name: &str, content: &str) -> Option<(String, bool)> {
    todo!()
}

/// Returns (new_body, section_found).
pub fn apply_upsert(body: &str, section_name: &str, content: &str) -> (String, bool) {
    todo!()
}
