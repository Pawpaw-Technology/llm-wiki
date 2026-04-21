# Plan A — Workspace Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `lw workspace add | list | use | current | remove` subcommands backed by `~/.llm-wiki/config.toml`, plus a 4-layer root resolution chain, so users can register multiple wiki vaults and switch between them. Preserves existing single-vault `cwd` auto-discover behavior.

**Architecture:** New `config` module owns `~/.llm-wiki/config.toml` schema and I/O. New `workspace` module implements CRUD subcommands. `main.rs::resolve_root` is upgraded from 2 layers (`--root` > env > cwd) to 4 layers (`--root` > env > current registered workspace > cwd). All logic lives in `lw-cli` since it's user-config (not wiki-internal); `lw-core` stays focused on wiki content.

**Tech Stack:** Rust 2024, `clap` 4.6 derive, `toml` 1, `dirs` 5, `serde`, `assert_cmd` for CLI integration tests.

**Working dir:** All commands run from `tool/llm-wiki/` (the llm-wiki submodule inside the mono repo). Per memory `feedback_worktree_submodule.md`, do not use worktree isolation for submodule changes — work directly in `tool/llm-wiki/`.

**Spec reference:** `docs/superpowers/specs/2026-04-19-llm-wiki-product-wrapper-design.md` §4.1, §8.

---

## File structure

| File                                   | Status | Responsibility                                         |
| -------------------------------------- | ------ | ------------------------------------------------------ |
| `crates/lw-cli/Cargo.toml`             | modify | Add `dirs` and `toml` deps                             |
| `crates/lw-cli/src/config.rs`          | create | `Config` type + TOML I/O for `~/.llm-wiki/config.toml` |
| `crates/lw-cli/src/workspace.rs`       | create | CRUD command handlers                                  |
| `crates/lw-cli/src/main.rs`            | modify | Wire `Workspace` subcommand; upgrade `resolve_root`    |
| `crates/lw-cli/tests/workspace_cli.rs` | create | End-to-end CLI integration tests                       |

---

## Task 1: Add dependencies

**Files:**

- Modify: `crates/lw-cli/Cargo.toml`

- [ ] **Step 1: Add deps**

Edit `crates/lw-cli/Cargo.toml`, append to `[dependencies]` (preserve existing entries):

```toml
dirs = "5"
toml = "1"
```

- [ ] **Step 2: Verify build still passes**

Run from `tool/llm-wiki/`:

```bash
cargo build -p lw-cli
```

