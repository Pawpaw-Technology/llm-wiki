use crate::output::Format;
use lw_core::fs::load_schema;
use lw_core::ingest::{extract_h1, ingest_source, slug_from_title_or_h1};
use lw_core::page::slugify;
use serde::Serialize;
use std::io::{self, Read};
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
    source: Option<&str>,
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

    // Track URL origin for frontmatter sources
    let mut url_origin: Option<String> = None;

    // Determine if source is a URL (before resolving) so dry-run can skip download
    let source_str = if !stdin_mode {
        source.map(|s| s.to_string())
    } else {
        None
    };
    let source_is_url = source_str.as_deref().is_some_and(is_url);

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

    // Validate category early to reject path traversal before any I/O
    lw_core::fs::validate_wiki_path(root, &format!("{}/probe.md", cat))?;

    if dry_run {
        // Dry run: skip download, derive metadata from source alone.
        let auto_title = if source_is_url {
            title
                .clone()
                .unwrap_or_else(|| filename_from_url(source_str.as_deref().unwrap()))
        } else if let Some(ref s) = source_str {
            derive_title(title.as_deref(), Path::new(s), None)
        } else {
            title.clone().unwrap_or_else(|| "Untitled".to_string())
        };
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
                if source_is_url {
                    println!("source_url: {}", source_str.as_deref().unwrap());
                }
            }
        }
        return Ok(());
    }

    // Resolve source: URL download, stdin, or local file
    let _url_temp_dir;
    let _url_file_path;
    let _stdin_temp_dir;
    let _stdin_file_path;
    let source_path = if stdin_mode {
        let mut content = String::new();
        io::stdin().lock().read_to_string(&mut content)?;
        // Stage in a tempdir so `ingest_source` picks up the slug-derived
        // name rather than a random tempfile prefix.
        let slug = slug_from_title_or_h1(title.as_deref(), &content);
        let dir = tempfile::tempdir()?;
        let file_path = dir.path().join(format!("{slug}.md"));
        std::fs::write(&file_path, &content)?;
        _stdin_temp_dir = dir;
        _stdin_file_path = file_path;
        _stdin_file_path.as_path()
    } else {
        let source_str = source_str.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "No source specified.\n  \
                 Usage: lw ingest <file|url> [--category X] [--yes]\n  \
                 Or:    cat file | lw ingest --stdin --title \"Title\" --yes"
            )
        })?;

        if is_url(source_str) {
            let (dir, path) = download_url(source_str)?;
            _url_temp_dir = Some(dir);
            _url_file_path = path;
            url_origin = Some(source_str.to_string());
            _url_file_path.as_path()
        } else {
            let p = Path::new(source_str);
            if !p.exists() {
                anyhow::bail!(
                    "Source file not found: {}\n  Usage: lw ingest <file|url> [--raw-type papers]",
                    p.display()
                );
            }
            p
        }
    };

    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(ingest_source(root, source_path, raw_subdir))?;
    eprintln!("Saved to {}", result.raw_path.display());

    let auto_title = derive_title(title.as_deref(), source_path, url_origin.as_deref());

    // Present metadata (or auto-approve with --yes)
    if !yes {
        eprintln!();
        eprintln!("  Title:    {}", auto_title);
        eprintln!("  Tags:     [{}]", page_tags.join(", "));
        eprintln!("  Category: {}", cat);
        eprintln!("  Decay:    {}", schema.decay_for_category(&cat));
        if let Some(ref url) = url_origin {
            eprintln!("  Source:   {}", url);
        }
        eprintln!();
        eprintln!("  Raw filed. Use a skill command or agent script to create the wiki page.");
    }

    let raw_ref = format!(
        "raw/{}/{}",
        raw_subdir,
        result.raw_path.file_name().unwrap().to_string_lossy()
    );

    let output = IngestOutput {
        path: raw_ref,
        title: auto_title.clone(),
        category: cat.clone(),
        decay: schema.decay_for_category(&cat).to_string(),
        dry_run: false,
    };

    match output_format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Format::Human | Format::Brief => {
            println!("path: {}", output.path);
            println!("title: {}", auto_title);
            println!("category: {}", cat);
        }
    }

    Ok(())
}

/// Derive a title for an ingested page.
/// Priority: explicit --title > H1 from content > URL filename > file stem
fn derive_title(
    explicit_title: Option<&str>,
    source_path: &Path,
    url_origin: Option<&str>,
) -> String {
    if let Some(t) = explicit_title {
        return t.to_string();
    }
    // Try H1 from markdown
    if let Ok(content) = std::fs::read_to_string(source_path)
        && let Some(h1) = extract_h1(&content)
    {
        return h1;
    }
    // URL filename
    if let Some(url) = url_origin {
        return filename_from_url(url);
    }
    // Fall back to file stem
    source_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string())
}

/// Detect if a source string looks like a URL (http:// or https://).
pub fn is_url(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

/// Derive a filename from a URL for saving to raw/.
///
/// Extracts the last path segment, stripping query parameters.
/// Falls back to "download" if no meaningful segment can be derived.
pub fn filename_from_url(url: &str) -> String {
    // Strip scheme
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    // Strip query params and fragment
    let without_query = without_scheme.split('?').next().unwrap_or(without_scheme);
    let without_fragment = without_query.split('#').next().unwrap_or(without_query);

    // Get path portion (after host)
    let path = if let Some(slash_pos) = without_fragment.find('/') {
        &without_fragment[slash_pos..]
    } else {
        ""
    };

    // Get last non-empty segment
    let segment = path
        .trim_end_matches('/')
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("download");

    segment.to_string()
}

/// Download a URL to a temporary directory with a proper filename.
///
/// Returns the temp directory (to keep it alive) and the path to the file.
fn download_url(url: &str) -> anyhow::Result<(tempfile::TempDir, std::path::PathBuf)> {
    eprintln!("Downloading {}...", url);
    let response = ureq::get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to download URL: {}", e))?;

    let dir = tempfile::tempdir()?;
    let filename = filename_from_url(url);
    let file_path = dir.path().join(&filename);
    let mut file = std::fs::File::create(&file_path)?;
    std::io::copy(&mut response.into_body().as_reader(), &mut file)?;
    Ok((dir, file_path))
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
