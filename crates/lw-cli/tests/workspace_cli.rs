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
#[serial_test::serial]
fn full_workspace_lifecycle() {
    let home = TempDir::new().unwrap();
    let vault_a = TempDir::new().unwrap();
    let vault_b = TempDir::new().unwrap();

    // Add first workspace, init the wiki structure
    lw(home.path())
        .args([
            "workspace",
            "add",
            "alpha",
            vault_a.path().to_str().unwrap(),
            "--init",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Added workspace 'alpha'"))
        .stdout(predicate::str::contains("set as current"));

    assert!(vault_a.path().join(".lw/schema.toml").exists());

    // Add second
    lw(home.path())
        .args([
            "workspace",
            "add",
            "beta",
            vault_b.path().to_str().unwrap(),
            "--init",
        ])
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
#[serial_test::serial]
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
#[serial_test::serial]
fn invalid_name_fails() {
    let home = TempDir::new().unwrap();
    let vault = TempDir::new().unwrap();
    lw(home.path())
        .args([
            "workspace",
            "add",
            "BadName",
            vault.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("lowercase"));
}

#[test]
#[serial_test::serial]
fn list_empty_message() {
    let home = TempDir::new().unwrap();
    lw(home.path())
        .args(["workspace", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no workspaces registered"));
}

#[test]
#[serial_test::serial]
fn current_workspace_path_missing_yields_actionable_error() {
    // Register a workspace whose path we then delete out from under it.
    // resolve_root() must surface a distinct, actionable error rather than
    // silently falling through to cwd discovery.
    let home = TempDir::new().unwrap();
    let vault = TempDir::new().unwrap();
    let vault_path = vault.path().to_path_buf();

    lw(home.path())
        .args([
            "workspace",
            "add",
            "ghosted",
            vault_path.to_str().unwrap(),
            "--init",
        ])
        .assert()
        .success();

    // Drop the vault TempDir to delete the directory on disk.
    drop(vault);
    assert!(!vault_path.exists(), "vault must be gone for the test");

    // Run a non-explicit-root command from a non-wiki cwd. The cwd must
    // not be inside any wiki ancestor, so use an isolated tempdir.
    let elsewhere = TempDir::new().unwrap();
    lw(home.path())
        .current_dir(elsewhere.path())
        .args(["status"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ghosted"))
        .stderr(predicate::str::contains("no longer exists"))
        .stderr(predicate::str::contains("lw workspace remove ghosted"));
}

#[test]
#[serial_test::serial]
fn current_verbose_warns_but_continues_when_table_missing_entry() {
    // Hand-craft a corrupt config where workspace.current points at a
    // name that has no corresponding workspaces[<name>] entry. `lw
    // workspace current -v` should warn on stderr but still print the
    // resolution chain so the user can debug.
    let home = TempDir::new().unwrap();
    let cfg_path = home.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        "[workspace]\ncurrent = \"orphan\"\n\n[workspaces]\n",
    )
    .unwrap();

    lw(home.path())
        .args(["workspace", "current", "-v"])
        .assert()
        .success()
        .stderr(predicate::str::contains("warning"))
        .stderr(predicate::str::contains("orphan"))
        .stderr(predicate::str::contains("config corrupt"))
        .stdout(predicate::str::contains("Resolution chain"))
        .stdout(predicate::str::contains("(missing entry)"));
}

#[test]
#[serial_test::serial]
fn current_verbose_warns_when_registered_path_missing() {
    // Register a workspace pointing at a temp dir, then drop it so the path
    // no longer exists on disk. `lw workspace current -v` should warn on
    // stderr but still succeed (exit 0) — it's a diagnostic command and the
    // user needs to see the resolution chain to debug.
    let home = TempDir::new().unwrap();
    let vault = TempDir::new().unwrap();
    let vault_path = vault.path().to_path_buf();

    lw(home.path())
        .args([
            "workspace",
            "add",
            "gone",
            vault_path.to_str().unwrap(),
            "--init",
        ])
        .assert()
        .success();

    drop(vault);
    assert!(!vault_path.exists(), "vault must be gone for the test");

    lw(home.path())
        .args(["workspace", "current", "-v"])
        .assert()
        .success()
        .stderr(predicate::str::contains("gone"))
        .stderr(predicate::str::contains("does not exist"))
        .stdout(predicate::str::contains("Resolution chain"));
}
