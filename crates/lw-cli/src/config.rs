use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

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

use std::fs;
use std::io;
use std::path::Path;

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

    /// Atomic write: stage to .tmp sibling, fsync, rename.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("toml.tmp");
        let body = toml::to_string_pretty(self)?;
        fs::write(&tmp, body)?;
        // fsync the file so rename is durable
        let f = fs::File::open(&tmp)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
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
}
