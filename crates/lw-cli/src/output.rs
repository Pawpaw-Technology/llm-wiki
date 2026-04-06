use crate::query::HitWithFreshness;
use lw_core::page::Page;

use serde::Serialize;

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Format {
    Human,
    Json,
    Brief,
}

#[derive(Serialize)]
pub struct QueryEnvelope {
    pub command: String,
    pub query: String,
    pub total: usize,
    pub returned: usize,
    pub results: Vec<QueryResult>,
}

#[derive(Serialize)]
pub struct QueryResult {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub category: String,
    pub snippet: String,
    pub freshness: String,
}

impl QueryResult {
    pub fn from_enriched(enriched: &HitWithFreshness) -> Self {
        Self {
            path: enriched.hit.path.clone(),
            title: enriched.hit.title.clone(),
            tags: enriched.hit.tags.clone(),
            category: enriched.hit.category.clone(),
            snippet: enriched.hit.snippet.clone(),
            freshness: enriched.freshness.to_string(),
        }
    }
}

pub fn print_query_results_with_freshness(
    query: &str,
    hits: &[HitWithFreshness],
    total: usize,
    format: &Format,
) {
    match format {
        Format::Json => {
            let envelope = QueryEnvelope {
                command: "query".into(),
                query: query.into(),
                total,
                returned: hits.len(),
                results: hits.iter().map(QueryResult::from_enriched).collect(),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&envelope)
                    .expect("serialization of string-only struct")
            );
        }
        Format::Human => {
            if hits.is_empty() {
                eprintln!("No results for \"{}\"", query);
                std::process::exit(2);
            }
            println!();
            for (i, enriched) in hits.iter().enumerate() {
                let hit = &enriched.hit;
                let tags = if hit.tags.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", hit.tags.join(", "))
                };
                let suffix = enriched.freshness.suffix();
                println!("  {}. wiki/{}{}{}", i + 1, hit.path, tags, suffix);
                if !hit.snippet.is_empty() {
                    println!("     {}", hit.snippet.trim());
                }
            }
            println!("\n  {} result(s)", total);
        }
        Format::Brief => {
            if hits.is_empty() {
                std::process::exit(2);
            }
            for enriched in hits {
                let hit = &enriched.hit;
                println!(
                    "{}\t{}\t[{}]\t{}",
                    hit.path,
                    hit.title,
                    hit.tags.join(","),
                    enriched.freshness
                );
            }
        }
    }
}

#[derive(Serialize)]
struct PageEnvelope {
    path: String,
    title: String,
    tags: Vec<String>,
    decay: Option<String>,
    sources: Vec<String>,
    body: String,
}

pub fn print_page(path: &str, page: &Page, format: &Format) {
    match format {
        Format::Human => {
            println!("# {}", page.title);
            if !page.tags.is_empty() {
                println!("Tags: {}", page.tags.join(", "));
            }
            if let Some(decay) = &page.decay {
                println!("Decay: {decay}");
            }
            if !page.sources.is_empty() {
                println!("Sources: {}", page.sources.join(", "));
            }
            println!();
            print!("{}", page.body);
        }
        Format::Json => {
            let envelope = PageEnvelope {
                path: path.to_string(),
                title: page.title.clone(),
                tags: page.tags.clone(),
                decay: page.decay.clone(),
                sources: page.sources.clone(),
                body: page.body.clone(),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&envelope)
                    .expect("serialization of string-only struct")
            );
        }
        Format::Brief => {
            println!("{}\t{}\t[{}]", path, page.title, page.tags.join(","));
        }
    }
}
