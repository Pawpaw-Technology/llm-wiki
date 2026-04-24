use crate::{Result, WikiError};
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub struct IngestResult {
    pub raw_path: PathBuf,
}

/// Write `content` directly into `<wiki_root>/raw/<raw_subdir>/<filename>`.
///
/// This is the MCP-native counterpart to `ingest_source`: agents that already
/// have the full text in-memory (e.g. pasted by the user) can file it without
/// round-tripping through a staging file on disk.
///
/// `filename` must be a plain file name — no path separators, no `..`, no
/// absolute paths. `raw_subdir` is treated as a category name under `raw/`
/// and is validated the same way. Both restrictions exist because the args
/// cross a trust boundary (MCP client → server) and the caller picks them.
#[tracing::instrument(skip(content))]
pub async fn ingest_content(
    wiki_root: &Path,
    raw_subdir: &str,
    filename: &str,
    content: &str,
) -> Result<IngestResult> {
    ensure_single_component("raw_subdir", raw_subdir)?;
    ensure_single_component("filename", filename)?;

    let dest_dir = wiki_root.join("raw").join(raw_subdir);
    std::fs::create_dir_all(&dest_dir)?;
    let raw_path = dest_dir.join(filename);
    std::fs::write(&raw_path, content)?;

    Ok(IngestResult { raw_path })
}

/// Reject anything that isn't a single, ordinary path component:
/// no separators, no `..`, no absolute roots, no empty strings.
fn ensure_single_component(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(WikiError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{field} must not be empty"),
        )));
    }
    let p = Path::new(value);
    let mut comps = p.components();
    match (comps.next(), comps.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(WikiError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid {field}: `{value}` must be a single path component"),
        ))),
    }
}

/// Extract the first non-empty `# ` heading from `content`, if any.
pub fn extract_h1(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(rest) = line.trim().strip_prefix("# ") {
            let rest = rest.trim();
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// Derive a filename slug from an optional title, falling back to the first
/// H1 in `content`, then `"untitled"`. The returned slug is guaranteed
/// non-empty and is suitable for both CLI stdin ingest and MCP content ingest.
pub fn slug_from_title_or_h1(title: Option<&str>, content: &str) -> String {
    use crate::page::slugify;
    title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(slugify)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            extract_h1(content)
                .map(|h| slugify(&h))
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "untitled".to_string())
}

#[tracing::instrument]
pub async fn ingest_source(
    wiki_root: &Path,
    source: &Path,
    raw_subdir: &str,
) -> Result<IngestResult> {
    ensure_single_component("raw_subdir", raw_subdir)?;

    // Copy source to raw/
    let filename = source.file_name().ok_or_else(|| {
        crate::WikiError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "source has no filename",
        ))
    })?;
    let dest_dir = wiki_root.join("raw").join(raw_subdir);
    std::fs::create_dir_all(&dest_dir)?;
    let raw_path = dest_dir.join(filename);
    std::fs::copy(source, &raw_path)?;

    Ok(IngestResult { raw_path })
}
