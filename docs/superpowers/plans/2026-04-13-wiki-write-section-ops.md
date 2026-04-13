# wiki_write Section-Level Operations — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `append_section` and `upsert_section` modes to `wiki_write`, enabling agents and CLI users to make targeted edits to wiki page sections without full-page overwrites.

**Architecture:** comrak parses markdown body into AST (with sourcepos) to locate section byte ranges. Pure functions in `section.rs` splice content into the original body string. Frontmatter is never round-tripped through serde — raw bytes are preserved. MCP and CLI each do their own file I/O and call into `section.rs` for the splice.

**Tech Stack:** Rust, comrak (markdown AST), existing lw-core/lw-mcp/lw-cli crates

---

## File Map

| File                            | Role               | Action                                      |
| ------------------------------- | ------------------ | ------------------------------------------- |
| `crates/lw-core/Cargo.toml`     | Dependencies       | Add comrak                                  |
| `crates/lw-core/src/section.rs` | Pure section logic | **Create**                                  |
| `crates/lw-core/src/lib.rs`     | Module exports     | Add `pub mod section;`                      |
| `crates/lw-mcp/src/lib.rs`      | MCP tool handler   | Extend `WikiWriteArgs`, branch `wiki_write` |
| `crates/lw-cli/src/main.rs`     | CLI commands       | Add `Write` variant to `Commands`           |
| `crates/lw-cli/src/write.rs`    | CLI write handler  | **Create**                                  |

---

### Task 1: Add comrak dependency and create section.rs stub

**Files:**

- Modify: `crates/lw-core/Cargo.toml`
- Create: `crates/lw-core/src/section.rs`
- Modify: `crates/lw-core/src/lib.rs`

- [ ] **Step 1: Add comrak to lw-core/Cargo.toml**

Add under `[dependencies]`:

```toml
comrak = { version = "0.36", default-features = false }
```

- [ ] **Step 2: Create section.rs with public types and empty function signatures**

Create `crates/lw-core/src/section.rs`:

```rust
use crate::{Result, WikiError};
use comrak::{Arena, parse_document, Options};
use comrak::nodes::NodeValue;

/// Result of finding a section in a markdown body.
#[derive(Debug, Clone)]
pub struct SectionMatch {
    /// Byte offset of the heading line start in the body string.
    pub heading_start: usize,
    /// Byte offset immediately after the heading line's `\n`.
    pub heading_end: usize,
    /// Byte offset where the section ends (next same-or-higher heading, or body end).
    pub section_end: usize,
    /// The heading level (1-6).
    pub level: u8,
    /// True if there were multiple matches (we used the first).
    pub multiple_matches: bool,
}

/// Split a raw wiki file into (frontmatter_with_delimiters, body).
///
/// Returns the frontmatter portion including the `---\n...\n---\n` delimiters,
/// and the remaining body. If no frontmatter is found, returns ("", full_content).
pub fn split_frontmatter(raw: &str) -> (&str, &str) {
    todo!()
}

/// Find a section by name in a markdown body string.
///
/// Matching is case-insensitive and ignores heading-level prefix.
/// Returns `None` if the section is not found.
/// Headings inside code fences are ignored.
pub fn find_section(body: &str, section_name: &str) -> Option<SectionMatch> {
    todo!()
}

/// Append content to the end of a named section.
///
/// If the section exists, inserts content before `section_end` with exactly
/// one blank line between existing content and appended content.
/// If the section does not exist, creates a `## {section_name}` heading
/// at the end of the body.
/// If content is empty, returns `None` (no-op — caller should not write).
pub fn apply_append(body: &str, section_name: &str, content: &str) -> Option<(String, bool)> {
    todo!()
}

