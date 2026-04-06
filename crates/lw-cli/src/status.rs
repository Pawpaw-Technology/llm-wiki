use crate::output::Format;
use lw_core::status::gather_status;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct StatusJson {
    command: String,
    root: String,
    wiki_name: String,
    total_pages: usize,
    categories: Vec<CategoryJson>,
    freshness: FreshnessJson,
    index_present: bool,
}

#[derive(Serialize)]
struct CategoryJson {
    name: String,
    page_count: usize,
}

#[derive(Serialize)]
struct FreshnessJson {
    fresh: usize,
    suspect: usize,
    stale: usize,
    unknown: usize,
}

pub fn run(root: &Path, format: &Format) -> anyhow::Result<()> {
    let status = gather_status(root)?;

    match format {
        Format::Json => {
            let json = StatusJson {
                command: "status".into(),
                root: status.root,
                wiki_name: status.wiki_name,
                total_pages: status.total_pages,
                categories: status
                    .categories
                    .iter()
                    .map(|c| CategoryJson {
                        name: c.name.clone(),
                        page_count: c.page_count,
                    })
                    .collect(),
                freshness: FreshnessJson {
                    fresh: status.freshness.fresh,
                    suspect: status.freshness.suspect,
                    stale: status.freshness.stale,
                    unknown: status.freshness.unknown,
                },
                index_present: status.index_present,
            };
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        Format::Human => {
            println!();
            println!("  Wiki:   {} ({})", status.wiki_name, status.root);
            println!("  Pages:  {}", status.total_pages);
            println!(
                "  Index:  {}",
                if status.index_present {
                    "present"
                } else {
                    "missing"
                }
            );
            println!();
            println!("  Freshness:");
            let total = status.total_pages.max(1);
            println!(
                "    FRESH {:<4}  SUSPECT {:<4}  STALE {:<4}  unknown {:<4}",
                status.freshness.fresh,
                status.freshness.suspect,
                status.freshness.stale,
                status.freshness.unknown,
            );
            // Bar visualization
            let bar_width = 40;
            let fresh_w = (status.freshness.fresh * bar_width) / total;
            let suspect_w = (status.freshness.suspect * bar_width) / total;
            let stale_w = (status.freshness.stale * bar_width) / total;
            let unknown_w = bar_width - fresh_w - suspect_w - stale_w;
            println!(
                "    [{}{}{}{}]",
                "=".repeat(fresh_w),
                "~".repeat(suspect_w),
                "!".repeat(stale_w),
                ".".repeat(unknown_w),
            );
            println!();
            println!("  Categories:");
            for cat in &status.categories {
                println!("    {:<20} {:>4} pages", cat.name, cat.page_count);
            }
            println!();
        }
        Format::Brief => {
            println!(
                "{}\t{}\t{}\t{}/{}/{}/{}",
                status.root,
                status.wiki_name,
                status.total_pages,
                status.freshness.fresh,
                status.freshness.suspect,
                status.freshness.stale,
                status.freshness.unknown,
            );
        }
    }
    Ok(())
}
