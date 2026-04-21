pub mod descriptor;
pub mod mcp;
pub mod skills;

use descriptor::Descriptor;
use std::path::PathBuf;

/// Resolution order for the integrations descriptor dir:
/// 1. $LW_INTEGRATIONS_DIR
/// 2. $LW_HOME/integrations
/// 3. ~/.llm-wiki/integrations
/// 4. <exe_dir>/../share/llm-wiki/integrations
/// 5. <repo>/integrations  (dev fallback)
pub fn integrations_root() -> anyhow::Result<PathBuf> {
    if let Ok(p) = std::env::var("LW_INTEGRATIONS_DIR") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(home) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(home).join("integrations"));
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".llm-wiki").join("integrations");
        if p.exists() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe()?;
    if let Some(exe_dir) = exe.parent() {
        let share = exe_dir.join("../share/llm-wiki/integrations");
        if share.exists() {
            return Ok(share);
        }
        let mut cur = exe_dir.to_path_buf();
        for _ in 0..6 {
            let candidate = cur.join("integrations");
            if candidate.exists() {
                return Ok(candidate);
            }
            if !cur.pop() {
                break;
            }
        }
    }
    anyhow::bail!("Cannot locate integrations directory")
}

pub fn load_all() -> anyhow::Result<Vec<(String, Descriptor)>> {
    let root = integrations_root()?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("invalid descriptor filename"))?
                .to_string();
            let body = std::fs::read_to_string(&path)?;
            let desc: Descriptor = toml::from_str(&body)
                .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?;
            out.push((id, desc));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}
