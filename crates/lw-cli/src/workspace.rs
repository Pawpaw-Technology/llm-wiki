use crate::config::{config_path, Config, WorkspaceEntry};
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
    cfg.workspaces
        .insert(name.into(), WorkspaceEntry { path: abs.clone() });
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
