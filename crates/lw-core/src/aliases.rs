//! Title/alias index — answers "which page does this term refer to?"
//!
//! Walks `wiki/`, indexes each page's `title` + `aliases:` frontmatter list +
//! slugified filename under a normalized term, and persists the result to
//! `.lw/aliases/index.json`. Foundation for #42 unlinked-mention detection
//! (#101 matcher → #102 lint rule + #103 MCP suggestions).
//!
//! Stub — implementation lands as part of issue #100. The signatures here are
//! the public contract that tests pin down.

use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Relative path (from wiki root) to the alias-index sidecar directory.
pub const ALIASES_DIR: &str = ".lw/aliases";

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
    /// Look up pages that match the given term. The argument is normalized
    /// internally so callers can pass raw text without preprocessing.
    pub fn lookup(&self, _term: &str) -> &[PageRef] {
        unimplemented!("issue #100")
    }
}

/// Normalize a term for indexing/lookup: lowercase + Unicode NFC.
pub fn normalize(_term: &str) -> String {
    unimplemented!("issue #100")
}

/// Walk `wiki_root/wiki/` and build the in-memory alias index.
pub fn build_index(_wiki_root: &Path) -> Result<AliasIndex> {
    unimplemented!("issue #100")
}

/// Rebuild the full index from scratch and persist it to
/// `.lw/aliases/index.json`. Writes the `.built` sentinel on success.
pub fn rebuild_index(_wiki_root: &Path) -> Result<()> {
    unimplemented!("issue #100")
}

/// Ensure the alias index has been built. Checks the `.built` sentinel and
/// runs `rebuild_index` only when missing — mirrors `backlinks::ensure_index`.
pub fn ensure_index(_wiki_root: &Path) -> Result<()> {
    unimplemented!("issue #100")
}

/// Incrementally update the alias index for a single source page.
/// Returns the sidecar paths written so callers can include them in the same
/// auto-commit as the page (Option A pattern from #97; mirrors the
/// `update_after_write` plumbing established by #99).
pub fn update_for_page(_wiki_root: &Path, _source_rel: &Path) -> Result<Vec<PathBuf>> {
    unimplemented!("issue #100")
}
