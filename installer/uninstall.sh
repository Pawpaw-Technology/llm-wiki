#!/bin/sh
# llm-wiki uninstaller — reverses install.sh per spec §11.5.

set -eu

# LW_HOME is the canonical env var (also read by the lw binary); LW_INSTALL_PREFIX
# is accepted as an alias for back-compat. Priority: LW_INSTALL_PREFIX > LW_HOME > default.
LW_INSTALL_PREFIX="${LW_INSTALL_PREFIX:-${LW_HOME:-$HOME/.llm-wiki}}"
LW_YES=0
LW_KEEP_CONFIG=0
LW_PURGE=0

usage() {
  cat <<EOF
Usage: uninstall.sh [options]

Options:
  --yes, -y          Skip confirmation prompt
  --keep-config      Preserve ~/.llm-wiki/config.toml in place
  --purge            Also delete .bak files left by integration writes
  --prefix <dir>     Override install prefix
  --help, -h         Show this help

Environment:
  LW_HOME            Install prefix (preferred; also read by the lw binary)
  LW_INSTALL_PREFIX  Alias for LW_HOME (back-compat)
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    -y|--yes) LW_YES=1; shift ;;
    --keep-config) LW_KEEP_CONFIG=1; shift ;;
    --purge) LW_PURGE=1; shift ;;
    --prefix) LW_INSTALL_PREFIX="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown flag: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ ! -d "$LW_INSTALL_PREFIX" ]; then
  echo "Nothing to uninstall — $LW_INSTALL_PREFIX not present."
  exit 0
fi

# --- Confirm ----------------------------------------------------------------

if [ "$LW_YES" -ne 1 ] && [ -t 0 ]; then
  printf "Uninstall llm-wiki from %s? [y/N] " "$LW_INSTALL_PREFIX"
  read -r ans
  case "$ans" in
    y|Y|yes|YES) ;;
    *) echo "Aborted."; exit 1 ;;
  esac
fi

# --- Step 1: reverse integrations ------------------------------------------

if [ -x "$LW_INSTALL_PREFIX/bin/lw" ] && [ -d "$LW_INSTALL_PREFIX/integrations" ]; then
  echo "Reversing integrations..."
  for tool_toml in "$LW_INSTALL_PREFIX/integrations/"*.toml; do
    [ -f "$tool_toml" ] || continue
    tool_id=$(basename "$tool_toml" .toml)
    LW_INTEGRATIONS_DIR="$LW_INSTALL_PREFIX/integrations" \
      LW_SKILLS_DIR="$LW_INSTALL_PREFIX/skills" \
      LW_HOME="$LW_INSTALL_PREFIX" \
      "$LW_INSTALL_PREFIX/bin/lw" integrate "$tool_id" --uninstall 2>/dev/null || \
      echo "  (skipped $tool_id — not currently installed)"
  done
fi

# --- Step 2: remove PATH marker block --------------------------------------

strip_path_marker() {
  rc="$1"
  [ ! -f "$rc" ] && return 0
  if ! grep -q "# >>> llm-wiki >>>" "$rc" 2>/dev/null; then
    return 0
  fi
  tmp=$(mktemp)
  awk '
    /^# >>> llm-wiki >>>$/ { in_block=1; next }
    /^# <<< llm-wiki <<<$/ { in_block=0; next }
    !in_block { print }
  ' "$rc" > "$tmp"
  mv "$tmp" "$rc"
  echo "  PATH marker removed from $rc"
}

for rc in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.profile" "$HOME/.config/fish/config.fish"; do
  strip_path_marker "$rc"
done

# --- Step 3: preserve config, then remove ----------------------------------

CONFIG="$LW_INSTALL_PREFIX/config.toml"
if [ -f "$CONFIG" ] && [ "$LW_KEEP_CONFIG" -ne 1 ]; then
  TS=$(date +%s)
  BAK="$HOME/.llm-wiki.config.toml.bak.${TS}"
  cp "$CONFIG" "$BAK"
  echo "  config saved to $BAK"
fi

# --- Step 4: optional purge of integration backup files --------------------

if [ "$LW_PURGE" -eq 1 ]; then
  echo "Purging integration backup files..."
  for d in "$HOME/.claude" "$HOME/.codex" "$HOME/.openclaw"; do
    [ -d "$d" ] || continue
    find "$d" -maxdepth 1 -name '*.bak.*' -print -delete 2>/dev/null || true
  done
fi

# --- Step 5: remove install prefix -----------------------------------------

if [ "$LW_KEEP_CONFIG" -eq 1 ] && [ -f "$CONFIG" ]; then
  # Move config back into place after wipe? Simplest: skip wipe.
  rm -rf "${LW_INSTALL_PREFIX:?}/bin" "${LW_INSTALL_PREFIX:?}/skills" \
         "${LW_INSTALL_PREFIX:?}/templates" "${LW_INSTALL_PREFIX:?}/integrations" \
         "${LW_INSTALL_PREFIX:?}/installer" "${LW_INSTALL_PREFIX:?}/version"
  echo "  removed binary, skills, templates, integrations (config preserved)"
else
  rm -rf "${LW_INSTALL_PREFIX:?}"
  echo "  removed $LW_INSTALL_PREFIX"
fi

echo ""
echo "Uninstall complete."
echo "Vault directories were NOT touched. Check ~/.llm-wiki.config.toml.bak.* if you want to re-register."
