# LLM Wiki — Product Wrapper Design

**Date:** 2026-04-19 (revised 2026-04-20 after multi-reviewer pass)
**Status:** Approved (brainstorm + review)
**Scope:** Turn `llm-wiki` from a tool repo into a self-contained, installable open-source product. Repurpose the mono repo from install surface into dev workspace.

---

## 1. Problem

Today the `llm-wiki` ecosystem is assembled by humans: clone mono repo → init submodules → build Rust → wire MCP into Claude Code by hand → maintain a private wiki repo. There is no path for an outside open-source user to one-click install and start using it under their own agent tool (Claude Code, Codex, OpenClaw, …).

The mono repo also conflates two roles: (a) co-locating code for context engineering, (b) being the install surface. (b) is wrong — products should ship from a single self-contained repo, not from a git-submodule meta-layout. (a) is still useful and stays.

## 1.5 Why this, not Obsidian + filesystem-MCP

A markdown folder + filesystem MCP gives an agent generic file ops. `llm-wiki` adds:

- **Scope-gated ingest** — the import flow is opinionated: fetch, parse, scope-check, route to `raw/` vs wiki page; never silent-fails on out-of-scope.
- **Wiki-aware operations** — `wiki_query`, `wiki_lint`, `wiki_browse`, `wiki_tags` understand the wiki's structure (categories, tags, freshness from git, orphans, broken links). A filesystem MCP doesn't.
- **Curated skills** — packaged prompts that turn the import / curate workflows into one-shot agent actions instead of N-step manual prompts.
- **Single curl install** — no Obsidian app, no plugin marketplace, runs anywhere a CLI runs (servers, headless dev boxes).

Obsidian + fs-MCP can read your notes; `llm-wiki` is built to _grow_ and _maintain_ a knowledge base under agent guidance.

## 2. Goals

- One-line `curl | sh` install for any macOS / Linux user.
- llm-wiki becomes a **product umbrella** (like Claude Code): a single user-facing thing built from layered crates, distributed as one binary + skills.
- Multi-vault, Obsidian-style: users register multiple wiki workspaces and switch between them. Single-vault users pay nothing for this — workspace registry is opt-in; absent registration falls back to cwd auto-discover.
- Cross-agent-tool: 1.0 ships with Claude Code, Codex, OpenClaw integrations.
- BYO data: no bundled wiki content; users point at their own repo / local dir, or start from a template.
- Updates flow from upstream via `lw upgrade`.

**Primary user (1.0):** engineers and researchers who already drive their day with an agent CLI and want a managed wiki layer with opinionated import + lint, not just a markdown folder.

**Leading indicators (post-launch tracking, no telemetry):**

- GitHub release download counts per version
- GitHub stars (weak adoption proxy)
- Manual install survey via README link (opt-in form)

No phone-home metrics. OSS users get to stay anonymous.

## 3. Non-goals

- No built-in agent loop (`lw chat`). The user's existing agent tool drives interaction via MCP.
- No bundled batch/cron orchestration in the product. The TS `agents/` repo and `codebridge` stay separate, consume `lw` via CLI, and are advanced/team-scope.
- No new repo for the installer. Everything lives in `llm-wiki`.
- Windows is out of scope for 1.0 (revisit in 1.x).
- No telemetry / phone-home in any form.

## 4. Architecture

```
┌─────────────────────────────────────────────────┐
│  llm-wiki  (umbrella product repo)              │
│  - installer (curl install.sh)                   │
│  - skills/ (canonical, agent-tool agnostic)     │
│  - templates/ (starter vaults)                  │
│  - lw binary (workspace, upgrade, integrate +    │
│    existing query/ingest/lint/serve commands)    │
│  Install targets:                                │
│    ~/.llm-wiki/   (binary, skills, templates,   │
│                     config, version)             │
│    integrations write into each agent tool's     │
│    config dir (~/.claude, ~/.codex, ...)         │
└─────────────────────────────────────────────────┘
         │  cargo workspace
         ├── lw-core    (wiki I/O, search, lint)
         ├── lw-mcp     (MCP server lib)
         └── lw-cli     (binary `lw` — umbrella)

External, unchanged:
  llm-wiki-agents (TS, batch/cron) → consumes lw CLI
  codebridge (LLM dispatch)         → dep of agents
  llm-wiki-mono                     → kept as dev workspace
                                      (context engineering),
                                      no longer install surface
```

