#!/usr/bin/env bash
# Weekly wiki freshness check. Triggered by cron.
# Usage: crontab -e → 0 9 * * 1 /path/to/weekly-lint.sh
#
# Runs Claude Code to triage stale pages:
# - Reads each stale page and its sources
# - Updates content or marks for human review
# - Commits changes with descriptive messages
set -euo pipefail
WIKI_ROOT="${WIKI_ROOT:?Set WIKI_ROOT to your wiki directory}"
cd "$WIKI_ROOT"

echo "Running weekly lint..."
lw status --format brief

claude -p "Run 'lw serve' as MCP server. Use wiki_lint to find stale pages. \
For each STALE page, read it with wiki_read and decide: \
1) Update the content using wiki_write if you can improve it \
2) Skip if it needs human expertise \
Commit changes with descriptive messages after each update."
