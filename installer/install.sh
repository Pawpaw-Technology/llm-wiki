#!/bin/sh
# llm-wiki installer — curl-installable, idempotent, non-interactive by default.
# Spec: docs/superpowers/specs/2026-04-19-llm-wiki-product-wrapper-design.md §7

set -eu

# --- Defaults & flags --------------------------------------------------------

# LW_HOME is the canonical env var (also read by the lw binary); LW_INSTALL_PREFIX
# is accepted as an alias for back-compat. Priority: LW_INSTALL_PREFIX > LW_HOME > default.
LW_INSTALL_PREFIX="${LW_INSTALL_PREFIX:-${LW_HOME:-$HOME/.llm-wiki}}"
LW_VERSION="${LW_VERSION:-latest}"
LW_REPO="${LW_REPO:-Pawpaw-Technology/llm-wiki}"
LW_YES=0
LW_NO_INTEGRATE=0

usage() {
  cat <<EOF
Usage: install.sh [options]

Options:
  --yes, -y           Auto-integrate detected agent tools (no prompts)
  --no-integrate      Install only; skip ALL user-dir writes.
                      Suppresses shell-rc PATH injection, MCP config
                      writes, and skills install — useful for sandboxed
                      / scripted installs.
  --prefix <dir>      Install to <dir> instead of \$HOME/.llm-wiki
  --version <tag>     Install a specific release tag (default: latest)
  --help, -h          Show this help

Environment:
  LW_HOME             Install prefix (preferred; also read by the lw binary)
  LW_INSTALL_PREFIX   Alias for LW_HOME (back-compat)
  LW_VERSION          Same as --version
  LW_REPO             GitHub repo slug (default: Pawpaw-Technology/llm-wiki)
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    -y|--yes) LW_YES=1; shift ;;
    --no-integrate) LW_NO_INTEGRATE=1; shift ;;
    --prefix) LW_INSTALL_PREFIX="$2"; shift 2 ;;
    --version) LW_VERSION="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown flag: $1" >&2; usage >&2; exit 2 ;;
  esac
done

# --- TTY detection -----------------------------------------------------------

if [ -t 1 ]; then
  IS_TTY=1
else
  IS_TTY=0
fi

# --- OS / arch detection -----------------------------------------------------

UNAME_S=$(uname -s)
UNAME_M=$(uname -m)

case "$UNAME_S" in
  Darwin) OS=darwin ;;
  Linux)  OS=linux ;;
  *) echo "Unsupported OS: $UNAME_S (only macOS and Linux are supported in this version)" >&2; exit 1 ;;
esac

case "$UNAME_M" in
  x86_64|amd64) ARCH=x86_64 ;;
  arm64|aarch64) ARCH=aarch64 ;;
  *) echo "Unsupported arch: $UNAME_M" >&2; exit 1 ;;
esac

ASSET="lw-${ARCH}-${OS}.tar.gz"

# --- GitHub release URL resolution -------------------------------------------

if [ "$LW_VERSION" = "latest" ]; then
  BASE_URL="https://github.com/${LW_REPO}/releases/latest/download"
else
  BASE_URL="https://github.com/${LW_REPO}/releases/download/${LW_VERSION}"
fi

DIST_URL="${BASE_URL}/${ASSET}"
SHA_URL="${BASE_URL}/${ASSET}.sha256"

# --- Helper: fetch w/ curl preferred, wget fallback --------------------------

fetch() {
  url="$1"; out="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$out"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$out" "$url"
  else
    echo "Need curl or wget" >&2; exit 1
  fi
}

# --- Helper: sha256 verify ---------------------------------------------------

verify_sha256() {
  file="$1"; expected_sha_file="$2"
  if command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$file" | awk '{print $1}')
  elif command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$file" | awk '{print $1}')
  else
    echo "Need shasum or sha256sum" >&2; exit 1
  fi
  expected=$(awk '{print $1}' "$expected_sha_file")
  if [ "$actual" != "$expected" ]; then
    echo "sha256 mismatch! expected $expected, got $actual" >&2
    exit 1
  fi
}

# --- Download + verify -------------------------------------------------------

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT INT TERM

echo "Fetching ${ASSET} from ${BASE_URL}..."
fetch "$DIST_URL" "$TMPDIR/$ASSET"
fetch "$SHA_URL" "$TMPDIR/$ASSET.sha256"
verify_sha256 "$TMPDIR/$ASSET" "$TMPDIR/$ASSET.sha256"
echo "  sha256 ok"

# --- Extract -----------------------------------------------------------------

mkdir -p "$LW_INSTALL_PREFIX"
tar -C "$TMPDIR" -xzf "$TMPDIR/$ASSET"

