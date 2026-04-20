use crate::config::{Config, config_path};
use crate::integrations::{
    descriptor::{Descriptor, McpConfig, SkillsConfig, expand_tilde},
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

    pub fn warn(
        label: impl Into<String>,
        detail: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            status: Status::Warn,
            detail: Some(detail.into()),
            remediation: Some(remediation.into()),
        }
    }

    pub fn fail(
        label: impl Into<String>,
        detail: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            status: Status::Fail,
            detail: Some(detail.into()),
            remediation: Some(remediation.into()),
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let mut results = vec![
        check_binary(),
        check_path_env(),
        check_config_loadable(),
        check_current_workspace(),
        check_version_compat(),
    ];
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
    println!(
        "{} passed, {} warned, {} failed",
        results.len() - fails - warns,
        warns,
        fails
    );
}

// --- Individual checks ---------------------------------------------------

fn check_binary() -> CheckResult {
    match std::env::current_exe() {
        Ok(p) => CheckResult::ok(
            "binary location",
            format!("{} (v{CURRENT_BINARY_VERSION})", p.display()),
        ),
        Err(e) => CheckResult::fail(
            "binary location",
            e.to_string(),
            "reinstall via curl install.sh",
        ),
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
        Some(p) => path_var
            .split(':')
            .any(|seg| std::path::Path::new(seg) == p),
        None => false,
    };

    match (prefix, in_path) {
        (Some(p), true) => CheckResult::ok("PATH includes lw bin", p.display().to_string()),
        (Some(p), false) => CheckResult::warn(
            "PATH includes lw bin",
            format!("{} not in PATH", p.display()),
            "open a new shell, or `source ~/.zshrc` (or your rc file)",
        ),
        (None, _) => CheckResult::fail(
            "PATH includes lw bin",
            "cannot resolve home dir",
            "set HOME or LW_INSTALL_PREFIX",
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
            "edit by hand or `lw workspace remove` and re-add",
        ),
    }
}

fn check_current_workspace() -> CheckResult {
    let path = match config_path() {
        Ok(p) => p,
        Err(_) => return CheckResult::ok("current workspace", "skipped (no config dir)"),
    };
    let cfg = match Config::load_from(&path) {
        Ok(c) => c,
        Err(_) => return CheckResult::ok("current workspace", "skipped (config unparseable)"),
    };
    match cfg.workspace.current.as_deref() {
        None => CheckResult::ok("current workspace", "(none — using cwd auto-discover)"),
        Some(name) => match cfg.workspaces.get(name) {
            Some(entry) if entry.path.exists() => CheckResult::ok(
                "current workspace",
                format!("{name} → {}", entry.path.display()),
            ),
            Some(entry) => CheckResult::fail(
                "current workspace",
                format!("{name} → {} (path does not exist)", entry.path.display()),
                format!(
                    "`lw workspace use <other>` or recreate {}",
                    entry.path.display()
                ),
            ),
            None => CheckResult::fail(
                "current workspace",
                format!("'{name}' set as current but missing from workspaces table"),
                "edit ~/.llm-wiki/config.toml or remove and re-add",
            ),
        },
    }
}

fn check_version_compat() -> CheckResult {
    let path = match version_file_path() {
        Ok(p) => p,
        Err(_) => return CheckResult::warn("version file", "skipped", "set HOME"),
    };
    let v = match VersionFile::load_from(&path) {
        Ok(v) => v,
        Err(e) => return CheckResult::fail("version file", e.to_string(), "reinstall"),
    };
    if v.binary.is_empty() && v.assets.is_empty() {
        return CheckResult::warn(
            "version file",
            "missing — pre-installer setup or partial install",
            "run install.sh",
        );
    }
    if !v.is_compatible() {
        return CheckResult::fail(
            "binary/assets version",
            format!("binary={}, assets={} (mismatch)", v.binary, v.assets),
            "run `lw upgrade` to realign",
        );
    }
    CheckResult::ok("binary/assets version", format!("both at {}", v.binary))
}

