---
name: llm-wiki:knowledge-capture
description: Use when the user wants to preserve knowledge from a conversation — a concept explained, a debugging solution found, an architecture decision made, or a quick observation to file for later. Extracts, classifies, and writes structured wiki pages (or quick journal captures) via the lw MCP tools.
when-to-use: |
  Trigger phrases: "add this to my wiki", "capture this", "document this decision", "save what we just figured out",
  "write up this concept", "keep a record of this fix", "document the architecture choice", "add a wiki page for this",
  "preserve this for later", "make a note of this", "journal this thought", "quick capture this", "write this up as a guide".
  Also trigger proactively after: resolving a non-trivial debugging session, making an architecture decision,
  explaining a library or tool in depth, walking through a how-to that took real effort to figure out.
---

You are helping the user capture knowledge from a conversation into their llm-wiki vault. Two paths:

- **Full page** — durable knowledge worth structuring. Follow the 7-step workflow below.
- **Quick capture** — a thought to keep but not worth structuring now. Use `wiki_capture`; triage later (see **Journal triage**).

Pick the right path before doing anything.

## Pick the path

```
Is it raw source material (article, paper, transcript) to file before distilling?
  → wiki_ingest (lands in raw/, NOT in wiki/). Promote later by reading raw and creating a wiki page.

Is it a quick thought, observation, or link worth keeping but not worth structuring now?
  → wiki_capture (lands in wiki/_journal/YYYY-MM-DD.md, timestamped). Triage later.

Is it knowledge to preserve as a permanent wiki page (concept / guide / decision / reference)?
  → wiki_new + wiki_write (full 7-step workflow below).

Is it an addition to an existing page?
  → wiki_write upsert_section / append_section. Skip Step 4; jump to Step 5 existing-page path.
```

When in doubt between capture and full page: capture is cheaper and reversible. Promote later.

---

## Step 1 — Extract

Identify what is worth capturing from the conversation. Look for:

- **Concepts**: definitions, mental models, how a library/tool/algorithm works
- **Decisions**: architecture choices, trade-off resolutions, why X was chosen over Y
- **Guides**: step-by-step solutions, debugging walkthroughs, how-tos
- **References**: API surfaces, configuration schemas, interface specs
- **Journal entries**: freeform session logs, meeting notes, timestamped observations

If multiple distinct pieces surface, list them and ask the user which to capture first, or capture each in turn.

## Step 2 — Classify

Map the extracted knowledge to exactly one content type:

| Type        | Use when                                                                     |
| ----------- | ---------------------------------------------------------------------------- |
| `concept`   | You're documenting what something _is_ — a library, algorithm, pattern, term |
| `guide`     | You're documenting how to _do_ something — a procedure, fix, or walkthrough  |
| `decision`  | You're recording a choice made and why — architecture, tooling, process      |
| `reference` | You're documenting an interface to look up — API, config schema, CLI flags   |
| `journal`   | Freeform timestamped log; use `wiki_capture`, not the full workflow          |

The type drives the page template in Step 5.

## Step 3 — Search

Before creating a new page, check whether one already exists. **Update existing > create new.**

Basic search:

```json
{
  "tool": "wiki_query",
  "args": { "query": "<key concept or slug>", "limit": 5 }
}
```

Filtered search — `wiki_query` accepts frontmatter filters:

```json
{
  "tool": "wiki_query",
  "args": {
    "query": "comrak",
    "tags": "rust,markdown",
    "category": "tools",
    "sort": "created_desc",
    "limit": 5
  }
}
```

Filter reference:

- `tags` — comma-separated, AND-ed; case-sensitive
- `category` — directory name; case-sensitive
- `status` — frontmatter `status` field (e.g. `draft`, `published`)
- `author` — frontmatter `author` field
- `sort` — `relevance` (default) | `created_desc` | `created_asc` | `title`

For pure-frontmatter queries (e.g. "all draft pages by alice, newest first") pass an empty `query` with the filters set.

If a hit looks relevant, read it:

```json
{ "tool": "wiki_read", "args": { "path": "<category>/<slug>.md" } }
```

If the existing page covers the topic: update it (Step 5, existing-page path). Otherwise: continue to Step 4.

## Step 4 — Locate

