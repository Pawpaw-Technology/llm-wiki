use crate::page::Page;
use crate::schema::WikiSchema;
use crate::{Result, WikiError};
use std::path::{Path, PathBuf};

#[tracing::instrument(skip(schema))]
pub fn init_wiki(root: &Path, schema: &WikiSchema) -> Result<()> {
    let lw_dir = root.join(".lw");
    std::fs::create_dir_all(&lw_dir)?;
    std::fs::write(lw_dir.join("schema.toml"), schema.to_toml())?;
    for cat in schema.category_dirs() {
        std::fs::create_dir_all(root.join("wiki").join(&cat))?;
    }
    for sub in &["papers", "articles", "assets"] {
        std::fs::create_dir_all(root.join("raw").join(sub))?;
    }
    Ok(())
}

#[tracing::instrument]
pub fn read_page(path: &Path) -> Result<Page> {
    let content =
        std::fs::read_to_string(path).map_err(|_| WikiError::PageNotFound(path.to_path_buf()))?;
    Page::parse(&content).map_err(|e| match e {
        WikiError::YamlParse(reason) => WikiError::Frontmatter {
            path: path.to_path_buf(),
            reason,
        },
        other => other,
    })
}

#[tracing::instrument(skip(page))]
pub fn write_page(path: &Path, page: &Page) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, page.to_markdown())?;
    Ok(())
}

#[tracing::instrument]
pub fn list_pages(wiki_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut pages = Vec::new();
    walk_md(wiki_dir, wiki_dir, &mut pages)?;
    pages.sort();
    Ok(pages)
}

fn walk_md(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_md(base, &path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "md")
            && let Ok(rel) = path.strip_prefix(base)
        {
            out.push(rel.to_path_buf());
        }
    }
    Ok(())
}

#[tracing::instrument]
pub fn load_schema(root: &Path) -> Result<WikiSchema> {
    let schema_path = root.join(".lw/schema.toml");
    if !schema_path.exists() {
        return Err(WikiError::NotAWiki(root.to_path_buf()));
    }
    let content = std::fs::read_to_string(&schema_path)?;
    WikiSchema::parse(&content)
}

pub fn category_from_path(rel_path: &Path) -> Option<String> {
    rel_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
}

/// Walk up from `start` to find the wiki root (directory containing `.lw/schema.toml`).
/// Similar to how git finds `.git/`.
#[tracing::instrument]
pub fn discover_wiki_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if current.join(".lw/schema.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}
