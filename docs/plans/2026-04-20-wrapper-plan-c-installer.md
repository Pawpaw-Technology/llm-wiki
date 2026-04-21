# Plan C — Installer, Upgrade, Uninstall, CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a one-line `curl | sh` installer (versioned, sha256-verified), `lw upgrade` and `lw uninstall` subcommands, the `~/.llm-wiki/version` metadata file, and a CI release pipeline that bundles binary + skills + templates + integrations + installer scripts into a single per-platform tarball plus a separately-signed `install.sh` asset.

**Architecture:** Shell scripts (`install.sh`, `uninstall.sh`) own all platform-bridging logic — OS/arch detection, tarball fetch and verify, PATH manipulation in shell rc, self-delete safety. Rust subcommands (`lw upgrade`, `lw uninstall`) are thin wrappers that exec the shipped scripts in `~/.llm-wiki/installer/`. The release tarball is one bundle per platform; `install.sh` itself ships as a separate release asset so it can be `curl`-fetched without first downloading the binary.

**Tech Stack:** POSIX `sh` for installers (portable across macOS / Linux), Rust for CLI subcommands, `ureq` (already in deps) for GitHub releases API, GitHub Actions for release pipeline.

**Working dir:** `tool/llm-wiki/`. Depends on Plan A (workspace registry) and Plan B (skills/templates/integrations assets exist in repo).

**Spec reference:** `docs/superpowers/specs/2026-04-19-llm-wiki-product-wrapper-design.md` §7, §11, §11.5, §14.

---

## File structure

| File                                      | Status | Responsibility                                                     |
| ----------------------------------------- | ------ | ------------------------------------------------------------------ |
| `crates/lw-cli/src/version_file.rs`       | create | TOML `~/.llm-wiki/version` reader/writer                           |
| `crates/lw-cli/src/upgrade.rs`            | create | `lw upgrade` (--check via GitHub API; --apply execs install.sh)    |
| `crates/lw-cli/src/uninstall.rs`          | create | `lw uninstall` (execs `~/.llm-wiki/installer/uninstall.sh`)        |
| `crates/lw-cli/src/main.rs`               | modify | Wire `Upgrade` + `Uninstall` subcommands                           |
| `installer/install.sh`                    | create | Curl-installable script (also embedded for re-use)                 |
| `installer/uninstall.sh`                  | create | Reverse flow per spec §11.5                                        |
| `installer/test-install.sh`               | create | Containerized smoke test driver                                    |
| `installer/Dockerfile.test-linux`         | create | Clean Linux env for smoke test                                     |
| `.github/workflows/release.yml`           | modify | Bundle assets + publish install.sh as a separate asset with sha256 |
| `crates/lw-cli/tests/version_file_cli.rs` | create | Integration test                                                   |

---

## Task 1: Version file format + Rust reader/writer

**Files:**

- Create: `crates/lw-cli/src/version_file.rs`
- Modify: `crates/lw-cli/src/main.rs` (add `mod version_file;`)

- [ ] **Step 1: Write tests + impl**

