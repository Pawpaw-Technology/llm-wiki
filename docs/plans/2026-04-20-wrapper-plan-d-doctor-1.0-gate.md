# Plan D — Multi-tool, Doctor, 1.0 Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Codex and OpenClaw integration descriptors, implement `lw doctor` with the full check matrix from spec §12, rewrite the user-facing README per §13, redirect the mono repo's install surface to the new curl line, and execute the three-part 1.0 gate (install / behavior / uninstall smoke tests on macOS + Linux across all three agent tools) — with transcripts committed under `docs/smoke-tests/v1.0.0/`.

**Architecture:** `lw doctor` is a sequence of small check functions, each producing a `CheckResult { status, label, detail, remediation }`. The runner prints a checklist and exits non-zero if any check fails. Codex and OpenClaw descriptors reuse the same TOML schema as Claude Code (Plan B); OpenClaw's descriptor has no `[mcp]` section (skill-only path per spec §9). The 1.0 gate is a composite: automated install/uninstall via the container test driver from Plan C, plus manually-driven `llm-wiki:import` walkthroughs against each agent tool with the transcripts checked into the repo.

**Tech Stack:** Rust 2024, existing deps. Smoke tests use Docker (Plan C's Dockerfile) for Linux, fresh macOS user accounts (or VMs) for darwin coverage.

**Working dir:** `tool/llm-wiki/`. Depends on Plans A, B, C. The mono repo README edit happens in `llm-wiki-mono` (the parent repo); the agents README edit happens in `llm-wiki-agents`.

**Spec reference:** `docs/superpowers/specs/2026-04-19-llm-wiki-product-wrapper-design.md` §9, §12, §13, §14.

---

## File structure

| File                                              | Status  | Responsibility                                  |
| ------------------------------------------------- | ------- | ----------------------------------------------- |
| `integrations/codex.toml`                         | create  | Codex MCP + skills adapter                      |
| `integrations/openclaw.toml`                      | create  | OpenClaw skill-only adapter                     |
| `crates/lw-cli/src/doctor.rs`                     | create  | `lw doctor` checks + reporter                   |
| `crates/lw-cli/src/main.rs`                       | modify  | Wire `Doctor` subcommand                        |
| `crates/lw-cli/tests/doctor_cli.rs`               | create  | E2E doctor with controlled env                  |
| `README.md` (llm-wiki repo)                       | rewrite | Per spec §13 user journey                       |
| `docs/smoke-tests/v1.0.0/install-linux.log`       | create  | Captured during gate                            |
| `docs/smoke-tests/v1.0.0/install-darwin.log`      | create  | Captured during gate                            |
| `docs/smoke-tests/v1.0.0/behavior-claude-code.md` | create  | Transcript                                      |
| `docs/smoke-tests/v1.0.0/behavior-codex.md`       | create  | Transcript                                      |
| `docs/smoke-tests/v1.0.0/behavior-openclaw.md`    | create  | Transcript                                      |
| `docs/smoke-tests/v1.0.0/uninstall.log`           | create  | Captured during gate                            |
| (mono repo) `README.md`                           | modify  | Replace install instructions with curl redirect |
| (agents repo) `README.md`                         | modify  | Point at new install method                     |

---

## Task 1: Codex + OpenClaw integration descriptors

**Files:**

- Create: `integrations/codex.toml`
- Create: `integrations/openclaw.toml`

- [ ] **Step 1: Create Codex descriptor**

Codex stores config in `~/.codex/config.toml` and skills under `~/.codex/skills/`. (Per spec §9 table.)

Create `integrations/codex.toml`:

```toml
name = "Codex"

[detect]
config_dir = "~/.codex"

# Codex's MCP config is TOML, not JSON. The current MCP merge engine
# (lw-cli/src/integrations/mcp.rs) only handles JSON. For 1.0 we ship the
# descriptor with only the [skills] section so `lw integrate codex` performs
# the skill-link half. The MCP wiring lands in 1.1 once the merge engine grows
# TOML support; until then, README documents the manual MCP step for Codex.

[skills]
target_dir = "~/.codex/skills/llm-wiki/"
mode = "symlink"
```

- [ ] **Step 2: Create OpenClaw descriptor**

Per spec §9 table, OpenClaw is skill-only in 1.0 (user judgment: skill install is the bulk of value; MCP wiring lands in 1.1 if non-trivial).

Create `integrations/openclaw.toml`:

```toml
name = "OpenClaw"

[detect]
config_dir = "~/.openclaw"

[skills]
target_dir = "~/.openclaw/skills/llm-wiki/"
mode = "symlink"
```

- [ ] **Step 3: Verify descriptors parse**

```bash
cargo test -p lw-cli integrations::descriptor::tests::skills_only_descriptor_no_mcp -- --test-threads=1
```

Expected: passed (the test from Plan B Task 4 covers MCP-less descriptors; just confirm).

```bash
ls integrations/
```

Expected: `claude-code.toml`, `codex.toml`, `openclaw.toml`.

- [ ] **Step 4: Commit**

```bash
git add integrations/codex.toml integrations/openclaw.toml
git commit -m "feat(integrations): add Codex and OpenClaw descriptors (skill-only for 1.0)"
```

---

## Task 2: `lw doctor` — implementation

**Files:**

- Create: `crates/lw-cli/src/doctor.rs`
- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Write check infrastructure + tests**

Create `crates/lw-cli/src/doctor.rs`:

```rust
use crate::config::{Config, config_path};
use crate::integrations::{
    descriptor::{Descriptor, expand_tilde},
    integrations_root, load_all, mcp,
};
use crate::version_file::{CURRENT_BINARY_VERSION, VersionFile, version_file_path};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug)]
pub struct CheckResult {
    pub label: String,
    pub status: Status,
    pub detail: Option<String>,
    pub remediation: Option<String>,
}

impl CheckResult {
    pub fn ok(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            status: Status::Ok,
            detail: Some(detail.into()),
            remediation: None,
        }
    }

    pub fn warn(label: impl Into<String>, detail: impl Into<String>, remediation: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            status: Status::Warn,
            detail: Some(detail.into()),
            remediation: Some(remediation.into()),
        }
    }

    pub fn fail(label: impl Into<String>, detail: impl Into<String>, remediation: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            status: Status::Fail,
            detail: Some(detail.into()),
            remediation: Some(remediation.into()),
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let mut results = Vec::new();
    results.push(check_binary());
    results.push(check_path_env());
    results.push(check_config_loadable());
    results.push(check_current_workspace());
    results.push(check_version_compat());
    results.extend(check_integrations());
    results.push(check_serve_smoke());

    print_report(&results);

    if results.iter().any(|r| r.status == Status::Fail) {
        std::process::exit(1);
    }
    Ok(())
}

fn print_report(results: &[CheckResult]) {
    println!("lw doctor");
    println!("=========");
    for r in results {
        let mark = match r.status {
            Status::Ok => "✓",
            Status::Warn => "⚠",
            Status::Fail => "✗",
        };
        println!("{mark} {}", r.label);
        if let Some(d) = &r.detail {
            println!("    {}", d);
        }
        if let Some(rem) = &r.remediation {
            println!("    → {}", rem);
        }
    }
    let fails = results.iter().filter(|r| r.status == Status::Fail).count();
    let warns = results.iter().filter(|r| r.status == Status::Warn).count();
    println!("---");
    println!("{} passed, {} warned, {} failed", results.len() - fails - warns, warns, fails);
}

// --- Individual checks ---------------------------------------------------

fn check_binary() -> CheckResult {
    match std::env::current_exe() {
        Ok(p) => CheckResult::ok(
            "binary location",
            format!("{} (v{CURRENT_BINARY_VERSION})", p.display()),
        ),
        Err(e) => CheckResult::fail("binary location", e.to_string(), "reinstall via curl install.sh"),
    }
}

fn check_path_env() -> CheckResult {
    let prefix = std::env::var("LW_INSTALL_PREFIX")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".llm-wiki")))
        .map(|p| p.join("bin"));

    let path_var = std::env::var("PATH").unwrap_or_default();
    let in_path = match &prefix {
        Some(p) => path_var.split(':').any(|seg| std::path::Path::new(seg) == p),
        None => false,
    };

    match (prefix, in_path) {
        (Some(p), true) => CheckResult::ok("PATH includes lw bin", p.display().to_string()),
        (Some(p), false) => CheckResult::warn(
            "PATH includes lw bin",
            format!("{} not in PATH", p.display()),
            "open a new shell, or `source ~/.zshrc` (or your rc file)".into(),
        ),
        (None, _) => CheckResult::fail(
            "PATH includes lw bin",
            "cannot resolve home dir".into(),
            "set HOME or LW_INSTALL_PREFIX".into(),
        ),
    }
}

fn check_config_loadable() -> CheckResult {
    let path = match config_path() {
        Ok(p) => p,
        Err(e) => return CheckResult::fail("config.toml", e.to_string(), "set HOME or LW_HOME"),
    };
    if !path.exists() {
        return CheckResult::ok(
            "config.toml",
            format!("{} (not present — single-vault mode)", path.display()),
        );
    }
    match Config::load_from(&path) {
        Ok(_) => CheckResult::ok("config.toml", format!("{} parses", path.display())),
        Err(e) => CheckResult::fail(
            "config.toml",
            format!("{} parse error: {e}", path.display()),
            "edit by hand or `lw workspace remove` and re-add".into(),
        ),
    }
}

fn check_current_workspace() -> CheckResult {
    let path = match config_path() {
        Ok(p) => p,
        Err(_) => return CheckResult::ok("current workspace", "skipped (no config dir)".into()),
    };
    let cfg = match Config::load_from(&path) {
        Ok(c) => c,
        Err(_) => return CheckResult::ok("current workspace", "skipped (config unparseable)".into()),
    };
    match cfg.workspace.current.as_deref() {
        None => CheckResult::ok("current workspace", "(none — using cwd auto-discover)".into()),
        Some(name) => match cfg.workspaces.get(name) {
            Some(entry) if entry.path.exists() => {
                CheckResult::ok("current workspace", format!("{name} → {}", entry.path.display()))
            }
            Some(entry) => CheckResult::fail(
                "current workspace",
                format!("{name} → {} (path does not exist)", entry.path.display()),
                format!("`lw workspace use <other>` or recreate {}", entry.path.display()),
            ),
            None => CheckResult::fail(
                "current workspace",
                format!("'{name}' set as current but missing from workspaces table"),
                "edit ~/.llm-wiki/config.toml or remove and re-add".into(),
            ),
        },
    }
}

fn check_version_compat() -> CheckResult {
    let path = match version_file_path() {
        Ok(p) => p,
        Err(_) => return CheckResult::warn("version file", "skipped".into(), "set HOME".into()),
    };
    let v = match VersionFile::load_from(&path) {
        Ok(v) => v,
        Err(e) => return CheckResult::fail("version file", e.to_string(), "reinstall".into()),
    };
    if v.binary.is_empty() && v.assets.is_empty() {
        return CheckResult::warn(
            "version file",
            "missing — pre-installer setup or partial install".into(),
            "run install.sh".into(),
        );
    }
    if !v.is_compatible() {
        return CheckResult::fail(
            "binary/assets version",
            format!("binary={}, assets={} (mismatch)", v.binary, v.assets),
            "run `lw upgrade` to realign".into(),
        );
    }
    CheckResult::ok(
        "binary/assets version",
        format!("both at {}", v.binary),
    )
}

fn check_integrations() -> Vec<CheckResult> {
    let mut out = Vec::new();
    let descriptors = match load_all() {
        Ok(d) => d,
        Err(e) => {
            out.push(CheckResult::warn(
                "integrations",
                e.to_string(),
                "reinstall to restore integrations dir".into(),
            ));
            return out;
        }
    };
    for (id, desc) in descriptors {
        if !desc.detect_present() {
            out.push(CheckResult::ok(
                format!("integration: {id}"),
                format!("{} not detected — skipped", desc.name),
            ));
            continue;
        }
        out.extend(check_one_integration(&id, &desc));
    }
    out
}

fn check_one_integration(id: &str, desc: &Descriptor) -> Vec<CheckResult> {
    let mut out = Vec::new();
    if let Some(mcp_cfg) = &desc.mcp {
        let path = expand_tilde(&mcp_cfg.config_path);
        if !path.exists() {
            out.push(CheckResult::warn(
                format!("integration: {id} (MCP)"),
                format!("{} missing", path.display()),
                format!("`lw integrate {id}` to install"),
            ));
        } else {
            match std::fs::read_to_string(&path).and_then(|s| {
                serde_json::from_str::<Value>(&s)
                    .map_err(|e| std::io::Error::other(e.to_string()))
            }) {
                Ok(cfg) => {
                    let parts: Vec<&str> = mcp_cfg.key_path.split('.').collect();
                    let mut cursor = &cfg;
                    let mut found = true;
                    for p in &parts {
                        match cursor.get(*p) {
                            Some(c) => cursor = c,
                            None => {
                                found = false;
                                break;
                            }
                        }
                    }
                    if !found || cursor.is_null() {
                        out.push(CheckResult::warn(
                            format!("integration: {id} (MCP)"),
                            format!("entry {} not present", mcp_cfg.key_path),
                            format!("`lw integrate {id}` to install"),
                        ));
                    } else {
                        let entry_version = cursor
                            .get(mcp::VERSION_MARKER)
                            .and_then(|v| v.as_str())
                            .unwrap_or("(missing)");
                        let cmd = cursor.get("command").and_then(|v| v.as_str()).unwrap_or("");
                        // Stale binary path check: command is bare "lw" or absolute.
                        // If absolute and points somewhere unexpected, warn.
                        let cmd_warn = if cmd != "lw" {
                            let p = std::path::Path::new(cmd);
                            if p.is_absolute() && !p.exists() {
                                Some(format!("MCP command points at non-existent {cmd}"))
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        let label = format!("integration: {id} (MCP)");
                        let detail = format!(
                            "{} entry version={entry_version}",
                            mcp_cfg.key_path
                        );
                        match cmd_warn {
                            Some(w) => out.push(CheckResult::warn(label, w, format!("`lw integrate {id}` to refresh"))),
                            None if entry_version != CURRENT_BINARY_VERSION => out.push(
                                CheckResult::warn(
                                    label,
                                    format!("{detail} (current lw is {CURRENT_BINARY_VERSION})"),
                                    format!("`lw integrate {id}` to refresh"),
                                ),
                            ),
                            None => out.push(CheckResult::ok(label, detail)),
                        }
                    }
                }
                Err(e) => out.push(CheckResult::fail(
                    format!("integration: {id} (MCP)"),
                    format!("{} unparseable: {e}", path.display()),
                    "restore from .bak.* file or repair JSON".into(),
                )),
            }
        }
    }
    if let Some(skills_cfg) = &desc.skills {
        let target = expand_tilde(&skills_cfg.target_dir);
        if !target.exists() && target.symlink_metadata().is_err() {
            out.push(CheckResult::warn(
                format!("integration: {id} (skills)"),
                format!("{} missing", target.display()),
                format!("`lw integrate {id}` to install"),
            ));
        } else {
            // If symlink, verify target resolves
            match target.canonicalize() {
                Ok(_) => out.push(CheckResult::ok(
                    format!("integration: {id} (skills)"),
                    target.display().to_string(),
                )),
                Err(e) => out.push(CheckResult::fail(
                    format!("integration: {id} (skills)"),
                    format!("{} dangling: {e}", target.display()),
                    format!("`lw integrate {id}` to relink"),
                )),
            }
        }
    }
    out
}

fn check_serve_smoke() -> CheckResult {
    use std::process::{Command, Stdio};

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return CheckResult::fail("lw serve smoke", e.to_string(), "reinstall".into()),
    };
    let tmp = match tempfile::TempDir::new() {
        Ok(t) => t,
        Err(e) => return CheckResult::fail("lw serve smoke", e.to_string(), "check disk space".into()),
    };
    // Initialize an empty wiki for the smoke test
    let schema = lw_core::schema::WikiSchema::default();
    if let Err(e) = lw_core::fs::init_wiki(tmp.path(), &schema) {
        return CheckResult::fail("lw serve smoke", e.to_string(), "fs error".into());
    }
    let mut child = match Command::new(&exe)
        .args(["serve", "--root"])
        .arg(tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return CheckResult::fail("lw serve smoke", e.to_string(), "binary unrunnable".into()),
    };

    // Give it 200ms to come up. If it crashed, try_wait returns Some(status).
    std::thread::sleep(Duration::from_millis(200));
    match child.try_wait() {
        Ok(Some(status)) => {
            return CheckResult::fail(
                "lw serve smoke",
                format!("`lw serve` exited immediately with {status}"),
                "check `RUST_LOG=debug lw serve` for the underlying error".into(),
            );
        }
        Ok(None) => {}
        Err(e) => return CheckResult::fail("lw serve smoke", e.to_string(), "process API error".into()),
    }
    let _ = child.kill();
    let _ = child.wait();
    CheckResult::ok("lw serve smoke", "starts and stays up for 200ms".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn with_env<F: FnOnce()>(home: &std::path::Path, f: F) {
        let prev = std::env::var("LW_HOME").ok();
        unsafe { std::env::set_var("LW_HOME", home) };
        f();
        match prev {
            Some(p) => unsafe { std::env::set_var("LW_HOME", p) },
            None => unsafe { std::env::remove_var("LW_HOME") },
        }
    }

    #[test]
    fn check_config_when_absent_is_ok() {
        let home = TempDir::new().unwrap();
        with_env(home.path(), || {
            let r = check_config_loadable();
            assert_eq!(r.status, Status::Ok);
        });
    }

    #[test]
    fn check_current_workspace_when_path_missing_fails() {
        let home = TempDir::new().unwrap();
        let cfg_dir = home.path();
        let cfg = Config {
            workspace: crate::config::WorkspaceState { current: Some("ghost".into()) },
            workspaces: std::collections::BTreeMap::from([
                (
                    "ghost".to_string(),
                    crate::config::WorkspaceEntry { path: PathBuf::from("/does/not/exist/x") },
                ),
            ]),
        };
        cfg.save_to(&cfg_dir.join("config.toml")).unwrap();
        with_env(home.path(), || {
            let r = check_current_workspace();
            assert_eq!(r.status, Status::Fail);
        });
    }

    #[test]
    fn check_version_compat_warns_when_missing() {
        let home = TempDir::new().unwrap();
        with_env(home.path(), || {
            let r = check_version_compat();
            assert_eq!(r.status, Status::Warn);
        });
    }

    #[test]
    fn check_version_compat_fails_when_skewed() {
        let home = TempDir::new().unwrap();
        let v = VersionFile {
            binary: "0.2.0".into(),
            assets: "0.1.0".into(),
            installed_at: "x".into(),
        };
        v.save_to(&home.path().join("version")).unwrap();
        with_env(home.path(), || {
            let r = check_version_compat();
            assert_eq!(r.status, Status::Fail);
        });
    }

    #[test]
    fn check_serve_smoke_passes_with_real_binary() {
        // This test only runs if the dev build has a `lw` binary at target/debug/lw
        // Otherwise it would invoke the test harness binary, which would not be `lw`.
        // We skip by checking current_exe name.
        let exe = std::env::current_exe().unwrap();
        let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !stem.starts_with("lw") {
            // Not running as the lw binary — skip.
            return;
        }
        let r = check_serve_smoke();
        assert_eq!(r.status, Status::Ok);
    }
}
```

- [ ] **Step 2: Add `mod doctor;` to `crates/lw-cli/src/main.rs`**

Insert near other mod declarations (alphabetic order).

- [ ] **Step 3: Run unit tests**

```bash
cargo test -p lw-cli doctor:: -- --test-threads=1
```

Expected: 4 passed, 1 conditionally skipped depending on test binary name.

- [ ] **Step 4: Commit**

```bash
git add crates/lw-cli/src/doctor.rs crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): add lw doctor with full check matrix per spec §12"
```

---

## Task 3: Wire `Doctor` subcommand + integration test

**Files:**

- Modify: `crates/lw-cli/src/main.rs`
- Create: `crates/lw-cli/tests/doctor_cli.rs`

- [ ] **Step 1: Wire subcommand**

In `crates/lw-cli/src/main.rs`, add to `enum Commands`:

```rust
    /// Diagnose installation health (config, integrations, MCP, version skew)
    #[command(after_help = "Examples:\n  lw doctor\n  # Exit 1 if any check fails; suitable for CI.")]
    Doctor,
```

In the `match cli.command`:

```rust
        Commands::Doctor => doctor::run(),
```

- [ ] **Step 2: Build + smoke**

```bash
cargo build -p lw-cli
LW_HOME=/tmp/lw-empty ./target/debug/lw doctor
```

Expected: prints checklist, mostly OKs and a few warns about absent config / version file. Exit 0 because no Fail.

- [ ] **Step 3: Write integration test**

Create `crates/lw-cli/tests/doctor_cli.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn lw(env_home: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("lw").unwrap();
    cmd.env("LW_HOME", env_home);
    cmd.env_remove("LW_WIKI_ROOT");
    cmd
}

#[test]
fn doctor_clean_install_passes() {
    let home = TempDir::new().unwrap();
    // Stage a minimal "installed" layout
    std::fs::create_dir_all(home.path().join("integrations")).unwrap();
    std::fs::create_dir_all(home.path().join("skills/llm-wiki-import")).unwrap();
    std::fs::write(
        home.path().join("skills/llm-wiki-import/SKILL.md"),
        "---\nname: llm-wiki:import\n---\n",
    )
    .unwrap();
    std::fs::write(
        home.path().join("version"),
        format!(
            "binary = \"{}\"\nassets = \"{}\"\ninstalled_at = \"now\"\n",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_VERSION")
        ),
    )
    .unwrap();

    lw(home.path())
        .args(["doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("lw doctor"))
        .stdout(predicate::str::contains("✓"));
}

#[test]
fn doctor_fails_on_version_skew() {
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("integrations")).unwrap();
    std::fs::write(
        home.path().join("version"),
        "binary = \"0.2.0\"\nassets = \"0.1.0\"\ninstalled_at = \"now\"\n",
    )
    .unwrap();

    lw(home.path())
        .args(["doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("✗"));
}

#[test]
fn doctor_fails_on_missing_workspace_path() {
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("integrations")).unwrap();
    std::fs::write(
        home.path().join("config.toml"),
        r#"[workspace]
current = "ghost"

[workspaces.ghost]
path = "/does/not/exist/abc123"
"#,
    )
    .unwrap();

    lw(home.path())
        .args(["doctor"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("path does not exist"));
}
```

- [ ] **Step 4: Run integration test**

```bash
cargo test -p lw-cli --test doctor_cli -- --test-threads=1
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/lw-cli/src/main.rs crates/lw-cli/tests/doctor_cli.rs
git commit -m "feat(lw-cli): wire Doctor subcommand + e2e integration tests"
```

---

## Task 4: README rewrite (llm-wiki repo)

**Files:**

- Modify: `README.md` (in the llm-wiki repo root)

- [ ] **Step 1: Rewrite README per spec §13**

Replace `README.md` (in the llm-wiki repo, i.e. `tool/llm-wiki/README.md` from mono's perspective) with:

````markdown
# llm-wiki

A CLI + skill bundle that turns any markdown folder into an agent-driven knowledge base. Bring your own data; bring your own agent tool. Built for Claude Code, Codex, and OpenClaw.

> Why not just `markdown folder + filesystem MCP`? llm-wiki adds scope-gated ingest (`SCOPE.md` controls what gets in), wiki-aware operations (lint, freshness, orphans, broken links via git), and curated skills that turn import + curate into one-shot agent actions instead of N-step manual prompting.

## Install

```bash
curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/latest/download/install.sh | sh
```
````

The installer fetches the matching prebuilt binary for your platform (macOS / Linux, x86_64 / aarch64), verifies sha256, and stages everything under `~/.llm-wiki/`. Pin a version: `... releases/download/v0.2.0/install.sh`. CI / unattended? Append `--no-integrate` (default for non-TTY) or `--yes` (auto-integrate detected agent tools).

## First vault, with a template

```bash
lw workspace add my-research ~/wiki/research --template research-papers
lw integrate --auto
```

The `--template` flag copies a starter vault (3 templates ship: `general`, `research-papers`, `engineering-notes`), each with a `SCOPE.md` and a category schema you can edit. `lw integrate --auto` detects installed agent tools and wires up the MCP server + canonical skill.

## Daily use

Open your agent tool inside the vault. The `llm-wiki:import` skill triggers on phrases like "add this to my wiki" or "save this article":

```
You: add this to my wiki — https://arxiv.org/abs/2501.12345
Agent: [fetches paper, reads SCOPE.md, judges fit]
       Fits the "research papers" scope. Filing under raw/papers/2501.12345.md.
       Want me to draft a wiki page summary too?
```

Out-of-scope content prompts before being silently dropped:

```
You: add my grocery list to the wiki
Agent: This looks out of scope (vault Purpose: ML research papers). Add anyway, or skip?
```

## Multiple vaults (Obsidian-style, optional)

```bash
lw workspace add personal ~/notes/personal --template general
lw workspace use personal      # current
lw workspace use my-research   # switch
lw workspace list
```

Switching vaults requires restarting your agent tool — the MCP server binds the current vault at launch (so an in-flight session can't silently flip mid-conversation).

## Update

```bash
lw upgrade --check    # exit 1 if newer release exists
lw upgrade            # apply
```

## Troubleshoot

```bash
lw doctor             # one-shot health check (config, integrations, MCP, version compat)
lw workspace current -v   # show full root-resolution chain
```

Backups from MCP config writes land at `<config>.bak.<timestamp>` next to the original file. The uninstaller (`lw uninstall`) preserves your vault directories and saves `~/.llm-wiki/config.toml` to `~/.llm-wiki.config.toml.bak.<timestamp>` so reinstalls can restore vault registrations.

## Architecture

- `lw-core` — wiki I/O, search (Tantivy), lint
- `lw-mcp` — MCP server library
- `lw-cli` — `lw` binary (umbrella for workspace / integrate / upgrade / uninstall / doctor + the original wiki commands)
- `skills/llm-wiki-import/` — canonical agent skill
- `templates/` — starter vaults
- `integrations/` — TOML descriptors per agent tool

Tool is open source; wiki content repos are private (BYO).

## Status

- 1.0: Claude Code (full), Codex (skills + manual MCP), OpenClaw (skill-only)
- 1.1 roadmap: Codex MCP wiring, OpenClaw MCP wiring, `llm-wiki:curate` skill, more templates

## License

MIT.

````

- [ ] **Step 2: Verify README renders**

```bash
# Sanity check: must mention install, workspace, integrate, upgrade, doctor, uninstall.
for k in install workspace integrate upgrade doctor uninstall; do
  grep -q "lw $k" README.md && echo "  $k mention OK"
done
````

Expected: all 6 confirmations.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs(README): rewrite for product wrapper user journey per spec §13"
```

---

## Task 5: Mono repo + agents repo README updates (out-of-tree)

These edits happen in **different repos** (not the llm-wiki product repo). Push through the existing PR flow for those repos. Do them in this order so the install path works before mono points users at it:

- [ ] **Step 1: Confirm Plan C release tag exists**

```bash
gh release list --repo Pawpaw-Technology/llm-wiki | head -1
```

Expected: at least `v0.2.0` (or whatever Plan C's first release was). If nothing released yet, **stop here and finish Plan C** before continuing.

- [ ] **Step 2: Edit mono README**

In `llm-wiki-mono` repo (path on disk: `/Users/vergil/Devwork/homebrew/llm-wiki-mono` or the mono root):

Replace the top-level "Install" section of `README.md` (and anywhere else install instructions live) with:

```markdown
## Install (end users)

This repo is the development workspace for the llm-wiki ecosystem. **End users should not clone this repo.** Install from the product:

\`\`\`bash
curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/latest/download/install.sh | sh
\`\`\`

This monorepo continues to exist for **dev-time context engineering** (co-locating tool + agents + codebridge for navigation), not as the install surface.
```

(Replace the literal `\` with backslash; the markdown above is showing escaped fences inside the plan.)

Commit in the mono repo:

```bash
git add README.md
git commit -m "docs: redirect install to llm-wiki curl line; mono is dev workspace only"
git push
```

- [ ] **Step 3: Edit agents repo README**

In `llm-wiki-agents` repo (path on disk: `tool/.../agents` from mono, or its own clone):

Add to the top of `README.md`:

```markdown
## Prerequisites

The `lw` CLI must be installed and on PATH:

\`\`\`bash
curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/latest/download/install.sh | sh
\`\`\`

This package is the **batch / cron orchestration layer** that calls `lw` for bulk operations. End users typically interact via the agent tool (Claude Code / Codex / OpenClaw); this layer is for scheduled jobs (daily ingest, weekly lint, etc.).
```

Commit + push in agents repo.

- [ ] **Step 4: Verify**

```bash
# In mono:
grep -A3 "## Install" /Users/vergil/Devwork/homebrew/llm-wiki-mono/README.md
# In agents:
grep "curl -fsSL" tool/llm-wiki-mono/agents/README.md   # adjust path
```

Expected: each shows the curl line.

---

## Task 6: 1.0 gate execution — install / behavior / uninstall smoke tests

This task is **manual + scripted** and produces transcripts that go in the repo as evidence per spec §14.

**Files (created during execution):**

- `docs/smoke-tests/v1.0.0/install-linux.log`
- `docs/smoke-tests/v1.0.0/install-darwin.log`
- `docs/smoke-tests/v1.0.0/behavior-claude-code.md`
- `docs/smoke-tests/v1.0.0/behavior-codex.md`
- `docs/smoke-tests/v1.0.0/behavior-openclaw.md`
- `docs/smoke-tests/v1.0.0/uninstall.log`

- [ ] **Step 1: Cut a candidate release**

Tag `v1.0.0-rc.1` (or next available rc) and push:

```bash
git tag v1.0.0-rc.1
git push origin v1.0.0-rc.1
```

Wait for `release.yml` to finish; verify all 4 platform tarballs + install.sh + sha256 are uploaded:

```bash
gh release view v1.0.0-rc.1
```

Expected: 11+ assets (4 tarballs × 2 (file + sha) + install.sh + uninstall.sh + their sha256).

- [ ] **Step 2: Linux install smoke (containerized)**

```bash
docker build -f installer/Dockerfile.test-linux -t lw-installer-test .
docker run --rm -e LW_VERSION=v1.0.0-rc.1 lw-installer-test 2>&1 | tee /tmp/install-linux.log
```

Expected: ends with `=== ALL SMOKE TESTS PASSED ===`. If not: fix the issue, cut `v1.0.0-rc.2`, retry.

Save the log:

```bash
mkdir -p docs/smoke-tests/v1.0.0
cp /tmp/install-linux.log docs/smoke-tests/v1.0.0/install-linux.log
```

- [ ] **Step 3: Darwin install smoke (manual on a clean macOS user account)**

Create a fresh macOS user account (System Settings → Users), log in, open Terminal:

```bash
LW_VERSION=v1.0.0-rc.1 sh -c "$(curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/download/v1.0.0-rc.1/install.sh)" 2>&1 | tee /tmp/install-darwin.log
exec $SHELL
lw --version
lw workspace add demo ~/demo-vault --template general
lw doctor
```

Capture the log; copy back to dev machine and place at `docs/smoke-tests/v1.0.0/install-darwin.log`.

- [ ] **Step 4: Behavior smoke — Claude Code**

In a fresh Claude Code session inside the demo vault (created in Step 2 or 3):

1. Test in-scope import:
   - Prompt: `add this to my wiki — https://arxiv.org/abs/2305.10601`
   - Expected: agent invokes `wiki_ingest`, files under `raw/`, prints confirmation.
2. Test out-of-scope:
   - Prompt: `add my shopping list (eggs, milk, bread) to the wiki`
   - Expected: agent asks for confirmation since it doesn't fit "general"/"research" scope, does NOT silent-import.

Capture the conversation (copy the full transcript from Claude Code's UI) and write to `docs/smoke-tests/v1.0.0/behavior-claude-code.md` with format:

```markdown
# Claude Code — llm-wiki:import smoke test (v1.0.0-rc.1)

Date: <date>
Vault: ~/demo-vault (template: general)
Tool: Claude Code <version>

## Test 1: in-scope URL

> User: add this to my wiki — https://arxiv.org/abs/2305.10601

[paste full transcript of agent response and tool calls]

## Test 2: out-of-scope item

> User: add my shopping list...

[paste full transcript]

## Result

- [ ] Test 1: in-scope import → file written to raw/papers/...
- [ ] Test 2: out-of-scope → agent asked for confirmation, did not silent-import

PASS / FAIL / DOWNGRADED
```

Mark the result.

- [ ] **Step 5: Behavior smoke — Codex**

Repeat Step 4 in a fresh Codex session. Note: in 1.0 the Codex MCP entry must be wired manually (per `integrations/codex.toml` comment). Document the manual step in the transcript:

```markdown
## Setup notes (1.0 limitation)

Codex MCP wiring is manual in 1.0. Prior to this test we ran:
\`\`\`
echo '[mcp_servers.llm-wiki]
command = "lw"
args = ["serve"]
' >> ~/.codex/config.toml
\`\`\`
```

If the skill cannot be triggered because Codex doesn't expose a skill concept the way Claude Code does, mark `DOWNGRADED` and document what works (e.g., the import flow works manually by quoting the SKILL.md prompt) — per spec §14 step 5 "Tools where the skill prompt cannot be triggered faithfully ... downgrade to documented limitation in 1.0".

- [ ] **Step 6: Behavior smoke — OpenClaw**

Repeat Step 4 in OpenClaw (skill-only path; per Task 1, no MCP wiring in 1.0). The skill is dropped into `~/.openclaw/skills/llm-wiki/`. If OpenClaw exposes the skill content to its agent, run the two tests; if it doesn't (skill-only means user manually invokes the prompt), document that limitation and mark `DOWNGRADED`.

- [ ] **Step 7: Uninstall smoke**

On Linux container:

```bash
docker run --rm -e LW_VERSION=v1.0.0-rc.1 --entrypoint /bin/sh lw-installer-test -c "
  /home/tester/test-install.sh && \
  echo '--- uninstall ---' && \
  sh \$HOME/.llm-wiki/installer/uninstall.sh --yes && \
  [ ! -d \$HOME/.llm-wiki ] && echo OK && \
  [ -d \$HOME/demo-vault ] && echo VAULT_PRESERVED
" 2>&1 | tee /tmp/uninstall.log
```

Expected: ends with `OK` then `VAULT_PRESERVED`.

Save: `cp /tmp/uninstall.log docs/smoke-tests/v1.0.0/uninstall.log`.

- [ ] **Step 8: Commit transcripts**

```bash
git add docs/smoke-tests/v1.0.0/
git commit -m "test(smoke): v1.0.0-rc.1 install/behavior/uninstall transcripts across 3 tools"
```

- [ ] **Step 9: 1.0 gate decision**

Open the spec § "1.0 gate (composite)" and tick each:

- (install) `lw doctor` green on macOS + Linux for Claude Code, Codex, OpenClaw
- (behavior) `llm-wiki:import` end-to-end transcript per tool, with in-scope / out-of-scope cases both behaving correctly (PASS or documented DOWNGRADED with reason)
- (uninstall) `lw uninstall` reverses, leaves vault data untouched

If all three are green (or documented downgrades), proceed to Task 7. If any **regress** (not just downgrade) — fix and cut the next rc.

---

## Task 7: Tag and ship 1.0.0

**Files:** None (release operation)

- [ ] **Step 1: Update version in workspace Cargo.toml**

If the rc was on a different version field, bump to the final:

```bash
# Edit Cargo.toml workspace.package.version → "1.0.0"
sed -i.bak 's/version.workspace = true/version.workspace = true/' crates/*/Cargo.toml  # no-op if already
# Edit the workspace Cargo.toml manually:
#   [workspace.package]
#   version = "1.0.0"
cargo build --release -p lw-cli   # verify
```

- [ ] **Step 2: Commit version bump**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to 1.0.0"
```

- [ ] **Step 3: Tag and push**

```bash
git tag v1.0.0
git push origin main
git push origin v1.0.0
```

- [ ] **Step 4: Verify release**

```bash
gh release view v1.0.0
```

Expected: full asset list per Plan C release.yml (4 tarballs + install.sh + uninstall.sh + sha256 each).

Edit release notes to include link to the smoke-test transcripts:

```bash
gh release edit v1.0.0 --notes "$(cat <<'EOF'
First general-availability release.

## Quick start
\`\`\`bash
curl -fsSL https://github.com/Pawpaw-Technology/llm-wiki/releases/latest/download/install.sh | sh
\`\`\`

## Cross-tool support (1.0)
- Claude Code — full (MCP + skills)
- Codex — skills + manual MCP setup (auto MCP wiring in 1.1)
- OpenClaw — skills only (MCP wiring evaluated for 1.1)

## Verified by
Smoke transcripts in [docs/smoke-tests/v1.0.0/](https://github.com/Pawpaw-Technology/llm-wiki/tree/main/docs/smoke-tests/v1.0.0).

## Roadmap
See spec §15 in repo for deferred items (Homebrew, cargo install, signed releases, curate skill, more templates).
EOF
)"
```

- [ ] **Step 5: Post-release: edit mono README + agents README to reference v1.0.0**

In mono and agents repos, replace `releases/latest` references in the install line with `releases/download/v1.0.0` if you want pinned, or leave as `latest` (recommended for evergreen). Push.

---

## Known 1.0 limitations (deferred to 1.1)

Two spec items intentionally **not** implemented in this plan; document in release notes and revisit:

1. **Running `lw serve` mismatch detection** (spec §4.1, §12). Cross-platform process introspection (parse `ps` for live `lw serve --root <X>` whose `<X>` differs from the current registered workspace) is non-trivial and platform-specific. For 1.0, `lw doctor` documents the behavior in its output ("note: running MCP processes bind their vault at launch — restart your agent if you switched"). 1.1 will add the actual scan.
2. **Startup upgrade nudge** (spec §11). The 24h-cached "newer release available" hint on `lw` startup is not wired in 1.0 — `lw upgrade --check` is the explicit workaround. 1.1 will add the cached background check, suppressed in non-TTY / `CI=1` / `LW_NO_NUDGE=1` per spec.

---

## Done criteria

- `integrations/codex.toml` and `integrations/openclaw.toml` shipped, both pass parsing and `lw integrate <tool>` runs without error
- `lw doctor` implements all checks from spec §12, exits non-zero on Fail, prints checklist with remediation
- README rewritten per spec §13 (install / first-vault / daily-use / multi-vault / upgrade / troubleshoot / architecture / status / license)
- Mono repo README install-surface removed; replaced with redirect to curl line
- Agents repo README updated to reference new install
- 1.0 composite gate executed: install (containerized Linux + manual darwin), behavior (transcripts per tool, downgrades documented), uninstall (containerized) — transcripts committed under `docs/smoke-tests/v1.0.0/`
- `v1.0.0` tag pushed; release notes link transcripts and document tool-tier matrix
- All Rust tests green, clippy clean, fmt clean
