# llm-wiki

A CLI + skill bundle that turns any markdown folder into an agent-driven knowledge base. Bring your own data; bring your own agent tool. Built for Claude Code, Codex, and OpenClaw.

> Why not just `markdown folder + filesystem MCP`? llm-wiki adds scope-gated ingest (`SCOPE.md` controls what gets in), wiki-aware operations (lint, freshness, orphans, broken links via git), and curated skills that turn import + curate into one-shot agent actions instead of N-step manual prompting.

## Install

```bash
curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/latest/download/install.sh | sh
```

The installer fetches the matching prebuilt binary for your platform (macOS / Linux, x86_64 / aarch64), verifies sha256, and stages everything under `~/.llm-wiki/`. Pin a version: `... releases/download/v0.2.0/install.sh`. CI / unattended? Append `--no-integrate` (default for non-TTY) or `--yes` (auto-integrate detected agent tools).

## First vault, with a template

```bash
lw workspace add my-research ~/wiki/research --template research-papers
lw integrate --auto
```

The `--template` flag copies a starter vault (3 templates ship: `general`, `research-papers`, `engineering-notes`), each with a `SCOPE.md` and a category schema you can edit. `lw integrate --auto` detects installed agent tools and wires up the MCP server + canonical skill.

## Daily use

Open your agent tool inside the vault. The `llm-wiki:import` skill triggers on phrases like "add this to my wiki" or "save this article":

```
You: add this to my wiki — https://arxiv.org/abs/2501.12345
Agent: [fetches paper, reads SCOPE.md, judges fit]
       Fits the "research papers" scope. Filing under raw/papers/2501.12345.md.
       Want me to draft a wiki page summary too?
```

Out-of-scope content prompts before being silently dropped:

```
You: add my grocery list to the wiki
Agent: This looks out of scope (vault Purpose: ML research papers). Add anyway, or skip?
```

## Multiple vaults (Obsidian-style, optional)

```bash
lw workspace add personal ~/notes/personal --template general
lw workspace use personal      # current
lw workspace use my-research   # switch
lw workspace list
```

Switching vaults requires restarting your agent tool — the MCP server binds the current vault at launch (so an in-flight session can't silently flip mid-conversation).

## Update

```bash
lw upgrade --check    # exit 1 if newer release exists
lw upgrade            # apply
```

## Troubleshoot

```bash
lw doctor             # one-shot health check (config, integrations, MCP, version compat)
lw workspace current -v   # show full root-resolution chain
```

If your agent tool isn't picking up llm-wiki, reset that integration:

```bash
lw integrate claude-code --uninstall && lw integrate claude-code
```

Backups from MCP config writes land at `<config>.bak.<timestamp>` next to the original file. The uninstaller (`lw uninstall`) preserves your vault directories and saves `~/.llm-wiki/config.toml` to `~/.llm-wiki.config.toml.bak.<timestamp>` so reinstalls can restore vault registrations.

## Architecture

- `lw-core` — wiki I/O, search (Tantivy), lint
- `lw-mcp` — MCP server library
- `lw-cli` — `lw` binary (umbrella for workspace / integrate / upgrade / uninstall / doctor + the original wiki commands)
- `skills/llm-wiki-import/` — canonical agent skill
- `templates/` — starter vaults
- `integrations/` — TOML descriptors per agent tool

Tool is open source; wiki content repos are private (BYO).

## Status

- 1.0: Claude Code (full MCP + skills), Codex (skills only), OpenClaw (skills only)
- 1.1 roadmap: Codex auto-MCP wiring, OpenClaw auto-MCP wiring, `llm-wiki:curate` skill, more templates

## License

MIT.
