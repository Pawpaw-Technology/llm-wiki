use crate::fs::{list_pages, load_schema, read_page};
use crate::git::{compute_freshness, page_age_days, FreshnessLevel};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug)]
pub struct WikiStatus {
    pub root: String,
    pub wiki_name: String,
    pub total_pages: usize,
    pub categories: Vec<CategoryStatus>,
    pub freshness: FreshnessDistribution,
    pub index_present: bool,
}

#[derive(Debug)]
pub struct CategoryStatus {
    pub name: String,
    pub page_count: usize,
}

#[derive(Debug, Default)]
pub struct FreshnessDistribution {
    pub fresh: usize,
    pub suspect: usize,
    pub stale: usize,
    pub unknown: usize,
}

#[tracing::instrument]
pub fn gather_status(root: &Path) -> crate::Result<WikiStatus> {
    let schema = load_schema(root)?;
    let wiki_dir = root.join("wiki");
    let pages = list_pages(&wiki_dir)?;

    let mut cat_counts: HashMap<String, usize> = HashMap::new();
    let mut freshness = FreshnessDistribution::default();

    for rel_path in &pages {
        // Category from first path component
        let cat = rel_path
            .iter()
            .next()
            .filter(|_| rel_path.components().count() > 1)
            .map(|c| c.to_string_lossy().to_string())
            .unwrap_or_else(|| "_uncategorized".to_string());
        *cat_counts.entry(cat).or_default() += 1;

        // Freshness
        let abs_path = wiki_dir.join(rel_path);
        let decay = read_page(&abs_path)
            .ok()
            .and_then(|p| p.decay.clone())
            .unwrap_or_else(|| "normal".to_string());

        match page_age_days(&abs_path) {
            Some(age) => {
                let level = compute_freshness(&decay, age, schema.wiki.default_review_days);
                match level {
                    FreshnessLevel::Fresh => freshness.fresh += 1,
                    FreshnessLevel::Suspect => freshness.suspect += 1,
                    FreshnessLevel::Stale => freshness.stale += 1,
                }
            }
            None => freshness.unknown += 1,
        }
    }

    let mut categories: Vec<CategoryStatus> = cat_counts
        .into_iter()
        .map(|(name, page_count)| CategoryStatus { name, page_count })
        .collect();
    categories.sort_by(|a, b| b.page_count.cmp(&a.page_count));

    let index_present = root.join(".lw/search").exists();

    Ok(WikiStatus {
        root: root.display().to_string(),
        wiki_name: schema.wiki.name,
        total_pages: pages.len(),
        categories,
        freshness,
        index_present,
    })
}
