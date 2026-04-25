---
name: llm-wiki:knowledge-capture
description: Use when the user wants to preserve knowledge from a conversation — a concept explained, a debugging solution found, an architecture decision made, or any insight worth keeping. Extracts, classifies, and writes structured wiki pages via the lw MCP tools.
when-to-use: |
  Trigger phrases: "add this to my wiki", "capture this", "document this decision", "save what we just figured out",
  "write up this concept", "keep a record of this fix", "document the architecture choice", "add a wiki page for this",
  "preserve this for later", "make a note of this", "journal this session", "write this up as a guide".
  Also trigger proactively after: resolving a non-trivial debugging session, making an architecture decision,
  explaining a library or tool in depth, walking through a how-to that took real effort to figure out.
---

You are helping the user capture knowledge from a conversation into their llm-wiki vault. Follow the 6-step workflow below. Each step is concrete; do not skip steps or batch them silently.

## Step 1 — Extract

Identify what is worth capturing from the conversation. Look for:

- **Concepts**: definitions, mental models, how a library/tool/algorithm works
- **Decisions**: architecture choices, trade-off resolutions, why X was chosen over Y
- **Guides**: step-by-step solutions, debugging walkthroughs, how-tos
- **References**: API surfaces, configuration schemas, interface specs
- **Journal entries**: freeform session logs, meeting notes, timestamped observations

If multiple distinct pieces of knowledge surface, list them and ask the user which to capture first, or capture each in turn.

## Step 2 — Classify

Map the extracted knowledge to exactly one content type:

| Type        | Use when                                                                     |
| ----------- | ---------------------------------------------------------------------------- |
| `concept`   | You're documenting what something _is_ — a library, algorithm, pattern, term |
| `guide`     | You're documenting how to _do_ something — a procedure, fix, or walkthrough  |
| `decision`  | You're recording a choice made and why — architecture, tooling, process      |
| `reference` | You're documenting an interface to look up — API, config schema, CLI flags   |
| `journal`   | Freeform timestamped log; no fixed structure needed                          |

The type drives the page template in Step 5.

## Step 3 — Search

Before creating a new page, check whether one already exists. **Update existing > create new.**

```json
{
  "tool": "wiki_query",
  "args": { "query": "<key concept or slug>", "limit": 5 }
}
```

If a hit looks relevant, read it:

```json
{ "tool": "wiki_read", "args": { "path": "<category>/<slug>.md" } }
```

If the existing page covers the topic: update it (go to Step 5, existing-page path). If not: create a new page (continue to Step 4).

## Step 4 — Locate

### Pick a category

Read `.lw/schema.toml` with `wiki_read` if you need to confirm available categories. Typical categories in a starter vault: `concepts`, `guides`, `decisions`, `tools`, `reference`, `_journal`. Choose the one that fits — categories are directories, not tags.

### Check existing tags

Reuse existing tags rather than inventing new ones:

```json
{ "tool": "wiki_tags", "args": {} }
```

Pick tags from the returned list. Add a new tag only if none of the existing ones fit.

### Generate a slug

Use lowercase kebab-case: `comrak-ast-parser`, `ci-lockfile-failure`, `tantivy-vs-sqlite-fts`. The slug becomes the filename.

## Step 5 — Create or Update

### New page

First scaffold with `wiki_new` (this creates the file and enforces schema):

```json
{
  "tool": "wiki_new",
  "args": {
    "category": "tools",
    "slug": "comrak-ast-parser",
    "title": "Comrak AST Parser",
    "tags": ["rust", "markdown", "ast"]
  }
}
```

