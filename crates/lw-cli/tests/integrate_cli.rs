use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn lw(
    env_home: &std::path::Path,
    integrations: &std::path::Path,
    skills: &std::path::Path,
) -> Command {
    let mut cmd = Command::cargo_bin("lw").unwrap();
    cmd.env("LW_HOME", env_home);
    cmd.env("LW_INTEGRATIONS_DIR", integrations);
    cmd.env("LW_SKILLS_DIR", skills);
    cmd.env_remove("LW_WIKI_ROOT");
    cmd
}

fn make_descriptor(integrations: &std::path::Path, fake_home: &std::path::Path) {
    std::fs::create_dir_all(integrations).unwrap();
    let claude_dir = fake_home.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let settings_path = claude_dir.join("settings.json");
    let skills_target = claude_dir.join("skills/llm-wiki/");

    let toml = format!(
        r#"
name = "Claude Code"

[detect]
config_dir = "{}"

[mcp]
config_path = "{}"
format = "json"
key_path = "mcpServers.llm-wiki"
command = "lw"
args = ["serve"]

[skills]
target_dir = "{}"
mode = "symlink"
"#,
        claude_dir.display(),
        settings_path.display(),
        skills_target.display()
    );
    std::fs::write(integrations.join("claude-code.toml"), toml).unwrap();
}

fn make_skills(skills_dir: &std::path::Path) {
    let s = skills_dir.join("llm-wiki-import");
    std::fs::create_dir_all(&s).unwrap();
    std::fs::write(s.join("SKILL.md"), "---\nname: llm-wiki:import\n---\nbody").unwrap();
}

#[test]
#[serial_test::serial]
fn integrate_install_writes_settings_and_links_skills() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    make_descriptor(integrations.path(), fake_home.path());
    make_skills(skills.path());

    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code"])
        .assert()
        .success();

    let settings_path = fake_home.path().join(".claude/settings.json");
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(settings["mcpServers"]["llm-wiki"]["command"], "lw");
    assert_eq!(
        settings["mcpServers"]["llm-wiki"]["args"],
        serde_json::json!(["serve"])
    );
    assert!(settings["mcpServers"]["llm-wiki"]["_lw_version"].is_string());

    let skills_link = fake_home.path().join(".claude/skills/llm-wiki/");
    assert!(skills_link.join("llm-wiki-import/SKILL.md").exists());
}

#[test]
#[serial_test::serial]
fn integrate_uninstall_removes_entry_and_skills() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    make_descriptor(integrations.path(), fake_home.path());
    make_skills(skills.path());

    // Install first
    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code"])
        .assert()
        .success();
    // Then uninstall
    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code", "--uninstall"])
        .assert()
        .success();

    let settings_path = fake_home.path().join(".claude/settings.json");
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert!(settings["mcpServers"].get("llm-wiki").is_none());
    assert!(!fake_home.path().join(".claude/skills/llm-wiki/").exists());
}

#[test]
#[serial_test::serial]
fn integrate_preserves_other_mcp_entries() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    make_descriptor(integrations.path(), fake_home.path());
    make_skills(skills.path());

    let settings_path = fake_home.path().join(".claude/settings.json");
    std::fs::write(
        &settings_path,
        r#"{"mcpServers":{"other":{"command":"other"}},"permissions":{"allow":["foo"]}}"#,
    )
    .unwrap();

    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code"])
        .assert()
        .success();

    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(settings["mcpServers"]["other"]["command"], "other");
    assert_eq!(settings["permissions"]["allow"], serde_json::json!(["foo"]));
    assert_eq!(settings["mcpServers"]["llm-wiki"]["command"], "lw");

    // Backup must exist
    let entries: Vec<_> = std::fs::read_dir(fake_home.path().join(".claude"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.contains(".bak."))
        .collect();
    assert!(!entries.is_empty(), "expected at least one backup file");
}

/// Criterion 4 (end-to-end): install → user edits entry → uninstall leaves it alone.
///
/// The managed entry is installed, then the user modifies it by adding an `env`
/// field. `lw integrate --uninstall` must preserve the entry and emit a warning
/// to stderr. Exit code is still 0.
#[test]
#[serial_test::serial]
fn integrate_uninstall_preserves_user_edited_mcp_entry() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    make_descriptor(integrations.path(), fake_home.path());
    make_skills(skills.path());

    // Install the managed entry.
    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code"])
        .assert()
        .success();

    // Simulate the user adding a custom `env` field to the entry.
    let settings_path = fake_home.path().join(".claude/settings.json");
    let mut settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    settings["mcpServers"]["llm-wiki"]["env"] =
        serde_json::json!({"LW_WIKI_ROOT": "/my/custom/wiki"});
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).unwrap(),
    )
    .unwrap();

    // Uninstall must exit 0 and warn, but must NOT remove the entry.
    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code", "--uninstall"])
        .assert()
        .success()
        .stderr(predicate::str::contains("user-edited"));

    // Entry is still present with the custom field intact.
    let settings_after: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(
        settings_after["mcpServers"]["llm-wiki"]["command"], "lw",
        "command must still be present"
    );
    assert_eq!(
        settings_after["mcpServers"]["llm-wiki"]["env"]["LW_WIKI_ROOT"], "/my/custom/wiki",
        "user-added env field must survive uninstall"
    );
}

