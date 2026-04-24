use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub workspace: WorkspaceState,
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceEntry>,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceState {
    pub current: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceEntry {
    pub path: PathBuf,
}

/// Default location: $LW_HOME/config.toml, where LW_HOME falls back to ~/.llm-wiki/.
pub fn config_path() -> anyhow::Result<PathBuf> {
    if let Ok(custom) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(custom).join("config.toml"));
    }
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot resolve home directory"))?;
    Ok(home.join(".llm-wiki").join("config.toml"))
}

impl Config {
    /// Load from disk. Returns Default if file does not exist.
    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        match fs::read_to_string(path) {
            Ok(s) => Ok(toml::from_str(&s)?),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Atomic write: stage to a unique temp file in the target dir, fsync, rename.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let body = toml::to_string_pretty(self)?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        tmp.write_all(body.as_bytes())?;
        tmp.as_file().sync_all()?;
        tmp.persist(path).map_err(|e| e.error)?;

        sync_parent_dir(parent)?;
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> anyhow::Result<()> {
    let dir = fs::File::open(parent)?;
    dir.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_roundtrips() {
        let cfg = Config::default();
        let s = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn config_with_workspaces_roundtrips() {
        let mut workspaces = BTreeMap::new();
        workspaces.insert(
            "personal".into(),
            WorkspaceEntry {
                path: PathBuf::from("/tmp/personal"),
            },
        );
        workspaces.insert(
            "work".into(),
            WorkspaceEntry {
                path: PathBuf::from("/tmp/work"),
            },
        );
        let cfg = Config {
            workspace: WorkspaceState {
                current: Some("personal".into()),
            },
            workspaces,
        };
        let s = toml::to_string(&cfg).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn missing_workspace_section_defaults() {
        let s = "";
        let cfg: Config = toml::from_str(s).unwrap();
        assert!(cfg.workspace.current.is_none());
        assert!(cfg.workspaces.is_empty());
    }
}

#[cfg(test)]
mod io_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let cfg = Config::load_from(&dir.path().join("nope.toml")).unwrap();
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn save_creates_parent_dir() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a/b/c/config.toml");
        Config::default().save_to(&nested).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn save_then_load_preserves_data() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.workspace.current = Some("foo".into());
        cfg.workspaces.insert(
            "foo".into(),
            WorkspaceEntry {
                path: PathBuf::from("/tmp/foo"),
            },
        );
        cfg.save_to(&path).unwrap();
        let back = Config::load_from(&path).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn save_is_atomic_no_tmp_left_behind() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        Config::default().save_to(&path).unwrap();
        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["config.toml".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn save_does_not_follow_preexisting_tmp_symlink() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let victim = dir.path().join("victim.txt");
        fs::write(&victim, "do not overwrite").unwrap();
        symlink(&victim, path.with_extension("toml.tmp")).unwrap();

        let mut cfg = Config::default();
        cfg.workspace.current = Some("safe".into());
        cfg.save_to(&path).unwrap();

        assert_eq!(fs::read_to_string(&victim).unwrap(), "do not overwrite");
        assert!(
            !fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(Config::load_from(&path).unwrap(), cfg);
    }
}