Expected: clean build, no warnings about new deps.

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/Cargo.toml Cargo.lock
git commit -m "build(lw-cli): add dirs and toml deps for workspace registry"
```

---

## Task 2: Config types + TOML round-trip

**Files:**

- Create: `crates/lw-cli/src/config.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/lw-cli/src/config.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub workspace: WorkspaceState,
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceEntry>,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceState {
    pub current: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceEntry {
    pub path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_roundtrips() {
        let cfg = Config::default();
        let s = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn config_with_workspaces_roundtrips() {
        let mut workspaces = BTreeMap::new();
        workspaces.insert(
            "personal".into(),
            WorkspaceEntry { path: PathBuf::from("/tmp/personal") },
        );
        workspaces.insert(
            "work".into(),
            WorkspaceEntry { path: PathBuf::from("/tmp/work") },
        );
        let cfg = Config {
            workspace: WorkspaceState { current: Some("personal".into()) },
            workspaces,
        };
        let s = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn missing_workspace_section_defaults() {
        let s = "";
        let cfg: Config = toml::from_str(s).unwrap();
        assert!(cfg.workspace.current.is_none());
        assert!(cfg.workspaces.is_empty());
    }
}
```

Add `mod config;` near the top of `crates/lw-cli/src/main.rs` (alphabetic order with other `mod` lines).

- [ ] **Step 2: Run tests and verify they pass**

```bash
cargo test -p lw-cli config::tests
```

Expected: 3 passed. (Tests pass because the type defs already match — this is a structural test guarding against accidental schema changes.)

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/src/config.rs crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): add Config schema for ~/.llm-wiki/config.toml"
```

---

## Task 3: Config load / save with home dir resolution

**Files:**

- Modify: `crates/lw-cli/src/config.rs`

- [ ] **Step 1: Write failing tests**

Append to `crates/lw-cli/src/config.rs`:

```rust
use std::fs;
use std::io;
use std::path::Path;

/// Default location: $LW_HOME/config.toml, where LW_HOME falls back to ~/.llm-wiki/.
pub fn config_path() -> anyhow::Result<PathBuf> {
    if let Ok(custom) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(custom).join("config.toml"));
    }
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot resolve home directory"))?;
    Ok(home.join(".llm-wiki").join("config.toml"))
}

impl Config {
    /// Load from disk. Returns Default if file does not exist.
    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        match fs::read_to_string(path) {
            Ok(s) => Ok(toml::from_str(&s)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Atomic write: stage to .tmp sibling, fsync, rename.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("toml.tmp");
        let body = toml::to_string_pretty(self)?;
        fs::write(&tmp, body)?;
        // fsync the file so rename is durable
        let f = fs::File::open(&tmp)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod io_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let cfg = Config::load_from(&dir.path().join("nope.toml")).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn save_creates_parent_dir() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a/b/c/config.toml");
        Config::default().save_to(&nested).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn save_then_load_preserves_data() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.workspace.current = Some("foo".into());
        cfg.workspaces.insert(
            "foo".into(),
            WorkspaceEntry { path: PathBuf::from("/tmp/foo") },
        );
        cfg.save_to(&path).unwrap();
        let back = Config::load_from(&path).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn save_is_atomic_no_tmp_left_behind() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        Config::default().save_to(&path).unwrap();
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["config.toml".to_string()]);
    }
}
```

Add to `crates/lw-cli/Cargo.toml` under `[dev-dependencies]` (verify `tempfile = "3"` is already present — it is per the existing file).

- [ ] **Step 2: Run tests**

```bash
cargo test -p lw-cli config::
```

Expected: 7 passed (3 schema + 4 I/O).

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/src/config.rs
git commit -m "feat(lw-cli): add Config::load_from / save_to with atomic write"
```

---

## Task 4: Workspace add — name + path validation

**Files:**

- Create: `crates/lw-cli/src/workspace.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/lw-cli/src/workspace.rs`:

```rust
use crate::config::{Config, WorkspaceEntry, config_path};
use std::path::{Path, PathBuf};

/// Validate workspace name: lowercase alphanumeric + dashes, 1-32 chars.
fn validate_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() || name.len() > 32 {
        anyhow::bail!("workspace name must be 1-32 chars (got {})", name.len());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        anyhow::bail!(
            "workspace name must be lowercase alphanumeric + dashes (got '{name}')"
        );
    }
    Ok(())
}

/// Resolve to absolute path; canonicalize if it exists, else absolute-ize.
fn resolve_path(path: &Path) -> anyhow::Result<PathBuf> {
    if path.exists() {
        Ok(path.canonicalize()?)
    } else if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

pub fn add(name: &str, path: &Path, init: bool) -> anyhow::Result<()> {
    validate_name(name)?;
    let abs = resolve_path(path)?;

    let cfg_path = config_path()?;
    let mut cfg = Config::load_from(&cfg_path)?;

    if cfg.workspaces.contains_key(name) {
        anyhow::bail!("workspace '{name}' already exists");
    }

    if init {
        if !abs.exists() {
            std::fs::create_dir_all(&abs)?;
        }
        let is_empty = std::fs::read_dir(&abs)?.next().is_none();
        if !is_empty && !abs.join(".lw/schema.toml").exists() {
            anyhow::bail!(
                "--init requires an empty directory or an existing wiki (got non-empty non-wiki at {})",
                abs.display()
            );
        }
        if !abs.join(".lw/schema.toml").exists() {
            let schema = lw_core::schema::WikiSchema::default();
            lw_core::fs::init_wiki(&abs, &schema)?;
        }
    }

    let first_workspace = cfg.workspaces.is_empty();
    cfg.workspaces.insert(name.into(), WorkspaceEntry { path: abs.clone() });
    if first_workspace {
        cfg.workspace.current = Some(name.into());
    }
    cfg.save_to(&cfg_path)?;

    println!("Added workspace '{name}' at {}", abs.display());
    if first_workspace {
        println!("  set as current (first workspace)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn with_lw_home<F: FnOnce()>(home: &Path, f: F) {
        let prev = std::env::var("LW_HOME").ok();
        // SAFETY: tests are single-threaded for this env-var section. cargo test
        // runs tests in parallel by default; serialize with --test-threads=1 in CI.
        unsafe { std::env::set_var("LW_HOME", home) };
        f();
        match prev {
            Some(p) => unsafe { std::env::set_var("LW_HOME", p) },
            None => unsafe { std::env::remove_var("LW_HOME") },
        }
    }

    #[test]
    fn name_validation_rejects_uppercase() {
        assert!(validate_name("Foo").is_err());
    }

    #[test]
    fn name_validation_rejects_spaces() {
        assert!(validate_name("foo bar").is_err());
    }

    #[test]
    fn name_validation_rejects_empty() {
        assert!(validate_name("").is_err());
    }

    #[test]
    fn name_validation_accepts_dashes_and_digits() {
        assert!(validate_name("my-vault-2").is_ok());
    }

    #[test]
    fn add_first_workspace_sets_current() {
        let home = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("personal", vault.path(), false).unwrap();
            let cfg = Config::load_from(&config_path().unwrap()).unwrap();
            assert_eq!(cfg.workspace.current.as_deref(), Some("personal"));
            assert_eq!(cfg.workspaces.len(), 1);
        });
    }

    #[test]
    fn add_second_workspace_does_not_change_current() {
        let home = TempDir::new().unwrap();
        let v1 = TempDir::new().unwrap();
        let v2 = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("personal", v1.path(), false).unwrap();
            add("work", v2.path(), false).unwrap();
            let cfg = Config::load_from(&config_path().unwrap()).unwrap();
            assert_eq!(cfg.workspace.current.as_deref(), Some("personal"));
            assert_eq!(cfg.workspaces.len(), 2);
        });
    }

    #[test]
    fn add_duplicate_name_errors() {
        let home = TempDir::new().unwrap();
        let v = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("foo", v.path(), false).unwrap();
            assert!(add("foo", v.path(), false).is_err());
        });
    }

    #[test]
    fn add_with_init_creates_wiki_in_empty_dir() {
        let home = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("foo", vault.path(), true).unwrap();
            assert!(vault.path().join(".lw/schema.toml").exists());
        });
    }

    #[test]
    fn add_with_init_rejects_nonempty_non_wiki() {
        let home = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("stranger.txt"), "hi").unwrap();
        with_lw_home(home.path(), || {
            assert!(add("foo", vault.path(), true).is_err());
        });
    }
}
```

Add `mod workspace;` to `crates/lw-cli/src/main.rs` (alphabetic order).

- [ ] **Step 2: Run tests**

```bash
cargo test -p lw-cli workspace::tests -- --test-threads=1
```

Expected: 8 passed. `--test-threads=1` is required because tests mutate `LW_HOME` env var.

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/src/workspace.rs crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): add workspace::add with name/path validation and --init"
```

