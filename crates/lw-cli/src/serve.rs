use std::path::Path;

pub fn run(root: &Path) -> anyhow::Result<()> {
    if !root.join(".lw/schema.toml").exists() {
        anyhow::bail!(
            "Not a wiki directory: {} (missing .lw/schema.toml)\n  Run: lw init --root {}",
            root.display(),
            root.display()
        );
    }
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(lw_mcp::run_stdio(root.to_path_buf()))
}
