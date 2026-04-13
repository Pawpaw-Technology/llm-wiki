# wiki_write Section-Level Operations

**Issue:** #35  
**Epic:** #34 (knowledge capture)  
**Date:** 2026-04-13  
**Status:** Draft

## Problem

`wiki_write` currently does full-page overwrites only. When agents or users want to append a reference, add a log entry, or update a single section, they must read the entire page, splice content in the caller, and write back — losing any concurrent edits elsewhere and requiring the caller to understand markdown structure.

## Design

### Write Modes

Three modes for `wiki_write`, backwards-compatible:

| Mode                  | Behavior                          | `content` is                   | `section` required |
| --------------------- | --------------------------------- | ------------------------------ | ------------------ |
| `overwrite` (default) | Replace entire page               | Full page with frontmatter     | No                 |
| `append_section`      | Append to end of a named section  | Body fragment (no frontmatter) | Yes                |
| `upsert_section`      | Replace or create a named section | Body fragment (no frontmatter) | Yes                |

### Section Matching

- **Fuzzy match**: caller passes plain text like `"References"`, not `"## References"`
- **Case-insensitive** with trimmed whitespace
- **Any heading level**: `##`, `###`, etc. all match
- **First match wins**: if multiple sections share a name, operate on the first one; return a warning in the response

### Section Boundary

A section spans from its heading to the start of the next heading at the **same or higher level**, or end of document.

```
## References          ← section start (byte offset S)
- link 1
### Subsection         ← still inside "References"
- detail
## Next Section        ← section end (byte offset E)
```

Section byte range: `[S, E)`. The heading line itself is included.

### Section-Not-Found Behavior

When the target section does not exist:

- **`append_section`**: create a new `## {section}` heading + content at the end of the page body
- **`upsert_section`**: same — create at the end

Default heading level for new sections: `##`.

### Operations

**`append_section`**: insert content before byte offset E (section end). Ensure exactly one blank line (`\n\n`) between existing content and appended content. Trim trailing whitespace from appended content. If content is empty, short-circuit — do not write the file (avoids mtime change and git noise).

**`upsert_section`**: replace bytes `[heading_end, E)` where `heading_end` is the byte after the heading line's `\n`. Preserves the heading line itself. If content is empty, the section heading remains but the body is cleared.

**`overwrite`**: existing `Page::parse` + `write_page` path, unchanged.

Default heading level for newly created sections (both modes): `##`.

### Implementation Strategy

**comrak for parsing, string slicing for splicing.**

1. Read raw file as a string
2. Split frontmatter from body at the closing `---` delimiter (byte-level split, not `Page::parse` — avoids serde round-trip that reorders YAML keys)
3. comrak parses body into AST with `sourcepos` enabled
4. Walk AST to find Heading nodes; extract text content and byte positions
5. Determine section byte range `[S, E)` based on heading levels
6. Splice content into the original body string at the computed offsets
7. Reassemble: raw frontmatter (byte-for-byte original) + modified body, write to disk
8. Parse the written page via `Page::parse` for index metadata only (title, tags)
9. Update tantivy search index incrementally (same as current overwrite path)

**Key: section ops never round-trip frontmatter through serde.** `Page::parse`/`to_markdown()` are not used for the read-modify-write cycle — only for extracting metadata for the index and response.

`section.rs` exposes **pure functions** operating on `&str` → `String` (body in, body out). No filesystem I/O. The MCP handler and CLI command each do their own file read/write and call into `section.rs` for the splice. This keeps section.rs trivially testable.

This gives:

- **Correct parsing** — comrak handles code fences, HTML, setext headings, nested quotes
- **Perfect fidelity** — untouched regions are byte-for-byte identical; git diffs are minimal
- **No round-trip issues** — comrak is read-only; frontmatter is never serialized/deserialized for section ops