/// Build a descriptor TOML that declares strong detection (binary + version_cmd).
fn make_strong_descriptor(
    integrations: &std::path::Path,
    fake_home: &std::path::Path,
    id: &str,
    binary: &str,
    version_cmd: &[&str],
) {
    std::fs::create_dir_all(integrations).unwrap();
    let cfg_dir = fake_home.join(format!(".{id}"));
    std::fs::create_dir_all(&cfg_dir).unwrap();
    let settings_path = cfg_dir.join("settings.json");
    let skills_target = cfg_dir.join("skills/llm-wiki/");

    let version_cmd_toml = version_cmd
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let toml = format!(
        r#"
name = "Fake {id}"

[detect]
config_dir = "{}"
binary = "{binary}"
version_cmd = [{version_cmd_toml}]

[mcp]
config_path = "{}"
format = "json"
key_path = "mcpServers.llm-wiki"
command = "lw"
args = ["serve"]

[skills]
target_dir = "{}"
mode = "symlink"
"#,
        cfg_dir.display(),
        settings_path.display(),
        skills_target.display()
    );
    std::fs::write(integrations.join(format!("{id}.toml")), toml).unwrap();
}

/// Drop a shell script at `bin_dir/name` that exits with `exit_code`, and mark it executable.
fn stage_fake_binary(bin_dir: &std::path::Path, name: &str, exit_code: i32) {
    std::fs::create_dir_all(bin_dir).unwrap();
    let path = bin_dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\nexit {exit_code}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
}

#[test]
#[serial_test::serial]
fn integrate_auto_skips_when_binary_missing_from_path() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    let fake_path = TempDir::new().unwrap(); // empty PATH — no binaries at all

    make_strong_descriptor(
        integrations.path(),
        fake_home.path(),
        "ghostbin",
        "lw-probe-ghost-zzz",
        &["--version"],
    );
    make_skills(skills.path());

    lw(env_home.path(), integrations.path(), skills.path())
        .env("PATH", fake_path.path())
        .args(["integrate", "--auto", "--yes"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("lw-probe-ghost-zzz")
                .and(predicate::str::contains("not").and(predicate::str::contains("PATH"))),
        );

    // Must NOT have installed the MCP entry.
    let settings_path = fake_home.path().join(".ghostbin/settings.json");
    assert!(
        !settings_path.exists(),
        "MCP config should not be written when binary is not detected"
    );
}

#[test]
#[serial_test::serial]
fn integrate_auto_skips_when_version_probe_fails() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    let fake_path = TempDir::new().unwrap();

    stage_fake_binary(fake_path.path(), "brokenbin", 1);
    make_strong_descriptor(
        integrations.path(),
        fake_home.path(),
        "brokenbin",
        "brokenbin",
        &["--version"],
    );
    make_skills(skills.path());

    lw(env_home.path(), integrations.path(), skills.path())
        .env("PATH", fake_path.path())
        .args(["integrate", "--auto", "--yes"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("brokenbin")
                .and(predicate::str::contains("version").or(predicate::str::contains("probe"))),
        );

    let settings_path = fake_home.path().join(".brokenbin/settings.json");
    assert!(!settings_path.exists());
}

#[test]
#[serial_test::serial]
fn integrate_auto_installs_when_binary_and_version_ok() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    let fake_path = TempDir::new().unwrap();

    stage_fake_binary(fake_path.path(), "goodbin", 0);
    make_strong_descriptor(
        integrations.path(),
        fake_home.path(),
        "goodbin",
        "goodbin",
        &["--version"],
    );
    make_skills(skills.path());

    lw(env_home.path(), integrations.path(), skills.path())
        .env("PATH", fake_path.path())
        .args(["integrate", "--auto", "--yes"])
        .assert()
        .success();

    let settings_path = fake_home.path().join(".goodbin/settings.json");
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(settings["mcpServers"]["llm-wiki"]["command"], "lw");
}

#[test]
#[serial_test::serial]
fn integrate_auto_with_no_tools_succeeds() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    // Descriptor present but its detect.config_dir does NOT exist
    let phantom = fake_home.path().join("never-installed");
    std::fs::create_dir_all(integrations.path()).unwrap();
    let toml = format!(
        r#"
name = "Phantom"

[detect]
config_dir = "{}"

[mcp]
config_path = "{}"
format = "json"
key_path = "mcpServers.llm-wiki"
command = "lw"
args = ["serve"]

[skills]
target_dir = "{}"
mode = "symlink"
"#,
        phantom.display(),
        phantom.join("settings.json").display(),
        phantom.join("skills/").display()
    );
    std::fs::write(integrations.path().join("phantom.toml"), toml).unwrap();
    make_skills(skills.path());

    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "--auto", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No supported agent tools detected",
        ));
}
