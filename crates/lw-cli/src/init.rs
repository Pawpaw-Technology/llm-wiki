use lw_core::fs::init_wiki;
use lw_core::schema::WikiSchema;
use std::path::Path;

pub fn run(root: &Path) -> anyhow::Result<()> {
    if root.join(".lw/schema.toml").exists() {
        anyhow::bail!(
            "Wiki already initialized at {}\n  To reconfigure, edit .lw/schema.toml",
            root.display()
        );
    }

    let schema = WikiSchema::default();
    init_wiki(root, &schema)?;

    let cats: Vec<&str> = schema.tags.categories.iter().map(|s| s.as_str()).collect();
    println!("Initialized wiki at {}", root.display());
    println!("  schema: .lw/schema.toml");
    println!("  wiki:   wiki/{{{},_uncategorized}}/", cats.join(","));
    println!("  raw:    raw/{{papers,articles,assets}}/");
    Ok(())
}
