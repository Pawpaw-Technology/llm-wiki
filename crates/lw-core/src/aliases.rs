//! Title/alias index — answers "which page does this term refer to?"
//!
//! Walks `wiki/`, indexes each page's `title` + `aliases:` frontmatter list +
//! slugified filename under a normalized term, and persists the result to
//! `.lw/aliases/index.json`. Foundation for #42 unlinked-mention detection
//! (#101 matcher → #102 lint rule + #103 MCP suggestions).
//!
//! Storage shape mirrors `backlinks` in spirit but inverts the layout: a
//! single index file per vault rather than per-target sidecars, because the
//! lookup direction is `term → pages` (one-to-many flat) rather than
//! `target → sources` (many-to-one). The sentinel + `.built` short-circuit
//! pattern, the `update_for_page` returning `Vec<PathBuf>` for Option A
//! auto-commit (issue #97 / #99), and the `ensure_index` lazy-build behavior
//! all match `backlinks` so callers downstream can treat the two indexes
//! symmetrically.

use crate::Result;
use crate::backlinks::slug_from_wiki_path;
use crate::fs::{atomic_write, read_page};
use crate::page::Page;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// Relative path (from wiki root) to the alias-index sidecar directory.
pub const ALIASES_DIR: &str = ".lw/aliases";

/// Filename of the persisted index inside `ALIASES_DIR`.
const INDEX_FILE: &str = "index.json";

/// Sentinel written by `rebuild_index`/`save_index` so `ensure_index` can
/// short-circuit without re-walking the wiki — same role as the backlinks
/// `.built` marker (see `backlinks::ensure_index`).
const BUILT_SENTINEL: &str = ".built";

/// One page that a normalized term resolves to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageRef {
    /// Filename stem (e.g. `transformer-architecture`).
    pub slug: String,
    /// Original page title with casing preserved.
    pub title: String,
    /// Path with `wiki/` prefix (e.g. `wiki/architecture/transformer-architecture.md`).
    pub path: String,
}

/// In-memory `term → pages` map. Terms are normalized via [`normalize`].
/// Multiple pages may map to the same term; the consumer (lint, matcher, MCP)
/// decides how to handle ambiguity.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AliasIndex {
    pub terms: BTreeMap<String, Vec<PageRef>>,
}

impl AliasIndex {
    /// Build an in-memory index by walking `wiki_root/wiki/`. Equivalent to
    /// the free `build_index` function — provided as an associated function
    /// so callers can use `AliasIndex::build(...)` per the issue #100 spec.
    pub fn build(wiki_root: &Path) -> Result<Self> {
        build_index(wiki_root)
    }