---

## Task 5: Workspace list / current / use\_ / remove

**Files:**

- Modify: `crates/lw-cli/src/workspace.rs`

- [ ] **Step 1: Write failing tests + impls**

Append to `crates/lw-cli/src/workspace.rs`:

```rust
pub fn list() -> anyhow::Result<()> {
    let cfg = Config::load_from(&config_path()?)?;
    if cfg.workspaces.is_empty() {
        println!("(no workspaces registered — use `lw workspace add` to create one)");
        return Ok(());
    }
    let current = cfg.workspace.current.as_deref();
    for (name, entry) in &cfg.workspaces {
        let marker = if Some(name.as_str()) == current { "*" } else { " " };
        println!("{marker} {name:20} {}", entry.path.display());
    }
    Ok(())
}

pub fn current(verbose: bool) -> anyhow::Result<()> {
    let cfg = Config::load_from(&config_path()?)?;
    let cur = cfg.workspace.current.as_deref();
    match cur {
        Some(name) => match cfg.workspaces.get(name) {
            Some(entry) => {
                println!("{name}\t{}", entry.path.display());
            }
            None => {
                anyhow::bail!(
                    "current workspace '{name}' is registered but missing from workspaces table — config corrupt"
                );
            }
        },
        None => println!("(no current workspace)"),
    }
    if verbose {
        println!();
        println!("Resolution chain (--root > LW_WIKI_ROOT env > current workspace > cwd):");
        println!(
            "  --root flag:        {}",
            "(only available at command time)"
        );
        println!(
            "  LW_WIKI_ROOT env:   {}",
            std::env::var("LW_WIKI_ROOT").unwrap_or_else(|_| "(unset)".into())
        );
        println!(
            "  current workspace:  {}",
            cur.map(|n| {
                cfg.workspaces
                    .get(n)
                    .map(|e| e.path.display().to_string())
                    .unwrap_or_else(|| "(missing entry)".into())
            })
            .unwrap_or_else(|| "(unset)".into())
        );
        println!(
            "  cwd auto-discover:  {}",
            std::env::current_dir()
                .ok()
                .and_then(|p| lw_core::fs::discover_wiki_root(&p))
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(no wiki ancestor)".into())
        );
    }
    Ok(())
}

pub fn use_(name: &str) -> anyhow::Result<()> {
    let cfg_path = config_path()?;
    let mut cfg = Config::load_from(&cfg_path)?;
    if !cfg.workspaces.contains_key(name) {
        anyhow::bail!(
            "workspace '{name}' not found (use `lw workspace list` to see registered)"
        );
    }
    cfg.workspace.current = Some(name.into());
    cfg.save_to(&cfg_path)?;
    println!("Current workspace set to '{name}'");
    println!(
        "Note: any running `lw serve` MCP processes still point at the previous vault. Restart your agent tool to pick up."
    );
    Ok(())
}

pub fn remove(name: &str) -> anyhow::Result<()> {
    let cfg_path = config_path()?;
    let mut cfg = Config::load_from(&cfg_path)?;
    if cfg.workspaces.remove(name).is_none() {
        anyhow::bail!("workspace '{name}' not found");
    }
    if cfg.workspace.current.as_deref() == Some(name) {
        cfg.workspace.current = None;
    }
    cfg.save_to(&cfg_path)?;
    println!("Removed workspace '{name}' from registry (vault directory untouched)");
    Ok(())
}

#[cfg(test)]
mod crud_tests {
    use super::*;
    use super::tests::with_lw_home;
    use tempfile::TempDir;

    #[test]
    fn use_unknown_errors() {
        let home = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            assert!(use_("ghost").is_err());
        });
    }

    #[test]
    fn use_sets_current() {
        let home = TempDir::new().unwrap();
        let v = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("a", v.path(), false).unwrap();
            add("b", v.path(), false).unwrap();
            use_("b").unwrap();
            let cfg = Config::load_from(&config_path().unwrap()).unwrap();
            assert_eq!(cfg.workspace.current.as_deref(), Some("b"));
        });
    }

    #[test]
    fn remove_clears_current_if_was_current() {
        let home = TempDir::new().unwrap();
        let v = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("a", v.path(), false).unwrap();
            remove("a").unwrap();
            let cfg = Config::load_from(&config_path().unwrap()).unwrap();
            assert!(cfg.workspace.current.is_none());
            assert!(cfg.workspaces.is_empty());
        });
    }

    #[test]
    fn remove_unknown_errors() {
        let home = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            assert!(remove("ghost").is_err());
        });
    }
}
```

