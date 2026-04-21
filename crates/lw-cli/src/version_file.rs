use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Per spec §11: the binary and the assets (skills/templates) ship with matched
/// versions and `lw doctor` enforces compatibility. We record both in
/// `~/.llm-wiki/version` so an upgrade-skew (e.g., user manually replaced the
/// binary) is detectable.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct VersionFile {
    #[serde(default)]
    pub binary: String,
    #[serde(default)]
    pub assets: String,
    /// ISO-8601 timestamp set at install/upgrade time.
    #[serde(default)]
    pub installed_at: String,
}

pub fn version_file_path() -> anyhow::Result<PathBuf> {
    if let Ok(home) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(home).join("version"));
    }
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot resolve home directory"))?;
    Ok(home.join(".llm-wiki").join("version"))
}

impl VersionFile {
    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(toml::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    // Used by `lw doctor` in Plan D; scaffolded here so the installer can
    // write metadata without a second edit pass later.
    #[allow(dead_code)]
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(self)?;
        std::fs::write(path, body)?;
        Ok(())
    }

    // Used by `lw doctor` in Plan D.
    #[allow(dead_code)]
    pub fn is_compatible(&self) -> bool {
        !self.binary.is_empty() && self.binary == self.assets
    }
}

pub const CURRENT_BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_when_missing() {
        let dir = TempDir::new().unwrap();
        let v = VersionFile::load_from(&dir.path().join("nope")).unwrap();
        assert_eq!(v, VersionFile::default());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("version");
        let v = VersionFile {
            binary: "0.2.0".into(),
            assets: "0.2.0".into(),
            installed_at: "2026-04-20T12:00:00Z".into(),
        };
        v.save_to(&path).unwrap();
        let back = VersionFile::load_from(&path).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn is_compatible_when_versions_match() {
        let v = VersionFile {
            binary: "0.2.0".into(),
            assets: "0.2.0".into(),
            installed_at: "x".into(),
        };
        assert!(v.is_compatible());
    }

    #[test]
    fn is_incompatible_when_skewed() {
        let v = VersionFile {
            binary: "0.2.0".into(),
            assets: "0.1.9".into(),
            installed_at: "x".into(),
        };
        assert!(!v.is_compatible());
    }

    #[test]
    fn is_incompatible_when_empty() {
        let v = VersionFile::default();
        assert!(!v.is_compatible());
    }
}
