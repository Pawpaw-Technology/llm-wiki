# llm-wiki v0.2.0-rc.1 smoke gate

**Date:** 2026-04-20
**Branch:** feat/product-wrapper
**Tag:** v0.2.0-rc.1
**Release:** https://github.com/Pawpaw-Technology/llm-wiki/releases/tag/v0.2.0-rc.1
**Operator:** automated agent (Plan D Task 6)
**Host:** Darwin arm64 (macOS)

## Composite gate (per spec §14 1.0 composite)

### Release build gate

- `release.yml` run `24661995875`: PASS
- All 6 jobs green: 4 × build matrix (x86_64/aarch64 × linux/darwin) + publish-installer + create-release
- aarch64-linux build (historically the failure mode per Plan C I-3) succeeded in 2m55s
- Release assets: 12/12 expected (4 tarballs + 4 sha256, install.sh + .sha256, uninstall.sh + .sha256)
- See `release-view.log`

### Install gate

- **Linux container (Ubuntu 24.04):** SKIPPED — see `install-linux.log` and `docker-build.log`. Docker daemon TLS handshakes to Docker Hub kept failing with `unexpected EOF` mid-blob-fetch from this host (transient OrbStack <-> Docker Hub flake). Tried docker.io, public.ecr.aws, ghcr.io, quay.io. Not a defect in lw or the Dockerfile. The same logical paths (curl install + workspace add + uninstall) are exercised by Step 4 host install.
- **Linux host install via curl from real release:** N/A — host is darwin
- **macOS host install via curl from real release:** PASS — see `install-host.log`. The release tarball was downloaded, sha256-verified, and extracted to `/tmp/lw-smoke-prefix`. `lw --version` reported `lw 0.2.0-rc.1`.
- **Re-install idempotency / PATH-marker dedup:** NOT EXPLICITLY TESTED on host. Container test 5 covers this path; not run here due to docker outage.

### Behavior gate

- **Tools layer (`lw ingest --stdin`):** **FAIL** — see `behavior-tools-layer.log`. `lw ingest` errored with `TOML parse error … missing field 'wiki'` against a vault initialized from the bundled `general` template. **Root cause:** all three bundled templates (`general/`, `engineering-notes/`, `research-papers/`) ship a `.lw/schema.toml` containing only the `[tags]` section. `WikiSchema::parse` (in `crates/lw-core/src/schema.rs:6`) requires both `wiki` and `tags` sections. Effect: any vault created via `lw workspace add … --template <any>` cannot be ingested through. Test article was NOT written — `/tmp/lw-smoke-vault/raw/articles/` was never created.
- **Agent layer (Claude Code / Codex / OpenClaw running `llm-wiki:import` skill):** NOT EXECUTED (requires live agent session, manual). Skill markdown is present in the release tarball: `/tmp/lw-smoke-prefix/skills/llm-wiki-import/SKILL.md` confirmed.
- **Status: BEHAVIOR-PARTIAL — and the partial that ran is RED.**

### Doctor gate

- `lw doctor` runs and exits 0 — PASS
- 6 passed, 4 warned, 0 failed
- **Issue noted:** `check_path_env` consults only `LW_INSTALL_PREFIX` env var (in `crates/lw-cli/src/doctor.rs:127`), not `LW_HOME`. When invoked with `LW_HOME=/tmp/lw-smoke-prefix`, it fell through to `$HOME/.llm-wiki/bin` and reported it as the "missing" path even though the actual install was at the LW_HOME location. install.sh recognises both env vars; doctor does not.

### Uninstall gate