In `crates/lw-cli/src/workspace.rs`, change `mod tests` to `pub(super) mod tests` so `crud_tests` can reuse `with_lw_home`. Then change `fn with_lw_home` to `pub(super) fn with_lw_home` inside that module.

- [ ] **Step 2: Run tests**

```bash
cargo test -p lw-cli workspace -- --test-threads=1
```

Expected: 12 passed (8 from Task 4 + 4 new).

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/src/workspace.rs
git commit -m "feat(lw-cli): add workspace list/current/use/remove subcommands"
```

---

## Task 6: Upgrade `resolve_root` to 4-layer priority

**Files:**

- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Replace `resolve_root` body**

In `crates/lw-cli/src/main.rs`, find the existing `fn resolve_root(...)` and replace with:

```rust
fn resolve_root(cli_root: Option<PathBuf>) -> Result<PathBuf, String> {
    // Priority: --root flag > LW_WIKI_ROOT env (already merged into cli_root by clap) > current workspace > cwd
    if let Some(root) = cli_root {
        return Ok(root);
    }
    // Try current workspace from ~/.llm-wiki/config.toml
    if let Ok(cfg_path) = config::config_path()
        && let Ok(cfg) = config::Config::load_from(&cfg_path)
        && let Some(name) = &cfg.workspace.current
        && let Some(entry) = cfg.workspaces.get(name)
        && entry.path.exists()
    {
        return Ok(entry.path.clone());
    }
    // Final fallback: cwd auto-discover
    let cwd = std::env::current_dir().map_err(|e| format!("Cannot get cwd: {e}"))?;
    lw_core::fs::discover_wiki_root(&cwd).ok_or_else(|| {
        format!(
            "Not a wiki directory (or any parent): {}\n  Run: lw init --root <path>\n  Or: lw workspace add <name> <path> --init\n  Or set LW_WIKI_ROOT environment variable",
            cwd.display()
        )
    })
}
```

Note: clap's `#[arg(env = "LW_WIKI_ROOT")]` on the `--root` flag already folds the env var into `cli_root`, so we don't need a separate env check.

