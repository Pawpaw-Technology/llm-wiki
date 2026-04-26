//! Unlinked-mention matcher — flag terms in body text that match an indexed
//! page but are not already wrapped in `[[…]]`. Stub for issue #101.

use crate::aliases::AliasIndex;
use serde::{Deserialize, Serialize};

/// Maximum number of tokens to consider for a multi-word title lookup.
pub const MAX_WINDOW_TOKENS: usize = 4;

/// One unlinked mention found in a body. Multiple are emitted for ambiguous
/// matches (a term that resolves to several pages).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnlinkedMention {
    /// The exact text in the body that matched (preserving original casing /
    /// whitespace as a single normalized space between tokens).
    pub term: String,
    /// Slug of the page the term resolves to.
    pub target_slug: String,
    /// 1-based line number in `body` where the match occurs.
    pub line: u32,
    /// Snippet of the surrounding line (centred on the match, ~80 chars).
    pub context: String,
}

/// Scan `body` for terms that resolve to entries in `index` but are not yet
/// wrapped in `[[…]]` wikilinks. Honours the rules in the issue #101 spec.
pub fn find_unlinked_mentions(
    _body: &str,
    _index: &AliasIndex,
    _self_slug: &str,
) -> Vec<UnlinkedMention> {
    unimplemented!("issue #101")
}
