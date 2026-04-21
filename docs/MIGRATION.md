# Migrating from `llm-wiki-mono` to standalone `lw`

`llm-wiki-mono` was the dev-time monorepo that bundled `llm-wiki` + `llm-wiki-agents` + `codebridge` + a private wiki content repo as git submodules. **It is archived as of 2026-04-21.** This guide walks you through switching to the new flow: install the `lw` product directly via curl, register your existing wiki vault, and wire up your agent tool.

The migration is **non-destructive** — your wiki content (the markdown files in `mono/wiki/`) lives in its own git repo and isn't touched by anything below. You can run the steps below alongside your existing mono checkout, verify the new setup works, then delete the mono checkout when ready.

**Total time:** ~5 minutes if your tools (Claude Code / Codex / OpenClaw) are already installed.

---

## What changes

| Before (mono)                                                               | After (standalone)                                                               |
| --------------------------------------------------------------------------- | -------------------------------------------------------------------------------- | --- | ----- |
| `cd llm-wiki-mono/tool/llm-wiki && cargo build` to build `lw`               | `curl install.sh` once → `~/.llm-wiki/bin/lw`                                    |
| `git submodule update --remote` to bump tool version                        | `lw upgrade`                                                                     |
| Manually wire MCP into `~/.claude.json` / `~/.codex/...` etc.               | `lw integrate --auto` (handles all 3 tools, with backups)                        |
| One workspace per checkout (`cd` into the right dir, or set `LW_WIKI_ROOT`) | Multi-vault registry: `lw workspace add                                          | use | list` |
| No health check                                                             | `lw doctor` (10-check matrix with remediation hints)                             |
| Skills lived in `mono/.claude/skills/` (gitignored, manual setup)           | Canonical skill bundled in release; auto-linked to agent tools by `lw integrate` |
| Custom scripts to find the right `lw` binary                                | `lw` is on PATH (after `exec $SHELL` post-install)                               |

The `lw` CLI commands you already use (`init`, `query`, `ingest`, `read`, `lint`, `status`, `serve`, `write`) are **unchanged**. Five new wrapper commands (`workspace`, `integrate`, `upgrade`, `uninstall`, `doctor`) were added on top.

---

## Step 1 — Install `lw` (replaces the cargo build dance)

```bash
curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/latest/download/install.sh | sh
exec $SHELL    # reload PATH so ~/.llm-wiki/bin comes first
lw --version   # should print 0.2.2 or newer
```

The installer:

- Detects your OS/arch, downloads the right tarball, sha256-verifies it
- Stages everything under `~/.llm-wiki/` (binary + skills + templates + integrations + installer scripts)
- Appends a `# >>> llm-wiki >>>` PATH block to your shell rc (`.zshrc`/`.bashrc`/`.config/fish/config.fish`)
- Skips agent-tool integration in this step — that's Step 4

Flags worth knowing:

- `--no-integrate` — also skip PATH injection (full sandbox; useful for testing)
- `--yes` — auto-integrate detected agent tools during install
- `--prefix /custom/path` — install somewhere other than `~/.llm-wiki/`

---

## Step 2 — Clone wiki + agents + codebridge as siblings (replaces submodules)

If you only ever need the wiki content (the markdown files), clone just that:

```bash
mkdir -p ~/dev/llm-wiki && cd ~/dev/llm-wiki
git clone git@github.com:Pawpaw-Technology/llm-wiki-data.git
```

If you also work on the agents / codebridge layer:

```bash
git clone git@github.com:Pawpaw-Technology/llm-wiki.git           # the product source (mostly Rust)
git clone git@github.com:Pawpaw-Technology/llm-wiki-agents.git    # batch / cron orchestration (TypeScript)
git clone git@github.com:64andrewwalker/codebridge.git            # LLM dispatch library
```

Each repo is now self-contained and CI'd independently. There's no longer a "meta repo" to update — code, PRs, and releases happen in each repo individually.

---

## Step 3 — Register your wiki vault

```bash
lw workspace add team-wiki ~/dev/llm-wiki/llm-wiki-data
```