## 4.1 Runtime model (added per Arch review §1)

`lw serve` MCP processes are spawned by an agent tool and live for one session. The vault they operate on is **fixed at process launch**, resolved via:

**`--root` flag > `LW_WIKI_ROOT` env > current workspace from config > cwd auto-discover**

`lw workspace use <name>` mutates `~/.llm-wiki/config.toml` but **does not affect already-running MCP processes**. To pick up a new current vault, the user restarts their agent tool (which respawns `lw serve`).

`lw doctor` flags any running `lw serve` whose `--root` differs from the current registered workspace, with a one-line "restart your agent" hint. This avoids the silent-mid-session-switch footgun.

## 5. Repo evolution

| Repo                      | Today                                 | After                                                                                              |
| ------------------------- | ------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `llm-wiki-mono`           | submodule meta-repo + install surface | **kept as dev workspace** (context engineering); README redirects installs to `llm-wiki` curl line |
| `llm-wiki`                | tool only                             | product umbrella (binary + installer + skills + templates)                                         |
| `llm-wiki-agents`         | submodule                             | unchanged, independent                                                                             |
| `codebridge`              | submodule                             | unchanged, independent                                                                             |
| private wiki content repo | submodule of mono                     | unchanged; mono's pointer stays put. Irrelevant to product.                                        |

## 6. Repo layout (after)

```
llm-wiki/
├── Cargo.toml                # workspace
├── crates/
│   ├── lw-core/
│   ├── lw-mcp/
│   └── lw-cli/
│       └── src/cmd/
│           ├── workspace.rs  # add | list | use | current | remove
│           ├── upgrade.rs
│           ├── integrate.rs  # loads integrations/*.toml + per-tool logic
│           └── doctor.rs
├── installer/
│   ├── install.sh            # ships as release asset (versioned, sha256)
│   └── uninstall.sh
├── skills/                   # canonical, markdown + frontmatter
│   └── llm-wiki:import/      # 1.0 ships import only
├── templates/                # starter vaults for `lw workspace add --template`
│   ├── general/
│   ├── research-papers/
│   └── engineering-notes/
├── integrations/             # declarative descriptors per tool
│   ├── claude-code.toml
│   ├── codex.toml
│   └── openclaw.toml
├── README.md                 # includes onboarding/user journey
└── docs/
```

## 7. Install flow

Versioned, pinned to a release asset (per Sec review §1):

```bash
curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/latest/download/install.sh | sh
```

The install script itself is a release artifact with a published sha256, not a live `main`-branch raw URL. Users wanting reproducible installs can pin a tag:

```bash
curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/download/v0.2.0/install.sh | sh
```

Steps:

1. Detect OS/arch → resolve release tarball name (matches existing release.yml matrix: `lw-{x86_64,aarch64}-{linux,darwin}`). Bail with clear message on unsupported targets (Windows).
2. Fetch latest GitHub release tarball + sha256, verify, extract `lw` → `~/.llm-wiki/bin/lw`.
3. Fetch `skills/` and `templates/` for the **same release tag** → `~/.llm-wiki/{skills,templates}/`. Versions are pinned together (per PM review §9).
4. Write PATH export to detected shell rc (`~/.zshrc`, `~/.bashrc`, `~/.config/fish/config.fish`); idempotent, guarded by markers:
   ```
   # >>> llm-wiki >>>
   export PATH="$HOME/.llm-wiki/bin:$PATH"
   # <<< llm-wiki <<<
   ```
   Uninstall matches these markers exactly.
5. **Non-interactive default** (per Sec review §2): if stdout is not a TTY (CI, scripted install), the installer **skips integration prompts**. Integration is opt-in via `--yes` (auto-integrate all detected tools) or by running `lw integrate --auto` later. If TTY and tools detected, print one line: "Detected: Claude Code. Run `lw integrate claude-code` to wire it up."
6. Print next steps (TTY only, otherwise quiet success):
   ```
   lw workspace add my-vault ~/path/to/wiki --template general
   lw integrate --auto
   ```