Create `crates/lw-cli/src/version_file.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Per spec §11: the binary and the assets (skills/templates) ship with matched
/// versions and `lw doctor` enforces compatibility. We record both in
/// `~/.llm-wiki/version` so an upgrade-skew (e.g., user manually replaced the
/// binary) is detectable.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct VersionFile {
    #[serde(default)]
    pub binary: String,
    #[serde(default)]
    pub assets: String,
    /// ISO-8601 timestamp set at install/upgrade time.
    #[serde(default)]
    pub installed_at: String,
}

pub fn version_file_path() -> anyhow::Result<PathBuf> {
    if let Ok(home) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(home).join("version"));
    }
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot resolve home directory"))?;
    Ok(home.join(".llm-wiki").join("version"))
}

impl VersionFile {
    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(toml::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(self)?;
        std::fs::write(path, body)?;
        Ok(())
    }

    pub fn is_compatible(&self) -> bool {
        !self.binary.is_empty() && self.binary == self.assets
    }
}

pub const CURRENT_BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_when_missing() {
        let dir = TempDir::new().unwrap();
        let v = VersionFile::load_from(&dir.path().join("nope")).unwrap();
        assert_eq!(v, VersionFile::default());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("version");
        let v = VersionFile {
            binary: "0.2.0".into(),
            assets: "0.2.0".into(),
            installed_at: "2026-04-20T12:00:00Z".into(),
        };
        v.save_to(&path).unwrap();
        let back = VersionFile::load_from(&path).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn is_compatible_when_versions_match() {
        let v = VersionFile {
            binary: "0.2.0".into(),
            assets: "0.2.0".into(),
            installed_at: "x".into(),
        };
        assert!(v.is_compatible());
    }

    #[test]
    fn is_incompatible_when_skewed() {
        let v = VersionFile {
            binary: "0.2.0".into(),
            assets: "0.1.9".into(),
            installed_at: "x".into(),
        };
        assert!(!v.is_compatible());
    }

    #[test]
    fn is_incompatible_when_empty() {
        let v = VersionFile::default();
        assert!(!v.is_compatible());
    }
}
```

Add `mod version_file;` to `crates/lw-cli/src/main.rs`.

- [ ] **Step 2: Run tests**

```bash
cargo test -p lw-cli version_file::
```

Expected: 5 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/src/version_file.rs crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): add ~/.llm-wiki/version metadata file"
```

---

## Task 2: `install.sh` — full installer script

**Files:**

- Create: `installer/install.sh`

- [ ] **Step 1: Write the script**

Create `installer/install.sh`:

```sh
#!/bin/sh
# llm-wiki installer — curl-installable, idempotent, non-interactive by default.
# Spec: docs/superpowers/specs/2026-04-19-llm-wiki-product-wrapper-design.md §7

set -eu

# --- Defaults & flags --------------------------------------------------------

LW_INSTALL_PREFIX="${LW_INSTALL_PREFIX:-$HOME/.llm-wiki}"
LW_VERSION="${LW_VERSION:-latest}"
LW_REPO="${LW_REPO:-Pawpaw-Technology/llm-wiki}"
LW_YES=0
LW_NO_INTEGRATE=0

usage() {
  cat <<EOF
Usage: install.sh [options]

Options:
  --yes, -y           Auto-integrate detected agent tools (no prompts)
  --no-integrate      Install only; never prompt or write to agent configs
  --prefix <dir>      Install to <dir> instead of \$HOME/.llm-wiki
  --version <tag>     Install a specific release tag (default: latest)
  --help, -h          Show this help

Environment:
  LW_INSTALL_PREFIX   Same as --prefix
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
    rm -rf "$LW_INSTALL_PREFIX/$d.new"
    cp -R "$TMPDIR/$d" "$LW_INSTALL_PREFIX/$d.new"
    rm -rf "$LW_INSTALL_PREFIX/$d"
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

case "${SHELL:-}" in
  *zsh*)  inject_path "$HOME/.zshrc" ;;
  *bash*) inject_path "$HOME/.bashrc"; inject_path "$HOME/.bash_profile" ;;
  *fish*) inject_path "$HOME/.config/fish/config.fish" ;;
  *) inject_path "$HOME/.profile" ;;
esac

# Ensure PATH is good for THIS shell session
export PATH="${LW_INSTALL_PREFIX}/bin:$PATH"

# --- Optional integration ---------------------------------------------------

if [ "$LW_NO_INTEGRATE" -eq 1 ]; then
  :
elif [ "$LW_YES" -eq 1 ]; then
  "$LW_INSTALL_PREFIX/bin/lw" integrate --auto --yes || true
elif [ "$IS_TTY" -eq 1 ]; then
  # Detect available tools, suggest commands
  AVAIL=""
  for d in "$HOME/.claude" "$HOME/.codex" "$HOME/.openclaw"; do
    [ -d "$d" ] && AVAIL="${AVAIL} $(basename "$d" | sed 's/^.//')"
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
  1. Restart your shell (or: source ${rc:-~/.profile})
  2. lw workspace add my-vault ~/path/to/wiki --template general
  3. lw integrate --auto    # if you skipped during install
  4. Open your agent tool in the vault and try the llm-wiki:import skill