### Pick a category from the schema

Categories are directories, not tags. Each vault's `.lw/schema.toml` is the source of truth — never memorize a default set; read it:

```json
{ "tool": "wiki_read", "args": { "path": ".lw/schema.toml" } }
```

Each `[categories.<name>]` block declares:

- `required_fields` — frontmatter fields you must pass to `wiki_new` (e.g. `["title", "tags", "author"]`)
- `template` — the body skeleton `wiki_new` produces
- `review_days` — how long before lint marks the page stale

If you call `wiki_new` with a missing required field, the error names it: `"category guides requires field: author"`. Add the field and retry — no separate introspection call needed.

### Check existing tags

Reuse before inventing:

```json
{ "tool": "wiki_tags", "args": {} }
```

Add a new tag only when nothing existing fits.

### Generate a slug

Lowercase kebab-case, must match `[a-z0-9_-]+` (no path separators): `comrak-ast-parser`, `ci-lockfile-failure`, `tantivy-vs-sqlite-fts`. The slug becomes the filename.

## Step 5 — Create or Update

### New page

Scaffold with `wiki_new` — creates the file with schema-validated frontmatter and body template:

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

Then fill body sections with `wiki_write` `upsert_section`. Use the template for the content type (see **Content type templates** below).

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/comrak-ast-parser.md",
    "mode": "upsert_section",
    "section": "Overview",
    "content": "Comrak is a CommonMark-compliant Rust parser that exposes a full AST..."
  }
}
```

Repeat for each section. The `section` arg is case-insensitive; pass the heading text without `##` prefix.

### Existing page

`upsert_section` (replace section content) or `append_section` (add at end of section):

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

If the section heading does not exist, both modes create it at the end of the page; the response carries `"warning": "Section 'X' not found; created at end of page"`.

### Quick capture (skip the full workflow)

For ephemeral notes, use `wiki_capture` instead of `wiki_write`:

```json
{
  "tool": "wiki_capture",
  "args": {
    "content": "comrak's format_commonmark unwraps nested lists when re-emitting",
    "tags": ["rust", "markdown", "comrak"],
    "source": "https://docs.rs/comrak"
  }
}
```

Auto-creates `wiki/_journal/YYYY-MM-DD.md` if needed, prepends `**HH:MM**`, appends to a `## Captures` section. No frontmatter to write, no section logic, no schema check. Captures sit in the journal until promoted (see **Journal triage**).

### Act on `unlinked_mentions`

Every `wiki_write` and `wiki_new` response includes:

```json
{
  "status": "ok",
  "path": "tools/comrak-ast-parser.md",
  "unlinked_mentions": [
    {
      "term": "tantivy",
      "target_slug": "tantivy",
      "line": 12,
      "context": "...uses tantivy for indexing..."
    }
  ]
}
```

Each entry is a phrase in your written content that matches an existing page's title or alias but is not yet wrapped in `[[wikilinks]]`. **Decide per entry**:

- The mention is genuinely about that page → wrap it. Rewrite the section with `[[target-slug]]` via `upsert_section`.
- The mention is coincidental (homonym, common API symbol, generic word) → ignore.

Do not blindly link every mention.

### Source attribution

If knowledge originates from an external URL, paper, or tool output, record it in the body's **See Also** / **References** section. To set the `sources:` frontmatter field, use the read-modify-write pattern (see Step 6 — `upsert_section` does not touch frontmatter).

## Step 6 — Link

An orphan page is a knowledge dead end. After creating a page:

### 1. Discover related pages — two paths

For pages **already** linking to this topic:

```json
{ "tool": "wiki_backlinks", "args": { "path": "comrak-ast-parser" } }
```

Returns entries with `kind: "wikilink"` (body `[[link]]`) or `kind: "related"` (frontmatter entries). Use this to understand existing context before adding more links. Bare-slug mentions are NOT backlinks.

For pages that **should** link to you but don't yet — search by topic keywords (Tantivy treats `[...]` as range syntax, so pass slugs without brackets):

```json
{
  "tool": "wiki_query",
  "args": { "query": "markdown parser rust ast", "limit": 5 }
}
```

### 2. Add outbound wikilinks