/// Replace or create a named section.
///
/// If the section exists, replaces bytes `[heading_end, section_end)` with content.
/// The heading line itself is preserved.
/// If the section does not exist, creates a `## {section_name}` heading
/// at the end of the body with the given content.
pub fn apply_upsert(body: &str, section_name: &str, content: &str) -> (String, bool) {
    todo!()
}
```

- [ ] **Step 3: Register the module in lib.rs**

Add `pub mod section;` to `crates/lw-core/src/lib.rs` after `pub mod schema;`:

```rust
pub mod section;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p lw-core 2>&1 | tail -5`
Expected: Compiles (todo! macros are fine at compile time)

- [ ] **Step 5: Commit**

```bash
git add crates/lw-core/Cargo.toml crates/lw-core/src/section.rs crates/lw-core/src/lib.rs
git commit -m "feat(section): add comrak dep and section.rs stub with types"
```

---

### Task 2: Implement and test `split_frontmatter`

**Files:**

- Modify: `crates/lw-core/src/section.rs`

- [ ] **Step 1: Write failing tests for split_frontmatter**

Append to the bottom of `crates/lw-core/src/section.rs`:

```rust
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
        // Frontmatter with unusual quoting that serde would normalize
        let raw = "---\ntitle: \"Quoted Title\"\ntags: [a, b]\n---\n\n## Body\n";
        let (fm, body) = split_frontmatter(raw);
        assert_eq!(fm, "---\ntitle: \"Quoted Title\"\ntags: [a, b]\n---\n");
        assert_eq!(body, "\n## Body\n");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lw-core split_frontmatter -- --nocapture 2>&1 | tail -10`
Expected: FAIL — `not yet implemented`

- [ ] **Step 3: Implement split_frontmatter**

Replace the `todo!()` in `split_frontmatter`:

```rust
pub fn split_frontmatter(raw: &str) -> (&str, &str) {
    if !raw.starts_with("---\n") && !raw.starts_with("---\r\n") {
        return ("", raw);
    }
    // Find the closing --- delimiter (skip the opening one)
    let search_start = if raw.starts_with("---\r\n") { 5 } else { 4 };
    let closing_patterns: &[&str] = &["\n---\n", "\n---\r\n"];
    for pattern in closing_patterns {
        if let Some(pos) = raw[search_start..].find(pattern) {
            let split_at = search_start + pos + pattern.len();
            return (&raw[..split_at], &raw[split_at..]);
        }
    }
    // No closing delimiter found
    ("", raw)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lw-core split_frontmatter -- --nocapture 2>&1 | tail -10`
Expected: 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lw-core/src/section.rs
git commit -m "feat(section): implement split_frontmatter with byte-level split"
```

---

### Task 3: Implement and test `find_section`

**Files:**

- Modify: `crates/lw-core/src/section.rs`

- [ ] **Step 1: Write failing tests for find_section**

Add to the `tests` module in `section.rs`:

````rust
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
        // "Fake" is inside a code fence, should not be found as a section
        assert!(find_section(body, "Fake").is_none());
        // "Real" should be found
        assert!(find_section(body, "Real").is_some());
    }

    #[test]
    fn find_section_multiple_matches() {
        let body = "## Notes\nfirst\n## Notes\nsecond\n";
        let m = find_section(body, "Notes").unwrap();
        assert!(m.multiple_matches);
        // Should match the first one
        let section = &body[m.heading_start..m.section_end];
        assert!(section.contains("first"));
        assert!(!section.contains("second"));
    }

    #[test]
    fn find_section_setext_heading() {
        let body = "Overview\n========\nsome content\n\n## Next\n";
        let m = find_section(body, "Overview").unwrap();
        assert_eq!(m.level, 1);
        let section = &body[m.heading_start..m.section_end];
        assert!(section.contains("some content"));
        assert!(!section.contains("## Next"));
    }