Then fill body sections with `wiki_write` in `upsert_section` mode. Use the template for the content type (see **Content type templates** below).

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/comrak-ast-parser.md",
    "mode": "upsert_section",
    "section": "Overview",
    "content": "Comrak is a CommonMark-compliant Rust parser that exposes a full AST, enabling programmatic markdown manipulation."
  }
}
```

Repeat for each section in the template.

### Existing page

Use `upsert_section` to replace a section or `append_section` to add new content at the end of a section:

````json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/comrak-ast-parser.md",
    "mode": "upsert_section",
    "section": "Examples",
    "content": "### Stripping HTML\n\n```rust\ncomrak::markdown_to_html(input, &Options::default())\n```"
  }
}
````

### Source attribution

If the knowledge originates from a specific URL, paper, or external resource, include it in frontmatter. Since `wiki_new` creates the page, add `source:` by rewriting the frontmatter via `overwrite` mode immediately after scaffolding, or append it to the existing page's frontmatter manually. Document the source URL in the body's **See Also** or **References** section at minimum.

### Journal entries

Write directly via `wiki_write` in `overwrite` mode to `_journal/YYYY-MM-DD.md`. Include a YAML frontmatter block with `title:` and `tags:`. No `wiki_new` needed for journals — journals are freeform.

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "_journal/2026-04-25.md",
    "mode": "overwrite",
    "content": "---\ntitle: \"2026-04-25\"\ntags: [journal]\n---\n\n## Session\n\n..."
  }
}
```

## Step 6 — Link

An orphan page (no inbound links) is a knowledge dead end. After creating a page:

1. **Find related pages** — search for pages likely to mention this topic:

```json
{ "tool": "wiki_query", "args": { "query": "[[comrak-ast-parser]]" } }
```

Also search by topic keywords if the wikilink search returns nothing:

```json
{ "tool": "wiki_query", "args": { "query": "markdown parser rust ast" } }
```

2. **Add outbound wikilinks** in the new page's body. Use `[[slug]]` syntax for related pages.

3. **Update inbound links** — for each related page found, add `[[new-slug]]` to its relevant section via `upsert_section` or `append_section`. Also update the `related:` frontmatter field on both pages.

## Step 7 — Verify

Read back the page to confirm it looks right:

```json
{ "tool": "wiki_read", "args": { "path": "tools/comrak-ast-parser.md" } }
```

Run lint to catch orphans, broken links, and freshness issues:

```json
{ "tool": "wiki_lint", "args": {} }
```

Report the result to the user: page path, title, tags, and any lint warnings. If lint reports the new page as an orphan, go back to Step 6.

---

## Content type templates

### concept

Sections: **Overview** → **Definition** → **Key Properties** → **Examples** → **Related**

Use for: libraries, algorithms, patterns, terms, mental models.

### guide

Sections: **Overview** → **Prerequisites** → **Steps** → **Verification** → **Troubleshooting**

Use for: how-tos, debugging walkthroughs, setup procedures, fix recipes.

### decision

Sections: **Context** → **Decision** → **Rationale** → **Alternatives** → **Consequences**

Use for: architecture choices, tooling selections, process changes. Capture the _why_, not just the _what_.

### reference

Sections: **Overview** → **API / Interface** → **Configuration** → **Examples** → **See Also**

Use for: API surfaces, CLI flag references, config schemas, anything you look up, not read linearly.

### journal

Freeform. Suggested headings: **Session**, **Decisions**, **Open Questions**, **Next Steps**. Always timestamped.

---

## Worked examples

### Example 1 — Rust crate → concept page

**Situation**: The user and agent spent time understanding how `comrak` exposes its AST and how to walk it in Rust.

**Step 1**: Knowledge type = concept (what comrak is and how it works).
**Step 2**: Type = `concept`.
**Step 3**: Search first.

```json
{
  "tool": "wiki_query",
  "args": { "query": "comrak markdown rust", "limit": 5 }
}
```

No hit — proceed to create.

**Step 4**: Category = `tools`, tags from `wiki_tags` → reuse `rust`, `markdown`; add `ast` (new).

**Step 5a**: Scaffold.

```json
{
  "tool": "wiki_new",
  "args": {
    "category": "tools",
    "slug": "comrak-ast-parser",
    "title": "Comrak AST Parser",
    "tags": ["rust", "markdown", "ast"]
  }
}
```

