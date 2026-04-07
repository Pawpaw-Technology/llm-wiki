use crate::output::Format;
use lw_core::lint::{self, LintReport};
use std::path::Path;

pub fn run(root: &Path, category: &Option<String>, output_format: &Format) -> anyhow::Result<()> {
    let report = lint::run_lint(root, category.as_deref())?;

    match output_format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Format::Human | Format::Brief => {
            print_human_report(&report);
        }
    }
    Ok(())
}

fn print_human_report(report: &LintReport) {
    let total_issues = report.todo_pages.len()
        + report.broken_related.len()
        + report.orphan_pages.len()
        + report.missing_concepts.len()
        + report.freshness.stale;

    println!("Wiki Lint Report");
    println!("================");
    println!();

    if !report.todo_pages.is_empty() {
        println!("TODO Pages ({}):", report.todo_pages.len());
        for f in &report.todo_pages {
            println!("  - {}: {}", f.path, f.detail);
        }
        println!();
    }

    if !report.broken_related.is_empty() {
        println!("Broken Related ({}):", report.broken_related.len());
        for f in &report.broken_related {
            println!("  - {}: {}", f.path, f.detail);
        }
        println!();
    }

    if !report.orphan_pages.is_empty() {
        println!("Orphan Pages ({}):", report.orphan_pages.len());
        for f in &report.orphan_pages {
            println!("  - {}", f.path);
        }
        println!();
    }

    if !report.missing_concepts.is_empty() {
        println!("Missing Concepts ({}):", report.missing_concepts.len());
        for f in &report.missing_concepts {
            println!("  - {}: {}", f.path, f.detail);
        }
        println!();
    }

    println!(
        "Freshness: {} fresh, {} suspect, {} stale",
        report.freshness.fresh, report.freshness.suspect, report.freshness.stale
    );

    if total_issues == 0 {
        println!("\nAll clear!");
    } else {
        println!("\n{} issue(s) found.", total_issues);
    }
}