(Pick whatever name + path you want. The first workspace you register becomes "current" automatically.)

If you have multiple vaults (e.g. work + personal), register them all and switch with `lw workspace use <name>`. The currently-selected vault is what `lw serve` (the MCP server) binds to at launch — switching workspaces requires restarting your agent tool to pick up.

Verify:

```bash
lw workspace list             # shows all registered vaults; * marks current
lw workspace current -v       # full root-resolution chain (debugging)
lw status                     # uses current workspace; shows page count + categories + freshness
```

---

## Step 4 — Wire your agent tool

```bash
lw integrate --auto
```

Detects which of Claude Code / Codex / OpenClaw you have installed (presence of `~/.claude/`, `~/.codex/`, `~/.openclaw/`) and prompts to integrate each. Pass `--yes` to skip prompts.

What it does per tool:

- **Claude Code (full)**: writes an `mcpServers.llm-wiki` entry to `~/.claude.json` (atomic, with `.bak.<timestamp>` backup), and symlinks the canonical `llm-wiki-import` skill into `~/.claude/skills/llm-wiki/`
- **Codex (skills only in 1.0)**: symlinks skills into `~/.codex/skills/llm-wiki/`. MCP wiring is manual for 1.0 (Codex uses TOML, our merge engine is JSON-only); auto-MCP lands in 1.1
- **OpenClaw (skills only in 1.0)**: symlinks skills

To uninstall any single tool's integration: `lw integrate <tool> --uninstall` (atomic, restores prior state).

---

## Step 5 — Restart your agent tool, verify the MCP

For **Claude Code**: full **Cmd+Q** (not just close window) and reopen — MCP servers are loaded once at app launch.

In the new agent session:

- Run `/mcp` (Claude Code) — you should see `llm-wiki · ✔ connected` listed under "User MCPs"
- Try `wiki_query "<some keyword from your wiki>"` — should return scored hits with paths/tags/snippets
- Or just say "find pages about X in my wiki" — the agent will pick `wiki_query` itself
- Try the import skill: paste a URL with archival intent (e.g. "add this to my wiki — https://...") — agent should fetch, scope-check against `SCOPE.md` if present, and `wiki_ingest`

For **Codex / OpenClaw**: their skill load mechanism varies; consult the tool's docs. The `~/.<tool>/skills/llm-wiki/` symlink is in place either way.

---

## Step 6 — `lw doctor`

```bash
lw doctor
```

Should print a 10-check matrix with all `✓`. If anything is `⚠` or `✗`, the line includes a remediation hint. Common cases:

| Check                        | If failing                             | Fix                                                        |
| ---------------------------- | -------------------------------------- | ---------------------------------------------------------- |
| binary location              | rare — install corrupted               | re-run `curl install.sh`                                   |
| PATH includes lw bin         | you didn't `exec $SHELL` after install | open a new terminal or `source ~/.zshrc`                   |
| current workspace            | path no longer exists on disk          | `lw workspace remove <name>` then re-add at the right path |
| binary/assets version        | manual binary swap                     | `lw upgrade` to realign                                    |
| integration: <tool> (MCP)    | entry missing                          | `lw integrate <tool>`                                      |
| integration: <tool> (skills) | symlink dangling                       | `lw integrate <tool>`                                      |

`lw doctor` is the first thing to run when anything seems off.

---

## Step 7 — (Optional) delete the mono checkout

After Steps 1-6 work, the mono checkout has nothing your daily workflow needs:

```bash
# Sanity: confirm your wiki content lives elsewhere now
ls ~/dev/llm-wiki/llm-wiki-data/wiki/   # should show your categories

# OK to delete
rm -rf ~/path/to/llm-wiki-mono
```

If you had local-only branches in `mono/` you weren't ready to lose, push them to a personal fork first — `mono` itself is GitHub-archived (read-only) so direct push is rejected.

---

## Notable workflow shifts

### From "build to upgrade" to "release to upgrade"

In mono, getting the latest `lw` meant `cd tool/llm-wiki && git pull && cargo build --release`, then dealing with PATH or symlinks.

