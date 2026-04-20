use crate::integrations::descriptor::{SkillsConfig, SkillsMode, expand_tilde};
use std::path::{Path, PathBuf};

pub fn skills_root() -> anyhow::Result<PathBuf> {
    if let Ok(p) = std::env::var("LW_SKILLS_DIR") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(home) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(home).join("skills"));
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".llm-wiki").join("skills");
        if p.exists() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe()?;
    if let Some(exe_dir) = exe.parent() {
        let share = exe_dir.join("../share/llm-wiki/skills");
        if share.exists() {
            return Ok(share);
        }
        let mut cur = exe_dir.to_path_buf();
        for _ in 0..6 {
            let candidate = cur.join("skills");
            if candidate.exists() {
                return Ok(candidate);
            }
            if !cur.pop() {
                break;
            }
        }
    }
    anyhow::bail!("Cannot locate skills directory")
}

/// Normalise trailing-slash targets. `/foo/bar/` is legal in config but causes
/// `symlink(2)` and a few other syscalls to fail with ENOENT because they
/// treat the trailing slash as requiring an existing directory. Rebuild the
/// path from its `components()` which drop trailing separators.
fn normalise_target(p: PathBuf) -> PathBuf {
    let rebuilt: PathBuf = p.components().collect();
    if rebuilt.as_os_str().is_empty() {
        p
    } else {
        rebuilt
    }
}

pub fn install(cfg: &SkillsConfig) -> anyhow::Result<PathBuf> {
    let target = normalise_target(expand_tilde(&cfg.target_dir));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let source = skills_root()?;
    if !source.exists() {
        anyhow::bail!(
            "skills source not found at {} — install layout broken",
            source.display()
        );
    }
    // If target already exists, replace it.
    if target.exists() || target.symlink_metadata().is_ok() {
        if target.is_symlink() || target.is_file() {
            std::fs::remove_file(&target)?;
        } else if target.is_dir() {
            std::fs::remove_dir_all(&target)?;
        }
    }
    match cfg.mode {
        SkillsMode::Symlink => link_dir(&source, &target)?,
        SkillsMode::Copy => copy_recursive(&source, &target)?,
    }
    Ok(target)
}

pub fn uninstall(cfg: &SkillsConfig) -> anyhow::Result<bool> {
    let target = normalise_target(expand_tilde(&cfg.target_dir));
    if !target.exists() && target.symlink_metadata().is_err() {
        return Ok(false);
    }
    if target.is_symlink() || target.is_file() {
        std::fs::remove_file(&target)?;
    } else if target.is_dir() {
        std::fs::remove_dir_all(&target)?;
    }
    Ok(true)
}

#[cfg(unix)]
fn link_dir(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::os::unix::fs::symlink(src, dst)?;
    Ok(())
}

#[cfg(not(unix))]
fn link_dir(_src: &Path, _dst: &Path) -> anyhow::Result<()> {
    anyhow::bail!("symlink mode requires Unix; use mode = \"copy\" on this platform")
}

fn copy_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
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

    fn make_skills_root(dir: &Path) {
        let s = dir.join("skills").join("llm-wiki-import");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(s.join("SKILL.md"), "---\nname: test\n---\nbody").unwrap();
    }

    #[test]
    #[serial_test::serial]
    fn install_symlink_creates_link() {
        let src_dir = TempDir::new().unwrap();
        make_skills_root(src_dir.path());
        let target_dir = TempDir::new().unwrap();
        let cfg = SkillsConfig {
            target_dir: target_dir.path().join("llm-wiki").display().to_string(),
            mode: SkillsMode::Symlink,
        };
        unsafe { std::env::set_var("LW_SKILLS_DIR", src_dir.path().join("skills")) };
        let target = install(&cfg).unwrap();
        unsafe { std::env::remove_var("LW_SKILLS_DIR") };
        assert!(target.exists());
        assert!(target.is_symlink() || target.read_link().is_ok());
        assert!(target.join("llm-wiki-import/SKILL.md").exists());
    }

    #[test]
    #[serial_test::serial]
    fn install_copy_writes_files() {
        let src_dir = TempDir::new().unwrap();
        make_skills_root(src_dir.path());
        let target_dir = TempDir::new().unwrap();
        let cfg = SkillsConfig {
            target_dir: target_dir.path().join("llm-wiki").display().to_string(),
            mode: SkillsMode::Copy,
        };
        unsafe { std::env::set_var("LW_SKILLS_DIR", src_dir.path().join("skills")) };
        let target = install(&cfg).unwrap();
        unsafe { std::env::remove_var("LW_SKILLS_DIR") };
        assert!(target.join("llm-wiki-import/SKILL.md").exists());
        assert!(!target.is_symlink());
    }

    #[test]
    #[serial_test::serial]
    fn uninstall_removes_symlink() {
        let src_dir = TempDir::new().unwrap();
        make_skills_root(src_dir.path());
        let target_dir = TempDir::new().unwrap();
        let cfg = SkillsConfig {
            target_dir: target_dir.path().join("llm-wiki").display().to_string(),
            mode: SkillsMode::Symlink,
        };
        unsafe { std::env::set_var("LW_SKILLS_DIR", src_dir.path().join("skills")) };
        install(&cfg).unwrap();
        let removed = uninstall(&cfg).unwrap();
        unsafe { std::env::remove_var("LW_SKILLS_DIR") };
        assert!(removed);
        assert!(!target_dir.path().join("llm-wiki").exists());
    }

    #[test]
    #[serial_test::serial]
    fn install_replaces_existing_symlink() {
        let src_dir = TempDir::new().unwrap();
        make_skills_root(src_dir.path());
        let target_dir = TempDir::new().unwrap();
        let cfg = SkillsConfig {
            target_dir: target_dir.path().join("llm-wiki").display().to_string(),
            mode: SkillsMode::Symlink,
        };
        unsafe { std::env::set_var("LW_SKILLS_DIR", src_dir.path().join("skills")) };
        install(&cfg).unwrap();
        // Re-install must succeed (replaces existing)
        install(&cfg).unwrap();
        unsafe { std::env::remove_var("LW_SKILLS_DIR") };
    }
}
