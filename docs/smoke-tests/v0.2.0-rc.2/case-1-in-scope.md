# Case 1 — In-scope URL

## User

add this to my wiki — https://www.anthropic.com/news/claude-3-5-sonnet

## Agent (simulated reasoning per SKILL.md)

### Step 1: Identify input

URL detected (`https://...`). Per SKILL.md Step 1: URLs go through `wiki_ingest` with the URL directly — the tool handles fetching and parsing.

### Step 2: Check scope

Read `SCOPE.md` from the vault root. Contents:

```
# Scope

## Purpose

A general-purpose knowledge base. Capture interesting links, articles, notes, and references that you want to come back to later.

## Includes

- Articles and blog posts you found valuable
- Notes from books, talks, podcasts
- Links worth saving with context for why
- People and organizations worth tracking

## Excludes

- Private credentials, API keys, secrets
- Daily journal entries (use a separate journal tool)
- Code snippets that belong in a real codebase
```

Judgment: **in-scope** because the URL is a published Anthropic news article announcing a model release. It hits the "Articles and blog posts you found valuable" bucket directly, and matches Purpose ("links, articles ... you want to come back to later"). No Excludes apply (not a credential, not a journal entry, not a code snippet).

### Step 3: Route

Command run:

```bash
/tmp/lw-behavior-prefix/bin/lw --root /tmp/lw-behavior-vault \
  ingest 'https://www.anthropic.com/news/claude-3-5-sonnet' \
  --raw-type articles --category notes --yes
```

Actual output:

```
Downloading https://www.anthropic.com/news/claude-3-5-sonnet...
Saved to /tmp/lw-behavior-vault/raw/articles/claude-3-5-sonnet
path: raw/articles/claude-3-5-sonnet
title: claude-3-5-sonnet
category: notes
```

### Step 4: Confirm

File exists at `raw/articles/claude-3-5-sonnet` (128.3 KB, 131404 bytes). Confirmation message to user:

> Saved Anthropic's "Introducing Claude 3.5 Sonnet" article to `raw/articles/claude-3-5-sonnet` under the `notes` category (raw form, not yet promoted to a full wiki page).

## Verdict: PASS

The skill steps fired in the correct order, the scope check correctly judged the URL in-scope, and `lw ingest` succeeded with the expected file written under `raw/articles/`.

### Tool-layer observation (non-blocking)

The fetched content is stored as raw HTML rather than converted to markdown. Acceptable for `--raw-type articles` (the "raw" bucket is by definition pre-clean), but worth flagging for tool authors: a future polishing pass would want a markdown render before page promotion. This does NOT affect the skill gate verdict.