In the new page's body (typically in **Related** or **See Also**), add `[[slug]]` references to pages your content relates to.

### 3. Update inbound links

For each related page found, append the wikilink:

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/markdown-tooling.md",
    "mode": "upsert_section",
    "section": "Related",
    "content": "- [[comrak-ast-parser]]"
  }
}
```

### 4. Update `related:` frontmatter (when needed)

`upsert_section` and `append_section` preserve frontmatter unchanged. To modify frontmatter, use read-modify-write:

```json
{ "tool": "wiki_read", "args": { "path": "concepts/full-text-search.md" } }
```

Then overwrite with the modified frontmatter:

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "concepts/full-text-search.md",
    "mode": "overwrite",
    "content": "---\ntitle: \"Full-Text Search\"\ntags: [search, rust]\nrelated: [comrak-ast-parser]\n---\n\n<original body here>"
  }
}
```

Body wikilinks suffice for most cases. `related:` frontmatter is for explicit, schema-tracked associations (e.g. ADRs that supersede earlier ADRs).

## Step 7 — Verify

Read back:

```json
{ "tool": "wiki_read", "args": { "path": "tools/comrak-ast-parser.md" } }
```

Run lint:

```json
{ "tool": "wiki_lint", "args": {} }
```

The lint summary returns counts for: stale freshness, TODO markers, broken `related:` entries, orphan pages, missing concepts (broken wikilinks), and `stale_journal_pages` (journal pages older than the configured threshold).

Address findings that point at what you just wrote:

- **orphan** → go back to Step 6
- **missing-concept** → either create the missing target page or remove the wikilink
- **broken-related** → fix the path in frontmatter

Report to the user: page path, title, tags, and any unresolved lint findings.

---

## Journal triage

Captures accumulate in `wiki/_journal/YYYY-MM-DD.md`. Lint flags journal pages whose last commit is older than `[journal] stale_after_days` (default 7) as `stale_journal_pages`.

When the user asks you to triage, or when `wiki_lint` reports stale captures:

1. **Read the flagged journal**: `wiki_read _journal/YYYY-MM-DD.md`
2. **Walk each capture line**, decide:
   - **Promote** — durable knowledge → run the full 7-step workflow to extract it into a permanent page; in the journal, replace the line with a back-reference like `**14:32** [promoted to [[target-slug]]] <one-line summary>`.
   - **Keep as-is** — useful as a session log but not page-worthy → leave it.
   - **Discard** — trivial in retrospect → delete the line.
3. **Touch the journal** by committing the change (`wiki_write` mode `overwrite` against the journal does this). The new commit timestamp clears the staleness flag.

The journal is **never the source of truth** — it's an inbox. Promoted knowledge always gets its own permanent page.

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

Use for: API surfaces, CLI flag references, config schemas — anything you look up, not read linearly.

### journal

Freeform. `wiki_capture` handles the structure. If you must write directly, suggested headings: **Captures**, **Decisions**, **Open Questions**, **Next Steps**.

---

## Worked examples

### Example 1 — Rust crate → concept page

You and the user spent time understanding how `comrak` exposes its AST. Type = `concept`, category = `tools`.

Search first:

```json
{
  "tool": "wiki_query",
  "args": { "query": "comrak markdown rust", "limit": 5 }
}
```

No hit. Reuse `rust`, `markdown` from `wiki_tags`; add `ast`.

Scaffold:

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

Fill sections — Overview, Key Properties, Examples — via `wiki_write upsert_section` (one call each):

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/comrak-ast-parser.md",
    "mode": "upsert_section",
    "section": "Key Properties",
    "content": "- Arena-allocated AST nodes (`Arena<AstNode>`)\n- Full CommonMark + GFM extension support\n- `format_commonmark` round-trips cleanly for most inputs"
  }
}
```

Each response carries `unlinked_mentions`. If the Examples block mentions `tantivy` and an existing `tantivy` page exists, you'll see `{"term": "tantivy", "target_slug": "tantivy", ...}` — wrap it in the next `upsert_section`.

Find related pages (no brackets):

```json
{
  "tool": "wiki_query",
  "args": { "query": "markdown parser rust", "limit": 5 }
}
```

For each related hit, append the wikilink:

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/markdown-tooling.md",
    "mode": "upsert_section",
    "section": "Related",
    "content": "- [[comrak-ast-parser]]"
  }
}
```

