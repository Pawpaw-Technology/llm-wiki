use std::process::Command;

pub struct UninstallOpts {
    pub yes: bool,
    pub keep_config: bool,
    pub purge: bool,
}

pub fn run(opts: UninstallOpts) -> anyhow::Result<()> {
    let prefix = std::env::var("LW_INSTALL_PREFIX").unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|h| h.join(".llm-wiki").display().to_string())
            .unwrap_or_else(|| "$HOME/.llm-wiki".into())
    });

    let script = std::path::PathBuf::from(&prefix)
        .join("installer")
        .join("uninstall.sh");
    if !script.exists() {
        anyhow::bail!(
            "uninstall script not found at {} — manual cleanup required (rm -rf ~/.llm-wiki and strip PATH marker)",
            script.display()
        );
    }

    let mut cmd = Command::new("sh");
    cmd.arg(&script);
    if opts.yes {
        cmd.arg("--yes");
    }
    if opts.keep_config {
        cmd.arg("--keep-config");
    }
    if opts.purge {
        cmd.arg("--purge");
    }
    cmd.env("LW_INSTALL_PREFIX", &prefix);

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("uninstall script exited with {status}");
    }
    Ok(())
}
