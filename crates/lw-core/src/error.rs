use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum WikiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("index error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("index directory error: {0}")]
    OpenDirectory(#[from] tantivy::directory::error::OpenDirectoryError),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("YAML parse error: {0}")]
    YamlParse(String),

    #[error("invalid frontmatter in {path}: {reason}")]
    Frontmatter { path: PathBuf, reason: String },

    #[error("page not found: {0}")]
    PageNotFound(PathBuf),

    #[error("schema not found: {0}")]
    SchemaNotFound(PathBuf),

    #[error("not a wiki directory: {0} (missing .lw/schema.toml)\n  Run: lw init --root <path>")]
    NotAWiki(PathBuf),

    #[error("LLM backend unavailable")]
    LlmUnavailable,

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("git error: {0}")]
    Git(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, WikiError>;
