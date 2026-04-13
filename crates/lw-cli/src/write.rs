use lw_core::fs::{validate_wiki_path, write_page};
use lw_core::page::Page;
use lw_core::section;
use std::io::Read;
use std::path::Path;

pub fn run(
    root: &Path,
    path: &str,
    mode: &str,
    section_name: &Option<String>,
    content: &Option<String>,
    stdin_available: bool,
) -> Result<(), anyhow::Error> {
    let abs_path = validate_wiki_path(root, path)?;

    // Resolve content: --content takes priority, then stdin
    let resolved_content = match content {
        Some(c) => c.clone(),
        None if stdin_available => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            if buf.trim().is_empty() {
                anyhow::bail!(
                    "stdin is empty; provide content via --content or pipe non-empty input"
                );
            }
            buf
        }
        None => {
            anyhow::bail!("no content provided; use --content or pipe via stdin");
        }
    };

    match mode {
        "overwrite" => {
            let page = Page::parse(&resolved_content)?;
            write_page(&abs_path, &page)?;
            eprintln!("Wrote: {path}");
        }
        "append" | "append_section" => {
            let section_name = section_name
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--section required for append mode"))?;

            let raw = std::fs::read_to_string(&abs_path).map_err(|_| {
                anyhow::anyhow!("page not found; use overwrite mode to create: {path}")
            })?;

            let (frontmatter, body) = section::split_frontmatter(&raw);
            match section::apply_append(body, section_name, &resolved_content) {
                Some(r) => {
                    let output = format!("{}{}", frontmatter, r.body);
                    std::fs::write(&abs_path, output)?;
                    if r.multiple_matches {
                        eprintln!(
                            "Warning: section '{section_name}' matched multiple headings; operated on first"
                        );
                    }
                    if r.section_found {
                        eprintln!("Appended to section '{section_name}' in {path}");
                    } else {
                        eprintln!("Created section '{section_name}' at end of {path}");
                    }
                }
                None => {
                    eprintln!("Empty content, nothing to append.");
                }
            }
        }
        "upsert" | "upsert_section" => {
            let section_name = section_name
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--section required for upsert mode"))?;

            let raw = std::fs::read_to_string(&abs_path).map_err(|_| {
                anyhow::anyhow!("page not found; use overwrite mode to create: {path}")
            })?;

            let (frontmatter, body) = section::split_frontmatter(&raw);
            let r = section::apply_upsert(body, section_name, &resolved_content);
            let output = format!("{}{}", frontmatter, r.body);
            std::fs::write(&abs_path, output)?;
            if r.multiple_matches {
                eprintln!(
                    "Warning: section '{section_name}' matched multiple headings; operated on first"
                );
            }
            if r.section_found {
                eprintln!("Replaced section '{section_name}' in {path}");
            } else {
                eprintln!("Created section '{section_name}' at end of {path}");
            }
        }
        other => {
            anyhow::bail!("Unknown mode: '{other}'. Use 'overwrite', 'append', or 'upsert'.");
        }
    }

    Ok(())
}
