pub mod backlinks;
pub mod error;
pub mod fs;
pub mod git;
pub mod import;
pub mod ingest;
pub mod journal;
pub mod link;
pub mod lint;
pub mod page;
pub mod schema;
pub mod search;
pub mod section;
pub mod status;
pub mod tag;
pub use error::{Result, WikiError};

/// Relative path (from wiki root) to the tantivy search index directory.
///
/// All crates (CLI, MCP, status, tests) **must** use this constant instead of
/// hard-coding the path so the index location stays consistent.
pub const INDEX_DIR: &str = ".lw/search";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_dir_is_under_lw() {
        assert!(
            INDEX_DIR.starts_with(".lw/"),
            "INDEX_DIR must live under the .lw metadata directory"
        );
    }

    #[test]
    fn index_dir_is_search() {
        assert_eq!(
            INDEX_DIR, ".lw/search",
            "canonical index path must be .lw/search"
        );
    }
}
