use crate::page::Page;
use crate::schema::WikiSchema;
use crate::{Result, WikiError};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Write `body` to `path` atomically: stage to a unique temp file in the same
/// directory, fsync the data, rename into place, then fsync the parent directory
/// (Unix only). This mirrors the pattern in `Config::save_to` from lw-cli.
///
/// Using a randomly-named temp file (`NamedTempFile::new_in`) means we never
/// follow a pre-placed victim symlink at a predictable temp path, and a crash
/// between the write and the rename leaves only a stale temp file rather than a
/// truncated page.
#[tracing::instrument(skip(body))]
pub fn atomic_write(path: &Path, body: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(body)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;

    atomic_write_sync_parent(parent)?;
    Ok(())
}

#[cfg(unix)]
fn atomic_write_sync_parent(parent: &Path) -> Result<()> {
    let dir = std::fs::File::open(parent)?;
    dir.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn atomic_write_sync_parent(_parent: &Path) -> Result<()> {
    Ok(())
}

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
    atomic_write(path, page.to_markdown().as_bytes())
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
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            walk_md(base, &path, out)?;
        } else if file_type.is_file()
            && path.extension().is_some_and(|ext| ext == "md")
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

/// Request parameters for creating a new wiki page.
pub struct NewPageRequest<'a> {
    pub category: &'a str,
    pub slug: &'a str,
    pub title: String,
    pub tags: Vec<String>,
    pub author: Option<String>,
}

/// Create a new wiki page with schema-enforced frontmatter and body template.
///
/// Validates the slug, checks the category against the schema, enforces required
/// fields, refuses to overwrite an existing file, then calls `write_page` (which
/// uses `atomic_write` internally).
///
/// # Errors
///
/// - `InvalidSlug` — slug is empty, contains `/` or `..`, starts with `.`, or
///   contains characters outside `[a-z0-9_-]`
/// - `UnknownCategory` — category is not `_uncategorized` and not listed in
///   `schema.tags.categories`
/// - `MissingRequiredField` — a field declared in the category's `required_fields`
///   is not satisfied by the request
/// - `PageAlreadyExists` — a file already exists at the computed path
#[tracing::instrument(skip_all)]
pub fn new_page(
    wiki_root: &Path,
    schema: &WikiSchema,
    req: NewPageRequest<'_>,
) -> Result<(PathBuf, Page)> {
    // Step 1: validate slug — must match ^[a-z0-9_-]+$
    validate_slug(req.slug)?;

    // Step 2: validate category
    let category = req.category;
    if category != "_uncategorized" && !schema.tags.categories.contains(&category.to_string()) {
        let valid = schema.tags.categories.join(", ");
        return Err(WikiError::UnknownCategory {
            name: category.to_string(),
            valid,
        });
    }

    // Step 3: look up CategoryConfig (None → empty template, no required fields)
    let (template, required_fields) = match schema.category_config(category) {
        Some(cfg) => (cfg.template.clone(), cfg.required_fields.clone()),
        None => (String::new(), Vec::new()),
    };

    // Step 4: check required fields
    for field in &required_fields {
        let satisfied = match field.as_str() {
            "title" => !req.title.is_empty(),
            "tags" => !req.tags.is_empty(),
            "author" => req.author.is_some(),
            // Any field not representable in NewPageRequest is unsatisfiable
            _ => false,
        };
        if !satisfied {
            return Err(WikiError::MissingRequiredField {
                category: category.to_string(),
                field: field.clone(),
            });
        }
    }

    // Step 5: compute target path; refuse to overwrite.
    //
    // Surface the error with a VAULT-RELATIVE path (issue #87) — the Display
    // string flows out through CLI stderr (#60) and MCP error JSON (#61),
    // and agents that feed the path back into wiki_read/wiki_write expect
    // vault-relative input. strip_prefix should always succeed here since
    // `target` was just built from `wiki_root.join(...)`; the fallback is
    // defensive in case a future caller passes a non-canonical wiki_root.
    let target = wiki_root
        .join("wiki")
        .join(category)
        .join(format!("{}.md", req.slug));
    if target.exists() {
        let rel = target
            .strip_prefix(wiki_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| target.clone());
        return Err(WikiError::PageAlreadyExists { path: rel });
    }

    // Step 6: build Page
    let page = Page {
        title: req.title,
        tags: req.tags,
        decay: None,
        sources: vec![],
        author: req.author,
        generator: None,
        related: None,
        status: None,
        body: template,
    };

    // Step 7: write atomically via write_page (which calls atomic_write internally)
    write_page(&target, &page)?;

    // Step 8: return path + page
    Ok((target, page))
}

/// Validate that a slug matches `^[a-z0-9_-]+$` and contains no path-separator
/// sequences (`/`, `..`, leading `.`).
fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        return Err(WikiError::InvalidSlug {
            slug: slug.to_string(),
        });
    }
    // Reject leading dot (hidden files / relative-path abuse)
    if slug.starts_with('.') {
        return Err(WikiError::InvalidSlug {
            slug: slug.to_string(),
        });
    }
    // Reject any character outside [a-z0-9_-]
    let valid = slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !valid {
        return Err(WikiError::InvalidSlug {
            slug: slug.to_string(),
        });
    }
    Ok(())
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
