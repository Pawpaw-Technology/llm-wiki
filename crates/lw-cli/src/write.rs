use lw_core::fs::{atomic_write, validate_wiki_path, write_page};
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
            let section_name = require_section(section_name, mode)?;
            let result = run_section_op(&abs_path, path, section_name, |body| {
                section::apply_append(body, section_name, &resolved_content)
            })?;
            match result {
                Some(r) => report_section_result(&r, section_name, path, "Appended to"),
                None => eprintln!("Empty content, nothing to append."),
            }
        }
        "upsert" | "upsert_section" => {
            let section_name = require_section(section_name, mode)?;
            let result = run_section_op(&abs_path, path, section_name, |body| {
                Some(section::apply_upsert(body, section_name, &resolved_content))
            })?;
            match result {
                Some(r) => report_section_result(&r, section_name, path, "Replaced"),
                None => unreachable!("upsert always returns a result"),
            }
        }
        other => {
            anyhow::bail!("Unknown mode: '{other}'. Use 'overwrite', 'append', or 'upsert'.");
        }
    }

    Ok(())
}

fn require_section<'a>(
    section_name: &'a Option<String>,
    mode: &str,
) -> Result<&'a str, anyhow::Error> {
    section_name
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--section required for {mode} mode"))
}

fn run_section_op(
    abs_path: &Path,
    path: &str,
    _section_name: &str,
    op: impl FnOnce(&str) -> Option<section::SectionWriteResult>,
) -> Result<Option<section::SectionWriteResult>, anyhow::Error> {
    let raw = std::fs::read_to_string(abs_path)
        .map_err(|_| anyhow::anyhow!("page not found; use overwrite mode to create: {path}"))?;
    let (frontmatter, body) = section::split_frontmatter(&raw);
    match op(body) {
        Some(r) => {
            let output = format!("{}{}", frontmatter, r.body);
            atomic_write(abs_path, output.as_bytes())?;
            Ok(Some(r))
        }
        None => Ok(None),
    }
}

fn report_section_result(
    r: &section::SectionWriteResult,
    section_name: &str,
    path: &str,
    verb: &str,
) {
    if r.multiple_matches {
        eprintln!("Warning: section '{section_name}' matched multiple headings; operated on first");
    }
    if r.section_found {
        eprintln!("{verb} section '{section_name}' in {path}");
    } else {
        eprintln!("Created section '{section_name}' at end of {path}");
    }
}