# Tarball layout (built by release.yml):
#   lw                       binary
#   skills/                  skills tree
#   templates/               templates tree
#   integrations/            descriptors
#   installer/install.sh     this script (bundled for upgrade)
#   installer/uninstall.sh
#   VERSION                  e.g. "0.2.0"

mkdir -p "$LW_INSTALL_PREFIX/bin"
install -m 755 "$TMPDIR/lw" "$LW_INSTALL_PREFIX/bin/lw"

# Replace asset trees atomically per directory
for d in skills templates integrations installer; do
  if [ -d "$TMPDIR/$d" ]; then
    rm -rf "${LW_INSTALL_PREFIX:?}/$d.new"
    cp -R "$TMPDIR/$d" "$LW_INSTALL_PREFIX/$d.new"
    rm -rf "${LW_INSTALL_PREFIX:?}/$d"
    mv "$LW_INSTALL_PREFIX/$d.new" "$LW_INSTALL_PREFIX/$d"
  fi
done

# Make installer scripts executable
[ -f "$LW_INSTALL_PREFIX/installer/install.sh" ] && chmod +x "$LW_INSTALL_PREFIX/installer/install.sh"
[ -f "$LW_INSTALL_PREFIX/installer/uninstall.sh" ] && chmod +x "$LW_INSTALL_PREFIX/installer/uninstall.sh"

# Write VERSION file
RELEASE_VERSION=$(cat "$TMPDIR/VERSION" 2>/dev/null || echo "unknown")
NOW_ISO=$(date -u +%Y-%m-%dT%H:%M:%SZ)
cat > "$LW_INSTALL_PREFIX/version" <<EOF
binary = "${RELEASE_VERSION}"
assets = "${RELEASE_VERSION}"
installed_at = "${NOW_ISO}"
EOF

echo "Installed lw ${RELEASE_VERSION} to ${LW_INSTALL_PREFIX}"

# --- PATH injection ----------------------------------------------------------
#
# --no-integrate suppresses all user-dir writes, including shell-rc PATH
# injection. This matches the flag's documented scope and keeps
# sandboxed / scripted installs fully contained to $LW_INSTALL_PREFIX.

inject_path() {
  rc="$1"
  marker_start="# >>> llm-wiki >>>"
  marker_end="# <<< llm-wiki <<<"
  block="${marker_start}
export PATH=\"${LW_INSTALL_PREFIX}/bin:\$PATH\"
${marker_end}"

  [ ! -f "$rc" ] && return 0
  if grep -q "$marker_start" "$rc" 2>/dev/null; then
    return 0  # already present
  fi
  printf '\n%s\n' "$block" >> "$rc"
  echo "  PATH appended to $rc"
}

if [ "$LW_NO_INTEGRATE" -ne 1 ]; then
  case "${SHELL:-}" in
    *zsh*)  inject_path "$HOME/.zshrc" ;;
    *bash*) inject_path "$HOME/.bashrc"; inject_path "$HOME/.bash_profile" ;;
    *fish*) inject_path "$HOME/.config/fish/config.fish" ;;
    *) inject_path "$HOME/.profile" ;;
  esac

  # Ensure PATH is good for THIS shell session
  export PATH="${LW_INSTALL_PREFIX}/bin:$PATH"
fi

# --- Optional integration ---------------------------------------------------

if [ "$LW_NO_INTEGRATE" -eq 1 ]; then
  :
elif [ "$LW_YES" -eq 1 ]; then
  "$LW_INSTALL_PREFIX/bin/lw" integrate --auto --yes || true
elif [ "$IS_TTY" -eq 1 ]; then
  # Detect available tools, suggest commands
  AVAIL=""
  for d in "$HOME/.claude" "$HOME/.codex" "$HOME/.openclaw"; do
    [ -d "$d" ] && AVAIL="${AVAIL} $(basename "$d" | sed 's/^\.//')"
  done
  AVAIL=$(echo "$AVAIL" | sed 's/^ //')
  if [ -n "$AVAIL" ]; then
    echo ""
    echo "Detected agent tools:${AVAIL}"
    echo "Wire them up with: lw integrate --auto"
    echo "(Pass --yes during install to do this automatically.)"
  fi
fi

# --- Next steps --------------------------------------------------------------

if [ "$IS_TTY" -eq 1 ]; then
  cat <<EOF

Next steps:
  1. Restart your shell (or open a new terminal)
  2. lw workspace add my-vault ~/path/to/wiki --template general
  3. lw integrate --auto    # if you skipped during install
  4. Open your agent tool in the vault and try the llm-wiki:import skill

Docs: https://github.com/${LW_REPO}#readme
EOF
fi
