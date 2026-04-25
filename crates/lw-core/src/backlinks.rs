//! Backlink index — answers "what links to this page?"
//!
//! Sidecar JSON files at `.lw/backlinks/<target-slug>.json`. Built incrementally
//! on every successful page write; rebuilt on demand by walking the `wiki/` tree.
//!
//! A backlink source is one of:
//! - a `[[wikilink]]` (with optional `|display`) anywhere in another page's body
//! - an entry in another page's frontmatter `related:` list
//!
//! Bare slug mentions (no `[[…]]` brackets) are intentionally OUT OF SCOPE — see
//! issue #42.

use crate::Result;
use crate::fs::{atomic_write, read_page};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\]").expect("WIKILINK_RE is a valid regex"));

/// How a source page references a target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BacklinkKind {
    /// `[[target-slug]]` (or `[[target-slug|display]]`) in body.
    Wikilink,
    /// Frontmatter `related:` entry pointing at the target page.
    Related,
}

/// One inbound reference to a target page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BacklinkSource {
    pub path: String,
    pub kind: BacklinkKind,
    pub context: Option<String>,
}

/// All inbound references to a single target page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BacklinkRecord {
    pub target: String,
    pub sources: Vec<BacklinkSource>,
}

/// Relative path (from wiki root) to the backlink-sidecar directory.
pub const BACKLINKS_DIR: &str = ".lw/backlinks";

/// Extract all `(slug, line)` pairs for every `[[wikilink]]` in `body`.
/// Each tuple contains the resolved slug (pre-pipe portion) and the full line
/// containing the link. Wikilinks inside code fences are included intentionally
/// (Obsidian convention; documented in issue #39).
pub fn extract_link_lines(body: &str) -> Vec<(String, String)> {
    let mut results = Vec::new();
    for line in body.lines() {
        for cap in WIKILINK_RE.captures_iter(line) {
            let raw = cap[1].trim();
            let slug = match raw.split_once('|') {
                Some((s, _)) => s.trim().to_string(),
                None => raw.to_string(),
            };
            if !slug.is_empty() {
                results.push((slug, line.to_string()));
            }
        }
    }
    results
}

/// Build a context snippet centred on the first occurrence of `[[slug` in `line`.
/// For short lines the full line is returned. For longer lines, a ~80-character
/// window around the match is extracted and ellipsis added where truncated.
pub fn snippet_for(line: &str, slug: &str) -> String {
    if line.is_empty() {
        return String::new();
    }
    let needle = format!("[[{slug}");
    let Some(pos) = line.find(&needle) else {
        return line.to_string();
    };

    const RADIUS: usize = 80;
    if line.len() <= RADIUS * 2 {
        return line.to_string();
    }

    let start = pos.saturating_sub(RADIUS / 2);
    let end = (pos + needle.len() + RADIUS / 2).min(line.len());

    // Clamp to valid char boundaries
    let char_start = line
        .char_indices()
        .map(|(i, _)| i)
        .rfind(|&i| i <= start)
        .unwrap_or(0);
    let char_end = line
        .char_indices()
        .map(|(i, _)| i)
        .find(|&i| i >= end)
        .unwrap_or(line.len());

    let mut snip = line[char_start..char_end].to_string();
    if char_start > 0 {
        snip = format!("…{snip}");
    }
    if char_end < line.len() {
        snip = format!("{snip}…");
    }
    snip
}