    /// Look up pages that match the given term. The argument is normalized
    /// internally so callers can pass raw text without preprocessing.
    pub fn lookup(&self, term: &str) -> &[PageRef] {
        let normalized = normalize(term);
        self.terms
            .get(&normalized)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// Normalize a term for indexing/lookup: Unicode NFC + lowercase. Applied to
/// every string before it enters the term map and every lookup query.
pub fn normalize(term: &str) -> String {
    // NFC first so combining sequences collapse before case folding.
    term.nfc().collect::<String>().to_lowercase()
}

/// Walk `wiki_root/wiki/` and build the in-memory alias index.
pub fn build_index(wiki_root: &Path) -> Result<AliasIndex> {
    let mut index = AliasIndex::default();
    let wiki_dir = wiki_root.join("wiki");
    if !wiki_dir.exists() {
        return Ok(index);
    }
    for abs_path in collect_md_files(&wiki_dir)? {
        let rel = abs_path
            .strip_prefix(&wiki_dir)
            .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
        let page = match read_page(&abs_path) {
            Ok(p) => p,
            // Skip unreadable / malformed pages — they show up via lint, not
            // here. Same forgiving behavior as backlinks::build_index.
            Err(_) => continue,
        };
        let (page_ref, terms) = page_terms(rel, &page);
        for term in terms {
            index.terms.entry(term).or_default().push(page_ref.clone());
        }
    }
    Ok(index)
}

/// Rebuild the full index from scratch and persist it to
/// `.lw/aliases/index.json`. Writes the `.built` sentinel on success — only
/// `rebuild_index` writes the sentinel; incremental updates do not, so a
/// partial `update_for_page` against a fresh vault cannot fool a later
/// `ensure_index` into short-circuiting.
pub fn rebuild_index(wiki_root: &Path) -> Result<()> {
    let index = build_index(wiki_root)?;
    save_index(wiki_root, &index)?;
    write_built_sentinel(wiki_root)?;
    Ok(())
}

/// Ensure the alias index has been built. Checks the `.built` sentinel and
/// runs `rebuild_index` only when missing — mirrors `backlinks::ensure_index`.
pub fn ensure_index(wiki_root: &Path) -> Result<()> {
    let sentinel = wiki_root.join(ALIASES_DIR).join(BUILT_SENTINEL);
    if !sentinel.exists() {
        rebuild_index(wiki_root)?;
    }
    Ok(())
}

/// Incrementally update the alias index for a single source page.
///
/// Algorithm:
/// 1. Load the persisted index (or start empty).
/// 2. Strip every existing `PageRef` whose slug matches the source page —
///    this clears stale entries from prior titles/aliases regardless of what
///    they were.
/// 3. If the source file still exists, recompute its current terms and
///    re-insert them.
/// 4. Persist the updated index.
///
/// Returns the absolute paths written so callers can include them in the
/// same auto-commit as the page (Option A pattern from #97; mirrors the
/// `update_after_write` plumbing established by #99). The `.built` sentinel
/// is intentionally NOT included — `is_lw_ephemeral` filters it out of
/// dirty-warnings and it must not be committed.
pub fn update_for_page(wiki_root: &Path, source_rel: &Path) -> Result<Vec<PathBuf>> {
    let mut index = load_index(wiki_root)?.unwrap_or_default();
    // Identity in the index is the full `wiki/<cat>/<file>.md` path, NOT the
    // filename slug — `lw new` permits two pages with the same filename in
    // different categories. Stripping by slug would wipe sibling entries.
    let source_path = wiki_path(source_rel);

    // Strip stale entries for this page. Done unconditionally so a rename
    // or alias-drop in the new version cleanly leaves the old terms behind.
    if !source_path.is_empty() {
        for entries in index.terms.values_mut() {
            entries.retain(|p| p.path != source_path);
        }
        index.terms.retain(|_, v| !v.is_empty());
    }

    // Re-insert if the page still exists on disk.
    let abs_path = wiki_root.join("wiki").join(source_rel);
    if abs_path.exists()
        && let Ok(page) = read_page(&abs_path)
    {
        let (page_ref, terms) = page_terms(source_rel, &page);
        for term in terms {
            index.terms.entry(term).or_default().push(page_ref.clone());
        }
    }

    // Note: do NOT write `.built` here. Incremental updates may run before
    // any full build (e.g. on a fresh vault that just had its first page
    // edited via MCP), and a sentinel from a partial save would cause a
    // later `ensure_index` to skip the rebuild and miss every other page.
    let written = save_index(wiki_root, &index)?;
    Ok(vec![written])
}

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Convert a wiki-relative path (e.g. `tools/foo.md`) to its canonical
/// `wiki/...` form used throughout the index for page identity.
fn wiki_path(rel_path: &Path) -> String {
    format!("wiki/{}", rel_path.to_string_lossy().replace('\\', "/"))
}

/// Compute the `PageRef` and the deduped, normalized term list for a single
/// page given its path relative to `wiki/`.
fn page_terms(rel_path: &Path, page: &Page) -> (PageRef, Vec<String>) {
    let slug = slug_from_wiki_path(rel_path).unwrap_or_default();
    let page_ref = PageRef {
        slug: slug.clone(),
        title: page.title.clone(),
        path: wiki_path(rel_path),
    };

    let mut terms = Vec::new();
    if !slug.is_empty() {
        terms.push(normalize(&slug));
    }
    if !page.title.is_empty() {
        terms.push(normalize(&page.title));
    }
    for alias in &page.aliases {
        if !alias.is_empty() {
            terms.push(normalize(alias));
        }
    }
    // Dedup so a page whose normalized title equals its slug doesn't
    // double-count itself in any single term bucket.
    terms.sort();
    terms.dedup();
    (page_ref, terms)
}

/// Persist the index to `.lw/aliases/index.json`. Does NOT write the
/// `.built` sentinel — that is `rebuild_index`'s exclusive responsibility,
/// so an incremental `update_for_page` against a fresh vault does not
/// produce a partial-but-marked-built index. Returns the index file path so
/// callers (auto-commit) can pin it.
fn save_index(wiki_root: &Path, index: &AliasIndex) -> Result<PathBuf> {
    let dir = wiki_root.join(ALIASES_DIR);
    std::fs::create_dir_all(&dir)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
    let json = serde_json::to_vec_pretty(index)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
    let index_path = dir.join(INDEX_FILE);
    atomic_write(&index_path, &json)?;
    Ok(index_path)
}

/// Write the `.built` sentinel inside `ALIASES_DIR`. Called only by
/// `rebuild_index` after a successful full save — its existence signals
/// "the index reflects every page in the vault, ensure_index can
/// short-circuit". Body is intentionally empty; existence is the signal.
fn write_built_sentinel(wiki_root: &Path) -> Result<()> {
    let dir = wiki_root.join(ALIASES_DIR);
    std::fs::create_dir_all(&dir)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
    atomic_write(&dir.join(BUILT_SENTINEL), b"")?;
    Ok(())
}

/// Load the persisted index, returning `None` when the file is absent.
fn load_index(wiki_root: &Path) -> Result<Option<AliasIndex>> {
    let path = wiki_root.join(ALIASES_DIR).join(INDEX_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
    let index: AliasIndex = serde_json::from_str(&raw)
        .map_err(|e| crate::WikiError::Io(std::io::Error::other(e.to_string())))?;
    Ok(Some(index))
}

fn collect_md_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(dir, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
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
            walk(&path, out)?;
        } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}
