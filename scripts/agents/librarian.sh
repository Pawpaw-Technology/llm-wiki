#!/usr/bin/env bash
# Interactive Q&A agent for the wiki.
# Configure your LLM agent to use lw as MCP server:
#   { "mcpServers": { "wiki": { "command": "lw", "args": ["serve"] } } }
#
# Or run directly with Claude Code:
#   WIKI_ROOT=/path/to/wiki ./librarian.sh
set -euo pipefail
WIKI_ROOT="${WIKI_ROOT:?Set WIKI_ROOT to your wiki directory}"
cd "$WIKI_ROOT"

echo "=== Wiki Librarian ==="
echo "Wiki: $(lw status --format brief | head -1)"
echo "MCP:  lw serve --root $WIKI_ROOT"
echo ""
echo "Configure your agent with:"
echo '  { "mcpServers": { "wiki": { "command": "lw", "args": ["serve", "--root", "'$WIKI_ROOT'"] } } }'
