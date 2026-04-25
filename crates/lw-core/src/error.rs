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

    #[error("JSON parse error: {0}")]
    JsonParse(String),

    #[error("invalid frontmatter in {path}: {reason}")]
    Frontmatter { path: PathBuf, reason: String },

    #[error("page not found: {0}")]
    PageNotFound(PathBuf),

    #[error("not a wiki directory: {0} (missing .lw/schema.toml)\n  Run: lw init --root <path>")]
    NotAWiki(PathBuf),

    #[error("git error: {0}")]
    Git(String),

    #[error("path traversal attempt: {0}")]
    PathTraversal(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error(
        "index at {path} is locked by another lw process (likely `lw serve`) — try again once it exits"
    )]
    IndexLocked { path: PathBuf },

    #[error("page already exists: {path}")]
    PageAlreadyExists { path: PathBuf },

    #[error("unknown category: {name} (valid: {valid})")]
    UnknownCategory { name: String, valid: String },

    #[error("category {category} requires field: {field}")]
    MissingRequiredField { category: String, field: String },

    #[error("invalid slug: {slug} (must match [a-z0-9_-]+, no path separators)")]
    InvalidSlug { slug: String },
}

pub type Result<T> = std::result::Result<T, WikiError>;