Docs: https://github.com/${LW_REPO}#readme
EOF
fi
```

- [ ] **Step 2: Lint with shellcheck**

```bash
shellcheck installer/install.sh
```

Expected: no warnings. If shellcheck not installed: `brew install shellcheck` or skip and rely on CI.

- [ ] **Step 3: Manual sanity check (parse only, do not run against real GitHub)**

```bash
sh -n installer/install.sh && echo "OK"
```

Expected: `OK` (syntax valid).

```bash
LW_REPO=fake/fake LW_VERSION=v0 sh installer/install.sh --help
```

Expected: usage block, exit 0.

- [ ] **Step 4: Commit**

```bash
git add installer/install.sh
git commit -m "feat(installer): add curl-installable install.sh"
```

---

## Task 3: `uninstall.sh` — full reverse flow

**Files:**

- Create: `installer/uninstall.sh`

- [ ] **Step 1: Write the script**

Create `installer/uninstall.sh`:

```sh
#!/bin/sh
# llm-wiki uninstaller — reverses install.sh per spec §11.5.

set -eu

LW_INSTALL_PREFIX="${LW_INSTALL_PREFIX:-$HOME/.llm-wiki}"
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
  read ans
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
  rm -rf "$LW_INSTALL_PREFIX/bin" "$LW_INSTALL_PREFIX/skills" \
         "$LW_INSTALL_PREFIX/templates" "$LW_INSTALL_PREFIX/integrations" \
         "$LW_INSTALL_PREFIX/installer" "$LW_INSTALL_PREFIX/version"
  echo "  removed binary, skills, templates, integrations (config preserved)"
else
  rm -rf "$LW_INSTALL_PREFIX"
  echo "  removed $LW_INSTALL_PREFIX"
fi

echo ""
echo "Uninstall complete."
echo "Vault directories were NOT touched. Check ~/.llm-wiki.config.toml.bak.* if you want to re-register."
```

- [ ] **Step 2: Lint + syntax check**

```bash
sh -n installer/uninstall.sh && echo "OK"
shellcheck installer/uninstall.sh || true
```

Expected: `OK`. shellcheck warnings about `read ans` (POSIX `read` is fine) can be tolerated.

- [ ] **Step 3: Commit**

```bash
git add installer/uninstall.sh
git commit -m "feat(installer): add uninstall.sh per spec §11.5"
```

---

## Task 4: `lw upgrade` Rust subcommand

**Files:**

- Create: `crates/lw-cli/src/upgrade.rs`
- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Write the upgrade module**

Create `crates/lw-cli/src/upgrade.rs`:

```rust
use crate::version_file::{CURRENT_BINARY_VERSION, VersionFile, version_file_path};
use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
}

const RELEASES_API: &str = "https://api.github.com/repos/Pawpaw-Technology/llm-wiki/releases/latest";

pub fn check() -> anyhow::Result<()> {
    let installed = VersionFile::load_from(&version_file_path()?)?;
    let installed_str = if installed.binary.is_empty() {
        CURRENT_BINARY_VERSION.to_string()
    } else {
        installed.binary.clone()
    };
    let latest = fetch_latest_tag()?;
    let latest_clean = latest.trim_start_matches('v');
    if latest_clean == installed_str {
        println!("lw {installed_str} is up to date.");
        Ok(())
    } else {
        println!("Newer release available: {latest} (installed: {installed_str})");
        println!("Run `lw upgrade` to install.");
        std::process::exit(1)
    }
}

