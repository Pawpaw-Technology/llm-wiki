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
        anyhow::bail!("workspace name must be lowercase alphanumeric + dashes (got '{name}')");
    }
    Ok(())
}

/// Resolve to absolute path; canonicalize when possible, even for paths that
/// don't yet exist. Walks up to the closest existing ancestor, canonicalizes
/// that, and re-appends the missing tail. This gives `/tmp/wp` and
/// `/private/tmp/wp` the same identity on macOS (where `/tmp` is a symlink
/// to `/private/tmp`), so the duplicate-path check in `add` can catch them.
fn resolve_path(path: &Path) -> anyhow::Result<PathBuf> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if abs.exists() {
        Ok(abs.canonicalize()?)
    } else {
        Ok(lw_core::fs::canonicalize_ancestor(&abs))
    }
}

pub fn add(name: &str, path: &Path, init: bool, template: Option<&str>) -> anyhow::Result<()> {
    validate_name(name)?;
    let abs = resolve_path(path)?;

    let cfg_path = config_path()?;
    let mut cfg = Config::load_from(&cfg_path)?;

    if cfg.workspaces.contains_key(name) {
        anyhow::bail!("workspace '{name}' already exists");
    }

    if init && template.is_some() {
        anyhow::bail!("--init and --template are mutually exclusive");
    }

    // Catch the case where two visibly-different paths point at the same
    // physical directory (e.g. `/tmp/wp` vs `/private/tmp/wp` on macOS).
    // `resolve_path` canonicalizes missing paths via their closest existing
    // ancestor, so the comparison is symlink-safe.
    for (other_name, entry) in &cfg.workspaces {
        let other_canon = if entry.path.exists() {
            entry
                .path
                .canonicalize()
                .unwrap_or_else(|_| entry.path.clone())
        } else {
            lw_core::fs::canonicalize_ancestor(&entry.path)
        };
        if other_canon == abs {
            anyhow::bail!(
                "path {} is already registered as workspace '{other_name}'",
                abs.display()
            );
        }
    }

    if let Some(tpl) = template {
        if abs.exists() && std::fs::read_dir(&abs)?.next().is_some() {
            anyhow::bail!("--template requires an empty or non-existent directory");
        }
        crate::templates::copy_template(tpl, &abs)?;
    } else if init {
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
    cfg.workspaces
        .insert(name.into(), WorkspaceEntry { path: abs.clone() });
    if first_workspace {
        cfg.workspace.current = Some(name.into());
    }
    cfg.save_to(&cfg_path)?;

    println!("Added workspace '{name}' at {}", abs.display());
    if let Some(tpl) = template {
        println!("  initialized from template '{tpl}'");
    }
    if first_workspace {
        println!("  set as current (first workspace)");
    }
    Ok(())
}

pub fn list() -> anyhow::Result<()> {
    let cfg = Config::load_from(&config_path()?)?;
    if cfg.workspaces.is_empty() {
        println!("(no workspaces registered — use `lw workspace add` to create one)");
        return Ok(());
    }
    let current = cfg.workspace.current.as_deref();
    for (name, entry) in &cfg.workspaces {
        let marker = if Some(name.as_str()) == current {
            "*"
        } else {
            " "
        };
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
                // If the registered entry's path no longer exists, this is
                // the loudest diagnostic command — warn unconditionally so
                // users can see it in both verbose and non-verbose mode.
                // `resolve_root` already converts this into an actionable
                // error for subcommands like `lw status`; surface the same
                // signal from `workspace current` so users discover it
                // without having to trip another command first.
                if !entry.path.exists() {
                    eprintln!(
                        "warning: current workspace '{name}' path {} does not exist",
                        entry.path.display()
                    );
                }
            }
            None => {
                // Demoted from anyhow::bail!: keep going so `lw workspace
                // current -v` still prints the resolution chain — that
                // chain is the most useful diagnostic when config is
                // corrupt.
                eprintln!(
                    "warning: current workspace '{name}' is registered but missing from workspaces table — config corrupt"
                );
            }
        },
        None => println!("(no current workspace)"),
    }
    if verbose {
        println!();
        println!("Resolution chain (--root > LW_WIKI_ROOT env > current workspace > cwd):");
        println!("  --root flag:        (only available at command time)");
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
        anyhow::bail!("workspace '{name}' not found (use `lw workspace list` to see registered)");
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
pub(super) mod tests {
    use super::*;
    use tempfile::TempDir;

    /// RAII guard that restores LW_HOME on drop, including panic unwind.
    /// This prevents env leakage between tests when an assertion in `f`
    /// panics. Tests are also `#[serial_test::serial]` annotated so they
    /// never run concurrently against the same env var.
    struct LwHomeGuard(Option<String>);

    impl Drop for LwHomeGuard {
        fn drop(&mut self) {
            // SAFETY: serialized via #[serial_test::serial] on every caller.
            match self.0.take() {
                Some(p) => unsafe { std::env::set_var("LW_HOME", p) },
                None => unsafe { std::env::remove_var("LW_HOME") },
            }
        }
    }

    pub(super) fn with_lw_home<F: FnOnce()>(home: &Path, f: F) {
        let _guard = LwHomeGuard(std::env::var("LW_HOME").ok());
        // SAFETY: serialized via #[serial_test::serial] on every caller.
        unsafe { std::env::set_var("LW_HOME", home) };
        f();
        // _guard restores LW_HOME on drop, including the panic-unwind path.
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
    #[serial_test::serial]
    fn add_first_workspace_sets_current() {
        let home = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("personal", vault.path(), false, None).unwrap();
            let cfg = Config::load_from(&config_path().unwrap()).unwrap();
            assert_eq!(cfg.workspace.current.as_deref(), Some("personal"));
            assert_eq!(cfg.workspaces.len(), 1);
        });
    }

    #[test]
    #[serial_test::serial]
    fn add_second_workspace_does_not_change_current() {
        let home = TempDir::new().unwrap();
        let v1 = TempDir::new().unwrap();
        let v2 = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("personal", v1.path(), false, None).unwrap();
            add("work", v2.path(), false, None).unwrap();
            let cfg = Config::load_from(&config_path().unwrap()).unwrap();
            assert_eq!(cfg.workspace.current.as_deref(), Some("personal"));
            assert_eq!(cfg.workspaces.len(), 2);
        });
    }

    #[test]
    #[serial_test::serial]
    fn add_duplicate_name_errors() {
        let home = TempDir::new().unwrap();
        let v = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("foo", v.path(), false, None).unwrap();
            assert!(add("foo", v.path(), false, None).is_err());
        });
    }

    #[test]
    #[serial_test::serial]
    fn add_with_init_creates_wiki_in_empty_dir() {
        let home = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("foo", vault.path(), true, None).unwrap();
            assert!(vault.path().join(".lw/schema.toml").exists());
        });
    }

    #[test]
    #[serial_test::serial]
    fn add_with_init_rejects_nonempty_non_wiki() {
        let home = TempDir::new().unwrap();
        let vault = TempDir::new().unwrap();
        std::fs::write(vault.path().join("stranger.txt"), "hi").unwrap();
        with_lw_home(home.path(), || {
            assert!(add("foo", vault.path(), true, None).is_err());
        });
    }

    #[test]
    #[serial_test::serial]
    fn add_with_template_copies_tree() {
        let home = TempDir::new().unwrap();
        let templates_dir = TempDir::new().unwrap();
        // Stub templates
        let demo = templates_dir.path().join("templates").join("demo");
        std::fs::create_dir_all(demo.join(".lw")).unwrap();
        std::fs::create_dir_all(demo.join("wiki/_uncategorized")).unwrap();
        std::fs::write(
            demo.join(".lw/schema.toml"),
            "[tags]\ncategories = [\"_uncategorized\"]\n",
        )
        .unwrap();
        std::fs::write(demo.join("SCOPE.md"), "# Scope\n").unwrap();
        std::fs::write(demo.join("wiki/_uncategorized/welcome.md"), "# Hi\n").unwrap();

        let vault = TempDir::new().unwrap();
        let target: &Path = &vault.path().join("v");
        with_lw_home(home.path(), || {
            unsafe {
                std::env::set_var("LW_TEMPLATES_DIR", templates_dir.path().join("templates"))
            };
            add("foo", target, false, Some("demo")).unwrap();
            unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };
            assert!(target.join("SCOPE.md").exists());
            assert!(target.join(".lw/schema.toml").exists());
        });
    }
}

