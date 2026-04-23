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

    /// Short human-readable skip reason shared by `integrate --auto` and
    /// `lw doctor`. `None` means "don't report" (tool is healthy, or just
    /// not installed — nothing to say).
    pub fn skip_reason(&self) -> Option<String> {
        match self {
            DetectOutcome::Present | DetectOutcome::MissingConfigDir { .. } => None,
            DetectOutcome::BinaryNotOnPath { binary } => {
                Some(format!("binary `{binary}` not found on PATH"))
            }
            DetectOutcome::VersionCheckFailed { binary, reason } => {
                Some(format!("`{binary}` version probe failed ({reason})"))
            }
        }
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
    /// Strong detection (config dir + binary on PATH + version probe exit 0)
    /// kicks in when the descriptor declares `detect.binary`. Otherwise this
    /// falls back to the legacy config-dir-exists check — descriptors for
    /// tools whose binary name we don't know yet can opt out by omitting
    /// `detect.binary`.
    pub fn detect(&self) -> DetectOutcome {
        let dir = expand_tilde(&self.detect.config_dir);
        if !dir.exists() {
            return DetectOutcome::MissingConfigDir { path: dir };
        }
        let Some(bin) = self.detect.binary.as_deref() else {
            return DetectOutcome::Present;
        };
        let Some(resolved) = binary_in_path(bin) else {
            return DetectOutcome::BinaryNotOnPath {
                binary: bin.to_string(),
            };
        };
        let probe_args = self.detect.effective_version_cmd();
        match run_version_check(&resolved, &probe_args, VERSION_PROBE_TIMEOUT) {
            Ok(()) => DetectOutcome::Present,
            Err(reason) => DetectOutcome::VersionCheckFailed {
                binary: bin.to_string(),
                reason,
            },
        }
    }
}

/// 5 s is generous enough for Python-based CLIs (kimi) on cold cache without
/// making `lw doctor` feel sluggish when several integrations are detected.
const VERSION_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Resolve `binary` against `$PATH`. Returns the first executable hit.
///
/// On Unix we require at least one execute bit set; on non-Unix we accept
/// any regular file match. Windows support for strong detection is out of
/// scope for 1.0.
fn binary_in_path(binary: &str) -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(binary);
        let Ok(meta) = candidate.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o111 == 0 {
                continue;
            }
        }
        return Some(candidate);
    }
    None
}

/// Run `<binary> <args...>` with a deadline. `Ok(())` only on exit-0 within
/// the timeout.
///
/// We spawn the child on a background thread and use a bounded channel to
/// enforce the deadline. If the deadline fires the thread (and the child it
/// spawned) are detached and may outlive this call; that's acceptable for a
/// one-shot CLI invocation and keeps us free of extra dependencies.
fn run_version_check(
    binary: &std::path::Path,
    args: &[String],
    timeout: std::time::Duration,
) -> Result<(), String> {
    use std::process::{Command, Stdio};
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();
    let bin_owned = binary.to_path_buf();
    let args_owned = args.to_vec();
    std::thread::spawn(move || {
        let status = Command::new(&bin_owned)
            .args(&args_owned)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = tx.send(status);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(format!("exit code {}", status.code().unwrap_or(-1))),
        Ok(Err(e)) => Err(format!("spawn failed: {e}")),
        Err(_) => Err(format!("timed out after {}s", timeout.as_secs())),
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

        let skills = d
            .skills
            .as_ref()
            .expect("kimi descriptor should link skills");
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
