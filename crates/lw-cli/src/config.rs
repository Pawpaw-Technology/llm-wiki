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
