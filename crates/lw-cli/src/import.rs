use crate::output::Format;
use lw_core::fs::{load_schema, write_page};
use lw_core::import::parse_twitter_json;
use std::path::Path;

pub fn run(
    root: &Path,
    file: &Path,
    format: &str,
    category: &str,
    limit: Option<usize>,
    dry_run: bool,
    _output_format: &Format,
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

    if dry_run {
        println!("dry_run: true");
        println!("count: {}", pages.len());
        println!("category: {}", category);
        for p in &pages {
            println!("  {} -> wiki/{}/{}.md", p.source_id, category, p.slug);
        }
        return Ok(());
    }

    let mut created = 0;
    for p in &pages {
        let page_path = root
            .join("wiki")
            .join(category)
            .join(format!("{}.md", p.slug));
        write_page(&page_path, &p.page)?;
        created += 1;
    }

    // Copy raw source
    let raw_dest = root.join("raw/articles").join(file.file_name().unwrap());
    std::fs::create_dir_all(raw_dest.parent().unwrap())?;
    if !raw_dest.exists() {
        std::fs::copy(file, &raw_dest)?;
    }

    // Machine-useful output
    println!("imported: {}", created);
    println!("category: {}", category);
    println!("raw_copy: {}", raw_dest.display());

    Ok(())
}