Verify:

```json
{ "tool": "wiki_read", "args": { "path": "tools/comrak-ast-parser.md" } }
```

```json
{ "tool": "wiki_lint", "args": {} }
```

If `orphan_count` includes the new page, go back to Step 6.

---

### Example 2 — Capture → triage → promote

Demonstrates the journal-first workflow: a quick capture, then promotion days later.

**Day 1, mid-session.** While debugging, you find a non-obvious comrak behavior. Worth keeping but you don't want to break flow:

```json
{
  "tool": "wiki_capture",
  "args": {
    "content": "comrak's format_commonmark unwraps nested lists when re-emitting — surprising for round-tripping",
    "tags": ["rust", "markdown", "comrak"],
    "source": "https://docs.rs/comrak"
  }
}
```

Response:

```json
{
  "status": "ok",
  "path": "wiki/_journal/2026-04-28.md",
  "created": true,
  "line": "**14:32** comrak's format_commonmark unwraps nested lists ... `#rust` `#markdown` `#comrak` ([source](https://docs.rs/comrak))"
}
```

Move on. Done.

**Day 8, triage prompt.** User says "any captures to triage?"

```json
{ "tool": "wiki_lint", "args": {} }
```

Response includes `stale_journal_pages: ["_journal/2026-04-28.md"]`. Read it:

```json
{ "tool": "wiki_read", "args": { "path": "_journal/2026-04-28.md" } }
```

The comrak observation is durable knowledge — promote it. Search for an existing target:

```json
{ "tool": "wiki_query", "args": { "query": "comrak", "category": "tools" } }
```

Hit: `tools/comrak-ast-parser.md` already exists. Append the gotcha rather than creating a new page:

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "tools/comrak-ast-parser.md",
    "mode": "append_section",
    "section": "Gotchas",
    "content": "- `format_commonmark` unwraps nested lists when re-emitting; not safe for round-tripping deeply nested markdown ([source](https://docs.rs/comrak))."
  }
}
```

Response includes `unlinked_mentions: [{"term": "format_commonmark", ...}]` — that's an API symbol, not a wiki page; ignore.

(If `wiki_query` had returned no hit, you'd run `wiki_new` for a fresh page and complete Steps 5–7 instead.)

Replace the journal line with a back-reference (read-modify-write, since journal frontmatter must be preserved):

```json
{ "tool": "wiki_read", "args": { "path": "_journal/2026-04-28.md" } }
```

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "_journal/2026-04-28.md",
    "mode": "overwrite",
    "content": "---\ntitle: \"2026-04-28\"\ntags: [journal]\ncreated: 2026-04-28\n---\n\n## Captures\n\n- **14:32** [promoted to [[comrak-ast-parser]]] comrak nested-list unwrap gotcha\n"
  }
}
```

The new commit clears the staleness flag. Re-lint to confirm `stale_journal_count: 0`.

---

### Example 3 — Architecture decision → ADR

Team chose Tantivy over SQLite FTS5 for search. Type = `decision`, category = `decisions`.

Search:

```json
{
  "tool": "wiki_query",
  "args": { "query": "tantivy sqlite search backend", "limit": 5 }
}
```

No hit. Tags: `rust`, `search`, `architecture`.

Scaffold:

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

Fill the five ADR sections via `wiki_write upsert_section`. Excerpt:

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "decisions/tantivy-over-sqlite-fts.md",
    "mode": "upsert_section",
    "section": "Rationale",
    "content": "- Tantivy is purpose-built for full-text search; BM25 scoring out of the box.\n- SQLite FTS5 lacks snippet highlighting and relevance tuning without significant custom work.\n- Tantivy's `IndexWriter`/`IndexReader` split maps naturally to the `lw serve` (writer) vs `lw query` (reader) concurrency model.\n- Pure Rust; no extra C FFI."
  }
}
```

Repeat for **Context**, **Decision**, **Alternatives**, **Consequences**.

Link related pages — ADRs benefit from explicit `related:` frontmatter, so use both body wikilinks and frontmatter.

