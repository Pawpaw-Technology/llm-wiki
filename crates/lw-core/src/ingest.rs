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

#[tracing::instrument]
pub async fn ingest_source(
    wiki_root: &Path,
    source: &Path,
    raw_subdir: &str,
) -> Result<IngestResult> {
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
