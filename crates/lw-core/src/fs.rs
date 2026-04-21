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

/// Extract category from a wiki-relative path (first path component).
/// e.g. "architecture/transformer.md" → Some("architecture")
/// e.g. "test.md" → None (no category directory)
pub fn category_from_path(rel_path: &Path) -> Option<String> {
    let mut components = rel_path.components();
    let first = components.next()?;
    // Only return category if there's at least one more component (the filename)
    components.next()?;
    Some(first.as_os_str().to_string_lossy().to_string())
}

/// Validate that a relative path is safe to use within the wiki directory.
/// Returns the absolute path if valid, or a `PathTraversal` error if the path
/// attempts to escape the `wiki_root/wiki/` directory.
///
/// Rejects:
/// - Paths containing `..` components
/// - Absolute paths
/// - Any path whose canonical form would land outside `wiki_root/wiki/`
#[tracing::instrument]
pub fn validate_wiki_path(wiki_root: &Path, relative_path: &str) -> Result<PathBuf> {
    let rel = Path::new(relative_path);

    // Reject absolute paths
    if rel.is_absolute() {
        return Err(WikiError::PathTraversal(
            "absolute paths are not allowed".to_string(),
        ));
    }

    // Reject any path component that is ".."
    for component in rel.components() {
        if let std::path::Component::ParentDir = component {
            return Err(WikiError::PathTraversal(
                "path must not contain '..' components".to_string(),
            ));
        }
    }

    let wiki_dir = wiki_root.join("wiki");
    let resolved = wiki_dir.join(rel);

    // If the path exists on disk, canonicalize to resolve symlinks.
    // For new pages that don't exist yet, canonicalize the closest existing
    // ancestor and append the remaining components. This avoids false positives
    // on platforms where temp paths traverse symlinks (e.g. macOS /tmp →
    // /private/tmp).
    let check_path = if resolved.exists() {
        resolved.canonicalize().unwrap_or_else(|_| resolved.clone())
    } else {
        canonicalize_ancestor(&resolved)
    };
    let check_base = if wiki_dir.exists() {
        wiki_dir.canonicalize().unwrap_or_else(|_| wiki_dir.clone())
    } else {
        canonicalize_ancestor(&wiki_dir)
    };

    if !check_path.starts_with(&check_base) {
        return Err(WikiError::PathTraversal(
            "resolved path is outside the wiki directory".to_string(),
        ));
    }

    Ok(resolved)
}

/// Canonicalize a path that may not fully exist on disk by walking up to the
/// closest existing ancestor, canonicalizing it, and re-appending the
/// non-existent tail components.
///
/// This is useful when a caller wants a stable, symlink-resolved identity for
/// a path that will be created later (e.g. `/tmp/wp` on macOS, where `/tmp`
/// is a symlink to `/private/tmp` — a plain absolute path passthrough would
/// treat `/tmp/wp` and `/private/tmp/wp` as different, but both canonicalize
/// to `/private/tmp/wp`).
pub fn canonicalize_ancestor(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            tail.push(name.to_os_string());
        } else {
            // Reached filesystem root or an unparseable path — give up
            return path.to_path_buf();
        }
        existing.pop();
    }
    let mut result = existing.canonicalize().unwrap_or_else(|_| existing.clone());
    for component in tail.into_iter().rev() {
        result.push(component);
    }
    result
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
