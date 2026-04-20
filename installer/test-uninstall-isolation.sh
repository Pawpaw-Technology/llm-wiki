#!/bin/sh
# Isolation test for the v0.2.0-rc.1 B2 blocker.
#
# Scenario: a user installs llm-wiki under a custom $LW_INSTALL_PREFIX
# (common for sandboxed / CI installs) and later runs uninstall.sh.
# The uninstaller must NOT reach out to the user's real agent-tool
# config dirs (~/.claude, ~/.codex, ~/.openclaw), because install.sh
# under a custom prefix never wrote to them in the first place.
#
# The test fakes a $HOME under /tmp with prepared agent configs, runs
# install then uninstall against a custom prefix, and asserts that the
# fake agent configs are byte-identical after uninstall.
#
# Usage:
#   LW_VERSION=v0.2.0-rc.1 sh installer/test-uninstall-isolation.sh
#
# Requires network access to fetch the release (same as test-install.sh).
# Run in a throwaway container or VM — this script rewrites $HOME.

set -eu

LW_VERSION="${LW_VERSION:-latest}"
LW_REPO="${LW_REPO:-Pawpaw-Technology/llm-wiki}"

CANARY='{"mcpServers":{"user-entry":{"command":"user","args":["do-not-touch"]}}}'

# --- Prepare fake $HOME with pre-existing agent state --------------------

FAKEHOME=$(mktemp -d)
mkdir -p "$FAKEHOME/.claude" "$FAKEHOME/.codex" "$FAKEHOME/.openclaw"
printf '%s' "$CANARY" > "$FAKEHOME/.claude/settings.json"
printf '%s' "$CANARY" > "$FAKEHOME/.codex/config.json"
printf '%s' "$CANARY" > "$FAKEHOME/.openclaw/mcp.json"

export HOME="$FAKEHOME"

# --- Install to custom prefix (explicit --no-integrate for safety) -------

PREFIX=$(mktemp -d)/llm-wiki

echo "=== Step 1: install to custom prefix ($PREFIX) ==="
curl -fsSL "https://github.com/${LW_REPO}/releases/download/${LW_VERSION}/install.sh" \
  > /tmp/install.sh 2>/dev/null \
  || curl -fsSL "https://github.com/${LW_REPO}/releases/latest/download/install.sh" > /tmp/install.sh
LW_INSTALL_PREFIX="$PREFIX" sh /tmp/install.sh --no-integrate --version "$LW_VERSION"

[ -x "$PREFIX/bin/lw" ] || { echo "FAIL: binary not installed to $PREFIX"; exit 1; }
echo "  OK"

echo "=== Step 2: uninstall from custom prefix ==="
LW_INSTALL_PREFIX="$PREFIX" sh "$PREFIX/installer/uninstall.sh" --yes

[ ! -d "$PREFIX" ] || { echo "FAIL: prefix not removed"; exit 1; }
echo "  OK"

echo "=== Step 3: fake agent-config files must be byte-identical ==="
for f in \
  "$FAKEHOME/.claude/settings.json" \
  "$FAKEHOME/.codex/config.json" \
  "$FAKEHOME/.openclaw/mcp.json"; do
  actual=$(cat "$f")
  if [ "$actual" != "$CANARY" ]; then
    echo "FAIL: uninstaller mutated $f"
    echo "  expected: $CANARY"
    echo "  actual:   $actual"
    exit 1
  fi
done
echo "  OK — all agent configs untouched"

# --- Cleanup ------------------------------------------------------------

rm -rf "$FAKEHOME" "$(dirname "$PREFIX")"

echo ""
echo "=== ISOLATION TEST PASSED ==="
