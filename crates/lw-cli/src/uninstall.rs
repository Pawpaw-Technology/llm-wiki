use std::process::Command;

pub struct UninstallOpts {
    pub yes: bool,
    pub keep_config: bool,
    pub purge: bool,
}

pub fn run(opts: UninstallOpts) -> anyhow::Result<()> {
    let prefix = crate::install_prefix::install_prefix();

    let script = prefix.join("installer").join("uninstall.sh");
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
    // Pass both: LW_HOME is canonical (read by the lw binary), LW_INSTALL_PREFIX
    // is kept as alias for back-compat with uninstall.sh's flag naming.
    cmd.env("LW_INSTALL_PREFIX", &prefix);
    cmd.env("LW_HOME", &prefix);

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("uninstall script exited with {status}");
    }
    Ok(())
}