Now: `lw upgrade --check` queries GitHub releases; `lw upgrade` re-runs the install with the latest tarball. Binary + skills + templates + integrations are version-pinned together so doctor can detect skew.

### From "submodule pointer dance" to "release tag"

Old: bump submodule pointer in mono after merging in `tool/llm-wiki`. Anyone pulling mono had to remember to `git submodule update`.

New: tag a release (`v0.2.3` etc.) on llm-wiki and release.yml builds 4 platforms + publishes install.sh. Users get it via `lw upgrade`.

### From "set LW_WIKI_ROOT and remember it" to "registered workspaces"

Old: depended on the env var or `cd` into the right dir.

New: `lw workspace add` registers a name → path mapping persisted in `~/.llm-wiki/config.toml`. Resolution priority: `--root` flag > `LW_WIKI_ROOT` env > current registered workspace > cwd auto-discover. The bottom two layers preserve all old behavior; the new middle layer activates only when you've registered.

### From "manual MCP setup" to `lw integrate`

Old: edit `~/.claude.json` by hand for Claude Code, hope you didn't break the surrounding JSON.

New: `lw integrate claude-code` does an atomic merge with version markers + sibling-preserved + automatic `.bak.<timestamp>`. Reversible with `--uninstall`.

---

## Troubleshooting

**`lw: command not found` after install**
You didn't reload your shell. `exec $SHELL` or open a new terminal. Confirm with `which lw` — should be `~/.llm-wiki/bin/lw`.

**`/mcp` doesn't show llm-wiki after install + integrate**
You closed the window instead of quitting Claude Code. Cmd+Q (verify dock icon disappears) and reopen. `/reload-plugins` does NOT reload user-MCPs from `~/.claude.json`.

**Old `/usr/local/bin/lw` or `/opt/homebrew/bin/lw` shadowing the new one**
Check with `which -a lw`. Old binary from a Homebrew tap or `cargo install` may be earlier in PATH. Easiest cleanup: `cargo uninstall lw-cli` for the cargo one; for the homebrew one, `brew uninstall lw` if it came from a tap, otherwise `rm /opt/homebrew/bin/lw` (it's just a symlink in most cases).

**`lw doctor` warns "current workspace 'foo' points to /old/mono/wiki which no longer exists"**
You deleted the mono checkout but the registered workspace still points inside it. Fix:

```bash
lw workspace remove foo
lw workspace add foo ~/dev/llm-wiki/llm-wiki-data
```

**Integration backup files cluttering up `~/.claude/`**
Each `lw integrate` (and `--uninstall`) creates a `.bak.<unix-timestamp>` file. Safe to delete after a few weeks of no rollback needed:

```bash
ls ~/.claude/settings.json.bak.* ~/.claude.json.bak.*
# rm them once you're confident
```

**Need a clean slate**
Full reset: `lw uninstall --yes`, then re-run the curl install. Vault data is preserved (registered workspace dirs are never touched); `~/.llm-wiki/config.toml` is moved to `~/.llm-wiki.config.toml.bak.<ts>` so reinstall can restore vault registrations.

---

## Where to find things now

| What                                                               | Where                                                                                                         |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------- |
| **Product (Rust + skills + templates + integrations + installer)** | https://github.com/Pawpaw-Technology/llm-wiki                                                                 |
| **Batch/cron orchestration (TypeScript)**                          | https://github.com/Pawpaw-Technology/llm-wiki-agents                                                          |
| **LLM dispatch library**                                           | https://github.com/64andrewwalker/codebridge                                                                  |
| **Wiki content (private)**                                         | https://github.com/Pawpaw-Technology/llm-wiki-data                                                            |
| **Specs / plans / smoke transcripts**                              | [llm-wiki/docs/](https://github.com/Pawpaw-Technology/llm-wiki/tree/main/docs) (specs/, plans/, smoke-tests/) |
| **Archived monorepo (read-only)**                                  | https://github.com/Pawpaw-Technology/llm-wiki-mono                                                            |

Each lives independently; CI, releases, issues happen in the individual repo.
