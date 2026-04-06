---
name: cli-for-agents
description: >-
  Designs or reviews CLIs so coding agents can run them reliably: non-interactive
  flags, layered --help with examples, stdin/pipelines, fast actionable errors,
  idempotency, dry-run, and predictable structure. Use when building a CLI,
  adding commands, writing --help, or when the user mentions agents, terminals,
  or automation-friendly CLIs.
---

# CLI for agents

Human-oriented CLIs often block agents: interactive prompts, huge upfront docs, and help text without copy-pasteable examples. Prefer patterns that work headlessly and compose in pipelines.

## Non-interactive first

- Every input should be expressible as a flag or flag value. Do not require arrow keys, menus, or timed prompts.
- If flags are missing, **then** fall back to interactive mode—not the other way around.

**Bad:** `mycli deploy` → `? Which environment? (use arrow keys)`
**Good:** `mycli deploy --env staging`

## Discoverability without dumping context

- Agents discover subcommands incrementally: `mycli`, then `mycli deploy --help`. Do not print the entire manual on every run.
- Let each subcommand own its documentation so unused commands stay out of context.

## `--help` that works

- Every subcommand has `--help`.
- Every `--help` includes **Examples** with real invocations. Examples do more than prose for pattern-matching.

```text
Options:
  --env     Target environment (staging, production)
  --tag     Image tag (default: latest)
  --force   Skip confirmation

Examples:
  mycli deploy --env staging
  mycli deploy --env production --tag v1.2.3
  mycli deploy --env staging --force
```

## stdin, flags, and pipelines

- Accept stdin where it makes sense (e.g. `cat config.json | mycli config import --stdin`).
- Avoid odd positional ordering and avoid falling back to interactive prompts for missing values.
- Support chaining: `mycli deploy --env staging --tag $(mycli build --output tag-only)`.

## Fail fast with actionable errors

- On missing required flags: exit immediately with a clear message and a **correct example invocation**, not a hang.

```text
Error: No image tag specified.
  mycli deploy --env staging --tag <image-tag>
  Available tags: mycli build list --output tags
```

## Idempotency

- Agents retry often. The same successful command run twice should be safe (no-op or explicit "already done"), not duplicate side effects.

## Destructive actions

- Add `--dry-run` (or equivalent) so agents can preview plans before committing.
- Offer `--yes` / `--force` to skip confirmations while keeping the safe default for humans.

## Predictable structure

- Use a consistent pattern everywhere, e.g. `resource` + `verb`: if `mycli service list` exists, `mycli deploy list` and `mycli config list` should follow the same shape.

## Success output

- On success, return machine-useful data: IDs, URLs, durations. Plain text is fine; avoid relying on decorative output alone.

```text
deployed v1.2.3 to staging
url: https://staging.myapp.com
deploy_id: dep_abc123
duration: 34s
```

## When reviewing an existing CLI

- Check: non-interactive path, layered help, examples on `--help`, stdin/pipeline story, error messages with invocations, idempotency, dry-run, confirmation bypass flags, consistent command structure, structured success output.

## Applying to `lw` CLI

Concrete rules for this project:

### Command structure: `lw <verb> [args] [flags]`

```
lw init [--root <path>]
lw query <text> [--tag X] [--category X] [--stale] [--format json|human|brief] [--limit N]
lw ingest <source> [--title X] [--category X] [--raw-type X] [--yes]
lw serve [--root <path>]
```

### Non-interactive mode

- `lw ingest --yes` skips all Y/n prompts (agent mode)
- `lw ingest` without `--yes` falls back to interactive prompts (human mode)
- All flags that replace interactive input: `--title`, `--category`, `--tags`

### Output contract

- `--format json` always produces valid JSON to stdout, nothing else
- `--format human` is the default, for terminal display
- `--format brief` is one-line-per-result, for piping
- Errors go to stderr, never stdout
- Exit code 0 = success, 1 = error, 2 = no results

### Piping

```bash
# Ingest from stdin
cat notes.md | lw ingest --stdin --title "Meeting Notes" --category ops --yes

# Chain: search → read
lw query "attention" --format brief | head -1 | cut -f1 | xargs lw read

# JSON processing
lw query "transformer" --format json | jq '.results[].path'
```

### Help examples on every subcommand

```
$ lw query --help

Search wiki pages

Usage: lw query <TEXT> [OPTIONS]

Arguments:
  <TEXT>  Search text

Options:
  --tag <TAG>          Filter by tag (repeatable)
  --category <CAT>     Filter by category
  --stale              Only show stale/suspect pages
  -f, --format <FMT>   Output format [default: human] [possible values: json, human, brief]
  -l, --limit <N>      Max results [default: 20]

Examples:
  lw query "attention mechanism"
  lw query "transformer" --tag architecture --format json
  lw query "" --tag training --category training
  lw query "gpu" --stale --format brief
```

### Error messages with fix hints

```
Error: Not a wiki directory: /tmp/foo (missing .lw/schema.toml)
  Run: lw init --root /tmp/foo

Error: Category 'ml' not found in schema.
  Available: architecture, training, infra, tools, product, ops
  Run: lw query --help

Error: Source file not found: paper.pdf
  Usage: lw ingest <path-to-file> [--raw-type papers]
```
