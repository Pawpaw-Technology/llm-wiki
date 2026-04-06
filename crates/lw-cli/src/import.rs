use crate::output::Format;
use lw_core::fs::{load_schema, write_page};
use lw_core::import::parse_twitter_json;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct ImportOutput {
    imported: usize,
    skipped: usize,
    total: usize,
    category: String,
    pages: Vec<ImportedPageInfo>,
}

#[derive(Serialize)]
struct ImportedPageInfo {
    path: String,
    title: String,
    source_id: String,
}

pub fn run(
    root: &Path,
    file: &Path,
    format: &str,
    category: &str,
    limit: Option<usize>,
    dry_run: bool,
    output_format: &Format,
) -> anyhow::Result<()> {
    let _schema = load_schema(root)?;

    if !file.exists() {
        anyhow::bail!(
            "Source file not found: {}\n  Usage: lw import <file> --format twitter-json",
            file.display()
        );
    }

    let content = std::fs::read_to_string(file)?;

    let pages = match format {
        "twitter-json" => parse_twitter_json(&content, limit)?,
        _ => anyhow::bail!("Unknown format: {}\n  Supported: twitter-json", format),
    };

    let total = pages.len();

    if dry_run {
        let page_infos: Vec<ImportedPageInfo> = pages
            .iter()
            .map(|p| ImportedPageInfo {
                path: format!("wiki/{}/{}.md", category, p.slug),
                title: p.title.clone(),
                source_id: p.source_id.clone(),
            })
            .collect();

        let output = ImportOutput {
            imported: 0,
            skipped: total,
            total,
            category: category.to_string(),
            pages: page_infos,
        };

        match output_format {
            Format::Json => {
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
            Format::Human | Format::Brief => {
                println!("dry_run: true");
                println!("count: {}", total);
                println!("category: {}", category);
                for p in &pages {
                    println!("  {} -> wiki/{}/{}.md", p.source_id, category, p.slug);
                }
            }
        }
        return Ok(());
    }

    let mut created = 0;
    let mut page_infos = Vec::new();
    for p in &pages {
        let rel_path = format!("wiki/{}/{}.md", category, p.slug);
        let page_path = root.join(&rel_path);
        write_page(&page_path, &p.page)?;
        page_infos.push(ImportedPageInfo {
            path: rel_path,
            title: p.title.clone(),
            source_id: p.source_id.clone(),
        });
        created += 1;
    }

    // Copy raw source
    let raw_dest = root.join("raw/articles").join(file.file_name().unwrap());
    std::fs::create_dir_all(raw_dest.parent().unwrap())?;
    if !raw_dest.exists() {
        std::fs::copy(file, &raw_dest)?;
    }

    let output = ImportOutput {
        imported: created,
        skipped: total - created,
        total,
        category: category.to_string(),
        pages: page_infos,
    };

    match output_format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        Format::Human | Format::Brief => {
            println!("imported: {}", created);
            println!("category: {}", category);
            println!("raw_copy: {}", raw_dest.display());
        }
    }

    Ok(())
}