pub fn apply(yes: bool) -> anyhow::Result<()> {
    let prefix = std::env::var("LW_INSTALL_PREFIX")
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join(".llm-wiki").display().to_string())
                .unwrap_or_else(|| "$HOME/.llm-wiki".into())
        });

    let installer = std::path::PathBuf::from(&prefix)
        .join("installer")
        .join("install.sh");
    if !installer.exists() {
        anyhow::bail!(
            "installer not found at {} — re-run the curl install command from the README",
            installer.display()
        );
    }

    let mut cmd = Command::new("sh");
    cmd.arg(&installer);
    if yes {
        cmd.arg("--yes");
    }
    cmd.env("LW_INSTALL_PREFIX", &prefix);

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("installer exited with {status}");
    }
    Ok(())
}

fn fetch_latest_tag() -> anyhow::Result<String> {
    let resp = ureq::get(RELEASES_API)
        .header("User-Agent", concat!("lw/", env!("CARGO_PKG_VERSION")))
        .call()?;
    let release: GhRelease = resp.into_body().read_json()?;
    Ok(release.tag_name)
}
```

- [ ] **Step 2: Wire into CLI**

In `crates/lw-cli/src/main.rs`, add to `enum Commands`:

```rust
    /// Check for or apply a newer llm-wiki release
    #[command(after_help = "Examples:\n  lw upgrade --check\n  lw upgrade\n  lw upgrade --yes")]
    Upgrade {
        /// Only check; do not download/replace
        #[arg(long)]
        check: bool,
        /// Pass --yes to the installer (auto-integrate)
        #[arg(short, long)]
        yes: bool,
    },
```

Add `mod upgrade;` near the top.

In the `match cli.command`:

```rust
        Commands::Upgrade { check, yes } => {
            if check {
                upgrade::check()
            } else {
                upgrade::apply(yes)
            }
        }
```

- [ ] **Step 3: Build + smoke test**

```bash
cargo build -p lw-cli
./target/debug/lw upgrade --help
```

Expected: help text. Do NOT run `lw upgrade --check` here without network; CI / smoke tests cover it.

- [ ] **Step 4: Commit**

```bash
git add crates/lw-cli/src/upgrade.rs crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): add lw upgrade --check / apply"
```

---

## Task 5: `lw uninstall` subcommand

**Files:**

- Create: `crates/lw-cli/src/uninstall.rs`
- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Write the uninstall wrapper**

Create `crates/lw-cli/src/uninstall.rs`:

```rust
use std::process::Command;

pub struct UninstallOpts {
    pub yes: bool,
    pub keep_config: bool,
    pub purge: bool,
}

pub fn run(opts: UninstallOpts) -> anyhow::Result<()> {
    let prefix = std::env::var("LW_INSTALL_PREFIX")
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join(".llm-wiki").display().to_string())
                .unwrap_or_else(|| "$HOME/.llm-wiki".into())
        });

    let script = std::path::PathBuf::from(&prefix)
        .join("installer")
        .join("uninstall.sh");
    if !script.exists() {
        anyhow::bail!(
            "uninstall script not found at {} — manual cleanup required (rm -rf ~/.llm-wiki and strip PATH marker)",
            script.display()
        );
    }

    let mut cmd = Command::new("sh");
    cmd.arg(&script);
    if opts.yes {
        cmd.arg("--yes");
    }
    if opts.keep_config {
        cmd.arg("--keep-config");
    }
    if opts.purge {
        cmd.arg("--purge");
    }
    cmd.env("LW_INSTALL_PREFIX", &prefix);

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("uninstall script exited with {status}");
    }
    Ok(())
}
```

- [ ] **Step 2: Wire into CLI**

In `crates/lw-cli/src/main.rs`, add to `enum Commands`:

```rust
    /// Remove llm-wiki from this machine (vault data preserved)
    #[command(after_help = "Examples:\n  lw uninstall\n  lw uninstall --yes\n  lw uninstall --keep-config\n  lw uninstall --yes --purge")]
    Uninstall {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        /// Keep ~/.llm-wiki/config.toml in place
        #[arg(long)]
        keep_config: bool,
        /// Also delete .bak files left by past integration writes
        #[arg(long)]
        purge: bool,
    },