7. Write `~/.llm-wiki/version` (records both binary version and skills/templates version, must match).

Failures: any step that can't recover prints a remediation hint and exits non-zero. No silent partial install.

Flags:

- `--yes` / `-y` — auto-integrate detected tools
- `--no-integrate` — install only, never prompt
- `--prefix <dir>` — override `~/.llm-wiki`

## 8. Workspace registry

`~/.llm-wiki/config.toml`:

```toml
[workspace]
current = "personal"

[workspaces.personal]
path = "/Users/me/Documents/MyWiki"

[workspaces.work]
path = "/Users/me/work/team-wiki"
```

Commands:

- `lw workspace add <name> <path> [--init] [--template <name>]` — register; `--init` runs `lw init` if dir is empty; `--template` copies starter vault from `~/.llm-wiki/templates/<name>` (includes a `SCOPE.md`).
- `lw workspace list`
- `lw workspace use <name>` — set `current`. **Note:** does not affect running `lw serve` processes; restart agent tool to pick up.
- `lw workspace current` — print current name + path.
- `lw workspace current -v` — print full resolution chain so user can debug "which layer wins".
- `lw workspace remove <name>` — unregister (does not touch the directory).

**Single-vault fallback:** if no workspace is ever registered, all commands fall back to `cwd` auto-discover (existing behavior). Multi-vault is pure opt-in.

Resolution priority (when `lw serve` or any command needs a root):
**`--root` flag > `LW_WIKI_ROOT` env > current registered workspace > cwd auto-discover**

## 9. Integrations (cross agent-tool)

**Declarative TOML descriptors** (per Arch review §3) — one per tool, plus a Rust trait for the rare case needing logic. Adding a new tool = ship a new TOML, no recompile.

`integrations/claude-code.toml`:

```toml
name = "Claude Code"
detect.config_dir = "~/.claude"

[mcp]
config_path = "~/.claude/settings.json"
format = "json"
key_path = "mcpServers.llm-wiki"
command = "lw"
args = ["serve"]

[skills]
target_dir = "~/.claude/skills/llm-wiki/"
mode = "symlink"
```

1.0 coverage:

| Tool        | MCP registration                       | Skills target                          | Notes                                                                                                                   |
| ----------- | -------------------------------------- | -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Claude Code | `~/.claude/settings.json` (mcpServers) | `~/.claude/skills/llm-wiki/` (symlink) | First-class                                                                                                             |
| Codex       | `~/.codex/config.toml` (mcp_servers)   | per Codex skill convention             | First-class                                                                                                             |
| OpenClaw    | (deferred to 1.1 unless trivial)       | OpenClaw skills dir (symlink)          | **Skill-only path for 1.0** — user judgment: skill install is the bulk of value; MCP wiring lands in 1.1 if non-trivial |

Commands:

- `lw integrate --auto` — detect installed tools, prompt per tool (or `--yes` to skip prompts).
- `lw integrate <tool>` — explicit.
- `lw integrate <tool> --uninstall` — reverse (matches version marker, leaves user's other config alone).

**MCP config write semantics** (per Arch review §4):

- **Atomic write**: stage to temp file, fsync, rename.
- **Backup**: write `<config>.bak.<unix-ts>` before mutating.
- **Merge by key with version field**: each managed entry has a `_lw_version` marker. On upgrade, if the existing entry's version matches the previous shipped version, merge silently; if user-edited (version mismatch or unexpected fields), print a unified diff and prompt before writing.
- **Never** overwrite the whole config file.

Skills are markdown prompts. Tools with a skill concept (Claude Code) get them symlinked into their skills dir. Tools without a native skill concept (potentially Codex/OpenClaw depending on their conventions) get a one-line prompt-include reference pointing at `~/.llm-wiki/skills/`. The "translation" is mechanical, not semantic — same prompt text, different host.

`lw upgrade` re-resolves links so updates propagate. Copy-mode (filesystems without symlinks) explicitly re-copies on upgrade — see §11.

## 10. Skills (canonical, 1 in 1.0)

Onboarding lives in **README**, not in a skill. Per Arch review minor: defer `llm-wiki:curate` to 1.1, ship import only in 1.0.

- **`llm-wiki:import`** (prefixed per PM review minor — avoids collisions with other plugins) — triggered when user shares a URL / pasted text / file path. The skill prompt instructs the agent to:
  1. Fetch raw content if needed.
  2. Check against the workspace's scope (read from `SCOPE.md` in vault root). **If `SCOPE.md` is absent, skip scope check** — permissive default.
  3. If in scope (or no scope defined) → ingest to `raw/` via `wiki_ingest`.
  4. If ambiguous or out-of-scope → ask user explicitly. Never silent-fail.

Skill is a markdown file with frontmatter (name, description, when-to-use). Pure prompt text + references to `wiki_*` MCP tools. No tool-specific syntax.

## 10.5 SCOPE.md contract

Loose, conventional structure (per Arch review §6 + PM review §7). Templates ship with this:

```markdown
# Scope

## Purpose

One paragraph: what this wiki is for.

## Includes

- bullet
- bullet

## Excludes

- bullet
- bullet
```

The import skill reads this as guidance (not strict validation — the LLM judges fit). Default-when-absent is **permissive** (no scope check applied). Templates always ship a starter `SCOPE.md` so most users get scope-checking by default.

## 11. Upgrade

- `lw upgrade --check` — query GitHub releases API, compare `~/.llm-wiki/version`. Exit 0 if current, exit 1 + prints latest if newer available.
- `lw upgrade` — runs install.sh's reinstall path: fetch latest binary + skills + templates (versions pinned together), replace, refresh integrations (re-merge MCP entries with new version marker, re-symlink or re-copy skills). Workspaces and config preserved.
- **Skill ↔ binary version** (per PM review §9): both written into `~/.llm-wiki/version`; `lw doctor` enforces compat. Mismatch surfaces as a hard warning.
- **Copy-mode upgrade** (per Arch review §7): when integration uses `mode = "copy"` (filesystems without symlinks), upgrade explicitly re-copies skills directory atomically. Documented behavior, not silent staleness.
- **Optional weak nudge**: on `lw` startup, if cached version check (24h TTL) sees newer, print one stderr line. **Suppressed when** `!isatty(stderr)` OR `CI=true` OR `LW_NO_NUDGE=1` (per PM minor).

## 11.5 Uninstall

`uninstall.sh` (shipped alongside `install.sh` as a release asset) and `lw uninstall` (CLI shortcut) do the same thing:

1. **Reverse all integrations** — for each tool with an entry whose `_lw_version` marker is present:
   - Remove the managed MCP entry from the tool's config (atomic write + `.bak.<unix-ts>` first).
   - Remove the symlinked / copied skills directory at the integration's target path.
   - User-edited entries (version marker missing or mismatched) are left in place with a printed warning — never silently discard user changes.
2. **Remove PATH injection** — strip the marked block (`# >>> llm-wiki >>>` … `# <<< llm-wiki <<<`) from each shell rc; idempotent.
3. **Remove `~/.llm-wiki/`** — binary, skills, templates, version file, integration descriptors. **Default: prompt for confirmation** (TTY) or require `--yes` (non-TTY).
4. **Preserve user data** — registered workspace directories are **never** touched. `config.toml` is moved to `~/.llm-wiki.config.toml.bak.<unix-ts>` so a re-install can restore vault registrations.
5. Print summary: what was removed, what was preserved, where backups live.

Flags:

- `--yes` / `-y` — skip confirmation
- `--keep-config` — preserve `~/.llm-wiki/config.toml` in place (skip the bak rename)
- `--purge` — also delete the `.bak` files left by integration writes (off by default — backups are insurance)

`lw doctor --uninstall-check` previews what uninstall would do without touching anything.

## 12. `lw doctor`

One-shot diagnostics:

- `lw` binary path + version
- PATH actually contains `~/.llm-wiki/bin`
- `~/.llm-wiki/config.toml` parseable; current workspace exists on disk
- **Skills version vs binary version compat** (per PM §9)
- For each known agent tool: detected? MCP entry present and version matches shipped descriptor? Skill symlink target exists and not dangling?
- **Stale MCP entry** (per Arch minor): MCP entry's `command` resolves to the current `lw` binary (catches old paths after upgrade)
- **Running `lw serve` mismatch** (per §4.1): any running `lw serve` whose `--root` differs from the current registered workspace
- `lw serve` smoke test (start, list tools, shut down)

Output: human-readable checklist with ✓/✗ and remediation hint per failing check.

## 13. README user journey

README sections, in order:

1. **What it is** — one paragraph. Includes the §1.5 positioning vs Obsidian+fs-MCP in one sentence.
2. **Install** — one curl line (versioned).
3. **First vault, with a template** — the recommended path:
   ```
   lw workspace add my-research ~/wiki/research --template research-papers
   lw integrate --auto
   ```
   Template copies a working `SCOPE.md` and a few example pages so the agent has something to query immediately.
4. **Daily use** — example agent conversation showing the import skill: paste a URL → agent fetches, scope-checks, ingests.
5. **Update** — `lw upgrade`.
6. **Troubleshoot** — `lw doctor`, common issues, where backup files live.
7. **Multi-vault (optional)** — `lw workspace add` more, switch with `lw workspace use`, restart your agent.

## 14. Migration / rollout

Order per Arch review §5 (verify-then-redirect, never strand users):

1. In `llm-wiki` repo: add `installer/`, `skills/`, `templates/`, `integrations/`, new `lw-cli` cmd modules. CI release asset publishes `install.sh` (with sha256) alongside binaries.
2. **Decide skill packaging before 0.2.0** (per Arch minor): bundled in main release tarball is the default; revisit only if size becomes a problem.
3. Cut `0.2.0` release with the wrapper commands working but installer experimental. README marked "preview".
4. **Installation smoke test**: clean macOS + Linux VMs, install via curl, run `lw doctor` against all three agent tools (Claude Code, Codex, OpenClaw skill-only). Must pass clean. This validates _installation correctness_.
5. **Skill behavior smoke test** (per Arch review §2 — installation green ≠ skill works): for each of the three tools, in a fresh agent session against a `--template general` vault, walk the `llm-wiki:import` skill end-to-end:
   - Paste a sample URL → agent must invoke the skill, fetch, scope-check, and write to `raw/`.
   - Paste an out-of-scope item → agent must ask the user, not silent-import.
   - Tools where the skill prompt cannot be triggered faithfully (no skill concept, no prompt-include path) downgrade to "documented limitation" in 1.0 and the cross-tool claim in §2 is qualified accordingly.
   - Capture a transcript per tool; commit under `docs/smoke-tests/<version>/`.
6. **Uninstall smoke test**: run `lw uninstall --yes` on the same VMs; verify integrations cleanly removed, vault dirs untouched, backups present per §11.5.
7. Update `llm-wiki-agents` README to point at new install method.
8. Cut `1.0.0` once steps 4–6 are green across all targets.
9. **After** 1.0 is shipping: in `llm-wiki-mono` README, replace install instructions with a redirect to the curl line. **Do not archive mono** — it remains the dev workspace for context engineering. Removing the install-surface claim is a single README edit, not a repo retirement.

1.0 gate (composite):

- (install) `lw doctor` green on Claude Code + Codex + OpenClaw on clean macOS + Linux VMs
- (behavior) `llm-wiki:import` end-to-end transcript captured per tool, with the in-scope / out-of-scope cases both behaving correctly
- (uninstall) `lw uninstall` reverses all integrations cleanly, leaves vault data untouched

## 15. Deferred (post-1.0)

- **Distribution channels** (per Arch review §8): Homebrew tap (macOS table-stakes), `cargo install lw-cli`, signed releases via cosign or minisign.
- **Windows support**.
- **`llm-wiki:curate` skill** — wraps lint/classify/orphan with proposal UX. 1.1 target.
- **More templates** beyond the 3 starters.
- **OpenClaw MCP integration** if it turns out non-trivial in 1.0 testing.
- **Fourth+ agent tool integrations** (OpenCode, others) via TOML descriptor — community PR friendly thanks to §9.
- **Project rename** — "llm-wiki" is generic; SEO crowded. Worth a marketing-led naming pass before broad announcement.
- **More expressive SCOPE.md** — globs, structured rules, multi-scope per directory.