````

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lw-core find_section -- --nocapture 2>&1 | tail -10`
Expected: FAIL — `not yet implemented`

- [ ] **Step 3: Implement find_section**

Replace the `todo!()` in `find_section`:

```rust
pub fn find_section(body: &str, section_name: &str) -> Option<SectionMatch> {
    let arena = Arena::new();
    let mut options = Options::default();
    options.render.sourcepos = true;

    let root = parse_document(&arena, body, &options);

    let target = section_name.trim().to_lowercase();
    let mut matches: Vec<SectionMatch> = Vec::new();

    // Collect all headings with their positions and levels
    struct HeadingInfo {
        level: u8,
        start_byte: usize,
        end_byte: usize, // byte after the heading line's \n
        text: String,
    }
    let mut headings: Vec<HeadingInfo> = Vec::new();

    for node in root.children() {
        if let NodeValue::Heading(ref heading) = node.data.borrow().value {
            let sp = node.data.borrow().sourcepos;
            let start_byte = byte_offset_for_line(body, sp.start.line);
            // Find the end of the heading block (after last line)
            let end_byte = byte_offset_for_line_end(body, sp.end.line);

            // Extract plain text from heading children
            let mut text = String::new();
            for child in node.children() {
                collect_text(child, &mut text);
            }

            headings.push(HeadingInfo {
                level: heading.level,
                start_byte,
                end_byte,
                text,
            });
        }
    }

    // Find matching headings and compute section boundaries
    for (i, h) in headings.iter().enumerate() {
        if h.text.trim().to_lowercase() == target {
            // Section ends at the next heading of same-or-higher level, or body end
            let section_end = headings[i + 1..]
                .iter()
                .find(|next| next.level <= h.level)
                .map(|next| next.start_byte)
                .unwrap_or(body.len());

            matches.push(SectionMatch {
                heading_start: h.start_byte,
                heading_end: h.end_byte,
                section_end,
                level: h.level,
                multiple_matches: false,
            });
        }
    }

    match matches.len() {
        0 => None,
        1 => Some(matches.into_iter().next().unwrap()),
        _ => {
            let mut first = matches.into_iter().next().unwrap();
            first.multiple_matches = true;
            Some(first)
        }
    }
}

/// Get the byte offset of the start of a 1-based line number.
fn byte_offset_for_line(text: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut current_line = 1;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            current_line += 1;
            if current_line == line {
                return i + 1;
            }
        }
    }
    text.len()
}

/// Get the byte offset immediately after the last byte of a 1-based line number.
fn byte_offset_for_line_end(text: &str, line: usize) -> usize {
    let mut current_line = 1;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            if current_line == line {
                return i + 1;
            }
            current_line += 1;
        }
    }
    text.len()
}