#[cfg(test)]
mod crud_tests {
    use super::tests::with_lw_home;
    use super::*;
    use tempfile::TempDir;

    #[test]
    #[serial_test::serial]
    fn use_unknown_errors() {
        let home = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            assert!(use_("ghost").is_err());
        });
    }

    #[test]
    #[serial_test::serial]
    fn use_sets_current() {
        let home = TempDir::new().unwrap();
        // Two separate dirs — registering the same path under two names is
        // now (correctly) rejected by the duplicate-path check in `add`.
        let v_a = TempDir::new().unwrap();
        let v_b = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("a", v_a.path(), false, None).unwrap();
            add("b", v_b.path(), false, None).unwrap();
            use_("b").unwrap();
            let cfg = Config::load_from(&config_path().unwrap()).unwrap();
            assert_eq!(cfg.workspace.current.as_deref(), Some("b"));
        });
    }

    #[test]
    #[serial_test::serial]
    fn remove_clears_current_if_was_current() {
        let home = TempDir::new().unwrap();
        let v = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            add("a", v.path(), false, None).unwrap();
            remove("a").unwrap();
            let cfg = Config::load_from(&config_path().unwrap()).unwrap();
            assert!(cfg.workspace.current.is_none());
            assert!(cfg.workspaces.is_empty());
        });
    }

    #[test]
    #[serial_test::serial]
    fn remove_unknown_errors() {
        let home = TempDir::new().unwrap();
        with_lw_home(home.path(), || {
            assert!(remove("ghost").is_err());
        });
    }
}