### MCP Interface

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/lw.md",
    "content": "- [[comrak]]",
    "mode": "append_section",
    "section": "References"
  }
}
```

Response (success):

```json
{
  "status": "ok",
  "path": "tools/lw.md",
  "title": "LW CLI Tool",
  "tags": ["rust", "cli"],
  "mode": "append_section",
  "section": "References",
  "section_found": true
}
```

Response (section created):

```json
{
  "status": "ok",
  "path": "tools/lw.md",
  "title": "LW CLI Tool",
  "tags": ["rust", "cli"],
  "mode": "append_section",
  "section": "References",
  "section_found": false,
  "warning": "Section 'References' not found; created at end of page"
}
```

### CLI Interface

```bash
# Overwrite (existing behavior, unchanged)
echo "full content" | lw write tools/lw.md

# Append to section
lw write tools/lw.md --mode append --section "References" --content "- [[comrak]]"

# Upsert section (from stdin)
echo "New usage docs..." | lw write tools/lw.md --mode upsert --section "Usage"

# Append to section (from stdin)
echo "- new item" | lw write tools/lw.md --mode append --section "See Also"
```

### Error Cases

| Condition                                                    | Behavior                                                  |
| ------------------------------------------------------------ | --------------------------------------------------------- |
| `mode` is `append_section`/`upsert_section` but no `section` | Error: "section parameter required"                       |
| Page does not exist + mode is not `overwrite`                | Error: "page not found; use overwrite mode to create"     |
| Multiple sections with same name                             | Operate on first match, include `warning` in response     |
| Content is empty + `append_section`                          | Short-circuit: no file write, no index update             |
| Content is empty + `upsert_section`                          | Clears section body; heading line preserved               |
| Both `--content` and stdin provided (CLI)                    | Error: "provide content via --content or stdin, not both" |

## File Changes

| File                           | Change                                                                                      |
| ------------------------------ | ------------------------------------------------------------------------------------------- |
| `lw-core/Cargo.toml`           | Add `comrak` dependency                                                                     |
| `lw-core/src/section.rs` (new) | Pure functions: `find_section()`, `apply_append()`, `apply_upsert()`, `split_frontmatter()` |
| `lw-core/src/lib.rs`           | `pub mod section;`                                                                          |
| `lw-mcp/src/lib.rs`            | Extend `WikiWriteArgs` with `mode`/`section`; branch in `wiki_write`; read/write I/O here   |
| `lw-cli/src/main.rs`           | Add `lw write` subcommand with `--mode`, `--section`, `--content`                           |

## BDD Scenarios

### Feature: Section-level wiki page operations

````gherkin
Feature: wiki_write section operations
  As an agent or CLI user
  I want to append to or replace specific sections of a wiki page
  So that I can make targeted edits without overwriting the entire page

  Background:
    Given a wiki page "tools/example.md" with content:
      """
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
      """

  Scenario: Append content to an existing section
    When I wiki_write with mode "append_section", section "References", content "- new ref"
    Then the page body under "References" ends with "- new ref"
    And the "Overview" section is unchanged
    And the "See Also" section is unchanged
    And frontmatter is unchanged

  Scenario: Upsert replaces an existing section body
    When I wiki_write with mode "upsert_section", section "References", content "- replaced"
    Then the "References" section body is exactly "- replaced"
    And the "## References" heading is preserved
    And other sections are unchanged

  Scenario: Append to non-existent section creates it at end
    When I wiki_write with mode "append_section", section "Notes", content "some notes"
    Then a "## Notes" heading is appended at the end of the page
    And "some notes" appears under the new heading
    And the response includes warning "Section 'Notes' not found; created at end of page"
    And section_found is false

  Scenario: Section matching is case-insensitive
    When I wiki_write with mode "append_section", section "references", content "- item"
    Then "- item" is appended to the "References" section

  Scenario: Section matching ignores heading level prefix
    When I wiki_write with mode "append_section", section "References", content "- item"
    Then it matches "## References" regardless of level

  Scenario: Nested headings belong to parent section
    Given a page with:
      """
      ## Parent
      text
      ### Child
      child text
      ## Next
      """
    When I upsert_section "Parent" with content "replaced"
    Then "### Child" and "child text" are also replaced
    And "## Next" is unaffected

  Scenario: Multiple sections with same name
    Given a page with two "## Notes" sections
    When I append to section "Notes" with content "- added"
    Then "- added" is appended to the first "Notes" section
    And the response includes a warning about multiple matches

  Scenario: Overwrite mode is unchanged
    When I wiki_write with mode "overwrite" and content including full frontmatter
    Then the entire page is replaced (existing behavior)

  Scenario: Append/upsert on non-existent page returns error
    When I wiki_write with mode "append_section" to a path that does not exist
    Then the response is an error "page not found; use overwrite mode to create"

  Scenario: Mode requires section parameter
    When I wiki_write with mode "append_section" but no section parameter
    Then the response is an error "section parameter required"

  Scenario: Code fence headings are not matched
    Given a page with:
      """
      ## Real Section
      content
      ```markdown
      ## Fake Heading
      inside code
      ```
      ## Next
      """
    When I append to section "Fake Heading"
    Then section is not found (it's inside a code fence)
    And a new "## Fake Heading" is created at end of page

  Scenario: Append with empty content is a no-op
    When I wiki_write with mode "append_section", section "References", content ""
    Then the file is not modified (no write, no mtime change)

  Scenario: Upsert with empty content clears section body
    When I wiki_write with mode "upsert_section", section "References", content ""
    Then the "## References" heading is preserved
    And the section body is empty
    And "## See Also" immediately follows

  Scenario: Setext-style heading is matched
    Given a page with:
      """
      Overview
      ========
      some content

      ## Next
      """
    When I append to section "Overview" with content "- appended"
    Then "- appended" is appended to the "Overview" section
    And "## Next" is unaffected

  Scenario: Frontmatter is never corrupted
    When I perform any section operation
    Then the YAML frontmatter is byte-for-byte identical to before
````

## TDD Test Plan

Tests map 1:1 to the BDD scenarios above, implemented as Rust unit/integration tests:

### Unit tests (`lw-core/src/section.rs`)

1. `find_section_by_name` — finds section, returns byte range
2. `find_section_case_insensitive` — "references" matches "## References"
3. `find_section_any_level` — matches `###` heading too
4. `find_section_respects_nesting` — `### Child` inside `## Parent` is not a boundary
5. `find_section_not_found` — returns None
6. `find_section_skips_code_fences` — heading in ``` block is ignored
7. `find_section_multiple_matches` — returns first match + warning flag
8. `apply_append_existing` — inserts content at section end
9. `apply_append_missing` — creates new section at page end
10. `apply_upsert_existing` — replaces section body, preserves heading
11. `apply_upsert_missing` — creates new section at page end
12. `apply_append_empty_is_noop` — empty content returns body unchanged
13. `apply_upsert_empty_clears_body` — heading preserved, body cleared
14. `find_section_setext_heading` — `Overview\n========` matched by "Overview"
15. `frontmatter_preservation` — raw frontmatter bytes unchanged after any op
16. `newline_normalization` — exactly one blank line between existing content and appended content

### Integration tests (`lw-mcp` or `lw-cli`)

17. `mcp_wiki_write_append_section` — full MCP round-trip
18. `mcp_wiki_write_upsert_section` — full MCP round-trip
19. `mcp_wiki_write_overwrite_unchanged` — backwards compatibility
20. `mcp_wiki_write_missing_section_param` — error response
21. `mcp_wiki_write_page_not_found` — error response
22. `mcp_wiki_write_append_empty_noop` — no file change, no index update
23. `cli_write_append_section` — CLI with --mode append --section
24. `cli_write_upsert_from_stdin` — pipe content via stdin
25. `cli_write_content_and_stdin_error` — error when both provided