- [ ] **Step 2: Verify existing tests still pass**

```bash
cargo test -p lw-cli
```

Expected: all existing tests pass; `resolve_root` is exercised by integration tests for query/lint/etc., which all use explicit `--root` so they're unaffected.

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): resolve_root falls back to current workspace before cwd"
```

---

## Task 7: Wire Workspace subcommand into CLI

**Files:**

- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Add subcommand variant**

In `crates/lw-cli/src/main.rs`, inside `enum Commands`, add (preserve existing variants):

```rust
    /// Manage registered wiki workspaces (Obsidian-style vaults)
    #[command(after_help = "Examples:\n  lw workspace add personal ~/Documents/MyWiki --init\n  lw workspace list\n  lw workspace use work\n  lw workspace current -v\n  lw workspace remove old-vault")]
    Workspace {
        #[command(subcommand)]
        action: WorkspaceCmd,
    },
```

After `enum Commands { ... }`, add:

```rust
#[derive(clap::Subcommand)]
enum WorkspaceCmd {
    /// Register a new workspace
    Add {
        /// Workspace name (lowercase alphanumeric + dashes)
        name: String,
        /// Path to the vault directory
        path: PathBuf,
        /// Initialize an empty wiki at the path if it does not exist
        #[arg(long)]
        init: bool,
    },
    /// List all registered workspaces
    List,
    /// Print the current workspace name and path
    Current {
        /// Show the full root resolution chain for debugging
        #[arg(short, long)]
        verbose: bool,
    },
    /// Set the current workspace
    #[command(name = "use")]
    UseCmd {
        /// Name of the workspace to switch to
        name: String,
    },
    /// Remove a workspace from the registry (does not touch the directory)
    Remove {
        /// Name of the workspace to unregister
        name: String,
    },
}
```

In `fn main()`, inside the `match cli.command { ... }`, add (before the closing brace):

```rust
        Commands::Workspace { action } => match action {
            WorkspaceCmd::Add { name, path, init } => workspace::add(&name, &path, init),
            WorkspaceCmd::List => workspace::list(),
            WorkspaceCmd::Current { verbose } => workspace::current(verbose),
            WorkspaceCmd::UseCmd { name } => workspace::use_(&name),
            WorkspaceCmd::Remove { name } => workspace::remove(&name),
        },
```

- [ ] **Step 2: Build and run --help to confirm wiring**

```bash
cargo build -p lw-cli
./target/debug/lw workspace --help
```

Expected output includes:

```
Manage registered wiki workspaces (Obsidian-style vaults)

Usage: lw workspace <COMMAND>

Commands:
  add      Register a new workspace
  list     List all registered workspaces
  current  Print the current workspace name and path
  use      Set the current workspace
  remove   Remove a workspace from the registry (does not touch the directory)
```

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): wire workspace subcommand into CLI"
```

---

## Task 8: End-to-end integration test

**Files:**

- Create: `crates/lw-cli/tests/workspace_cli.rs`

- [ ] **Step 1: Write failing test**