Body link:

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "concepts/full-text-search.md",
    "mode": "upsert_section",
    "section": "Related",
    "content": "- [[tantivy-over-sqlite-fts]]"
  }
}
```

Frontmatter `related:` requires read-modify-write:

```json
{ "tool": "wiki_read", "args": { "path": "concepts/full-text-search.md" } }
```

```json
{
  "tool": "wiki_write",
  "args": {
    "path": "concepts/full-text-search.md",
    "mode": "overwrite",
    "content": "---\ntitle: \"Full-Text Search\"\ntags: [search, rust]\nrelated: [tantivy-over-sqlite-fts]\n---\n\n<original body>"
  }
}
```

Verify:

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

- **Never create orphan pages.** Every new page must have at least one inbound link from another page or be linked from a category index. After Step 6, confirm with `wiki_lint`.
- **Reuse existing tags.** Always call `wiki_tags` before inventing a new tag. New tags only when nothing fits.
- **Update existing pages before creating duplicates.** If `wiki_query` returns a close match, read it and update; don't create near-duplicates.
- **Read the schema before assuming categories.** Each vault's `.lw/schema.toml` is the source of truth for available categories and their `required_fields`. Don't memorize a default set — read it.
- **Act on `unlinked_mentions` thoughtfully.** Every write returns a list of phrases that match existing pages but aren't wrapped in `[[...]]`. Decide per entry: link if genuinely about that page, ignore if coincidental.
- **Backlinks include `[[wikilinks]]` and `related:` only.** Bare-slug mentions are NOT backlinks. To find pages mentioning a topic without linking, use `wiki_query` topic search.
- **Capture is reversible; pages aren't.** When in doubt, `wiki_capture` first, promote later. A misclassified permanent page is harder to undo.
- **Attribute sources.** External URLs, papers, or tool outputs go in **See Also** / **References** in the body, or in `sources:` frontmatter. Do not invent dates or authors.
- **Never silent-fail.** If a tool call returns an error, report it and explain before continuing.

## MCP tools available

- `wiki_query` — full-text search. Optional filters: `tags` (CSV, AND-ed, case-sensitive), `category`, `status`, `author`. `sort` ∈ `relevance` (default) | `created_desc` | `created_asc` | `title`. `limit` default 20. **Tantivy treats `[...]` as range syntax — search bare slugs without brackets.**
- `wiki_read` — read a page by `path` (vault-relative, e.g. `tools/comrak.md` or `.lw/schema.toml`); returns frontmatter fields + body + full markdown.
- `wiki_browse` — list pages filtered by `category`, `tag`, or `stale_only` boolean.
- `wiki_tags` — list all tags with page counts; optional `category` filter.
- `wiki_new` — scaffold a new page. Required: `category`, `slug` (`[a-z0-9_-]+`), `title`. Optional: `tags` (array), `author`. Errors name missing schema-required fields. Returns `unlinked_mentions`. Auto-commits unless `commit: false`.
- `wiki_write` — write/update a page. Required: `path`, `content`. `mode` ∈ `overwrite` (default) | `append_section` | `upsert_section`. `section` required for the latter two (case-insensitive heading match, no `##` prefix). Returns `unlinked_mentions`. Auto-commits unless `commit: false`; `push: true` pushes after.
- `wiki_capture` — append a timestamped entry to today's journal (`wiki/_journal/YYYY-MM-DD.md`); auto-creates the page if needed. Required: `content`. Optional: `tags`, `source`. Auto-commits.
- `wiki_backlinks` — return pages linking to a target. Arg: `path` (slug, category-relative, or vault path). Returns entries with `kind` ∈ `wikilink` | `related` and a `context` snippet for wikilinks.
- `wiki_ingest` — file source material into `raw/` (NOT `wiki/`). Pass `source_path` (file/URL) OR `content` (pasted markdown), not both. Optional: `filename`, `raw_type` ∈ `papers` | `articles` | `assets`, `title`, `tags`, `category` (suggested for the eventual page).
- `wiki_lint` — lint checks: stale pages, TODO markers, broken `related:`, orphans, missing concepts (broken wikilinks), `stale_journal_pages`. Optional `category` filter.
- `wiki_stats` — wiki health overview (page count, category breakdown, freshness distribution, index status). No args.