/// Recursively collect plain text from a comrak AST node.
fn collect_text<'a>(node: &'a comrak::arena_tree::Node<'a, std::cell::RefCell<comrak::nodes::Ast>>, out: &mut String) {
    match node.data.borrow().value {
        NodeValue::Text(ref t) => out.push_str(t),
        NodeValue::Code(ref c) => out.push_str(&c.literal),
        NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
        _ => {
            for child in node.children() {
                collect_text(child, out);
            }
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lw-core find_section -- --nocapture 2>&1 | tail -15`
Expected: 8 tests PASS

- [ ] **Step 5: Run clippy**

Run: `cargo clippy -p lw-core -- -D warnings 2>&1 | tail -10`
Expected: No warnings

- [ ] **Step 6: Commit**

```bash
git add crates/lw-core/src/section.rs
git commit -m "feat(section): implement find_section with comrak AST parsing"
```

---

### Task 4: Implement and test `apply_append` and `apply_upsert`

**Files:**

- Modify: `crates/lw-core/src/section.rs`

- [ ] **Step 1: Write failing tests for apply_append and apply_upsert**

Add to the `tests` module:

```rust
    #[test]
    fn apply_append_existing() {
        let (_, body) = split_frontmatter(SAMPLE_PAGE);
        let (result, found) = apply_append(body, "References", "- new ref").unwrap();
        assert!(found);
        assert!(result.contains("- existing ref"));
        assert!(result.contains("- new ref"));
        // new ref appears after existing ref
        let existing_pos = result.find("- existing ref").unwrap();
        let new_pos = result.find("- new ref").unwrap();
        assert!(new_pos > existing_pos);
        // See Also section is intact
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
        // New section is at the end
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
        // Other sections intact
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
        // See Also follows immediately (with blank line separation)
        assert!(result.contains("## See Also"));
    }

    #[test]
    fn tight_list_preservation() {
        // Appending a list item should preserve tight list (no blank line)
        let body = "## Refs\n- a\n\n## Next\n";
        let (result, _) = apply_append(body, "Refs", "- b").unwrap();
        // Single \n between items, no blank line — tight list preserved
        assert!(result.contains("- a\n- b\n"));
        assert!(!result.contains("- a\n\n- b"));
    }

    #[test]
    fn frontmatter_preservation() {
        let raw = "---\ntitle: \"Quoted\"\ntags: [a, b]\n---\n\n## Sec\ntext\n";
        let (fm, body) = split_frontmatter(raw);
        let (new_body, _) = apply_append(body, "Sec", "appended").unwrap();
        let reassembled = format!("{}{}", fm, new_body);
        // Frontmatter is byte-for-byte identical
        assert!(reassembled.starts_with("---\ntitle: \"Quoted\"\ntags: [a, b]\n---\n"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lw-core apply_ -- --nocapture 2>&1 | tail -10`
Expected: FAIL — `not yet implemented`

- [ ] **Step 3: Implement apply_append**

Replace the `todo!()` in `apply_append`:

```rust
pub fn apply_append(body: &str, section_name: &str, content: &str) -> Option<(String, bool)> {
    let content = content.trim_end();
    if content.is_empty() {
        return None; // No-op
    }

    match find_section(body, section_name) {
        Some(m) => {
            // Trim trailing whitespace from existing section content
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
            // Create new section at end of body
            let trimmed = body.trim_end();
            let mut result = String::with_capacity(body.len() + section_name.len() + content.len() + 10);
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
```

- [ ] **Step 4: Implement apply_upsert**

Replace the `todo!()` in `apply_upsert`:

```rust
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
            let mut result = String::with_capacity(body.len() + section_name.len() + content.len() + 10);
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lw-core -- --nocapture 2>&1 | tail -20`
Expected: All 19 unit tests PASS (3 split_frontmatter + 8 find_section + 8 apply)

- [ ] **Step 6: Run clippy and fmt**

Run: `cargo clippy -p lw-core -- -D warnings && cargo fmt --check -p lw-core`
Expected: Clean

- [ ] **Step 7: Commit**

```bash
git add crates/lw-core/src/section.rs
git commit -m "feat(section): implement apply_append and apply_upsert"
```

---

### Task 5: Extend MCP `wiki_write` with section modes

**Files:**

- Modify: `crates/lw-mcp/src/lib.rs`

- [ ] **Step 1: Extend WikiWriteArgs**

In `crates/lw-mcp/src/lib.rs`, replace the existing `WikiWriteArgs` struct:

```rust
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiWriteArgs {
    /// Relative path within wiki/ (e.g. "architecture/new-page.md")
    pub path: String,
    /// Content to write. For "overwrite" mode: full markdown with YAML frontmatter.
    /// For "append_section"/"upsert_section": body fragment without frontmatter.
    pub content: String,
    /// Write mode: "overwrite" (default), "append_section", or "upsert_section"
    #[serde(default = "default_write_mode")]
    pub mode: String,
    /// Section name for append/upsert modes (case-insensitive, no ## prefix needed)
    #[serde(default)]
    pub section: Option<String>,
}

fn default_write_mode() -> String {
    "overwrite".to_string()
}
```

- [ ] **Step 2: Rewrite wiki_write handler with mode branching**

Replace the `wiki_write` method body in the `#[tool_router]` impl block:

```rust
    #[tool(
        name = "wiki_write",
        description = "Write or update a wiki page. Modes: 'overwrite' (default) replaces the entire page (content must include YAML frontmatter). 'append_section' appends content to a named section. 'upsert_section' replaces or creates a named section. Section matching is case-insensitive."
    )]
    fn wiki_write(&self, Parameters(args): Parameters<WikiWriteArgs>) -> String {
        let abs_path = match validate_wiki_path(&self.wiki_root, &args.path) {
            Ok(p) => p,
            Err(e) => return serde_json::json!({"error": e.to_string()}).to_string(),
        };

        match args.mode.as_str() {
            "overwrite" => {
                // Existing behavior
                let page = match Page::parse(&args.content) {
                    Ok(p) => p,
                    Err(e) => {
                        return serde_json::json!({"error": format!("Invalid page content: {e}")})
                            .to_string();
                    }
                };
                if let Err(e) = write_page(&abs_path, &page) {
                    return serde_json::json!({"error": format!("Failed to write page: {e}")}).to_string();
                }
                if let Err(e) = self.searcher.index_page(&args.path, &page) {
                    tracing::warn!("Failed to index page {}: {}", args.path, e);
                }
                if let Err(e) = self.searcher.commit() {
                    tracing::warn!("Failed to commit index: {}", e);
                }
                serde_json::json!({
                    "status": "ok",
                    "path": args.path,
                    "title": page.title,
                    "tags": page.tags,
                })
                .to_string()
            }
            "append_section" | "upsert_section" => {
                let section_name = match &args.section {
                    Some(s) if !s.is_empty() => s.clone(),
                    _ => {
                        return serde_json::json!({"error": "section parameter required for append_section/upsert_section mode"}).to_string();
                    }
                };

                // Read raw file
                let raw = match std::fs::read_to_string(&abs_path) {
                    Ok(r) => r,
                    Err(_) => {
                        return serde_json::json!({"error": "page not found; use overwrite mode to create"}).to_string();
                    }
                };

                let (frontmatter, body) = lw_core::section::split_frontmatter(&raw);

                let (new_body, section_found, warning) = if args.mode == "append_section" {
                    match lw_core::section::apply_append(body, &section_name, &args.content) {
                        Some((new_body, found)) => {
                            let warning = if !found {
                                Some(format!("Section '{}' not found; created at end of page", section_name))
                            } else {
                                None
                            };
                            (new_body, found, warning)
                        }
                        None => {
                            // Empty content no-op
                            return serde_json::json!({
                                "status": "ok",
                                "path": args.path,
                                "mode": args.mode,
                                "section": section_name,
                                "noop": true,
                            })
                            .to_string();
                        }
                    }
                } else {
                    // upsert_section
                    let (new_body, found) = lw_core::section::apply_upsert(body, &section_name, &args.content);
                    let warning = if !found {
                        Some(format!("Section '{}' not found; created at end of page", section_name))
                    } else {
                        None
                    };
                    (new_body, found, warning)
                };

                // Reassemble and write
                let output = format!("{}{}", frontmatter, new_body);
                if let Err(e) = std::fs::write(&abs_path, &output) {
                    return serde_json::json!({"error": format!("Failed to write page: {e}")}).to_string();
                }

                // Re-index
                if let Ok(page) = Page::parse(&output) {
                    if let Err(e) = self.searcher.index_page(&args.path, &page) {
                        tracing::warn!("Failed to index page {}: {}", args.path, e);
                    }
                    if let Err(e) = self.searcher.commit() {
                        tracing::warn!("Failed to commit index: {}", e);
                    }

                    let mut resp = serde_json::json!({
                        "status": "ok",
                        "path": args.path,
                        "title": page.title,
                        "tags": page.tags,
                        "mode": args.mode,
                        "section": section_name,
                        "section_found": section_found,
                    });
                    if let Some(w) = warning {
                        resp["warning"] = serde_json::json!(w);
                    }
                    resp.to_string()
                } else {
                    serde_json::json!({
                        "status": "ok",
                        "path": args.path,
                        "mode": args.mode,
                        "section": section_name,
                        "section_found": section_found,
                    })
                    .to_string()
                }
            }
            other => {
                serde_json::json!({"error": format!("Unknown write mode: '{other}'. Use 'overwrite', 'append_section', or 'upsert_section'.")}).to_string()
            }
        }
    }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p lw-mcp 2>&1 | tail -5`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add crates/lw-mcp/src/lib.rs
git commit -m "feat(mcp): extend wiki_write with append_section and upsert_section modes"
```

---

### Task 6: Add `lw write` CLI subcommand

**Files:**

- Create: `crates/lw-cli/src/write.rs`
- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Create write.rs**

Create `crates/lw-cli/src/write.rs`:

```rust
use lw_core::fs::{validate_wiki_path, write_page};
use lw_core::page::Page;
use lw_core::section;
use std::io::Read;
use std::path::Path;

pub fn run(
    root: &Path,
    path: &str,
    mode: &str,
    section_name: &Option<String>,
    content: &Option<String>,
    stdin: bool,
) -> Result<(), anyhow::Error> {
    let abs_path = validate_wiki_path(root, path)?;

    // Resolve content from --content or stdin
    let resolved_content = match (content, stdin) {
        (Some(_), true) => {
            anyhow::bail!("provide content via --content or stdin, not both");
        }
        (Some(c), false) => c.clone(),
        (None, true) | (None, false) => {
            // Try reading from stdin (pipe detection)
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };

    match mode {
        "overwrite" => {
            let page = Page::parse(&resolved_content)?;
            write_page(&abs_path, &page)?;
            eprintln!("Wrote: {path}");
        }
        "append" | "append_section" => {
            let section_name = section_name
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--section required for append mode"))?;

            let raw = std::fs::read_to_string(&abs_path)
                .map_err(|_| anyhow::anyhow!("page not found; use overwrite mode to create: {path}"))?;

            let (frontmatter, body) = section::split_frontmatter(&raw);
            match section::apply_append(body, section_name, &resolved_content) {
                Some((new_body, found)) => {
                    let output = format!("{}{}", frontmatter, new_body);
                    std::fs::write(&abs_path, output)?;
                    if found {
                        eprintln!("Appended to section '{section_name}' in {path}");
                    } else {
                        eprintln!("Created section '{section_name}' at end of {path}");
                    }
                }
                None => {
                    eprintln!("Empty content, nothing to append.");
                }
            }
        }
        "upsert" | "upsert_section" => {
            let section_name = section_name
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--section required for upsert mode"))?;

            let raw = std::fs::read_to_string(&abs_path)
                .map_err(|_| anyhow::anyhow!("page not found; use overwrite mode to create: {path}"))?;

            let (frontmatter, body) = section::split_frontmatter(&raw);
            let (new_body, found) = section::apply_upsert(body, section_name, &resolved_content);
            let output = format!("{}{}", frontmatter, new_body);
            std::fs::write(&abs_path, output)?;
            if found {
                eprintln!("Replaced section '{section_name}' in {path}");
            } else {
                eprintln!("Created section '{section_name}' at end of {path}");
            }
        }
        other => {
            anyhow::bail!("Unknown mode: '{other}'. Use 'overwrite', 'append', or 'upsert'.");
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Add Write command to Commands enum in main.rs**

Add to the `Commands` enum after `Serve`:

```rust
    /// Write or update a wiki page (overwrite, append to section, or upsert section)
    #[command(
        after_help = "Examples:\n  echo 'full content' | lw write tools/page.md\n  lw write tools/page.md --mode append --section References --content '- [[link]]'\n  echo 'new docs' | lw write tools/page.md --mode upsert --section Usage"
    )]
    Write {
        /// Wiki-relative path (e.g. tools/page.md)
        path: String,
        /// Write mode: overwrite (default), append, upsert
        #[arg(long, default_value = "overwrite")]
        mode: String,
        /// Section name for append/upsert modes
        #[arg(long)]
        section: Option<String>,
        /// Content to write (alternative to stdin)
        #[arg(long)]
        content: Option<String>,
    },
```

- [ ] **Step 3: Add Write match arm in main()**

Add to the `match cli.command` block before the closing `};`:

```rust
        Commands::Write {
            path,
            mode,
            section,
            content,
        } => match resolve_root(cli.root) {
            Ok(root) => {
                let has_stdin = !atty::is(atty::Stream::Stdin);
                write::run(&root, &path, &mode, &section, &content, has_stdin)
                    .map_err(|e| lw_core::WikiError::Other(e.to_string()))
            }
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
```

- [ ] **Step 4: Add mod declaration and atty dependency**

Add `mod write;` at the top of `main.rs` with other mod declarations.

Add `atty = "0.2"` to `crates/lw-cli/Cargo.toml` under `[dependencies]`.

- [ ] **Step 5: Check if WikiError has an Other variant, or add it**

If `WikiError` doesn't have an `Other(String)` variant, add one to `crates/lw-core/src/error.rs`:

```rust
    #[error("{0}")]
    Other(String),
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo build -p lw-cli 2>&1 | tail -10`
Expected: Compiles

- [ ] **Step 7: Commit**

```bash
git add crates/lw-cli/src/write.rs crates/lw-cli/src/main.rs crates/lw-cli/Cargo.toml crates/lw-core/src/error.rs
git commit -m "feat(cli): add lw write subcommand with append/upsert modes"
```

---

### Task 7: Integration tests

**Files:**

- Create: `crates/lw-mcp/tests/wiki_write_section.rs` (or add to existing test file)

- [ ] **Step 1: Write MCP integration tests**

Create `crates/lw-mcp/tests/wiki_write_section.rs`. These tests call the section functions end-to-end with real files:

```rust
use lw_core::fs::{init_wiki, read_page, validate_wiki_path, write_page};
use lw_core::page::Page;
use lw_core::schema::WikiSchema;
use lw_core::section;
use tempfile::TempDir;

fn setup_wiki() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();
    let schema = WikiSchema::default();
    init_wiki(&root, &schema).unwrap();
    (tmp, root)
}

fn write_test_page(root: &std::path::Path, rel_path: &str, content: &str) {
    let abs = validate_wiki_path(root, rel_path).unwrap();
    let page = Page::parse(content).unwrap();
    write_page(&abs, &page).unwrap();
}

const TEST_PAGE: &str = "\
---
title: Test Page
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
fn mcp_wiki_write_append_section() {
    let (_tmp, root) = setup_wiki();
    write_test_page(&root, "tools/test.md", TEST_PAGE);

    let abs = validate_wiki_path(&root, "tools/test.md").unwrap();
    let raw = std::fs::read_to_string(&abs).unwrap();
    let (fm, body) = section::split_frontmatter(&raw);
    let (new_body, found) = section::apply_append(body, "References", "- new ref").unwrap();
    assert!(found);
    let output = format!("{}{}", fm, new_body);
    std::fs::write(&abs, &output).unwrap();

    let result = std::fs::read_to_string(&abs).unwrap();
    assert!(result.contains("- existing ref"));
    assert!(result.contains("- new ref"));
    assert!(result.contains("## See Also"));

    // Verify page is still parseable
    let page = Page::parse(&result).unwrap();
    assert_eq!(page.title, "Test Page");
}

#[test]
fn mcp_wiki_write_upsert_section() {
    let (_tmp, root) = setup_wiki();
    write_test_page(&root, "tools/test.md", TEST_PAGE);

    let abs = validate_wiki_path(&root, "tools/test.md").unwrap();
    let raw = std::fs::read_to_string(&abs).unwrap();
    let (fm, body) = section::split_frontmatter(&raw);
    let (new_body, found) = section::apply_upsert(body, "References", "- replaced");
    assert!(found);
    let output = format!("{}{}", fm, new_body);
    std::fs::write(&abs, &output).unwrap();

    let result = std::fs::read_to_string(&abs).unwrap();
    assert!(result.contains("## References"));
    assert!(result.contains("- replaced"));
    assert!(!result.contains("- existing ref"));
}

#[test]
fn mcp_wiki_write_overwrite_unchanged() {
    let (_tmp, root) = setup_wiki();
    write_test_page(&root, "tools/test.md", TEST_PAGE);

    let abs = validate_wiki_path(&root, "tools/test.md").unwrap();
    let new_content = "---\ntitle: Replaced\ntags: [new]\n---\n\n## New Content\nHello\n";
    let page = Page::parse(new_content).unwrap();
    write_page(&abs, &page).unwrap();

    let result = read_page(&abs).unwrap();
    assert_eq!(result.title, "Replaced");
}

#[test]
fn mcp_wiki_write_page_not_found() {
    let (_tmp, root) = setup_wiki();
    let abs = validate_wiki_path(&root, "tools/nonexistent.md").unwrap();
    let result = std::fs::read_to_string(&abs);
    assert!(result.is_err());
}

#[test]
fn mcp_wiki_write_append_empty_noop() {
    let (_tmp, root) = setup_wiki();
    write_test_page(&root, "tools/test.md", TEST_PAGE);

    let abs = validate_wiki_path(&root, "tools/test.md").unwrap();
    let mtime_before = std::fs::metadata(&abs).unwrap().modified().unwrap();

    let raw = std::fs::read_to_string(&abs).unwrap();
    let (_, body) = section::split_frontmatter(&raw);
    let result = section::apply_append(body, "References", "");
    assert!(result.is_none()); // No-op, don't write

    let mtime_after = std::fs::metadata(&abs).unwrap().modified().unwrap();
    assert_eq!(mtime_before, mtime_after);
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p lw-mcp -- --nocapture 2>&1 | tail -20`
Expected: All integration tests PASS

- [ ] **Step 3: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests PASS across all crates

- [ ] **Step 4: Run clippy on everything**

Run: `cargo clippy --all -- -D warnings 2>&1 | tail -10`
Expected: Clean

- [ ] **Step 5: Commit**

```bash
git add crates/lw-mcp/tests/wiki_write_section.rs
git commit -m "test(mcp): add integration tests for section-level wiki_write"
```

---

### Task 8: Final validation and cleanup

- [ ] **Step 1: Run the full test suite one more time**

Run: `cargo test 2>&1`
Expected: All 25+ tests pass

- [ ] **Step 2: Run make test (includes clippy + fmt)**

Run: `make test 2>&1 | tail -20`
Expected: Clean

- [ ] **Step 3: Verify backwards compatibility — lw write with overwrite**

In a temp wiki dir, test:

```bash
lw init --root /tmp/test-wiki
echo '---\ntitle: Test\ntags: []\n---\n\n## Hello\nWorld\n' > /tmp/test-wiki/wiki/tools/test.md
cat /tmp/test-wiki/wiki/tools/test.md | lw write tools/test.md --root /tmp/test-wiki
```

Expected: Page written, readable via `lw read tools/test.md --root /tmp/test-wiki`

- [ ] **Step 4: Test section append via CLI**

```bash
lw write tools/test.md --root /tmp/test-wiki --mode append --section "Hello" --content "- appended line"
lw read tools/test.md --root /tmp/test-wiki
```

Expected: Output contains "World" and "- appended line" under "## Hello"

- [ ] **Step 5: Commit any final fixes**

If any fixes needed, commit them. Otherwise, this step is a no-op.

- [ ] **Step 6: Push branch**

```bash
git push
```