Create `crates/lw-cli/tests/workspace_cli.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn lw(home: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("lw").unwrap();
    cmd.env("LW_HOME", home);
    // Clear LW_WIKI_ROOT to keep tests deterministic
    cmd.env_remove("LW_WIKI_ROOT");
    cmd
}

#[test]
fn full_workspace_lifecycle() {
    let home = TempDir::new().unwrap();
    let vault_a = TempDir::new().unwrap();
    let vault_b = TempDir::new().unwrap();

    // Add first workspace, init the wiki structure
    lw(home.path())
        .args(["workspace", "add", "alpha", vault_a.path().to_str().unwrap(), "--init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added workspace 'alpha'"))
        .stdout(predicate::str::contains("set as current"));

    assert!(vault_a.path().join(".lw/schema.toml").exists());

    // Add second
    lw(home.path())
        .args(["workspace", "add", "beta", vault_b.path().to_str().unwrap(), "--init"])
        .assert()
        .success();

    // List shows both, alpha marked current
    lw(home.path())
        .args(["workspace", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("* alpha"))
        .stdout(predicate::str::contains("  beta"));

    // Current prints alpha
    lw(home.path())
        .args(["workspace", "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("alpha"));

    // Switch to beta
    lw(home.path())
        .args(["workspace", "use", "beta"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Current workspace set to 'beta'"))
        .stdout(predicate::str::contains("Restart your agent"));

    // Verbose current shows resolution chain
    lw(home.path())
        .args(["workspace", "current", "-v"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Resolution chain"))
        .stdout(predicate::str::contains("LW_WIKI_ROOT env"))
        .stdout(predicate::str::contains("current workspace"));

    // Remove beta clears current
    lw(home.path())
        .args(["workspace", "remove", "beta"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed workspace 'beta'"));

    lw(home.path())
        .args(["workspace", "current"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(no current workspace)"));

    // Vault directories must remain on disk
    assert!(vault_a.path().exists());
    assert!(vault_b.path().exists());
}

#[test]
fn duplicate_add_fails() {
    let home = TempDir::new().unwrap();
    let vault = TempDir::new().unwrap();
    lw(home.path())
        .args(["workspace", "add", "x", vault.path().to_str().unwrap()])
        .assert()
        .success();
    lw(home.path())
        .args(["workspace", "add", "x", vault.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn invalid_name_fails() {
    let home = TempDir::new().unwrap();
    let vault = TempDir::new().unwrap();
    lw(home.path())
        .args(["workspace", "add", "BadName", vault.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("lowercase"));
}

#[test]
fn list_empty_message() {
    let home = TempDir::new().unwrap();
    lw(home.path())
        .args(["workspace", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no workspaces registered"));
}
```

- [ ] **Step 2: Run integration tests**

```bash
cargo test -p lw-cli --test workspace_cli -- --test-threads=1
```

Expected: 4 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/tests/workspace_cli.rs
git commit -m "test(lw-cli): end-to-end integration tests for workspace commands"
```

---

## Task 9: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Full test suite**

```bash
cargo test --workspace -- --test-threads=1
```

Expected: all green.

- [ ] **Step 2: Clippy**

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 3: Format check**

```bash
cargo fmt --all -- --check
```

Expected: clean.

- [ ] **Step 4: Manual smoke test**

```bash
LW_HOME=/tmp/lw-test ./target/debug/lw workspace add demo /tmp/demo-vault --init
LW_HOME=/tmp/lw-test ./target/debug/lw workspace list
LW_HOME=/tmp/lw-test ./target/debug/lw workspace current -v
rm -rf /tmp/lw-test /tmp/demo-vault
```

Expected: all commands succeed; `current -v` prints resolution chain with `current workspace: /tmp/demo-vault`.

- [ ] **Step 5: Commit verification artifacts (if any)**

If clippy or fmt made changes during the session, commit them:

```bash
git status
# If clean, no commit needed.
```

---

## Done criteria

- `lw workspace {add,list,current,use,remove}` all functional
- `~/.llm-wiki/config.toml` (or `$LW_HOME/config.toml`) reads/writes atomically
- `resolve_root` falls back to current workspace before cwd
- `lw workspace current -v` shows full resolution chain
- All tests green, clippy clean, fmt clean
- Single-vault users with no registration see no behavior change (cwd auto-discover still works)