fn check_integrations() -> Vec<CheckResult> {
    let mut out = Vec::new();
    match integrations_root() {
        Ok(r) if r.exists() => {}
        Ok(r) => {
            out.push(CheckResult::warn(
                "integrations",
                format!("integrations dir not found at {}", r.display()),
                "run `lw upgrade` (or reinstall via curl install.sh) to restore",
            ));
            return out;
        }
        Err(e) => {
            out.push(CheckResult::warn(
                "integrations",
                format!("cannot resolve integrations dir: {e}"),
                "set $LW_HOME or reinstall",
            ));
            return out;
        }
    }
    let descriptors = match load_all() {
        Ok(d) => d,
        Err(e) => {
            out.push(CheckResult::warn(
                "integrations",
                e.to_string(),
                "reinstall to restore integrations dir",
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
        out.push(check_mcp(id, mcp_cfg));
    }
    if let Some(skills_cfg) = &desc.skills {
        out.push(check_skills(id, skills_cfg));
    }
    out
}

fn check_mcp(id: &str, mcp_cfg: &McpConfig) -> CheckResult {
    let label = format!("integration: {id} (MCP)");
    let path = expand_tilde(&mcp_cfg.config_path);
    if !path.exists() {
        return CheckResult::warn(
            label,
            format!("{} missing", path.display()),
            format!("`lw integrate {id}` to install"),
        );
    }
    let cfg = match std::fs::read_to_string(&path).and_then(|s| {
        serde_json::from_str::<Value>(&s).map_err(|e| std::io::Error::other(e.to_string()))
    }) {
        Ok(cfg) => cfg,
        Err(e) => {
            return CheckResult::fail(
                label,
                format!("{} unparseable: {e}", path.display()),
                "restore from .bak.* file or repair JSON",
            );
        }
    };

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
        return CheckResult::warn(
            label,
            format!("entry {} not present", mcp_cfg.key_path),
            format!("`lw integrate {id}` to install"),
        );
    }

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
    let detail = format!("{} entry version={entry_version}", mcp_cfg.key_path);
    match cmd_warn {
        Some(w) => CheckResult::warn(label, w, format!("`lw integrate {id}` to refresh")),
        None if entry_version != CURRENT_BINARY_VERSION => CheckResult::warn(
            label,
            format!("{detail} (current lw is {CURRENT_BINARY_VERSION})"),
            format!("`lw integrate {id}` to refresh"),
        ),
        None => CheckResult::ok(label, detail),
    }
}

fn check_skills(id: &str, skills_cfg: &SkillsConfig) -> CheckResult {
    let label = format!("integration: {id} (skills)");
    let target = expand_tilde(&skills_cfg.target_dir);
    if !target.exists() && target.symlink_metadata().is_err() {
        return CheckResult::warn(
            label,
            format!("{} missing", target.display()),
            format!("`lw integrate {id}` to install"),
        );
    }
    // If symlink, verify target resolves
    match target.canonicalize() {
        Ok(_) => CheckResult::ok(label, target.display().to_string()),
        Err(e) => CheckResult::fail(
            label,
            format!("{} dangling: {e}", target.display()),
            format!("`lw integrate {id}` to relink"),
        ),
    }
}

fn check_serve_smoke() -> CheckResult {
    use std::process::{Command, Stdio};

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return CheckResult::fail("lw serve smoke", e.to_string(), "reinstall"),
    };
    let tmp = match tempfile::TempDir::new() {
        Ok(t) => t,
        Err(e) => return CheckResult::fail("lw serve smoke", e.to_string(), "check disk space"),
    };
    // Initialize an empty wiki for the smoke test
    let schema = lw_core::schema::WikiSchema::default();
    if let Err(e) = lw_core::fs::init_wiki(tmp.path(), &schema) {
        return CheckResult::fail("lw serve smoke", e.to_string(), "fs error");
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
        Err(e) => return CheckResult::fail("lw serve smoke", e.to_string(), "binary unrunnable"),
    };

    // Give it 200ms to come up. If it crashed, try_wait returns Some(status).
    std::thread::sleep(Duration::from_millis(200));
    match child.try_wait() {
        Ok(Some(status)) => {
            return CheckResult::fail(
                "lw serve smoke",
                format!("`lw serve` exited immediately with {status}"),
                "check `RUST_LOG=debug lw serve` for the underlying error",
            );
        }
        Ok(None) => {}
        Err(e) => return CheckResult::fail("lw serve smoke", e.to_string(), "process API error"),
    }
    let _ = child.kill();
    let _ = child.wait();
    CheckResult::ok("lw serve smoke", "starts and stays up for 200ms")
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
    #[serial_test::serial]
    fn check_config_when_absent_is_ok() {
        let home = TempDir::new().unwrap();
        with_env(home.path(), || {
            let r = check_config_loadable();
            assert_eq!(r.status, Status::Ok);
        });
    }

    #[test]
    #[serial_test::serial]
    fn check_current_workspace_when_path_missing_fails() {
        let home = TempDir::new().unwrap();
        let cfg_dir = home.path();
        let cfg = Config {
            workspace: crate::config::WorkspaceState {
                current: Some("ghost".into()),
            },
            workspaces: std::collections::BTreeMap::from([(
                "ghost".to_string(),
                crate::config::WorkspaceEntry {
                    path: PathBuf::from("/does/not/exist/x"),
                },
            )]),
        };
        cfg.save_to(&cfg_dir.join("config.toml")).unwrap();
        with_env(home.path(), || {
            let r = check_current_workspace();
            assert_eq!(r.status, Status::Fail);
        });
    }

    #[test]
    #[serial_test::serial]
    fn check_version_compat_warns_when_missing() {
        let home = TempDir::new().unwrap();
        with_env(home.path(), || {
            let r = check_version_compat();
            assert_eq!(r.status, Status::Warn);
        });
    }

    #[test]
    #[serial_test::serial]
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
    #[serial_test::serial]
    fn check_serve_smoke_passes_with_real_binary() {
        // This test only runs if the dev build has a `lw` binary at target/debug/lw.
        // Under `cargo test` the current_exe is a test harness (e.g.,
        // `target/debug/deps/lw-<hash>` for the bin crate, or
        // `target/debug/deps/doctor_cli-<hash>` for integration tests) — those
        // would not respond to `lw serve` and would crash immediately. Skip
        // unless the exe stem is exactly "lw".
        let exe = std::env::current_exe().unwrap();
        let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem != "lw" {
            eprintln!(
                "SKIPPED: check_serve_smoke_passes_with_real_binary requires running the lw binary directly (use the doctor_cli e2e test instead)"
            );
            return;
        }
        let r = check_serve_smoke();
        assert_eq!(r.status, Status::Ok);
    }
}