/// Derive the slug from a path relative to wiki/ (e.g. `"architecture/foo.md"` → `"foo"`).
/// Returns `None` for empty paths or paths without a file stem.
pub fn slug_from_wiki_path(rel_path: &Path) -> Option<String> {
    let stem = rel_path.file_stem()?;
    let s = stem.to_str()?;
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Walk the `wiki/` subtree and build the full in-memory backlink map.
/// Returns `BTreeMap<target_slug, Vec<BacklinkSource>>`.
pub fn build_index(wiki_root: &Path) -> Result<BTreeMap<String, Vec<BacklinkSource>>> {
    let wiki_dir = wiki_root.join("wiki");
    let mut map: BTreeMap<String, Vec<BacklinkSource>> = BTreeMap::new();

    if !wiki_dir.exists() {
        return Ok(map);
    }

    for entry in walkdir(&wiki_dir)? {
        let abs_path = entry;
        let rel = abs_path
            .strip_prefix(&wiki_dir)
            .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;

        if rel.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        // Path stored in BacklinkSource uses "wiki/" prefix per spec.
        let source_path = format!("wiki/{}", rel.to_string_lossy().replace('\\', "/"));

        let page = match read_page(&abs_path) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Body wikilinks
        let pairs = extract_link_lines(&page.body);
        for (slug, line) in pairs {
            let ctx = snippet_for(&line, &slug);
            map.entry(slug).or_default().push(BacklinkSource {
                path: source_path.clone(),
                kind: BacklinkKind::Wikilink,
                context: Some(ctx),
            });
        }

        // Frontmatter related: list — extract slug from path like "tools/bar.md"
        if let Some(related) = &page.related {
            for rel_entry in related {
                let slug = Path::new(rel_entry)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(String::from);
                if let Some(slug) = slug {
                    map.entry(slug).or_default().push(BacklinkSource {
                        path: source_path.clone(),
                        kind: BacklinkKind::Related,
                        context: None,
                    });
                }
            }
        }
    }

    Ok(map)
}

/// Persist a `BTreeMap<target_slug, sources>` as individual sidecar JSON files.
/// Files for slugs with no sources are not written (callers remove them explicitly).
pub fn write_index(wiki_root: &Path, map: &BTreeMap<String, Vec<BacklinkSource>>) -> Result<()> {
    let dir = wiki_root.join(BACKLINKS_DIR);
    std::fs::create_dir_all(&dir)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;

    for (target, sources) in map {
        if sources.is_empty() {
            continue;
        }
        let record = BacklinkRecord {
            target: target.clone(),
            sources: sources.clone(),
        };
        let json = serde_json::to_vec_pretty(&record)
            .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
        let path = dir.join(format!("{target}.json"));
        atomic_write(&path, &json)?;
    }
    Ok(())
}

/// Rebuild the full backlink index from scratch: walk wiki/, build map, persist sidecars.
pub fn rebuild_index(wiki_root: &Path) -> Result<()> {
    let map = build_index(wiki_root)?;
    write_index(wiki_root, &map)
}

/// Ensure the backlink index directory exists and contains at least one file.
/// If the directory is absent or empty, runs a full `rebuild_index`.
pub fn ensure_index(wiki_root: &Path) -> Result<()> {
    let dir = wiki_root.join(BACKLINKS_DIR);
    let needs_build = if dir.exists() {
        // Consider empty dir as needing a build
        std::fs::read_dir(&dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true)
    } else {
        true
    };
    if needs_build {
        rebuild_index(wiki_root)?;
    }
    Ok(())
}

/// Incrementally update sidecar files for the targets referenced by `source_rel`
/// (a path relative to `wiki_root/wiki/`).
///
/// Algorithm:
/// 1. Read the new content of `source_rel` to obtain its current outbound links.
/// 2. For every target whose sidecar may be affected by this source, reload the
///    sidecar, remove the old source entry, and either write the updated sidecar
///    or delete the file if no sources remain.
pub fn update_for_page(wiki_root: &Path, source_rel: &Path) -> Result<()> {
    let abs_path = wiki_root.join("wiki").join(source_rel);
    let source_path_str = format!("wiki/{}", source_rel.to_string_lossy().replace('\\', "/"));

    // Collect the new outbound link slugs from this source
    let new_slugs: Vec<String> = if abs_path.exists() {
        let page = match read_page(&abs_path) {
            Ok(p) => p,
            Err(_) => return Ok(()),
        };
        let mut slugs: Vec<String> = extract_link_lines(&page.body)
            .into_iter()
            .map(|(slug, _)| slug)
            .collect();
        // Also add related: slugs
        if let Some(related) = &page.related {
            for r in related {
                if let Some(slug) = Path::new(r)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(String::from)
                {
                    slugs.push(slug);
                }
            }
        }
        slugs
    } else {
        Vec::new()
    };

    let backlinks_dir = wiki_root.join(BACKLINKS_DIR);

    // Gather all slugs that previously had this source (by scanning existing sidecars).
    let old_slugs: Vec<String> = if backlinks_dir.exists() {
        let mut v = Vec::new();
        for entry in std::fs::read_dir(&backlinks_dir)
            .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?
        {
            let entry =
                entry.map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
                if let Ok(record) = serde_json::from_str::<BacklinkRecord>(&raw)
                    && record.sources.iter().any(|s| s.path == source_path_str)
                {
                    v.push(record.target);
                }
            }
        }
        v
    } else {
        Vec::new()
    };

    // Union of old and new slugs — sidecars that need updating
    let mut all_slugs: Vec<String> = old_slugs;
    for slug in &new_slugs {
        if !all_slugs.contains(slug) {
            all_slugs.push(slug.clone());
        }
    }

    std::fs::create_dir_all(&backlinks_dir)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;

    // Re-read current page for snippet generation
    let page = if abs_path.exists() {
        read_page(&abs_path).ok()
    } else {
        None
    };

    for slug in all_slugs {
        let sidecar = sidecar_path(wiki_root, &slug);

        // Load existing record (if any) and strip old entries from this source
        let mut existing_sources: Vec<BacklinkSource> = if sidecar.exists() {
            let raw = std::fs::read_to_string(&sidecar)
                .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
            serde_json::from_str::<BacklinkRecord>(&raw)
                .map(|r| r.sources)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        existing_sources.retain(|s| s.path != source_path_str);

        // Add new entries if this slug is still referenced
        if new_slugs.contains(&slug)
            && let Some(ref p) = page
        {
            // Wikilink entries
            for (link_slug, line) in extract_link_lines(&p.body) {
                if link_slug == slug {
                    let ctx = snippet_for(&line, &slug);
                    existing_sources.push(BacklinkSource {
                        path: source_path_str.clone(),
                        kind: BacklinkKind::Wikilink,
                        context: Some(ctx),
                    });
                }
            }
            // Related: entries
            if let Some(related) = &p.related {
                for r in related {
                    let r_slug = Path::new(r)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(String::from);
                    if r_slug.as_deref() == Some(slug.as_str()) {
                        existing_sources.push(BacklinkSource {
                            path: source_path_str.clone(),
                            kind: BacklinkKind::Related,
                            context: None,
                        });
                    }
                }
            }
        }

        if existing_sources.is_empty() {
            // Remove sidecar when no sources remain
            if sidecar.exists() {
                std::fs::remove_file(&sidecar)
                    .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
            }
        } else {
            let record = BacklinkRecord {
                target: slug,
                sources: existing_sources,
            };
            let json = serde_json::to_vec_pretty(&record)
                .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
            atomic_write(&sidecar, &json)?;
        }
    }

    Ok(())
}

/// Query the backlink index for a given target slug.
/// Returns `None` when no sidecar exists (i.e. no pages link to this target).
pub fn query(wiki_root: &Path, target: &str) -> Result<Option<BacklinkRecord>> {
    let path = sidecar_path(wiki_root, target);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
    let record: BacklinkRecord = serde_json::from_str(&raw)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
    Ok(Some(record))
}

/// Path to the sidecar file for a given target slug.
pub fn sidecar_path(wiki_root: &Path, target: &str) -> PathBuf {
    wiki_root.join(BACKLINKS_DIR).join(format!("{target}.json"))
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Collect all markdown file paths (absolute) under `wiki_dir`.
fn walkdir(wiki_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    collect_md(wiki_dir, &mut result)?;
    Ok(result)
}

fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let read_dir = std::fs::read_dir(dir)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
    for entry in read_dir {
        let entry =
            entry.map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
        if file_type.is_dir() {
            collect_md(&path, out)?;
        } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}
