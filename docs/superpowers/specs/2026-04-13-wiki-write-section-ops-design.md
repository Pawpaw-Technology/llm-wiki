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

**`append_section`**: insert content before byte offset E (section end), after a `\n`.

**`upsert_section`**: replace bytes from (end of heading line) to E with new content. Preserves the heading line itself.

**`overwrite`**: existing `Page::parse` + `write_page` path, unchanged.

### Implementation Strategy

**comrak for parsing, string slicing for splicing.**

1. Frontmatter is stripped by `gray_matter` (existing `Page::parse`) — comrak never sees it
2. comrak parses body into AST with `sourcepos` enabled
3. Walk AST to find Heading nodes; extract text content and byte positions
4. Determine section byte range `[S, E)` based on heading levels
5. Splice content into the original body string at the computed offsets
6. Reassemble frontmatter + modified body via `Page::to_markdown()`

This gives:

- **Correct parsing** — comrak handles code fences, HTML, setext headings, nested quotes
- **Perfect fidelity** — untouched regions are byte-for-byte identical; git diffs are minimal
- **No round-trip issues** — comrak is read-only here; we never serialize from AST

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

| Condition                                                    | Behavior                                              |
| ------------------------------------------------------------ | ----------------------------------------------------- |
| `mode` is `append_section`/`upsert_section` but no `section` | Error: "section parameter required"                   |
| Page does not exist + mode is not `overwrite`                | Error: "page not found; use overwrite mode to create" |
| Multiple sections with same name                             | Operate on first match, include `warning` in response |
| Content is empty string                                      | Allowed (upsert clears section body; append is no-op) |

## File Changes

| File                           | Change                                                               |
| ------------------------------ | -------------------------------------------------------------------- |
| `lw-core/Cargo.toml`           | Add `comrak` dependency                                              |
| `lw-core/src/section.rs` (new) | `find_section()`, `apply_append()`, `apply_upsert()`                 |
| `lw-core/src/lib.rs`           | `pub mod section;`                                                   |
| `lw-core/src/fs.rs`            | Add `write_section()` that combines read + section op + write        |
| `lw-mcp/src/lib.rs`            | Extend `WikiWriteArgs` with `mode`/`section`; branch in `wiki_write` |
| `lw-cli/src/main.rs`           | Add `lw write` subcommand with `--mode`, `--section`, `--content`    |

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
    When I wiki_write with mode "overwrite", content is full page with frontmatter
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
12. `frontmatter_preservation` — frontmatter bytes unchanged after any op

### Integration tests (`lw-mcp` or `lw-cli`)

13. `mcp_wiki_write_append_section` — full MCP round-trip
14. `mcp_wiki_write_upsert_section` — full MCP round-trip
15. `mcp_wiki_write_overwrite_unchanged` — backwards compatibility
16. `mcp_wiki_write_missing_section_param` — error response
17. `mcp_wiki_write_page_not_found` — error response
18. `cli_write_append_section` — CLI with --mode append --section
19. `cli_write_upsert_from_stdin` — pipe content via stdin
