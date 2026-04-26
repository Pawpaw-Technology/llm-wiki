---
name: llm-wiki:import
description: Use when the user wants to add a URL, pasted text, or local file to their llm-wiki. Fetches content if needed, checks against the vault's SCOPE.md, and ingests via `wiki_ingest`.
when-to-use: User shares a link, paste, or path with intent to save. Trigger phrases include "add this to my wiki", "save this", "remember this article", "ingest this paper", or any case where the user supplies content with archival intent.
---

You are helping the user maintain an llm-wiki vault. The user has shared content (a URL, pasted text, or a file path) and wants it added to the wiki.

## Step 1 — Identify the input type

- **URL**: a web link → pass the URL as `source_path` to `wiki_ingest` (the tool handles fetching and parsing).
- **Pasted text**: raw markdown or prose → pass it as `content` to `wiki_ingest` (preferred), or use `wiki_write` directly if it is already a finished wiki page. Do not stage pasted text to a temp file first; `content` is the MCP-native path.
- **Local file path**: a file the user already has → pass the absolute path as `source_path` to `wiki_ingest`.

`source_path` and `content` are mutually exclusive — pick one.

## Step 2 — Check scope

Read `SCOPE.md` from the vault root using `wiki_read SCOPE.md`. If `SCOPE.md` does not exist, **skip the scope check entirely** — proceed permissively.

If `SCOPE.md` exists, judge whether the new content fits the documented Purpose / Includes / Excludes. The judgment is yours as the agent — `SCOPE.md` is guidance, not a strict rule list.

## Step 3 — Route the content

- **Clearly in scope**: ingest immediately.
  - For URLs and files: `wiki_ingest` with `source_path` + `raw_type: "articles"` (or `"papers"` for arXiv-style links, `"assets"` for binary files).
  - For pasted markdown: `wiki_ingest` with `content: "<the pasted text>"`. Optionally pass `title` (used to derive the filename slug) or an explicit `filename: "my-note.md"`. The tool creates `raw/<raw_type>/<filename>` in the vault — you do not need to stage the content anywhere first.
  - Suggest a category from the wiki's known categories (read `.lw/schema.toml` if needed).
- **Clearly out of scope**: do not silently drop. Tell the user:
  > "This looks out of scope for your wiki (Purpose: <quote>). Want me to add it anyway, or skip?"
  > Wait for an answer.
- **Ambiguous**: ask before ingesting.
  > "I'm not sure this fits the scope. The vault is for <Purpose>; this looks like <observation>. Add it?"

## Step 4 — Confirm

After ingestion, print a one-line confirmation: file path written, category, freshness (raw vs full page).

## Inspecting `unlinked_mentions` after a write

Every successful `wiki_write` and `wiki_new` response includes an
`unlinked_mentions` field — always present, empty array when none:

```json
{
  "status": "ok",
  "path": "tools/my-page.md",
  "unlinked_mentions": [
    {
      "term": "Flash Attention",
      "target_slug": "flash-attention",
      "line": 4,
      "context": "Flash Attention is the main technique here."
    }
  ]
}
```

**Scope rules:**

- `overwrite` mode: mentions are scanned across the **entire** written page body.
- `append_section` / `upsert_section` modes: only the **section content** you
  wrote is scanned. Sibling sections are intentionally excluded — the agent only
  edited one section, and the rest may already be fully linked.

**When to follow up:**

After a write, check `unlinked_mentions`. For each entry, decide whether to wrap
the term in a `[[wikilink]]`:

- If the term refers to a closely related concept the reader would benefit from
  following, add a link by calling `wiki_write` in `upsert_section` mode on the
  affected section (or `overwrite` if you want to update the full page).
- If the mention is incidental (e.g., a generic word that happens to match a
  page title) or the term appears many times and the first occurrence is already
  linked, you can skip it.
- Never blindly link every suggestion — link to aid navigation, not for
  completeness.

## Hard rules

- Never silent-fail. If you cannot complete the import, tell the user why.
- Never overwrite an existing wiki page without confirmation; use `wiki_read` first to check.
- For URLs that fail to fetch (404, paywall, login wall), report the failure and ask whether to file the URL with a note rather than the content.
- Do not invent metadata (publication date, author) that you cannot verify from the source.

## MCP tools available

- `wiki_query` — search existing pages
- `wiki_read` — read a page
- `wiki_browse` — browse categories / tags
- `wiki_ingest` — file new raw content (URL, file, or stdin)
- `wiki_write` — write or update a wiki page
- `wiki_lint` — health check
- `wiki_tags` — list tags
