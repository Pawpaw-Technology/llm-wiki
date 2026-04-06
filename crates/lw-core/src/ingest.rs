use crate::Result;
use crate::llm::LlmBackend;
use crate::page::Page;
use std::path::{Path, PathBuf};

pub struct IngestResult {
    pub raw_path: PathBuf,
    pub draft: Option<Page>,
}

#[tracing::instrument(skip(llm))]
pub async fn ingest_source<L: LlmBackend>(
    wiki_root: &Path,
    source: &Path,
    raw_subdir: &str,
    llm: &L,
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

    // Try LLM draft generation
    let draft = if llm.available() {
        let source_content = std::fs::read_to_string(source).unwrap_or_default();
        let prompt = format!(
            "Read the following source material and generate a wiki page in markdown format.\n\
             The page MUST start with YAML frontmatter containing:\n\
             - title (required)\n\
             - tags (list of relevant tags)\n\
             - decay (fast/normal/evergreen)\n\n\
             Source:\n{}\n\n\
             Generate the wiki page:",
            source_content
        );

        let req = crate::llm::CompletionRequest {
            system: Some(
                "You are a wiki page generator. Output only the markdown page with frontmatter."
                    .to_string(),
            ),
            prompt,
            max_tokens: Some(2000),
        };

        match llm.complete(&req).await {
            Ok(resp) => Page::parse(&resp.text).ok(),
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(IngestResult { raw_path, draft })
}
