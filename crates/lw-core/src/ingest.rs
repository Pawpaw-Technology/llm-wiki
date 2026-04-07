use crate::Result;
use std::path::{Path, PathBuf};

pub struct IngestResult {
    pub raw_path: PathBuf,
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
