# CLAUDE.md

## What This Is

LLM Wiki (`lw`) — an installable, agent-tool-agnostic knowledge base product. Rust workspace producing a single binary `lw` (CLI + MCP server) plus the canonical skills, starter templates, integration descriptors, and POSIX installer that ship together as a release tarball. Inspired by [Karpathy's LLM Wiki pattern](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f).

**Repo:** https://github.com/Pawpaw-Technology/llm-wiki
**Install:** `curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/latest/download/install.sh | sh`
**Tool + skills + installer are open source. Wiki content (vaults) is bring-your-own.**

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
├── lw-core/         # core library — wiki I/O, search, lint, freshness
├── lw-cli/          # CLI binary (`lw`) — umbrella for all commands
└── lw-mcp/          # MCP server library (used by `lw serve`)

skills/
└── llm-wiki-import/ # canonical agent skill (markdown + frontmatter)

templates/           # starter vaults copied by `lw workspace add --template <name>`
├── general/
├── research-papers/
└── engineering-notes/

integrations/        # declarative TOML adapters per agent tool
├── claude-code.toml # full MCP + skills (1.0)
├── codex.toml       # skills only in 1.0; auto-MCP in 1.1
└── openclaw.toml    # skills only in 1.0

installer/
├── install.sh       # POSIX, curl-installable, sha256-verified
└── uninstall.sh     # reverses install; preserves vault data
```

Vaults (wiki content) are **separate git repos** of markdown files; users bring their own.

The release tarball (built by `.github/workflows/release.yml` on `v*` tag push) bundles `lw` + `skills/` + `templates/` + `integrations/` + `installer/` + a `VERSION` file into one per-platform archive. `install.sh` and `uninstall.sh` are also published as separate top-level release assets so the curl line works without first downloading the binary.

## Dev workspace (replacing the deprecated mono)

`llm-wiki-mono` was previously used as a checkout convenience that bundled `llm-wiki` + `llm-wiki-agents` + `codebridge` + a private `wiki` content repo as submodules. **It is now archived.** Contributors who want all four repos visible at once should clone them as siblings:

```bash
mkdir -p ~/dev/llm-wiki && cd ~/dev/llm-wiki
git clone git@github.com:Pawpaw-Technology/llm-wiki.git
git clone git@github.com:Pawpaw-Technology/llm-wiki-agents.git
git clone git@github.com:64andrewwalker/codebridge.git
# (private wiki repo only if you have access)
```

Each repo is self-contained and CI'd independently. There is no longer a "meta repo" to update — code, PRs, and releases happen in each repo individually.

Spec, plans, and smoke transcripts that documented the v0.2.0 product wrapper rollout (formerly under `mono/docs/superpowers/`) now live in this repo under `docs/specs/`, `docs/plans/`, `docs/smoke-tests/`.

## Key Design Rules

- Only one trait: `Searcher`; everything else is concrete types
- All time-related data comes from `git log`, never stored in frontmatter
- Freshness is computed, never stored
- Tags are free-form (not enumerated in schema); categories are directories
- `_uncategorized/` is the fallback directory; lint reminds to categorize

## CLI Commands

**Wiki ops** (work against `--root <vault>`, env `LW_WIKI_ROOT`, the current registered workspace, or cwd auto-discover):

```bash
lw init                                            # scaffold wiki at --root or cwd
lw query "attention" --format json                 # search
lw ingest paper.pdf --category architecture --yes  # raw filing
lw read architecture/transformer.md
lw write tools/page.md --mode upsert --section Usage
lw lint --format json
lw status
lw serve                                           # MCP server (stdio)
```

**Wrapper / lifecycle** (operate on the install + integrations, not on a vault):

```bash
lw workspace add my-vault ~/wiki --template general # register vault, copy starter
lw workspace list | use | current [-v] | remove
lw integrate --auto | <tool> [--uninstall]          # wire MCP + skills into agent tool
lw upgrade --check | (apply)                        # update lw + skills + templates
lw uninstall [--yes] [--keep-config] [--purge]
lw doctor                                           # full health checklist + remediation hints
```

## MCP Tools

`wiki_query`, `wiki_read`, `wiki_browse`, `wiki_tags`, `wiki_write`, `wiki_ingest`, `wiki_lint`, `wiki_stats`

## Workspace registry

`~/.llm-wiki/config.toml` (overridable via `LW_HOME`) holds registered vaults. `lw serve` resolves its root via:
**`--root` flag > `LW_WIKI_ROOT` env > current registered workspace > cwd auto-discover**.
Switching workspaces requires restarting the agent tool — MCP processes bind their vault at launch (see spec §4.1).

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
