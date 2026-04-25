//! `lw backlinks <slug-or-path>` — show inbound links for a page.

use crate::output::Format;
use anyhow::Result;
use lw_core::backlinks;
use std::path::Path;

pub fn run(root: &Path, target: &str, format: &Format) -> Result<()> {
    // Normalise the input to a slug: strip "wiki/" prefix, category, ".md".
    let raw = target.trim_start_matches("wiki/");
    let slug = Path::new(raw)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(raw)
        .to_string();

    // Ensure the index exists (lazy build on first call).
    backlinks::ensure_index(root)?;

    // Verify that the target page actually exists.
    let wiki_dir = root.join("wiki");
    let target_exists = if wiki_dir.exists() {
        std::fs::read_dir(&wiki_dir)
            .ok()
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    .any(|cat| cat.path().join(format!("{slug}.md")).exists())
            })
            .unwrap_or(false)
    } else {
        false
    };

    if !target_exists {
        eprintln!("no backlinks or page not found: {slug}");
        std::process::exit(2);
    }

    let record = backlinks::query(root, &slug)?;

    match format {
        Format::Json => {
            let sources = record.as_ref().map(|r| r.sources.as_slice()).unwrap_or(&[]);
            let entries: Vec<serde_json::Value> = sources
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "source": s.path,
                        "kind": serde_json::to_value(&s.kind).unwrap_or_default(),
                        "context": s.context,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({
                    "target": slug,
                    "backlinks": entries,
                })
            );
        }
        _ => match record {
            None => {
                eprintln!("no backlinks or page not found: {slug}");
                std::process::exit(2);
            }
            Some(rec) if rec.sources.is_empty() => {
                eprintln!("no backlinks or page not found: {slug}");
                std::process::exit(2);
            }
            Some(rec) => {
                println!("Backlinks for: {slug}");
                println!();
                for src in &rec.sources {
                    let kind = match src.kind {
                        lw_core::backlinks::BacklinkKind::Wikilink => "wikilink",
                        lw_core::backlinks::BacklinkKind::Related => "related",
                    };
                    print!("  {} [{}]", src.path, kind);
                    if let Some(ref ctx) = src.context {
                        println!();
                        println!("    {ctx}");
                    } else {
                        println!();
                    }
                }
            }
        },
    }

    Ok(())
}

/// Wire `update_for_page` after a successful CLI write, so the backlink index
/// stays up to date when the CLI modifies a page directly.
pub fn update_after_write(root: &Path, rel_path: &Path) {
    if let Err(e) = backlinks::update_for_page(root, rel_path) {
        tracing::warn!("backlink update failed for {}: {e}", rel_path.display());
    }
}
