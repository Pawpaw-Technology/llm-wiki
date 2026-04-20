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
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(s)
}

impl Descriptor {
    pub fn detect_present(&self) -> bool {
        expand_tilde(&self.detect.config_dir).exists()
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
}
