# LLM Wiki Design Spec

> Team-oriented, LLM-maintained knowledge base with CLI + MCP interface.
> Inspired by [Karpathy's LLM Wiki pattern](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f).

## Architecture Principles

| #   | Principle                               | Meaning                                                                   |
| --- | --------------------------------------- | ------------------------------------------------------------------------- |
| P1  | Stale docs = no docs                    | All design decisions prioritize freshness detection over content richness |
| P2  | Humans choose, never fill blanks        | All interactions are Y/n or multiple choice. LLM drafts, humans approve   |
| P3  | Over-classification = no classification | One layer of category dirs, tags for semantics. No nesting beyond that    |
| P4  | Decouple to delay tech debt             | Only extract traits when 2+ implementations exist                         |

## System Layers

```
Orchestration (shell + cron)
  triggers Claude Code / Codex / Kimi agents
       │
       ▼
LLM Agents (external processes)
  call tool layer via CLI or MCP
       │
       ▼
Tool Layer (Rust: lw-core + lw-cli + lw-mcp)
  pure tooling, zero LLM dependency for core ops
       │
       ▼
Wiki (independent git repo of markdown)
  pure data
```

The tool layer has zero runtime dependency on any LLM. If all agents are down,
`lw query "transformer"` still works. The `LlmBackend` trait exists so that
commands like `lw ingest` can optionally call an LLM for draft generation,
but every command has a non-LLM fallback path.

## Crate Topology

```
llm-wiki/                          # tool repo (this repo)
├── Cargo.toml                     # workspace root
├── crates/
│   ├── lw-core/                   # core library
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── schema.rs          # schema.toml parsing (concrete type)
│   │   │   ├── page.rs            # frontmatter + body parsing (concrete type)
│   │   │   ├── search.rs          # trait Searcher + tantivy impl
│   │   │   ├── llm.rs             # trait LlmBackend
│   │   │   ├── tag.rs             # taxonomy management (concrete type)
│   │   │   ├── link.rs            # [[wiki-link]] parsing, orphan detection
│   │   │   ├── fs.rs              # filesystem ops (concrete, no trait)
│   │   │   ├── ingest.rs          # raw -> wiki pipeline
│   │   │   ├── lint.rs            # freshness + health checks
│   │   │   ├── log.rs             # operations on git log
│   │   │   └── error.rs           # unified error type
│   │   └── Cargo.toml
│   │
│   ├── lw-cli/                    # CLI binary: `lw`
│   │   ├── src/main.rs
│   │   └── Cargo.toml             # deps: lw-core, clap
│   │
│   ├── lw-mcp/                    # MCP server logic (library crate)
│   │   ├── src/lib.rs
│   │   └── Cargo.toml             # deps: lw-core, rmcp
│   │
│   └── lw-server/                 # Phase 2 placeholder
│       └── .gitkeep
│
├── scripts/
│   ├── cron/                      # scheduled task templates
│   └── agents/                    # agent invocation examples
│
└── docs/
```

### Trait Boundaries

Only two traits exist. Everything else is concrete types.

```rust
/// Search backend.
/// Two implementations justified: tantivy (large wiki) vs grep-scan (small wiki).
pub trait Searcher: Send + Sync {
    fn search(&self, query: &SearchQuery) -> Result<SearchResults>;
    fn index_page(&self, page: &Page) -> Result<()>;
    fn remove_page(&self, path: &Path) -> Result<()>;
    fn rebuild(&self) -> Result<()>;
}

/// LLM abstraction. The core decoupling point between tool layer and intelligence.
/// Implementations: Claude API, OpenAI, Kimi, local ollama, subprocess/codebridge.
pub trait LlmBackend: Send + Sync {
    fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse>;
    fn available(&self) -> bool; // health check — tools work even when LLM is down
}
```

## Wiki Repo Structure

The wiki is a **separate git repo**, not inside the tool repo.

```
my-team-wiki/
├── .lw/
│   ├── schema.toml                 # minimal schema
│   └── search/                     # tantivy index (.gitignore)
├── raw/                            # immutable source material
│   ├── papers/
│   ├── articles/
│   └── assets/
├── wiki/                           # one layer of category dirs, no deeper
│   ├── architecture/
│   │   ├── transformer.md
│   │   └── flash-attention-2.md
│   ├── training/
│   ├── infra/
│   ├── tools/
│   ├── product/
│   ├── ops/
│   └── _uncategorized/             # fallback; lint reminds to categorize
└── README.md
```

**Not committed to git:**

- `.lw/search/` (tantivy index, rebuilt at runtime)
- Any index cache files

**Files that don't exist by design:**

- `log.md` — use `git log` instead; append-only files cause merge conflicts in team repos
- `index.json` — runtime cache rebuilt from frontmatter, not persisted

## Page Format

### Frontmatter

```yaml
---
title: Flash Attention 2
tags: [architecture, attention, optimization]
decay: normal
sources: [raw/papers/flash-attention-2.pdf]
author: vergil
generator: kimi
---
```

| Field       | Required | Description                                                      |
| ----------- | -------- | ---------------------------------------------------------------- |
| `title`     | yes      | The only hard requirement                                        |
| `tags`      | no       | Free-form, no pre-registration needed                            |
| `decay`     | no       | `fast` / `normal` / `evergreen`. Defaults inferred from category |
| `sources`   | no       | Paths to raw/ source material                                    |
| `author`    | no       | Human who triggered/owns this page                               |
| `generator` | no       | LLM that wrote it: `kimi` / `claude` / `human`                   |

**Deliberately excluded from frontmatter:**

- `created` / `updated` — use `git log --follow`
- `last_reviewed` — same, avoids LLM hallucination of dates
- `confidence` — replaced by `decay` (high confidence ~ evergreen, speculative ~ fast)
- `links` — parsed from `[[wiki-link]]` in body text, not duplicated in frontmatter

### Body

Standard markdown with `[[wiki-link]]` syntax for cross-references (Obsidian-compatible).
Links are resolved by filename without extension: `[[transformer]]` links to
`wiki/architecture/transformer.md`. The tool resolves across categories automatically.

## schema.toml

```toml
[wiki]
name = "Acme AI Team Wiki"
default_review_days = 90

[tags]
# Only categories are defined. Specific tags grow freely.
categories = ["architecture", "training", "infra", "tools", "product", "ops"]

[tags.decay_defaults]
product = "fast"
architecture = "normal"
training = "normal"
infra = "normal"
tools = "fast"
ops = "normal"
```

**Design rule:** categories live in schema, tags do not. Category = directory,
tag = semantic label. The two are orthogonal.

## Freshness Mechanism

Freshness is a **computed state**, never a stored field.

**Inputs:**

- `decay` level (from page frontmatter, or category default from schema)
- Last modified time (from `git log --follow -1 --format=%aI -- <path>`)

**Thresholds:**

| Decay       | Stale after        | Typical content                           |
| ----------- | ------------------ | ----------------------------------------- |
| `fast`      | 30 days            | Product pricing, API docs, model releases |
| `normal`    | 90 days            | Training methods, architecture analysis   |
| `evergreen` | Never (time-based) | Foundational theory, math derivations     |

**`lw lint` signal levels:**

| Level   | Meaning              | Trigger                                                                                        |
| ------- | -------------------- | ---------------------------------------------------------------------------------------------- |
| STALE   | Past decay threshold | `now - git_last_modified > threshold`                                                          |
| SUSPECT | Might need update    | New raw/ source with overlapping tags; broken `[[wiki-link]]`; sibling pages recently modified |
| FRESH   | No action needed     | Within threshold, or evergreen                                                                 |

## CLI Interface

### Phase 1 Commands

#### `lw init`

Generates `.lw/schema.toml` + directory skeleton in current directory.

```
$ lw init
Created .lw/schema.toml
Created wiki/{architecture,training,infra,tools,product,ops,_uncategorized}/
Created raw/{papers,articles,assets}/
Wiki initialized. Edit .lw/schema.toml to customize.
```

#### `lw ingest <file|url|stdin>`

Import source material to `raw/`, optionally generate wiki page draft via LLM.

```
$ lw ingest raw/papers/flash-attention-3.pdf

  Title: Flash Attention 3
  Tags: [architecture, attention, optimization]
  Category: architecture
  Decay: normal

  Create wiki/architecture/flash-attention-3.md? [Y/n]
  Edit tags? [enter to keep]
  Edit decay? [enter to keep]
```

- If LLM is unavailable, saves to raw/ and skips draft generation (logs a warning)
- URL sources are downloaded to raw/ first
- stdin allows piping: `cat notes.md | lw ingest --title "Meeting Notes"`

#### `lw query <text>`

Full-text search with optional filters.

```
$ lw query "flash attention" --tag optimization --category architecture

  1. architecture/flash-attention-2.md  [attention, optimization]  FRESH
  2. architecture/transformer.md        [transformer, attention]   FRESH

  [number] to read, [q] to quit
```

Options:

- `--tag <tag>` — filter by tag
- `--category <cat>` — filter by category directory
- `--stale` — only show stale/suspect pages
- `--format json|human|brief` — output format (default: human)
- `--limit <n>` — max results (default: 20)

JSON output follows agent-friendly envelope:

```json
{
  "command": "query",
  "query": "flash attention",
  "total": 2,
  "returned": 2,
  "results": [
    {
      "path": "wiki/architecture/flash-attention-2.md",
      "title": "Flash Attention 2",
      "tags": ["architecture", "attention", "optimization"],
      "category": "architecture",
      "freshness": "fresh",
      "snippet": "...Flash Attention reduces memory from O(N^2) to O(N)..."
    }
  ]
}
```

#### `lw serve`

Start MCP server over stdio. This command lives in `lw-cli` but delegates to
`lw-mcp` internals — the MCP crate is a library dependency, not a separate binary.
Single binary distribution: `lw` does everything.

Exposed tools:

| Tool          | Parameters                                    | Description                    |
| ------------- | --------------------------------------------- | ------------------------------ |
| `wiki_query`  | `query`, `tags?`, `category?`, `limit?`       | Full-text search               |
| `wiki_read`   | `path`                                        | Read a wiki page               |
| `wiki_ingest` | `source_path`, `title?`, `tags?`, `category?` | Import + generate draft        |
| `wiki_list`   | `category?`, `tag?`, `stale_only?`            | List pages                     |
| `wiki_lint`   | `category?`                                   | Return freshness report (JSON) |
| `wiki_write`  | `path`, `content`, `frontmatter`              | Write/update a wiki page       |

MCP mode has no interactive Y/n prompts. The calling LLM agent decides whether
to execute. P2 (humans choose) is enforced at CLI layer; MCP layer trusts
the caller.

## Orchestration Layer

Not part of the Rust codebase. Shell scripts + cron that trigger LLM agents.

Example scripts provided in `scripts/`:

```bash
# scripts/cron/weekly-lint.sh
# Triggered by cron, runs Claude Code against the wiki
cd /path/to/team-wiki
claude -p "Run lw lint and handle all STALE pages. \
For each one, read the page and its sources, then update it. \
Commit changes with descriptive messages."
```

```bash
# scripts/cron/daily-ingest.sh
# Process any new files dropped in raw/inbox/
cd /path/to/team-wiki
for f in raw/inbox/*; do
  lw ingest "$f"
done
```

```bash
# scripts/agents/librarian.sh
# The "librarian" agent — interactive Q&A powered by Kimi K2P5
cd /path/to/team-wiki
kimi -p "You are a librarian for this wiki. \
Use 'lw query' and 'lw serve' to answer questions. \
Always cite wiki pages in your answers."
```

These are templates. Teams customize for their own agent tooling and cron setup.

## Phase Roadmap

### Phase 1 (MVP)

- `lw-core`: schema, page, fs, search (tantivy), llm trait, tag, link, ingest, error
- `lw-cli`: init, ingest, query, serve
- `lw-mcp`: wiki_query, wiki_read, wiki_ingest, wiki_list, wiki_lint, wiki_write
- `scripts/`: example cron + agent templates

### Phase 2

- `lw lint` — interactive freshness triage
- `lw challenge` — force-review evergreen pages
- `lw classify` — batch re-tag pages
- `lw digest` — periodic summary generation
- `lw-server` — HTTP API for remote access
- Grep-scan Searcher implementation (for small wikis without tantivy index)

### Phase 3

- Plugin system (if trait boundaries prove stable)
- Multi-wiki federation (cross-instance query)
- Web UI for browsing

## Not Doing

- Plugin system (but trait boundaries are clean for future extraction)
- HTTP server in Phase 1
- Page type system (concept/entity/guide — removed)
- Nested subdirectories beyond category level
- Persisted index.json in git
- Temporal fields in frontmatter (git is source of truth)
- Enumerated tags in schema (only categories)
- Fill-in-the-blank CLI interactions
