# Case 2 — Out-of-scope item

## User

save my grocery list (eggs, milk, bread, oranges) to the wiki

## Agent (simulated reasoning per SKILL.md)

### Step 1: Identify input

Pasted text (inline list of items). Not a URL, not a file path. Per SKILL.md Step 1: pasted text routes through `wiki_write` for full pages or `wiki_ingest --stdin` for raw content — but routing only happens after the scope check passes.

### Step 2: Check scope

Read `SCOPE.md` (already loaded in this session, no second `wiki_read` needed):

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

Judgment: **out of scope.**

- Purpose says "links, articles, notes, references you want to come back to later" — a grocery list is ephemeral, not a reference.
- None of the four Includes buckets match (not an article, not a book/talk/podcast note, not a link, not a person/org).
- Closest Excludes match: "Daily journal entries (use a separate journal tool)" — a grocery list is the same class of ephemeral personal-life data that the vault explicitly redirects elsewhere.

### Step 3: Route

Per SKILL.md, "Clearly out of scope: do not silently drop. Tell the user." → No `lw ingest`, no `lw write` executed. Instead, the agent's reply to the user:

> This looks out of scope for your wiki. Purpose: "A general-purpose knowledge base. Capture interesting links, articles, notes, and references that you want to come back to later." A grocery list is closer to ephemeral todo/journal data, which your SCOPE.md's Excludes points to a separate journal tool. Want me to save it anyway (I can drop it under `raw/_uncategorized/`), or skip?

Awaiting user response before any write.

### Step 4: Confirm

N/A — no ingestion happened. Will confirm only after the user explicitly opts in.

### Verification: no files written

Pre-case-2 vault state:

```
raw/articles/claude-3-5-sonnet         (128.3K, from Case 1)
wiki/_uncategorized/welcome.md         (352B, template default)
```

Post-case-2 vault state (no commands run between):

```
raw/articles/claude-3-5-sonnet         (128.3K, from Case 1)
wiki/_uncategorized/welcome.md         (352B, template default)
```

Identical. Nothing new written. No grocery list anywhere in `raw/` or `wiki/`.

## Verdict: PASS

Agent correctly DECIDED not to ingest and instead asked. The skill's "out of scope → ask user" branch behaves exactly as specified, and no silent write happened.
