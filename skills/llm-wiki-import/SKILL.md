---
name: llm-wiki:import
description: Use when the user wants to add a URL, pasted text, or local file to their llm-wiki. Fetches content if needed, checks against the vault's SCOPE.md, and ingests via `wiki_ingest`.
when-to-use: User shares a link, paste, or path with intent to save. Trigger phrases include "add this to my wiki", "save this", "remember this article", "ingest this paper", or any case where the user supplies content with archival intent.
---

You are helping the user maintain an llm-wiki vault. The user has shared content (a URL, pasted text, or a file path) and wants it added to the wiki.

## Step 1 — Identify the input type

- **URL**: a web link → fetch it (use `wiki_ingest` with the URL directly; the tool handles fetching and parsing).
- **Pasted text**: raw markdown or prose → use `wiki_write` for full pages, or `wiki_ingest` from stdin.
- **Local file path**: a file the user already has → pass the path to `wiki_ingest`.

## Step 2 — Check scope

Read `SCOPE.md` from the vault root using `wiki_read SCOPE.md`. If `SCOPE.md` does not exist, **skip the scope check entirely** — proceed permissively.

If `SCOPE.md` exists, judge whether the new content fits the documented Purpose / Includes / Excludes. The judgment is yours as the agent — `SCOPE.md` is guidance, not a strict rule list.

## Step 3 — Route the content

- **Clearly in scope**: ingest immediately.
  - For URLs and files: `wiki_ingest` with `--raw-type articles` (or `papers` for arXiv-style links, `assets` for binary files).
  - For pasted markdown that looks like a finished article: `wiki_ingest --stdin`.
  - Suggest a category from the wiki's known categories (read `.lw/schema.toml` if needed).
- **Clearly out of scope**: do not silently drop. Tell the user:
  > "This looks out of scope for your wiki (Purpose: <quote>). Want me to add it anyway, or skip?"
  > Wait for an answer.
- **Ambiguous**: ask before ingesting.
  > "I'm not sure this fits the scope. The vault is for <Purpose>; this looks like <observation>. Add it?"

## Step 4 — Confirm

After ingestion, print a one-line confirmation: file path written, category, freshness (raw vs full page).

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
