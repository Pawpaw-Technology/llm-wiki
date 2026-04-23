use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Descriptor {
    pub name: String,
    pub detect: Detect,
    pub mcp: Option<McpConfig>,
    pub skills: Option<SkillsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Detect {
    pub config_dir: String,
    /// Binary name that should resolve on PATH when the tool is installed.
    /// When `None`, detection falls back to the legacy "config_dir exists" check.
    #[serde(default)]
    pub binary: Option<String>,
    /// Args to pass to `binary` for the liveness probe. Defaults to `["--version"]`.
    #[serde(default)]
    pub version_cmd: Option<Vec<String>>,
}

/// Result of evaluating a descriptor's `[detect]` section against the live system.
///
/// Downstream callers (doctor, integrate --auto) use this to explain *why*
/// a tool was or wasn't detected, rather than collapsing everything to a bool.
#[derive(Debug, Clone, PartialEq)]
pub enum DetectOutcome {
    /// Config dir exists; if `binary` was declared, it's on PATH and its
    /// version probe exited 0.
    Present,
    /// `config_dir` does not exist on disk.
    MissingConfigDir { path: PathBuf },
    /// `config_dir` exists but the declared `binary` was not found on PATH.
    BinaryNotOnPath { binary: String },
    /// `binary` was on PATH but the version probe failed (nonzero exit,
    /// timed out, or could not spawn).
    VersionCheckFailed { binary: String, reason: String },
}

impl DetectOutcome {
    pub fn is_present(&self) -> bool {
        matches!(self, DetectOutcome::Present)
    }
}

impl Detect {
    /// Probe args, defaulting to `["--version"]` when the descriptor omits `version_cmd`.
    pub fn effective_version_cmd(&self) -> Vec<String> {
        self.version_cmd
            .clone()
            .unwrap_or_else(|| vec!["--version".to_string()])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpConfig {
    pub config_path: String,
    pub format: McpFormat,
    pub key_path: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpFormat {
    Json,
    Toml,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillsConfig {
    pub target_dir: String,
    pub mode: SkillsMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillsMode {
    Symlink,
    Copy,
}

/// Tilde-expand a path string. `~` and `~/` resolve to $HOME.
pub fn expand_tilde(s: &str) -> PathBuf {
    if let Some(stripped) = s.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(stripped);
    }
    if s == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home;
    }
    PathBuf::from(s)
}

impl Descriptor {
    /// Evaluate the `[detect]` section against the current system.
    ///
    /// STUB — RED step. Replaced in the GREEN commit with binary + version probe.
    pub fn detect(&self) -> DetectOutcome {
        let dir = expand_tilde(&self.detect.config_dir);
        if dir.exists() {
            DetectOutcome::Present
        } else {
            DetectOutcome::MissingConfigDir { path: dir }
        }
    }

    pub fn detect_present(&self) -> bool {
        self.detect().is_present()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_claude_code_descriptor() {
        let toml_str = r#"
name = "Claude Code"

[detect]
config_dir = "~/.claude"

[mcp]
config_path = "~/.claude/settings.json"
format = "json"
key_path = "mcpServers.llm-wiki"
command = "lw"
args = ["serve"]

[skills]
target_dir = "~/.claude/skills/llm-wiki/"
mode = "symlink"
"#;
        let d: Descriptor = toml::from_str(toml_str).unwrap();
        assert_eq!(d.name, "Claude Code");
        assert_eq!(d.mcp.as_ref().unwrap().format, McpFormat::Json);
        assert_eq!(d.skills.as_ref().unwrap().mode, SkillsMode::Symlink);
    }

    #[test]
    fn skills_only_descriptor_no_mcp() {
        let toml_str = r#"
name = "Skill-only Tool"

[detect]
config_dir = "~/.someothertool"

[skills]
target_dir = "~/.someothertool/skills/llm-wiki/"
mode = "copy"
"#;
        let d: Descriptor = toml::from_str(toml_str).unwrap();
        assert!(d.mcp.is_none());
        assert_eq!(d.skills.as_ref().unwrap().mode, SkillsMode::Copy);
    }

    #[test]
    fn tilde_expansion() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~/foo"), home.join("foo"));
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
        assert_eq!(expand_tilde("rel"), PathBuf::from("rel"));
    }

    #[test]
    fn parses_detect_binary_and_version_cmd() {
        let toml_str = r#"
name = "Probe Tool"

[detect]
config_dir = "~/.probe"
binary = "probe"
version_cmd = ["info", "--plain"]
"#;
        let d: Descriptor = toml::from_str(toml_str).unwrap();
        assert_eq!(d.detect.binary.as_deref(), Some("probe"));
        assert_eq!(
            d.detect.version_cmd,
            Some(vec!["info".to_string(), "--plain".to_string()])
        );
        assert_eq!(
            d.detect.effective_version_cmd(),
            vec!["info".to_string(), "--plain".to_string()]
        );
    }

    #[test]
    fn detect_binary_defaults_to_none_and_version_cmd_defaults_to_version_flag() {
        let toml_str = r#"
name = "Weak Tool"

[detect]
config_dir = "~/.weak"
"#;
        let d: Descriptor = toml::from_str(toml_str).unwrap();
        assert!(d.detect.binary.is_none());
        assert!(d.detect.version_cmd.is_none());
        assert_eq!(d.detect.effective_version_cmd(), vec!["--version"]);
    }

    // ---------- shipped-descriptor contract tests ----------
    //
    // These assert that the files in `integrations/*.toml` keep the shape
    // the installer and the integrate command expect.

    fn load_shipped(name: &str) -> Descriptor {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest)
            .join("..")
            .join("..")
            .join("integrations")
            .join(name);
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("shipped descriptor {}: {e}", path.display()));
        toml::from_str(&body)
            .unwrap_or_else(|e| panic!("parse shipped descriptor {}: {e}", path.display()))
    }

    #[test]
    fn shipped_claude_code_has_binary_probe() {
        let d = load_shipped("claude-code.toml");
        assert_eq!(d.name, "Claude Code");
        assert_eq!(d.detect.binary.as_deref(), Some("claude"));
    }

    #[test]
    fn shipped_codex_has_binary_probe() {
        let d = load_shipped("codex.toml");
        assert_eq!(d.name, "Codex");
        assert_eq!(d.detect.binary.as_deref(), Some("codex"));
    }

    #[test]
    fn shipped_openclaw_stays_weak_detection() {
        // Intentional fallback sample: we don't know the binary name yet,
        // so only config_dir is checked. Deleting this assertion is fine
        // the day we wire up a real binary probe for OpenClaw.
        let d = load_shipped("openclaw.toml");
        assert!(d.detect.binary.is_none());
    }

    #[test]
    fn shipped_kimi_descriptor_is_complete() {
        let d = load_shipped("kimi.toml");
        assert_eq!(d.name, "Kimi Code");
        assert_eq!(d.detect.config_dir, "~/.kimi");
        assert_eq!(d.detect.binary.as_deref(), Some("kimi"));

        let mcp = d.mcp.as_ref().expect("kimi descriptor should declare MCP");
        assert_eq!(mcp.format, McpFormat::Json);
        assert_eq!(mcp.config_path, "~/.kimi/mcp.json");
        assert_eq!(mcp.key_path, "mcpServers.llm-wiki");
        assert_eq!(mcp.command, "lw");
        assert_eq!(mcp.args, vec!["serve".to_string()]);

        let skills = d.skills.as_ref().expect("kimi descriptor should link skills");
        assert_eq!(skills.target_dir, "~/.kimi/skills/llm-wiki/");
        assert_eq!(skills.mode, SkillsMode::Symlink);
    }

    // ---------- DetectOutcome contract (behavior covered by integration tests) ----------

    #[test]
    fn missing_config_dir_outcome() {
        let d = Descriptor {
            name: "Ghost".into(),
            detect: Detect {
                config_dir: "/does/not/exist/xyz789".into(),
                binary: None,
                version_cmd: None,
            },
            mcp: None,
            skills: None,
        };
        match d.detect() {
            DetectOutcome::MissingConfigDir { path } => {
                assert_eq!(path, PathBuf::from("/does/not/exist/xyz789"));
            }
            other => panic!("expected MissingConfigDir, got {other:?}"),
        }
    }

    #[test]
    fn binary_not_on_path_outcome() {
        // Create a real config dir so the dir check passes; then ask for a
        // binary we know can't be on any sane PATH.
        let tmp = tempfile::tempdir().unwrap();
        let d = Descriptor {
            name: "Ghostbin".into(),
            detect: Detect {
                config_dir: tmp.path().to_string_lossy().into_owned(),
                binary: Some("lw-nonexistent-probe-xyzzy".into()),
                version_cmd: None,
            },
            mcp: None,
            skills: None,
        };
        match d.detect() {
            DetectOutcome::BinaryNotOnPath { binary } => {
                assert_eq!(binary, "lw-nonexistent-probe-xyzzy");
            }
            other => panic!("expected BinaryNotOnPath, got {other:?}"),
        }
    }
}
