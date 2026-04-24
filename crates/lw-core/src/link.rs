use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static WIKI_LINK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());

pub fn extract_wiki_links(body: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut links = Vec::new();
    for cap in WIKI_LINK_RE.captures_iter(body) {
        let raw = cap[1].trim();
        // Handle [[slug|display text]] pipe syntax: take only the slug
        let target = match raw.split_once('|') {
            Some((slug, _display)) => slug.trim().to_string(),
            None => raw.to_string(),
        };
        if seen.insert(target.clone()) {
            links.push(target);
        }
    }
    links
}

pub fn resolve_link(target: &str, wiki_dir: &Path) -> Option<PathBuf> {
    let filename = format!("{}.md", target);
    let entries = std::fs::read_dir(wiki_dir).ok()?;
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        let candidate = path.join(&filename);
        if candidate
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_file())
        {
            let cat = path.file_name()?;
            return Some(PathBuf::from(cat).join(&filename));
        }
    }
    None
}

pub fn find_broken_links(body: &str, wiki_dir: &Path) -> Vec<String> {
    extract_wiki_links(body)
        .into_iter()
        .filter(|target| resolve_link(target, wiki_dir).is_none())
        .collect()
}