```

Add `mod uninstall;` near the top.

In the `match cli.command`:

```rust
        Commands::Uninstall { yes, keep_config, purge } => {
            uninstall::run(uninstall::UninstallOpts { yes, keep_config, purge })
        }
```

- [ ] **Step 3: Build + smoke test**

```bash
cargo build -p lw-cli
./target/debug/lw uninstall --help
```

Expected: help text shows all flags.

- [ ] **Step 4: Commit**

```bash
git add crates/lw-cli/src/uninstall.rs crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): add lw uninstall (wraps installer/uninstall.sh)"
```

---

## Task 6: `release.yml` — bundle assets + publish install.sh as separate asset

**Files:**

- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Replace release.yml**

Replace `.github/workflows/release.yml` with:

```yaml
name: Release

on:
  push:
    tags: ["v*"]

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build:
    name: Build (${{ matrix.target }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            asset: lw-x86_64-linux.tar.gz
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
            asset: lw-aarch64-linux.tar.gz
          - target: x86_64-apple-darwin
            os: macos-latest
            asset: lw-x86_64-darwin.tar.gz
          - target: aarch64-apple-darwin
            os: macos-latest
            asset: lw-aarch64-darwin.tar.gz
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install cross-compilation tools
        if: matrix.target == 'aarch64-unknown-linux-gnu'
        run: |
          sudo apt-get update
          sudo apt-get install -y gcc-aarch64-linux-gnu
          echo "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc" >> "$GITHUB_ENV"

      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      - name: Build release binary
        run: cargo build --release --target ${{ matrix.target }} -p lw-cli

      - name: Stage tarball contents
        run: |
          STAGE=stage
          mkdir -p "$STAGE"
          cp target/${{ matrix.target }}/release/lw "$STAGE/lw"
          cp -R skills "$STAGE/skills"
          cp -R templates "$STAGE/templates"
          cp -R integrations "$STAGE/integrations"
          mkdir -p "$STAGE/installer"
          cp installer/install.sh "$STAGE/installer/"
          cp installer/uninstall.sh "$STAGE/installer/"
          chmod +x "$STAGE/installer/install.sh" "$STAGE/installer/uninstall.sh"
          # VERSION = tag without leading 'v'
          echo "${GITHUB_REF_NAME#v}" > "$STAGE/VERSION"

      - name: Package tarball
        run: |
          mkdir -p dist
          tar -C stage -czf dist/${{ matrix.asset }} .
          shasum -a 256 dist/${{ matrix.asset }} | awk '{print $1"  "$2}' > dist/${{ matrix.asset }}.sha256

      - uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.asset }}
          path: dist/${{ matrix.asset }}*

  publish-installer:
    name: Publish install.sh as separate asset
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Stage installer + sha256
        run: |
          mkdir -p dist
          cp installer/install.sh dist/install.sh
          cp installer/uninstall.sh dist/uninstall.sh
          shasum -a 256 dist/install.sh | awk '{print $1"  "$2}' > dist/install.sh.sha256
          shasum -a 256 dist/uninstall.sh | awk '{print $1"  "$2}' > dist/uninstall.sh.sha256
      - uses: actions/upload-artifact@v4
        with:
          name: installer-scripts
          path: dist/*

  release:
    name: Create Release
    needs: [build, publish-installer]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          generate_release_notes: true
          files: artifacts/*
```

- [ ] **Step 2: Validate workflow YAML**

```bash
# Use yamllint if available, otherwise just confirm parseability with python
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" && echo "YAML OK"
```

Expected: `YAML OK`.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): bundle skills/templates/integrations + publish install.sh asset"
```

---

## Task 7: Containerized smoke test

**Files:**

- Create: `installer/Dockerfile.test-linux`
- Create: `installer/test-install.sh`

- [ ] **Step 1: Create Dockerfile**

Create `installer/Dockerfile.test-linux`:

```dockerfile
# Clean Linux env to smoke-test the installer end-to-end.
# Build: docker build -f installer/Dockerfile.test-linux -t lw-installer-test .
# Run:   docker run --rm -e LW_VERSION=v0.2.0 lw-installer-test

FROM ubuntu:24.04

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        curl ca-certificates tar gzip coreutils && \
    rm -rf /var/lib/apt/lists/*

RUN useradd -m tester
USER tester
WORKDIR /home/tester

COPY --chown=tester:tester installer/test-install.sh /home/tester/test-install.sh
RUN chmod +x /home/tester/test-install.sh

ENTRYPOINT ["/home/tester/test-install.sh"]
```

- [ ] **Step 2: Create test driver**

Create `installer/test-install.sh`:

```sh
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
[ -f "$HOME/.llm-wiki.config.toml.bak."* ] 2>/dev/null \
  || ls "$HOME"/.llm-wiki.config.toml.bak.* >/dev/null 2>&1 \
  || echo "  (note: no config.toml.bak — expected if no workspaces had been registered)"
echo "  OK"

echo ""
echo "=== ALL SMOKE TESTS PASSED ==="
```

- [ ] **Step 3: Local sanity check**

```bash
chmod +x installer/test-install.sh
sh -n installer/test-install.sh && echo "OK"
```

Expected: `OK`. Do NOT actually run the test until a release exists; this is for the smoke-test phase in Plan D.

- [ ] **Step 4: Commit**

```bash
git add installer/Dockerfile.test-linux installer/test-install.sh
git commit -m "test(installer): add containerized smoke test driver"
```

---

## Task 8: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Workspace test pass**

```bash
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: all green.

- [ ] **Step 2: Help command coverage**

```bash
cargo build -p lw-cli
for c in workspace integrate upgrade uninstall; do
  ./target/debug/lw $c --help >/dev/null && echo "  $c help OK"
done
```

Expected: all four commands' help renders without error.

- [ ] **Step 3: Manual end-to-end installer dry-run (no GitHub)**

```bash
# Stage a fake release locally
mkdir -p /tmp/lw-fake-release
mkdir -p /tmp/lw-fake-stage/installer
cp target/debug/lw /tmp/lw-fake-stage/lw
cp -R skills templates integrations /tmp/lw-fake-stage/
cp installer/install.sh installer/uninstall.sh /tmp/lw-fake-stage/installer/
echo "0.0.0-local" > /tmp/lw-fake-stage/VERSION
tar -C /tmp/lw-fake-stage -czf /tmp/lw-fake-release/lw-$(uname -m | sed 's/arm64/aarch64/')-$(uname -s | tr '[:upper:]' '[:lower:]').tar.gz .
shasum -a 256 /tmp/lw-fake-release/lw-*.tar.gz | awk '{print $1}' > /tmp/lw-fake-release/lw-$(uname -m | sed 's/arm64/aarch64/')-$(uname -s | tr '[:upper:]' '[:lower:]').tar.gz.sha256

# Skip the real install.sh (it expects GitHub URL); manually verify tarball layout
tar -tzf /tmp/lw-fake-release/lw-*.tar.gz | head -20
```

Expected: tarball lists `lw`, `skills/`, `templates/`, `integrations/`, `installer/`, `VERSION`.

- [ ] **Step 4: Commit any final fixes**

```bash
git status
# If clean, no commit needed.
```

---

## Done criteria

- `installer/install.sh` is a valid, shellcheck-clean POSIX script with `--yes` / `--no-integrate` / `--prefix` / `--version` flags
- `installer/uninstall.sh` is a valid POSIX script implementing spec §11.5 reverse flow
- `lw upgrade --check` queries GitHub releases API and exits 0/1 based on version comparison
- `lw upgrade` execs the bundled `install.sh` for clean reinstall
- `lw uninstall` execs the bundled `uninstall.sh` with passthrough flags
- `release.yml` produces 4 platform tarballs containing binary + skills + templates + integrations + installer + VERSION; also publishes `install.sh` and `uninstall.sh` as separate assets with sha256
- Containerized smoke test driver script exists for use in Plan D
- All Rust tests green, clippy clean, fmt clean
