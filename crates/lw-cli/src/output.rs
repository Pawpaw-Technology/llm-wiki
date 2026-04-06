use lw_core::page::Page;
use lw_core::search::SearchHit;
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
}

impl From<&SearchHit> for QueryResult {
    fn from(hit: &SearchHit) -> Self {
        Self {
            path: hit.path.clone(),
            title: hit.title.clone(),
            tags: hit.tags.clone(),
            category: hit.category.clone(),
            snippet: hit.snippet.clone(),
        }
    }
}

pub fn print_query_results(query: &str, hits: &[SearchHit], total: usize, format: &Format) {
    match format {
        Format::Json => {
            let envelope = QueryEnvelope {
                command: "query".into(),
                query: query.into(),
                total,
                returned: hits.len(),
                results: hits.iter().map(QueryResult::from).collect(),
            };
            println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
        }
        Format::Human => {
            if hits.is_empty() {
                eprintln!("No results for \"{}\"", query);
                std::process::exit(2);
            }
            println!();
            for (i, hit) in hits.iter().enumerate() {
                let tags = if hit.tags.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", hit.tags.join(", "))
                };
                println!("  {}. wiki/{}{}", i + 1, hit.path, tags);
                if !hit.snippet.is_empty() {
                    let clean = hit.snippet.replace("<b>", "").replace("</b>", "");
                    println!("     {}", clean.trim());
                }
            }
            println!("\n  {} result(s)", total);
        }
        Format::Brief => {
            if hits.is_empty() {
                std::process::exit(2);
            }
            for hit in hits {
                println!("{}\t{}\t[{}]", hit.path, hit.title, hit.tags.join(","));
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
            println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
        }
        Format::Brief => {
            println!("{}\t{}\t[{}]", path, page.title, page.tags.join(","));
        }
    }
}
