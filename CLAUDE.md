# CLAUDE.md

## What This Is

LLM Wiki (`lw`) — a team knowledge base toolkit. Rust workspace producing a single binary `lw` with CLI commands and an MCP server. Inspired by [Karpathy's LLM Wiki pattern](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f).

**Repo:** https://github.com/Pawpaw-Technology/llm-wiki
**Tool is open source. Wiki content repos are private.**

## Build & Test

```bash
cargo build                # build all crates
cargo test                 # run all tests
cargo run --bin lw         # run CLI
make test                  # via Makefile (includes clippy + fmt)
```

## Architecture

```
crates/
├── lw-core/    # core library — all logic lives here
├── lw-cli/     # CLI binary (`lw`)
├── lw-mcp/     # MCP server library (used by `lw serve`)
└── lw-server/  # Phase 2 HTTP server (placeholder)
```

Wiki is a **separate git repo** of markdown files, not in this repo.

## Key Design Rules

- Only one trait: `Searcher`; everything else is concrete types
- All time-related data comes from `git log`, never stored in frontmatter
- Freshness is computed, never stored
- Tags are free-form (not enumerated in schema); categories are directories
- `_uncategorized/` is the fallback directory; lint reminds to categorize

## CLI Commands

```bash
lw init                                          # scaffold wiki
lw query "attention" --format json               # search
lw ingest paper.pdf --category architecture --yes  # import (agent mode)
lw serve                                         # MCP server (stdio)
lw status                                        # wiki health overview
```

## MCP Tools

`wiki_query`, `wiki_read`, `wiki_browse`, `wiki_tags`, `wiki_write`, `wiki_ingest`, `wiki_lint`

---

## Development Workflow: 4-Step TDD

**Every task MUST follow this exact flow. No exceptions.**

### Step 1: RED — Write Failing Tests

- Write test files first. Tests define the contract.
- Create stub modules (empty or minimal) so the project compiles.
- Run tests. **Verify they FAIL.** Report failure output.
- **Commit the tests.** Message: `test(module): add tests for <feature>`

### Step 2: Review Tests

- Before implementing, review the tests:
  - Do they cover all spec requirements?
  - Are there missing edge cases?
  - Do they test behavior, not implementation details?
- If tests are insufficient, fix them and re-commit before proceeding.

### Step 3: GREEN — Implement to Pass

- Write the minimal implementation to make all tests pass.
- Run tests. **Verify they PASS.** Report pass output.
- Run `cargo clippy` and `cargo fmt`. Fix any issues.
- **Commit the implementation.** Message: `feat(module): <what was implemented>`

### Step 4: Review Implementation

- Spec compliance review: does the code match requirements? Nothing missing, nothing extra?
- Code quality review: clean error handling, no unwrap in library code, consistent naming?
- If issues found, fix and re-commit. Do not move to next task with open issues.

### Rules for Subagents

- **Subagents MUST commit their own work.** Uncommitted changes = task NOT done.
- **Subagents MUST report git SHA** of their commits in the status report.
- **Subagents work in worktrees** (`git worktree add`), never directly on main.
- **No racing** — parallel agents must be in separate worktrees touching different files.
- After merge to main, run full `cargo test` before pushing.

### Commit Discipline

- One logical change per commit. Tests and implementation are separate commits.
- Commit messages follow conventional commits: `feat()`, `fix()`, `test()`, `docs()`
- Never skip pre-commit hooks (`--no-verify`).
- Always create NEW commits, never amend.

---

## CI/CD

- GitHub Actions: fmt, clippy, test (ubuntu + macos matrix)
- Release workflow: cross-compile for 4 targets on tag push
- Docker: multi-stage build, non-root runtime

## Observability

- `#[tracing::instrument]` on key functions
- `RUST_LOG=debug lw query "test"` for debug output
- Logs to stderr, never stdout

## Project Conventions

- Edition 2024 (requires Rust 1.85+)
- Errors: `thiserror` for library (`WikiError`), `anyhow` for binaries
- CLI: `clap` derive, every `--help` has Examples section
- Agent-friendly: `--yes` skips prompts, `--format json` for machine output, errors to stderr
- Exit codes: 0=success, 1=error, 2=no results
