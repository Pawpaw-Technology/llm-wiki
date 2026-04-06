use crate::output::{self, Format};
use std::path::Path;

pub fn run(root: &Path, path: &str, format: &Format) -> anyhow::Result<()> {
    let abs_path = root.join("wiki").join(path);
    let page = lw_core::fs::read_page(&abs_path)?;
    output::print_page(path, &page, format);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a minimal wiki structure with a valid page.
    fn setup_wiki(tmp: &TempDir) -> std::path::PathBuf {
        let root = tmp.path().to_path_buf();
        // .lw/schema.toml so discover_wiki_root works
        fs::create_dir_all(root.join(".lw")).unwrap();
        fs::write(root.join(".lw/schema.toml"), "[wiki]\nname = \"test\"\n").unwrap();
        // wiki/architecture/transformer.md with valid frontmatter
        let page_dir = root.join("wiki/architecture");
        fs::create_dir_all(&page_dir).unwrap();
        fs::write(
            page_dir.join("transformer.md"),
            "---\ntitle: Flash Attention 2\ntags:\n  - architecture\n  - attention\ndecay: normal\n---\n\nFlash Attention 2 reduces memory usage.\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn test_read_existing_page() {
        let tmp = TempDir::new().unwrap();
        let root = setup_wiki(&tmp);
        let result = run(&root, "architecture/transformer.md", &Format::Human);
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    }

    #[test]
    fn test_read_nonexistent_page() {
        let tmp = TempDir::new().unwrap();
        let root = setup_wiki(&tmp);
        let result = run(&root, "nonexistent/page.md", &Format::Human);
        assert!(result.is_err(), "expected error for nonexistent page");
    }

    #[test]
    fn test_read_json_format() {
        let tmp = TempDir::new().unwrap();
        let root = setup_wiki(&tmp);
        let result = run(&root, "architecture/transformer.md", &Format::Json);
        assert!(
            result.is_ok(),
            "expected Ok for JSON format, got: {:?}",
            result
        );
    }

    #[test]
    fn test_read_brief_format() {
        let tmp = TempDir::new().unwrap();
        let root = setup_wiki(&tmp);
        let result = run(&root, "architecture/transformer.md", &Format::Brief);
        assert!(
            result.is_ok(),
            "expected Ok for Brief format, got: {:?}",
            result
        );
    }
}