**Step 5b**: Fill sections.

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/comrak-ast-parser.md",
    "mode": "upsert_section",
    "section": "Overview",
    "content": "Comrak is a CommonMark-compliant Rust crate that parses markdown into a full AST, enabling programmatic inspection and transformation."
  }
}
```

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/comrak-ast-parser.md",
    "mode": "upsert_section",
    "section": "Key Properties",
    "content": "- Arena-allocated AST nodes (`Arena<AstNode>`)\n- Full CommonMark + GFM extension support\n- Zero-copy where possible\n- `format_commonmark` round-trips cleanly"
  }
}
```

````json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/comrak-ast-parser.md",
    "mode": "upsert_section",
    "section": "Examples",
    "content": "```rust\nlet arena = Arena::new();\nlet root = parse_document(&arena, input, &Options::default());\nfor node in root.descendants() {\n    // inspect node.data.borrow().value\n}\n```"
  }
}
````

**Step 6**: Search for pages that mention markdown or parsers, add `[[comrak-ast-parser]]` to their Related section.

**Step 7**: `wiki_read` + `wiki_lint`. Confirm no orphan warning.

---

### Example 2 — Debugging session → guide page

**Situation**: A CI job failed because `Cargo.lock` was out of date after adding a dependency. The fix was to run `cargo generate-lockfile` locally and commit the result.

**Step 1**: Knowledge = how to resolve a stale Cargo.lock CI failure.
**Step 2**: Type = `guide`.
**Step 3**:

```json
{
  "tool": "wiki_query",
  "args": { "query": "cargo lock CI failure", "limit": 5 }
}
```

No relevant hit.

**Step 4**: Category = `guides`, tags → `rust`, `ci`, `cargo` (check `wiki_tags` first).

**Step 5a**: Scaffold.

```json
{
  "tool": "wiki_new",
  "args": {
    "category": "guides",
    "slug": "fix-stale-cargo-lock-ci",
    "title": "Fix: Stale Cargo.lock in CI",
    "tags": ["rust", "ci", "cargo"]
  }
}
```

**Step 5b**: Fill sections.

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "guides/fix-stale-cargo-lock-ci.md",
    "mode": "upsert_section",
    "section": "Overview",
    "content": "CI fails with `error: the lock file needs to be updated` after adding or updating a Cargo dependency without committing the updated `Cargo.lock`."
  }
}
```

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "guides/fix-stale-cargo-lock-ci.md",
    "mode": "upsert_section",
    "section": "Steps",
    "content": "1. Run `cargo generate-lockfile` (or `cargo build`) locally.\n2. Verify `Cargo.lock` is modified: `git diff Cargo.lock`.\n3. `git add Cargo.lock && git commit -m \"chore: update Cargo.lock\"`\n4. Push. CI should pass."
  }
}
```

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "guides/fix-stale-cargo-lock-ci.md",
    "mode": "upsert_section",
    "section": "Troubleshooting",
    "content": "- If `Cargo.lock` is in `.gitignore`, remove it for application crates (libraries may legitimately omit it).\n- If CI uses a different Rust toolchain version, pin with `rust-toolchain.toml`."
  }
}
```

**Step 6**: Search for related CI or Cargo pages and add bidirectional links.

**Step 7**: `wiki_read` + `wiki_lint`.

---

### Example 3 — Architecture decision → decision record

**Situation**: The team chose Tantivy (Rust full-text search) over SQLite FTS5 for the wiki search backend.

**Step 1**: Knowledge = architecture decision with rationale and alternatives.
**Step 2**: Type = `decision`.
**Step 3**:

```json
{
  "tool": "wiki_query",
  "args": { "query": "tantivy sqlite search backend", "limit": 5 }
}
```

No hit.

**Step 4**: Category = `decisions`, tags → `rust`, `search`, `architecture`.

**Step 5a**: Scaffold.

```json
{
  "tool": "wiki_new",
  "args": {
    "category": "decisions",
    "slug": "tantivy-over-sqlite-fts",
    "title": "ADR: Tantivy over SQLite FTS5 for search",
    "tags": ["rust", "search", "architecture"]
  }
}
```

