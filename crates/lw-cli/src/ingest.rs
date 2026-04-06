use lw_core::fs::{load_schema, write_page};
use lw_core::ingest::ingest_source;
use lw_core::llm::NoopLlm;
use lw_core::page::Page;
use std::io::{self, BufRead, Read, Write};
use std::path::Path;

pub fn run(
    root: &Path,
    source: Option<&Path>,
    stdin_mode: bool,
    title: &Option<String>,
    category: &Option<String>,
    tags: &Option<String>,
    raw_subdir: &str,
    yes: bool,
) -> anyhow::Result<()> {
    let schema = load_schema(root)?;

    // Handle stdin mode: write to temp file first
    let temp_file;
    let source_path = if stdin_mode {
        let mut content = String::new();
        io::stdin().lock().read_to_string(&mut content)?;
        temp_file = tempfile::NamedTempFile::new()?;
        std::fs::write(temp_file.path(), &content)?;
        temp_file.path()
    } else {
        source.ok_or_else(|| {
            anyhow::anyhow!(
                "No source file specified.\n  \
                 Usage: lw ingest <file> [--category X] [--yes]\n  \
                 Or:    cat file | lw ingest --stdin --title \"Title\" --yes"
            )
        })?
    };

    if !source_path.exists() {
        anyhow::bail!(
            "Source file not found: {}\n  Usage: lw ingest <path-to-file> [--raw-type papers]",
            source_path.display()
        );
    }

    // Phase 1: NoopLlm
    let llm = NoopLlm;
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(ingest_source(root, source_path, raw_subdir, &llm))?;
    eprintln!("Saved to {}", result.raw_path.display());

    // Build page from LLM draft or minimal
    let cat = category
        .clone()
        .unwrap_or_else(|| "_uncategorized".to_string());
    let page_tags: Vec<String> = tags
        .as_ref()
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let draft = if let Some(draft) = result.draft {
        draft
    } else {
        let auto_title = title.clone().unwrap_or_else(|| {
            source_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        });
        Page {
            title: auto_title,
            tags: page_tags,
            decay: Some(schema.decay_for_category(&cat).to_string()),
            sources: vec![format!(
                "raw/{}/{}",
                raw_subdir,
                source_path.file_name().unwrap().to_string_lossy()
            )],
            author: None,
            generator: None,
            body: format!(
                "TODO: summarize {}\n",
                source_path.file_name().unwrap().to_string_lossy()
            ),
        }
    };

    // Present for approval (or auto-approve with --yes)
    if !yes {
        eprintln!();
        eprintln!("  Title:    {}", draft.title);
        eprintln!("  Tags:     [{}]", draft.tags.join(", "));
        eprintln!("  Category: {}", cat);
        eprintln!("  Decay:    {}", draft.decay.as_deref().unwrap_or("normal"));
        eprintln!();
        if !confirm("Create wiki page?", true)? {
            eprintln!("Skipped.");
            return Ok(());
        }
    }

    let slug = slugify(&draft.title);
    let page_path = root.join("wiki").join(&cat).join(format!("{slug}.md"));
    write_page(&page_path, &draft)?;

    // Machine-useful success output
    println!("path: wiki/{}/{}.md", cat, slug);
    println!("title: {}", draft.title);
    println!("category: {}", cat);

    Ok(())
}

fn confirm(prompt: &str, default_yes: bool) -> io::Result<bool> {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    eprint!("  {} {} ", prompt, suffix);
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    Ok(if trimmed.is_empty() {
        default_yes
    } else {
        trimmed == "y" || trimmed == "yes"
    })
}

fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join("-")
}
