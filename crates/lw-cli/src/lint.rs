use crate::output::Format;
use lw_core::lint::{self, LintReport};
use std::path::Path;
use std::process;

/// Run the lint command.
///
/// `rule` — when `Some("unlinked-mentions")`, run only that rule and zero-out
/// all other rule results so the report (human or JSON) is scoped to just the
/// requested rule. Unknown rule names are silently treated as "all rules" to
/// stay forward-compatible.
///
/// Exit codes (per issue #102 + existing lint contract):
///   0 — clean (no findings from any enabled rule)
///   1 — at least one finding
pub fn run(
    root: &Path,
    category: &Option<String>,
    output_format: &Format,
    rule: Option<&str>,
) -> anyhow::Result<()> {
    let mut report = lint::run_lint(root, category.as_deref())?;

    // Apply rule filter: suppress all findings except those from the named rule.
    if let Some(rule_name) = rule {
        apply_rule_filter(&mut report, rule_name);
    }

    let has_findings = report.has_findings();

    match output_format {
        Format::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Format::Human | Format::Brief => {
            print_human_report(&report);
        }
    }

    if has_findings {
        process::exit(1);
    }
    Ok(())
}

/// Zero out every rule except `rule_name` so the report is scoped to that
/// single rule. Unknown rule names leave the report unchanged.
fn apply_rule_filter(report: &mut LintReport, rule_name: &str) {
    match rule_name {
        "unlinked-mentions" => {
            report.todo_pages.clear();
            report.broken_related.clear();
            report.orphan_pages.clear();
            report.missing_concepts.clear();
            report.stale_journal_pages.clear();
            report.freshness.stale = 0;
            report.freshness.stale_pages.clear();
        }
        _ => {
            // Unknown rule — run all rules (forward-compatible default).
        }
    }
}

fn print_human_report(report: &LintReport) {
    let total_issues = report.todo_pages.len()
        + report.broken_related.len()
        + report.orphan_pages.len()
        + report.missing_concepts.len()
        + report.stale_journal_pages.len()
        + report.freshness.stale
        + report.unlinked_mentions.len();

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

    if !report.stale_journal_pages.is_empty() {
        println!(
            "Unprocessed Journal Captures ({}):",
            report.stale_journal_pages.len()
        );
        for f in &report.stale_journal_pages {
            println!("  - {}: {}", f.path, f.detail);
        }
        println!();
    }

    if !report.unlinked_mentions.is_empty() {
        println!("Unlinked Mentions ({}):", report.unlinked_mentions.len());
        for f in &report.unlinked_mentions {
            println!("  {}", f.to_text_line());
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
