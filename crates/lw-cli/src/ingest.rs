use crate::output::Format;
use lw_core::fs::{load_schema, write_page};
use lw_core::ingest::ingest_source;
use lw_core::llm::NoopLlm;
use lw_core::page::{slugify, Page};
use serde::Serialize;
use std::io::{self, BufRead, Read, Write};
use std::path::Path;

#[derive(Serialize)]
struct IngestOutput {
    path: String,
    title: String,
    category: String,
    decay: String,
    dry_run: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    root: &Path,
    source: Option<&Path>,
    stdin_mode: bool,
    title: &Option<String>,
    category: &Option<String>,
    tags: &Option<String>,
    raw_subdir: &str,
    yes: bool,
    dry_run: bool,
    output_format: &Format,
) -> anyhow::Result<()> {
    let schema = load_schema(root)?;

    // Handle stdin mode: write to temp file first
    let temp_file;
    let source_path = if stdin_mode {
        let mut content = String::new();
        io::stdin().lock().read_to_string(&mut content)?;
        temp_file = tempfile::NamedTempFile::new()?;
        std::fs::write(temp_file.path(), &content)?;
        temp_file.path()
    } else {
        source.ok_or_else(|| {
            anyhow::anyhow!(
                "No source file specified.\n  \
                 Usage: lw ingest <file> [--category X] [--yes]\n  \
                 Or:    cat file | lw ingest --stdin --title \"Title\" --yes"
            )
        })?
    };

    if !source_path.exists() {
        anyhow::bail!(
            "Source file not found: {}\n  Usage: lw ingest <path-to-file> [--raw-type papers]",
            source_path.display()
        );
    }

    // Build page from LLM draft or minimal
    let cat = category
        .clone()
        .unwrap_or_else(|| "_uncategorized".to_string());
    let page_tags: Vec<String> = tags
        .as_ref()
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if dry_run {
        // Dry run: compute what would be created without writing anything
        let auto_title = title.clone().unwrap_or_else(|| {
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        });
        let slug = slugify(&auto_title);
        let decay = schema.decay_for_category(&cat).to_string();
        let rel_path = format!("wiki/{}/{}.md", cat, slug);

        let output = IngestOutput {
            path: rel_path.clone(),
            title: auto_title.clone(),
            category: cat.clone(),
            decay: decay.clone(),
            dry_run: true,
        };

        match output_format {
            Format::Json => {
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
            Format::Human | Format::Brief => {
                println!("dry-run: true");
                println!("path: {}", rel_path);
                println!("title: {}", auto_title);
                println!("category: {}", cat);
                println!("decay: {}", decay);
                println!("tags: [{}]", page_tags.join(", "));
            }
        }
        return Ok(());
    }

    // Phase 1: NoopLlm
    let llm = NoopLlm;
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(ingest_source(root, source_path, raw_subdir, &llm))?;
    eprintln!("Saved to {}", result.raw_path.display());

    let draft = if let Some(draft) = result.draft {
        draft
    } else {
        let auto_title = title.clone().unwrap_or_else(|| {
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        });
        Page {
            title: auto_title,
            tags: page_tags,
            decay: Some(schema.decay_for_category(&cat).to_string()),
            sources: vec![format!(
                "raw/{}/{}",
                raw_subdir,
                source_path.file_name().unwrap().to_string_lossy()
            )],
            author: None,
            generator: None,
            body: format!(
                "TODO: summarize {}\n",
                source_path.file_name().unwrap().to_string_lossy()
            ),
        }
    };

    // Present for approval (or auto-approve with --yes)
    if !yes {
        eprintln!();
        eprintln!("  Title:    {}", draft.title);
        eprintln!("  Tags:     [{}]", draft.tags.join(", "));
        eprintln!("  Category: {}", cat);
        eprintln!("  Decay:    {}", draft.decay.as_deref().unwrap_or("normal"));
        eprintln!();
        if !confirm("Create wiki page?", true)? {
            eprintln!("Skipped.");
            return Ok(());
        }
    }

    let slug = slugify(&draft.title);
    let rel_path = format!("wiki/{}/{}.md", cat, slug);
    let page_path = root.join(&rel_path);
    write_page(&page_path, &draft)?;

    let output = IngestOutput {
        path: rel_path.clone(),
        title: draft.title.clone(),
        category: cat.clone(),
        decay: draft.decay.clone().unwrap_or_else(|| "normal".to_string()),
        dry_run: false,
    };

    match output_format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Format::Human | Format::Brief => {
            println!("path: {}", rel_path);
            println!("title: {}", draft.title);
            println!("category: {}", cat);
        }
    }

    Ok(())
}

/// Detect if a source string looks like a URL (http:// or https://).
pub fn is_url(source: &str) -> bool {
    // Stub: always returns false — will be implemented in GREEN step
    let _ = source;
    false
}

/// Derive a filename from a URL for saving to raw/.
pub fn filename_from_url(url: &str) -> String {
    // Stub: returns empty string — will be implemented in GREEN step
    let _ = url;
    String::new()
}

fn confirm(prompt: &str, default_yes: bool) -> io::Result<bool> {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    eprint!("  {} {} ", prompt, suffix);
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    Ok(if trimmed.is_empty() {
        default_yes
    } else {
        trimmed == "y" || trimmed == "yes"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- URL detection ---

    #[test]
    fn is_url_detects_https() {
        assert!(is_url("https://arxiv.org/abs/2405.12345"));
    }

    #[test]
    fn is_url_detects_http() {
        assert!(is_url("http://example.com/paper.pdf"));
    }

    #[test]
    fn is_url_rejects_file_path() {
        assert!(!is_url("/home/user/paper.pdf"));
        assert!(!is_url("relative/path.md"));
        assert!(!is_url("paper.pdf"));
    }

    #[test]
    fn is_url_rejects_other_schemes() {
        assert!(!is_url("ftp://example.com/file"));
        assert!(!is_url("ssh://host/repo"));
    }

    // --- Filename derivation from URL ---

    #[test]
    fn filename_from_url_extracts_last_segment() {
        assert_eq!(
            filename_from_url("https://example.com/papers/attention.pdf"),
            "attention.pdf"
        );
    }

    #[test]
    fn filename_from_url_handles_trailing_slash() {
        let name = filename_from_url("https://arxiv.org/abs/2405.12345/");
        assert!(!name.is_empty());
        // Should produce something meaningful, not empty
    }

    #[test]
    fn filename_from_url_handles_query_params() {
        let name = filename_from_url("https://example.com/doc.pdf?token=abc");
        assert_eq!(name, "doc.pdf");
    }

    #[test]
    fn filename_from_url_handles_no_path() {
        let name = filename_from_url("https://example.com");
        assert!(!name.is_empty());
    }
}
