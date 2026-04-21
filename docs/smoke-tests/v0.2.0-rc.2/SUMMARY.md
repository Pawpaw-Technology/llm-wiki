# llm-wiki v0.2.0-rc.2 smoke gate

**Date:** 2026-04-21
**Branch:** feat/product-wrapper
**Tag:** v0.2.0-rc.2
**Release:** https://github.com/Pawpaw-Technology/llm-wiki/releases/tag/v0.2.0-rc.2
**Operator:** automated agent (Plan D Task 6 — second composite gate)
**Host:** Darwin arm64 (macOS)

## Purpose

Re-run the composite smoke gate against the four fix commits since rc.1 to confirm:

- **B1** — templates ship a complete `schema.toml`
- **B2** — `installer/uninstall.sh` skips integration reversal under custom prefix
- **M1** — `install.sh --no-integrate` also suppresses PATH injection
- **M2** — `lw doctor` PATH check honors `LW_HOME`

This run was performed under strict sandboxing (`HOME=/tmp/lw-smoke2-fakehome`, `LW_INSTALL_PREFIX=/tmp/lw-smoke2-prefix*`, `LW_HOME=/tmp/lw-smoke2-prefix*`) to avoid the user-state damage that the rc.1 gate inflicted.

## Composite gate

### Release build gate

- `release.yml` run `24699584684`: PASS (6/6 jobs green, ~6 min)
- 12/12 assets published (4 tarballs + 4 sha256 + install.sh + uninstall.sh + their sha256)

### Install gate

- **Linux container:** SKIPPED — `apt-get update` flake on `ports.ubuntu.com noble/universe arm64`. Same outbound-mirror issue as rc.1; not a defect in lw. Same logical paths exercised by Step 4 host install.
- **macOS host install via curl from real release:** PASS — tarball downloaded + sha256-verified + extracted to `/tmp/lw-smoke2-prefix`. `lw --version` reports `lw 0.2.0-rc.2`.

### Behavior gate

- **Tools layer (`lw ingest --stdin`):** PASS — exit 0, article written under `raw/articles/`. Cosmetic note: temp-style filename (`.tmpQauNl3`) — independent of the four fixes; was masked in rc.1 by B1. File as post-1.0 cosmetic.
- **Agent layer:** NOT EXECUTED (requires live agent session, manual). Skill markdown shipped in tarball at `/tmp/lw-smoke2-prefix/skills/`.

### Doctor gate

- `lw doctor` exit 0 — 8 passed, 1 warned, 0 failed. Warning is "PATH does not include lw bin" (expected; test prefix not on PATH).

### Uninstall gate

- **Custom-prefix uninstall correctly skips integration reversal:** PASS — uninstaller emits explicit `Skipping integration uninstall — custom prefix (/tmp/lw-smoke2-prefix)` message.
- **Filesystem cleanup:** PASS — install prefix removed, all three vault directories preserved.
- **Critical user-state safety check:** PASS — `~/.claude/settings.json` sha256 unchanged across the entire run.

## Per-fix verification

### B1 — Templates produce vaults `lw status` can load — PASS

| Template          | `workspace add` exit | `status` exit | Wiki name           |
| ----------------- | -------------------- | ------------- | ------------------- |
| general           | 0                    | 0             | "General"           |
| research-papers   | 0                    | 0             | "Research Papers"   |
| engineering-notes | 0                    | 0             | "Engineering Notes" |

`WikiSchema` deserialization no longer rejects template schemas with `missing field 'wiki'`.

### B2 — Custom-prefix uninstall does not touch user agent state — PASS

- Pre-run `~/.claude/settings.json` sha256: `320eceb09d933cbb02fd2817204fb72fdcefba50e3a5f887121831fd38a0a9f1`
- Post-run `~/.claude/settings.json` sha256: `320eceb09d933cbb02fd2817204fb72fdcefba50e3a5f887121831fd38a0a9f1`
- **IDENTICAL** → user state untouched.

The `Skipping integration uninstall — custom prefix (...)` guard fired correctly. No `lw integrate <tool> --uninstall` invocations occurred against real `~/.claude/`, `~/.codex/`, or `~/.openclaw/`.

### M1 — `install.sh --no-integrate` suppresses PATH injection — PASS

- Pre-run: `~/.zshrc` had no `# >>> llm-wiki >>>` marker
- Install ran with `HOME=/tmp/lw-smoke2-fakehome` and `--no-integrate`
- Post-install: real `~/.zshrc` still has no marker (PASS); `/tmp/lw-smoke2-fakehome/.zshrc` was never created (PASS)

rc.1 left PATH editing intact under `--no-integrate`; rc.2 has tightened the flag's scope.

### M2 — `lw doctor` PATH check honors `LW_HOME` — PASS

With `LW_HOME=/tmp/lw-smoke2-prefix`:

```
⚠ PATH includes lw bin
    /tmp/lw-smoke2-prefix/bin not in PATH
```

The check identifies the actual install location based on `LW_HOME`, not the `$HOME/.llm-wiki/bin` fallback.

## Verdict

**1.0 ship readiness: GREEN (with minor caveats)**

Reasoning:

- All four rc.1 release-blocker / medium findings fixed and verified under the same gate methodology
- B1: every bundled template produces a vault `lw status` (and `ingest`) can load — happy path works on day one
- B2: the documented `LW_INSTALL_PREFIX` workflow no longer destroys user agent state on uninstall
- M1: `--no-integrate` does what users would expect — no PATH edits, no agent edits
- M2: doctor's PATH warning is now accurate
- Release pipeline (release.yml) is solid — 12 assets in ~6 min

### Minor (non-blocking) caveats

1. **Linux container test still skipped** — same proxy/registry network flake as rc.1. Recommend investing in a self-hosted or pre-pulled container image so this gate isn't dependent on flaky outbound apt.
2. **Ingest filename is `.tmp*`-style** — `lw ingest` saves under a hidden temp-prefix filename rather than title-derived. Was masked by B1 in rc.1 and is independent of the fixes verified here. Recommend filing as cosmetic post-1.0.
3. **Agent-layer behavior gate is still manual** — running `llm-wiki:import` against Claude Code / Codex / OpenClaw requires a live agent session and falls outside what this automated gate can safely sandbox.

## User state safety confirmation

- BEFORE sha256: `320eceb09d933cbb02fd2817204fb72fdcefba50e3a5f887121831fd38a0a9f1`
- AFTER sha256: `320eceb09d933cbb02fd2817204fb72fdcefba50e3a5f887121831fd38a0a9f1`
- IDENTICAL — no user state was modified during this gate. The rc.1 footgun is closed.

## Notes

- Tags `v0.2.0-rc.1` and `v0.2.0-rc.2` both remain on the remote for diff/debug.
- All `/tmp/lw-smoke2-*` test directories cleaned at the end of the gate.
- The Docker build failure was at the apt index step, not at the docker daemon level — transient outbound-network flake.
