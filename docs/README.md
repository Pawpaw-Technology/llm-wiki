# llm-wiki docs

Architecture decisions, implementation plans, and verification artifacts for the llm-wiki product.

## Specs

- [2026-04-19 — Product wrapper design](specs/2026-04-19-product-wrapper-design.md) — turning lw from a Rust tool into an installable open-source product

## Plans

- [Plan A — Workspace foundation](plans/2026-04-20-wrapper-plan-a-workspace.md) — `lw workspace add/list/use/current/remove`, `~/.llm-wiki/config.toml`, 4-layer root resolution
- [Plan B — Skills, templates, integrations engine](plans/2026-04-20-wrapper-plan-b-skills-integrations.md) — `llm-wiki:import` skill, 3 starter templates, TOML descriptor adapter, atomic MCP merge
- [Plan C — Installer + upgrade + uninstall + CI](plans/2026-04-20-wrapper-plan-c-installer.md) — `install.sh`, `uninstall.sh`, `lw upgrade/uninstall`, release pipeline
- [Plan D — Multi-tool, doctor, 1.0 gate](plans/2026-04-20-wrapper-plan-d-doctor-1.0-gate.md) — Codex/OpenClaw descriptors, `lw doctor`, README rewrite, smoke gate

## Smoke tests

- [v0.2.0-rc.1](smoke-tests/v0.2.0-rc.1/SUMMARY.md) — first composite gate (found B1+B2+M1+M2)
- [v0.2.0-rc.2](smoke-tests/v0.2.0-rc.2/SUMMARY.md) — fixes verified (verdict: GREEN)

## Releases

- [v0.2.0](https://github.com/Pawpaw-Technology/llm-wiki/releases/tag/v0.2.0) — first product wrapper release
- [v0.2.1](https://github.com/Pawpaw-Technology/llm-wiki/releases/tag/v0.2.1) — `--help` examples updated for installer-first flow
