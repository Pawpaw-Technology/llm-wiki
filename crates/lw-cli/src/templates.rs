use std::path::{Path, PathBuf};

/// Resolution order for the templates root:
/// 1. $LW_TEMPLATES_DIR (explicit override, mostly for tests)
/// 2. $LW_HOME/templates/ (set by installer)
/// 3. ~/.llm-wiki/templates/ (default install location)
/// 4. <exe_dir>/../share/llm-wiki/templates/ (cargo-installed layout)
/// 5. <repo>/templates/ (development checkout — exe at target/debug/lw)
pub fn templates_root() -> anyhow::Result<PathBuf> {
    if let Ok(p) = std::env::var("LW_TEMPLATES_DIR") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(home) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(home).join("templates"));
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".llm-wiki").join("templates");
        if p.exists() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe()?;
    if let Some(exe_dir) = exe.parent() {
        let share = exe_dir.join("../share/llm-wiki/templates");
        if share.exists() {
            return Ok(share);
        }
        // Dev fallback: walk up looking for templates/
        let mut cur = exe_dir.to_path_buf();
        for _ in 0..6 {
            let candidate = cur.join("templates");
            if candidate.exists() {
                return Ok(candidate);
            }
            if !cur.pop() {
                break;
            }
        }
    }
    anyhow::bail!("Cannot locate templates directory")
}

pub fn list_available() -> anyhow::Result<Vec<String>> {
    let root = templates_root()?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Copy a template tree into `dest`. Dest must be empty or non-existent.
/// Skips `.gitkeep` placeholder files (they only exist to ship empty dirs).
pub fn copy_template(template_name: &str, dest: &Path) -> anyhow::Result<()> {
    let root = templates_root()?;
    let src = root.join(template_name);
    if !src.exists() {
        let avail = list_available().unwrap_or_default().join(", ");
        anyhow::bail!(
            "template '{template_name}' not found in {} (available: {avail})",
            root.display()
        );
    }
    if dest.exists() && std::fs::read_dir(dest)?.next().is_some() {
        anyhow::bail!("destination {} is not empty", dest.display());
    }
    std::fs::create_dir_all(dest)?;
    copy_recursive(&src, dest)
}

fn copy_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let name = entry.file_name();
        if name == ".gitkeep" {
            // Materialize the parent directory but don't copy the placeholder
            std::fs::create_dir_all(dst)?;
            continue;
        }
        let to = dst.join(&name);
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&to)?;
            copy_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_fake_templates(dir: &Path) {
        let t = dir.join("templates").join("demo");
        std::fs::create_dir_all(t.join(".lw")).unwrap();
        std::fs::create_dir_all(t.join("wiki/_uncategorized")).unwrap();
        std::fs::write(
            t.join(".lw/schema.toml"),
            "[tags]\ncategories = [\"_uncategorized\"]\n",
        )
        .unwrap();
        std::fs::write(t.join("SCOPE.md"), "# Scope\n").unwrap();
        std::fs::write(t.join("wiki/_uncategorized/welcome.md"), "# Hi\n").unwrap();
        std::fs::write(t.join(".gitkeep"), "").unwrap();
        std::fs::create_dir_all(t.join("raw")).unwrap();
        std::fs::write(t.join("raw/.gitkeep"), "").unwrap();
    }

    #[test]
    #[serial_test::serial]
    fn list_available_finds_dirs() {
        let dir = TempDir::new().unwrap();
        make_fake_templates(dir.path());
        // SAFETY: tests serialized via #[serial_test::serial]
        unsafe { std::env::set_var("LW_TEMPLATES_DIR", dir.path().join("templates")) };
        let avail = list_available().unwrap();
        assert_eq!(avail, vec!["demo".to_string()]);
        unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };
    }

    #[test]
    #[serial_test::serial]
    fn copy_template_writes_tree() {
        let templates_dir = TempDir::new().unwrap();
        make_fake_templates(templates_dir.path());
        let dest = TempDir::new().unwrap();
        unsafe { std::env::set_var("LW_TEMPLATES_DIR", templates_dir.path().join("templates")) };
        copy_template("demo", &dest.path().join("vault")).unwrap();
        unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };

        assert!(dest.path().join("vault/.lw/schema.toml").exists());
        assert!(dest.path().join("vault/SCOPE.md").exists());
        assert!(dest
            .path()
            .join("vault/wiki/_uncategorized/welcome.md")
            .exists());
        assert!(dest.path().join("vault/raw").exists());
        // .gitkeep must NOT be copied
        assert!(!dest.path().join("vault/raw/.gitkeep").exists());
        assert!(!dest.path().join("vault/.gitkeep").exists());
    }

    #[test]
    #[serial_test::serial]
    fn copy_template_unknown_errors() {
        let templates_dir = TempDir::new().unwrap();
        make_fake_templates(templates_dir.path());
        let dest = TempDir::new().unwrap();
        unsafe { std::env::set_var("LW_TEMPLATES_DIR", templates_dir.path().join("templates")) };
        let result = copy_template("ghost", &dest.path().join("vault"));
        unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };
        assert!(result.is_err());
    }

    #[test]
    #[serial_test::serial]
    fn copy_template_rejects_nonempty_dest() {
        let templates_dir = TempDir::new().unwrap();
        make_fake_templates(templates_dir.path());
        let dest = TempDir::new().unwrap();
        let vault = dest.path().join("vault");
        std::fs::create_dir(&vault).unwrap();
        std::fs::write(vault.join("stranger.txt"), "hi").unwrap();
        unsafe { std::env::set_var("LW_TEMPLATES_DIR", templates_dir.path().join("templates")) };
        let result = copy_template("demo", &vault);
        unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };
        assert!(result.is_err());
    }
}