**Step 5b**: Fill sections.

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "decisions/tantivy-over-sqlite-fts.md",
    "mode": "upsert_section",
    "section": "Context",
    "content": "The wiki needs full-text search across potentially thousands of markdown pages. We evaluated embedded options to avoid external service dependencies."
  }
}
```

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "decisions/tantivy-over-sqlite-fts.md",
    "mode": "upsert_section",
    "section": "Decision",
    "content": "Use Tantivy (`tantivy` crate) as the search backend, with an on-disk index stored in `.lw/index/`."
  }
}
```

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "decisions/tantivy-over-sqlite-fts.md",
    "mode": "upsert_section",
    "section": "Rationale",
    "content": "- Tantivy is purpose-built for full-text search; BM25 scoring out of the box.\n- SQLite FTS5 lacks snippet highlighting and relevance tuning without significant custom work.\n- Tantivy's `IndexWriter`/`IndexReader` split maps naturally to the `lw serve` (writer) vs `lw query` (reader) concurrency model.\n- Pure Rust; no C FFI needed beyond what rusqlite already pulls in."
  }
}
```

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "decisions/tantivy-over-sqlite-fts.md",
    "mode": "upsert_section",
    "section": "Alternatives",
    "content": "- **SQLite FTS5**: Simpler dependency story, but weaker scoring and no built-in snippets.\n- **Meilisearch** (external): Excellent UX but requires a running service — not viable for a local CLI tool.\n- **In-memory grep**: Zero dependencies but O(n) on every query; unacceptable at scale."
  }
}
```

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "decisions/tantivy-over-sqlite-fts.md",
    "mode": "upsert_section",
    "section": "Consequences",
    "content": "- The `IndexWriter` must be opened lazily (not in `WikiMcpServer::new`) to avoid holding the write lock for the MCP server's lifetime.\n- A `WikiError::IndexLocked` fallback lets read-only callers degrade gracefully.\n- Index rebuild on first run; subsequent starts skip rebuild if index is non-empty."
  }
}
```

**Step 6**: Search for pages mentioning search or architecture. Link bidirectionally. Update `related:` frontmatter on both sides.

**Step 7**:

```json
{
  "tool": "wiki_read",
  "args": { "path": "decisions/tantivy-over-sqlite-fts.md" }
}
```

```json
{ "tool": "wiki_lint", "args": {} }
```

---

## Hard rules

- **Never create orphan pages.** Every new page must have at least one inbound link from an existing page, or be linked from a category index. Check with `wiki_query "[[slug]]"` after linking to confirm.
- **Reuse existing tags.** Always call `wiki_tags` before inventing a new tag. New tags only when nothing existing fits.
- **Update existing pages before creating duplicates.** If `wiki_query` returns a close match, read it and decide: update vs. new page. Near-duplicates degrade the wiki.
- **Attribute sources.** When knowledge comes from an external URL, paper, or tool output, record it in the body's **See Also** / **References** section. Do not invent publication dates or authors.
- **Never silent-fail.** If a tool call returns an error, report it to the user and explain what went wrong before continuing.
- **Do not reference unshipped tools.** `wiki_capture` and `wiki_backlinks` do not exist yet. Use `wiki_query "[[slug]]"` to find pages that mention a given page (backlink simulation). Use `wiki_write` for all capture workflows.

## MCP tools available

- `wiki_query` — full-text search with optional `tags`, `category`, `limit` filters; returns hits with paths, titles, scores, snippets
- `wiki_read` — read a page by `path` (relative within wiki/); returns frontmatter fields + body
- `wiki_browse` — list pages filtered by `category`, `tag`, or `stale_only`
- `wiki_tags` — list all tags with page counts; optional `category` filter
- `wiki_new` — scaffold a new page; required args: `category`, `slug`, `title`; optional: `tags` (array), `author`
- `wiki_write` — write or update a page; `path` + `content` required; `mode`: `overwrite` (default, full page with frontmatter) | `append_section` | `upsert_section`; `section` required for append/upsert modes
- `wiki_ingest` — file raw source material into `raw/`; pass `source_path` (file/URL) OR `content` (pasted markdown), not both; optional: `filename`, `raw_type`, `title`, `tags`, `category`
- `wiki_lint` — run freshness, orphan, broken-link, and TODO checks; optional `category` filter
- `wiki_stats` — wiki health overview: page count, category breakdown, freshness distribution; no arguments
