#!/usr/bin/env bash
# Classification Agent — moves _uncategorized pages to correct categories
#
# This script triggers an LLM agent (Claude Code, Codex, Kimi) to:
# 1. Browse _uncategorized pages via MCP
# 2. Read each page, decide category + tags
# 3. Write updated page to correct category
#
# Usage:
#   WIKI_ROOT=/path/to/wiki ./classifier.sh
#   WIKI_ROOT=/path/to/wiki LW_BATCH=10 ./classifier.sh
#
# Requires: lw binary in PATH, claude/codex/kimi CLI available
set -euo pipefail

WIKI_ROOT="${WIKI_ROOT:?Set WIKI_ROOT to your wiki directory}"
LW_BATCH="${LW_BATCH:-20}"  # pages per run
LW_AGENT="${LW_AGENT:-claude}"  # claude | codex | kimi

cd "$WIKI_ROOT"

# Check wiki exists
if [ ! -f ".lw/schema.toml" ]; then
  echo "Error: No wiki at $WIKI_ROOT" >&2
  exit 1
fi

# Count uncategorized
count=$(ls wiki/_uncategorized/*.md 2>/dev/null | wc -l | tr -d ' ')
if [ "$count" -eq 0 ]; then
  echo "No uncategorized pages. Done."
  exit 0
fi

echo "Found $count uncategorized pages. Classifying up to $LW_BATCH..."

# Read schema categories
categories=$(grep -A1 'categories' .lw/schema.toml | tail -1 | tr -d '[]"' | tr ',' '\n' | tr -d ' ')

PROMPT="You are a wiki classifier for a technical team wiki.

Your MCP server is 'wiki' — use wiki_browse, wiki_read, and wiki_write tools.

## Task

1. Call wiki_browse with category '_uncategorized' to get the list of uncategorized pages (limit to $LW_BATCH).

2. For each page:
   a. Call wiki_read to get the full content
   b. Decide the best category from: $categories
   c. Decide 1-5 relevant tags (free-form, lowercase, hyphenated)
   d. Decide the decay level: fast (news/pricing/releases), normal (analysis/methods), evergreen (fundamentals/theory)

3. For each classified page, call wiki_write with:
   - path: '{category}/{original-filename}'  (move from _uncategorized to correct category)
   - content: the original markdown but with updated frontmatter (add tags, update decay)

4. After writing the new location, the old _uncategorized file remains — that's OK, we'll clean up separately.

## Rules
- If you're unsure about category, keep it in _uncategorized — don't force a bad classification
- Tags should be lowercase, hyphenated (e.g., 'reinforcement-learning', not 'Reinforcement Learning')
- Prefer fewer accurate tags over many vague ones
- Don't modify the body text, only the frontmatter
- Work through pages one by one, don't batch wiki_write calls

## Categories
$(echo "$categories" | while read -r cat; do echo "- $cat"; done)

Start classifying."

case "$LW_AGENT" in
  claude)
    claude -p "$PROMPT" --allowedTools "mcp__wiki__wiki_browse,mcp__wiki__wiki_read,mcp__wiki__wiki_write"
    ;;
  codex)
    codex -p "$PROMPT"
    ;;
  kimi)
    kimi -p "$PROMPT"
    ;;
  *)
    echo "Unknown agent: $LW_AGENT. Supported: claude, codex, kimi" >&2
    exit 1
    ;;
esac

echo "Classification complete. Run 'lw status' to review."
