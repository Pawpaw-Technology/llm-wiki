use crate::page::Page;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Taxonomy {
    tag_to_pages: HashMap<String, Vec<String>>,
}

impl Taxonomy {
    pub fn from_pages(pages: &[Page]) -> Self {
        let mut tag_to_pages: HashMap<String, Vec<String>> = HashMap::new();
        for page in pages {
            for tag in &page.tags {
                tag_to_pages
                    .entry(tag.clone())
                    .or_default()
                    .push(page.title.clone());
            }
        }
        Self { tag_to_pages }
    }

    pub fn tag_count(&self, tag: &str) -> usize {
        self.tag_to_pages.get(tag).map(|v| v.len()).unwrap_or(0)
    }

    pub fn all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self.tag_to_pages.keys().cloned().collect();
        tags.sort();
        tags
    }

    pub fn pages_with_tag(&self, tag: &str) -> Vec<String> {
        self.tag_to_pages.get(tag).cloned().unwrap_or_default()
    }

    pub fn tag_counts(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = self
            .tag_to_pages
            .iter()
            .map(|(k, v)| (k.clone(), v.len()))
            .collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        counts
    }
}