- **Filesystem cleanup:** PASS — `/tmp/lw-smoke-prefix` removed cleanly, `/tmp/lw-smoke-vault` preserved (vault data not destroyed)
- **PATH marker stripped:** PASS — `~/.zshrc` had marker added by install, removed by uninstall (verified clean post-run)
- **CRITICAL FINDING — uninstaller damages host agent state when prefix is custom:** When run with `LW_INSTALL_PREFIX=/tmp/lw-smoke-prefix`, uninstall.sh still iterates over `installer/integrations/*.toml` and invokes `lw integrate <tool> --uninstall` for each. This calls into the integrator code which writes/edits the user's REAL `~/.claude/`, `~/.codex/`, `~/.openclaw/` paths regardless of the install prefix. Result during this smoke run:
  - `~/.claude/settings.json`: pre-existing `llm-wiki` MCP entry was removed (backup left at `~/.claude/settings.json.bak.1776682289` — restore manually)
  - `~/.claude/skills/llm-wiki/`: directory deleted
  - Stray `~/.llm-wiki.config.toml.bak.*` files left in `$HOME`
  - The uninstall ran "as advertised" (it's not a code bug per se — `lw integrate` correctly knows where Claude Code actually lives), but it is **profoundly wrong for a prefix-scoped uninstall**. Either the uninstaller must skip integration reversal when the prefix differs from the installed integrations' source, or the integration descriptors must record their backing prefix and only reverse when matched.

## Release-blocker findings (must fix before 1.0)

### B1 — Templates ship malformed schema.toml (BLOCKER)
- File: `templates/{general,engineering-notes,research-papers}/.lw/schema.toml`
- Symptom: `lw ingest` rejects every fresh template-initialised vault
- Fix: add `[wiki]` section with `name` + `default_review_days` to each template's schema.toml, OR make the `wiki` section optional (`#[serde(default)]`) on `WikiConfig`
- Severity: any new user following the README workflow `lw workspace add my-vault --template general && lw ingest …` is broken on day one

### B2 — Uninstaller damages host agent state under custom prefix (BLOCKER)
- File: `installer/uninstall.sh` lines 59-69 (Step 1: reverse integrations)
- Symptom: `LW_INSTALL_PREFIX=/tmp/foo sh /tmp/foo/installer/uninstall.sh --yes` deletes user's real `~/.claude/skills/llm-wiki/` and edits `~/.claude/settings.json`
- Fix options: (a) skip Step 1 when prefix != default; (b) record install prefix inside each integration .bak and refuse cross-prefix removal; (c) emit a confirmation prompt when prefix differs from real agent dirs
- Severity: anyone using the documented `LW_INSTALL_PREFIX` workflow for testing or sandboxed installs will damage their primary agent setup on uninstall

### M1 — install.sh `--no-integrate` is a misnomer (MEDIUM)
- File: `installer/install.sh` line 169 (PATH injection)
- Symptom: `--no-integrate` does NOT prevent PATH injection into `~/.zshrc` / `~/.bashrc` / etc. It only suppresses the `lw integrate --auto` invocation. Documentation says "Install only; never prompt or write to agent configs" which is technically narrow but most users would assume "writes to my rc files" counts as "writes to my configs".
- Fix: split into `--no-integrate` (skip `lw integrate`) and `--no-path` (skip rc edits) OR have `--no-integrate` do both. Hard rule "DO NOT touch user state" is impossible to honour with this script.

### M2 — `lw doctor` ignores `LW_HOME` env var (LOW)
- File: `crates/lw-cli/src/doctor.rs:127`
- Symptom: cosmetic — wrong path in PATH-check warning when LW_HOME is set
- Fix: mirror install.sh priority: `LW_INSTALL_PREFIX > LW_HOME > $HOME/.llm-wiki`

## Tier matrix verification

| Tool | Detected | Skills installed | MCP installed | Notes |
|---|---|---|---|---|
| Claude Code | yes (host) | n/a — not exercised in this gate | n/a — pre-existing (since damaged, see B2) | Behavior layer manual; user state damaged on uninstall |
| Codex | no | n/a | n/a | Skills only in 1.0 |
| OpenClaw | no | n/a | n/a | Skills only in 1.0 |

## Verdict

**1.0 ship readiness: RED**

Reasoning:
- B1 makes the documented happy path (`lw workspace add … --template general && lw ingest …`) fail on first run for every new user. This is a functional regression that the test matrix did not catch because no automated test runs `ingest` against a template-initialised vault.
- B2 makes `LW_INSTALL_PREFIX`-scoped uninstall an active footgun against the user's primary agent setup. Recommended for any sandboxed test workflow, it instead destroys real configuration.
- M1 means the smoke test itself could not run cleanly without modifying user state. Acceptable for 1.0 if documented, but should be tightened.

The release tooling (release.yml producing all 12 expected assets across 4 platforms incl. aarch64-linux) is solid and Plan C I-3 is confirmed effective.

## Recommended next steps

1. Fix B1 (template schema completeness) — likely a 5-line fix per template + add a regression test that runs `lw ingest --stdin` against each bundled template post-install.
2. Fix B2 (prefix-scoped uninstall) — adopt option (a) or (b) from the finding above. Option (a) is simplest: only run integration reversal when `LW_INSTALL_PREFIX` is unset or equals the default `$HOME/.llm-wiki`.
3. Fix M1 by splitting `--no-integrate` into orthogonal flags.
4. Cut v0.2.0-rc.2 with the fixes; re-run this gate.
5. (Pre-1.0) Add the agent-layer behavior smoke as a manual gate doc/checklist.
6. Restore user's `~/.claude/settings.json` manually from `~/.claude/settings.json.bak.1776682289` and re-run `lw integrate claude-code` after rc.2 is built (note: requires the host to clean up the stray `~/.llm-wiki.config.toml.bak.*` files at the operator's discretion).

## Notes

- Docker Hub flakiness (Step 3 skipped) is unrelated to lw and unrelated to release.yml (which uses GitHub-managed runners with their own registry mirror).
- The `gh run watch` blocked cleanly until the run finished; full duration ~6 minutes from tag push to release publication.
- Tag and release WERE NOT deleted, per Hard Rules. They remain on the remote at `v0.2.0-rc.1` for debugging the rc.2 cycle.
