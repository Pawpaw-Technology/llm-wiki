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
fn duplicate_path_different_name_fails() {
    // Two visibly-different paths can resolve to the same physical
    // directory: on macOS/Linux `/tmp` is a symlink to `/private/tmp`, so
    // `/tmp/wp` and `/private/tmp/wp` point at the same place. Registering
    // both under different names was a footgun — the fix canonicalizes
    // non-existent paths via their closest existing ancestor, so the
    // duplicate-path check catches this case.
    let home = TempDir::new().unwrap();

    // Use a unique subdir under /tmp to avoid cross-test collisions.
    let unique = format!(
        "lw-dup-symlink-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let via_tmp = std::path::PathBuf::from("/tmp").join(&unique);
    let via_private_tmp = std::path::PathBuf::from("/private/tmp").join(&unique);

    // Guard: if this OS doesn't canonicalize /tmp to /private/tmp (e.g.
    // non-macOS Linux without that symlink), skip rather than false-fail.
    let tmp_canon = std::path::PathBuf::from("/tmp")
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
    if tmp_canon != std::path::Path::new("/private/tmp") {
        eprintln!("skipping: /tmp does not canonicalize to /private/tmp on this OS");
        return;
    }

    // Ensure neither path exists from a previous failed run.
    let _ = std::fs::remove_dir_all(&via_private_tmp);

    lw(home.path())
        .args(["workspace", "add", "a", via_tmp.to_str().unwrap(), "--init"])
        .assert()
        .success();

    lw(home.path())
        .args(["workspace", "add", "b", via_private_tmp.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "already registered as workspace 'a'",
        ));

    // Cleanup
    let _ = std::fs::remove_dir_all(&via_private_tmp);
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

/// Regression for v0.2.0-rc.1 smoke-gate blocker B1: the bundled templates
/// shipped with `.lw/schema.toml` files that lacked the `[wiki]` section,
/// so vaults created from them failed to load on every subsequent command.
/// These tests exercise the full end-to-end path for each template: copy
/// from the repo's `templates/` dir via `workspace add --template <t>`,
/// then run `status --root <vault>` and assert exit 0.
fn repo_templates_dir() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR is <repo>/crates/lw-cli at test time.
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("templates")
}

fn assert_template_produces_loadable_wiki(template: &str) {
    let home = TempDir::new().unwrap();
    let vault = TempDir::new().unwrap();
    let templates = repo_templates_dir();

    lw(home.path())
        .env("LW_TEMPLATES_DIR", &templates)
        .args([
            "workspace",
            "add",
            "demo",
            vault.path().to_str().unwrap(),
            "--template",
            template,
        ])
        .assert()
        .success();

    // If the schema is malformed, every read path blows up. `status` is
    // the cheapest one that exercises `load_schema`.
    lw(home.path())
        .args(["status", "--root", vault.path().to_str().unwrap()])
        .assert()
        .success();
}

#[test]
#[serial_test::serial]
fn workspace_add_template_general_produces_loadable_wiki() {
    assert_template_produces_loadable_wiki("general");
}

#[test]
#[serial_test::serial]
fn workspace_add_template_research_papers_produces_loadable_wiki() {
    assert_template_produces_loadable_wiki("research-papers");
}

#[test]
#[serial_test::serial]
fn workspace_add_template_engineering_notes_produces_loadable_wiki() {
    assert_template_produces_loadable_wiki("engineering-notes");
}
