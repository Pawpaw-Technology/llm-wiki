use crate::integrations::{
    descriptor::{Descriptor, McpFormat, expand_tilde},
    integrations_root, load_all, mcp, skills,
};
use serde_json::{Value, json};
use std::io::IsTerminal;

pub struct IntegrateOpts {
    pub yes: bool,
    pub uninstall: bool,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run(target: Option<&str>, opts: IntegrateOpts) -> anyhow::Result<()> {
    let descriptors = load_all()?;
    let to_process: Vec<(String, Descriptor)> = match target {
        Some(name) => descriptors
            .into_iter()
            .filter(|(id, _)| id == name)
            .collect(),
        None => {
            // --auto: only those whose detect.config_dir exists
            descriptors
                .into_iter()
                .filter(|(_, d)| d.detect_present())
                .collect()
        }
    };

    if to_process.is_empty() {
        match target {
            Some(t) => anyhow::bail!(
                "no integration descriptor named '{t}' (check {})",
                integrations_root()?.display()
            ),
            None => {
                println!(
                    "No supported agent tools detected. Install Claude Code, Codex, or OpenClaw first."
                );
                return Ok(());
            }
        }
    }

    for (id, desc) in to_process {
        if opts.uninstall {
            uninstall_one(&id, &desc)?;
        } else {
            let proceed = if opts.yes || target.is_some() {
                true
            } else if std::io::stdout().is_terminal() {
                prompt_yes_no(&format!("Integrate llm-wiki with {} ({}?)", desc.name, id))?
            } else {
                println!(
                    "Detected {} ({id}). Run `lw integrate {id}` or `lw integrate --auto --yes` to install.",
                    desc.name
                );
                false
            };
            if proceed {
                install_one(&id, &desc)?;
            }
        }
    }
    Ok(())
}

fn install_one(id: &str, desc: &Descriptor) -> anyhow::Result<()> {
    println!("Installing {} ({id})", desc.name);

    if let Some(mcp_cfg) = &desc.mcp {
        if mcp_cfg.format != McpFormat::Json {
            anyhow::bail!("only JSON MCP format is supported in this version");
        }
        let path = expand_tilde(&mcp_cfg.config_path);
        let mut config: Value = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&path)?)
                .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?
        } else {
            json!({})
        };
        let entry = json!({
            "command": mcp_cfg.command,
            "args": mcp_cfg.args,
            mcp::VERSION_MARKER: VERSION,
        });
        let outcome = mcp::merge_entry(&mut config, &mcp_cfg.key_path, entry, Some(VERSION))?;
        match outcome {
            mcp::MergeOutcome::Inserted => {
                println!("  MCP entry inserted at {}", path.display())
            }
            mcp::MergeOutcome::NoOp => {
                println!("  MCP entry already current at {}", path.display())
            }
            mcp::MergeOutcome::Updated => {
                println!("  MCP entry updated at {}", path.display())
            }
            mcp::MergeOutcome::Conflict { existing } => {
                eprintln!(
                    "  MCP entry at {} appears user-edited; not overwriting.",
                    path.display()
                );
                eprintln!("  Existing: {}", serde_json::to_string_pretty(&existing)?);
                eprintln!(
                    "  To force, remove the entry manually or run with `--force` (not yet supported)."
                );
                return Ok(());
            }
        }
        let body = serde_json::to_string_pretty(&config)? + "\n";
        let backup = mcp::atomic_write_with_backup(&path, &body)?;
        if let Some(b) = backup {
            println!("  backup: {}", b.display());
        }
    }

    if let Some(skills_cfg) = &desc.skills {
        let target = skills::install(skills_cfg)?;
        println!("  skills installed at {}", target.display());
    }

    Ok(())
}

fn uninstall_one(id: &str, desc: &Descriptor) -> anyhow::Result<()> {
    println!("Uninstalling {} ({id})", desc.name);

    if let Some(mcp_cfg) = &desc.mcp {
        let path = expand_tilde(&mcp_cfg.config_path);
        if path.exists() {
            let mut config: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)
                .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?;
            let removed = mcp::remove_entry(&mut config, &mcp_cfg.key_path);
            if removed {
                let body = serde_json::to_string_pretty(&config)? + "\n";
                let backup = mcp::atomic_write_with_backup(&path, &body)?;
                println!("  MCP entry removed from {}", path.display());
                if let Some(b) = backup {
                    println!("  backup: {}", b.display());
                }
            } else {
                println!("  MCP entry not present at {}", path.display());
            }
        }
    }

    if let Some(skills_cfg) = &desc.skills {
        let removed = skills::uninstall(skills_cfg)?;
        if removed {
            println!(
                "  skills removed from {}",
                expand_tilde(&skills_cfg.target_dir).display()
            );
        }
    }

    Ok(())
}

fn prompt_yes_no(question: &str) -> anyhow::Result<bool> {
    use std::io::Write;
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    Ok(matches!(
        buf.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
