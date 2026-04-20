use crate::version_file::{version_file_path, VersionFile, CURRENT_BINARY_VERSION};
use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
}

const RELEASES_API: &str =
    "https://api.github.com/repos/Pawpaw-Technology/llm-wiki/releases/latest";

pub fn check() -> anyhow::Result<()> {
    let installed = VersionFile::load_from(&version_file_path()?)?;
    let installed_str = if installed.binary.is_empty() {
        CURRENT_BINARY_VERSION.to_string()
    } else {
        installed.binary.clone()
    };
    let latest = fetch_latest_tag()?;
    let latest_clean = latest.trim_start_matches('v');
    if latest_clean == installed_str {
        println!("lw {installed_str} is up to date.");
        Ok(())
    } else {
        println!("Newer release available: {latest} (installed: {installed_str})");
        println!("Run `lw upgrade` to install.");
        std::process::exit(1)
    }
}

pub fn apply(yes: bool) -> anyhow::Result<()> {
    let prefix = crate::install_prefix::install_prefix();

    let installer = prefix.join("installer").join("install.sh");
    if !installer.exists() {
        anyhow::bail!(
            "installer not found at {} — re-run the curl install command from the README",
            installer.display()
        );
    }

    let mut cmd = Command::new("sh");
    cmd.arg(&installer);
    if yes {
        cmd.arg("--yes");
    }
    // Pass both: LW_HOME is canonical (read by the lw binary), LW_INSTALL_PREFIX
    // is kept as alias for back-compat with install.sh's flag naming.
    cmd.env("LW_INSTALL_PREFIX", &prefix);
    cmd.env("LW_HOME", &prefix);

    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("installer exited with {status}");
    }
    Ok(())
}

fn fetch_latest_tag() -> anyhow::Result<String> {
    let body = ureq::get(RELEASES_API)
        .header("User-Agent", concat!("lw/", env!("CARGO_PKG_VERSION")))
        .call()?
        .into_body()
        .read_to_string()?;
    let release: GhRelease = serde_json::from_str(&body)?;
    Ok(release.tag_name)
}
