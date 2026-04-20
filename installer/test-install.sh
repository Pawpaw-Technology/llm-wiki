#!/bin/sh
# Fresh-machine smoke test for install.sh / uninstall.sh.
# Usage inside container:
#   LW_VERSION=v0.2.0 ./test-install.sh

set -eu

LW_VERSION="${LW_VERSION:-latest}"
LW_REPO="${LW_REPO:-Pawpaw-Technology/llm-wiki}"

echo "=== Test 1: install + verify binary present ==="
curl -fsSL "https://github.com/${LW_REPO}/releases/${LW_VERSION#v}/download/install.sh" \
  > /tmp/install.sh 2>/dev/null \
  || curl -fsSL "https://github.com/${LW_REPO}/releases/latest/download/install.sh" > /tmp/install.sh
sh /tmp/install.sh --no-integrate

[ -x "$HOME/.llm-wiki/bin/lw" ] || { echo "FAIL: binary not installed"; exit 1; }
[ -d "$HOME/.llm-wiki/skills" ] || { echo "FAIL: skills not extracted"; exit 1; }
[ -d "$HOME/.llm-wiki/templates" ] || { echo "FAIL: templates not extracted"; exit 1; }
[ -d "$HOME/.llm-wiki/integrations" ] || { echo "FAIL: integrations not extracted"; exit 1; }
[ -f "$HOME/.llm-wiki/version" ] || { echo "FAIL: version file missing"; exit 1; }
[ -x "$HOME/.llm-wiki/installer/uninstall.sh" ] || { echo "FAIL: uninstaller not bundled"; exit 1; }
echo "  OK"

echo "=== Test 2: PATH marker injected (.bashrc) ==="
grep -q "# >>> llm-wiki >>>" "$HOME/.bashrc" || { echo "FAIL: PATH marker missing"; exit 1; }
grep -q "# <<< llm-wiki <<<" "$HOME/.bashrc" || { echo "FAIL: PATH marker end missing"; exit 1; }
echo "  OK"

echo "=== Test 3: lw runs and prints version ==="
export PATH="$HOME/.llm-wiki/bin:$PATH"
lw --version >/dev/null 2>&1 || lw --help >/dev/null 2>&1 || { echo "FAIL: lw not runnable"; exit 1; }
echo "  OK"

echo "=== Test 4: workspace add --template general works ==="
lw workspace add demo "$HOME/demo-vault" --template general
[ -f "$HOME/demo-vault/SCOPE.md" ] || { echo "FAIL: template not copied"; exit 1; }
[ -f "$HOME/demo-vault/.lw/schema.toml" ] || { echo "FAIL: schema not present"; exit 1; }
echo "  OK"

echo "=== Test 5: re-install is idempotent ==="
sh /tmp/install.sh --no-integrate
grep -c "# >>> llm-wiki >>>" "$HOME/.bashrc" | grep -q '^1$' \
  || { echo "FAIL: PATH marker duplicated"; exit 1; }
echo "  OK"

echo "=== Test 6: uninstall --yes ==="
sh "$HOME/.llm-wiki/installer/uninstall.sh" --yes
[ ! -d "$HOME/.llm-wiki" ] || { echo "FAIL: install prefix not removed"; exit 1; }
grep -q "# >>> llm-wiki >>>" "$HOME/.bashrc" 2>/dev/null \
  && { echo "FAIL: PATH marker not stripped"; exit 1; }
[ -d "$HOME/demo-vault" ] || { echo "FAIL: vault data was destroyed"; exit 1; }
ls "$HOME"/.llm-wiki.config.toml.bak.* >/dev/null 2>&1 \
  || echo "  (note: no config.toml.bak — expected if no workspaces had been registered)"
echo "  OK"

echo ""
echo "=== ALL SMOKE TESTS PASSED ==="
