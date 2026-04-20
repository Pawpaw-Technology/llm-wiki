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
#[serial_test::serial]
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
#[serial_test::serial]
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
#[serial_test::serial]
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

#[test]
#[serial_test::serial]
fn doctor_serve_smoke_passes_via_cargo_bin() {
    let home = TempDir::new().unwrap();
    // Stage minimal install so doctor doesn't fail on other checks
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
        .stdout(predicate::str::contains("lw serve smoke"))
        .stdout(predicate::str::contains("starts and stays up"));
}
