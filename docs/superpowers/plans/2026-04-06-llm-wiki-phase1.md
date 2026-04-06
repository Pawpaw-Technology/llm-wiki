# LLM Wiki Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `lw` CLI tool with init, ingest, query, and serve (MCP) commands that operate on a wiki git repo of markdown files.

**Architecture:** Rust workspace with three crates — `lw-core` (library), `lw-cli` (binary), `lw-mcp` (MCP server library). The wiki is a separate directory of markdown files with YAML frontmatter, organized by category directories. Search via tantivy, LLM calls abstracted behind a trait.

**Tech Stack:** Rust, tantivy 0.26, rmcp 1.3, clap 4.6, serde + toml, gray_matter 0.3, serde_yml, thiserror 2.0, tokio

---

## File Map

### lw-core (library)

| File                           | Responsibility                                                          |
| ------------------------------ | ----------------------------------------------------------------------- |
| `crates/lw-core/src/lib.rs`    | Public API re-exports                                                   |
| `crates/lw-core/src/error.rs`  | `WikiError` enum (thiserror)                                            |
| `crates/lw-core/src/page.rs`   | `Page` struct, frontmatter parsing, serialize to markdown               |
| `crates/lw-core/src/schema.rs` | `WikiSchema` struct, parse `schema.toml`, generate defaults             |
| `crates/lw-core/src/fs.rs`     | Read/write/list pages on disk, copy raw sources                         |
| `crates/lw-core/src/tag.rs`    | `Taxonomy` — collect tags from pages, category resolution, counts       |
| `crates/lw-core/src/link.rs`   | Parse `[[wiki-links]]` from body, resolve to paths, detect broken links |
| `crates/lw-core/src/search.rs` | `Searcher` trait + `TantivySearcher` impl                               |
| `crates/lw-core/src/llm.rs`    | `LlmBackend` trait + `NoopLlm` fallback                                 |
| `crates/lw-core/src/git.rs`    | Get last modified date from git log                                     |
| `crates/lw-core/src/ingest.rs` | Ingest pipeline: raw copy + optional LLM draft + page write             |
| `crates/lw-core/Cargo.toml`    | Dependencies                                                            |

### lw-cli (binary)

| File                          | Responsibility                                    |
| ----------------------------- | ------------------------------------------------- |
| `crates/lw-cli/src/main.rs`   | CLI entry point, clap parser, subcommand dispatch |
| `crates/lw-cli/src/init.rs`   | `lw init` — scaffold wiki directory               |
| `crates/lw-cli/src/query.rs`  | `lw query` — search + display results             |
| `crates/lw-cli/src/ingest.rs` | `lw ingest` — interactive ingest flow             |
| `crates/lw-cli/src/serve.rs`  | `lw serve` — start MCP server                     |
| `crates/lw-cli/src/output.rs` | Output formatting: json / human / brief           |
| `crates/lw-cli/Cargo.toml`    | Dependencies                                      |

### lw-mcp (library)

| File                       | Responsibility                                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------------- |
| `crates/lw-mcp/src/lib.rs` | MCP server struct, tool definitions (wiki_query/read/browse/write/ingest/lint/tags), handler impl |
| `crates/lw-mcp/Cargo.toml` | Dependencies                                                                                      |

### Tests

| File                                  | Tests for                                       |
| ------------------------------------- | ----------------------------------------------- |
| `crates/lw-core/tests/page_test.rs`   | Frontmatter parsing, round-trip                 |
| `crates/lw-core/tests/schema_test.rs` | Schema parsing, defaults                        |
| `crates/lw-core/tests/fs_test.rs`     | Read/write/list with tempdir                    |
| `crates/lw-core/tests/tag_test.rs`    | Tag collection, category resolution             |
| `crates/lw-core/tests/link_test.rs`   | Wiki-link parsing, resolution, broken detection |
| `crates/lw-core/tests/search_test.rs` | Index + search + snippets                       |
| `crates/lw-core/tests/ingest_test.rs` | Ingest pipeline with NoopLlm                    |
| `crates/lw-cli/tests/cli_test.rs`     | CLI integration tests (assert_cmd)              |

### Other

| File                           | Responsibility         |
| ------------------------------ | ---------------------- |
| `Cargo.toml`                   | Workspace root         |
| `crates/lw-server/.gitkeep`    | Phase 2 placeholder    |
| `scripts/cron/weekly-lint.sh`  | Example cron template  |
| `scripts/cron/daily-ingest.sh` | Example cron template  |
| `scripts/agents/librarian.sh`  | Example agent template |

---

## Task 1: Workspace Scaffold + Error Types

**Files:**

- Create: `Cargo.toml`
- Create: `crates/lw-core/Cargo.toml`
- Create: `crates/lw-core/src/lib.rs`
- Create: `crates/lw-core/src/error.rs`
- Create: `crates/lw-cli/Cargo.toml`
- Create: `crates/lw-cli/src/main.rs`
- Create: `crates/lw-mcp/Cargo.toml`
- Create: `crates/lw-mcp/src/lib.rs`
- Create: `crates/lw-server/.gitkeep`
- Create: `.gitignore`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = [
    "crates/lw-core",
    "crates/lw-cli",
    "crates/lw-mcp",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
lw-core = { path = "crates/lw-core" }
lw-mcp = { path = "crates/lw-mcp" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

- [ ] **Step 2: Create lw-core Cargo.toml**

```toml
# crates/lw-core/Cargo.toml
[package]
name = "lw-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
toml = "1.1"
serde_yml = "0.0.12"
gray_matter = "0.3"
tantivy = "0.26"
globset = "0.4"
regex = "1"

[dev-dependencies]
tempfile = "3"
tokio = { workspace = true }
```

- [ ] **Step 3: Create lw-core/src/error.rs**

```rust
// crates/lw-core/src/error.rs
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum WikiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("index error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("YAML parse error: {0}")]
    YamlParse(String),

    #[error("invalid frontmatter in {path}: {reason}")]
    Frontmatter { path: PathBuf, reason: String },

    #[error("page not found: {0}")]
    PageNotFound(PathBuf),

    #[error("schema not found: {0}")]
    SchemaNotFound(PathBuf),

    #[error("not a wiki directory: {0} (missing .lw/schema.toml)")]
    NotAWiki(PathBuf),

    #[error("LLM backend unavailable")]
    LlmUnavailable,

    #[error("LLM error: {0}")]
    Llm(String),
}

pub type Result<T> = std::result::Result<T, WikiError>;
```

- [ ] **Step 4: Create lw-core/src/lib.rs**

```rust
// crates/lw-core/src/lib.rs
pub mod error;

pub use error::{Result, WikiError};
```

- [ ] **Step 5: Create lw-cli crate shell**

```toml
# crates/lw-cli/Cargo.toml
[package]
name = "lw-cli"
version.workspace = true
edition.workspace = true

[[bin]]
name = "lw"
path = "src/main.rs"

[dependencies]
lw-core = { workspace = true }
lw-mcp = { workspace = true }
clap = { version = "4.6", features = ["derive"] }
serde_json = { workspace = true }
anyhow = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
```

```rust
// crates/lw-cli/src/main.rs
use clap::Parser;

#[derive(Parser)]
#[command(name = "lw", about = "LLM Wiki — team knowledge base toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize a new wiki in the current directory
    Init,
}

fn main() {
    let _cli = Cli::parse();
    println!("lw: not yet implemented");
}
```

- [ ] **Step 6: Create lw-mcp crate shell**

```toml
# crates/lw-mcp/Cargo.toml
[package]
name = "lw-mcp"
version.workspace = true
edition.workspace = true

[dependencies]
lw-core = { workspace = true }
rmcp = { version = "1.3", features = ["server"] }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
```

```rust
// crates/lw-mcp/src/lib.rs
//! MCP server for LLM Wiki.
//! Provides wiki_query, wiki_read, wiki_browse, wiki_write, wiki_ingest, wiki_lint, wiki_tags tools.
```

- [ ] **Step 7: Create .gitignore and placeholder**

```gitignore
# .gitignore
/target
.DS_Store
```

```
# crates/lw-server/.gitkeep
(empty file)
```

- [ ] **Step 8: Verify workspace compiles**

Run: `cargo build`
Expected: Compiles with no errors, produces `target/debug/lw` binary

- [ ] **Step 9: Run `lw --help` to verify CLI**

Run: `cargo run --bin lw -- --help`
Expected: Shows help text with `init` subcommand

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat: scaffold workspace with lw-core, lw-cli, lw-mcp crates"
```

---

## Task 2: Page Parsing (Frontmatter + Body)

**Files:**

- Create: `crates/lw-core/src/page.rs`
- Modify: `crates/lw-core/src/lib.rs`
- Create: `crates/lw-core/tests/page_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/lw-core/tests/page_test.rs
use lw_core::page::Page;

#[test]
fn parse_full_frontmatter() {
    let md = r#"---
title: Flash Attention 2
tags: [architecture, attention, optimization]
decay: normal
sources: [raw/papers/flash-attention-2.pdf]
author: vergil
generator: kimi
---

Flash Attention 2 reduces memory usage from O(N^2) to O(N).

See also [[transformer]] and [[scaling-laws]].
"#;
    let page = Page::parse(md).unwrap();
    assert_eq!(page.title, "Flash Attention 2");
    assert_eq!(page.tags, vec!["architecture", "attention", "optimization"]);
    assert_eq!(page.decay, Some("normal".to_string()));
    assert_eq!(page.sources, vec!["raw/papers/flash-attention-2.pdf"]);
    assert_eq!(page.author, Some("vergil".to_string()));
    assert_eq!(page.generator, Some("kimi".to_string()));
    assert!(page.body.contains("Flash Attention 2 reduces"));
    assert!(page.body.contains("[[transformer]]"));
}

#[test]
fn parse_minimal_frontmatter() {
    let md = r#"---
title: Backpropagation
---

The chain rule applied to computational graphs.
"#;
    let page = Page::parse(md).unwrap();
    assert_eq!(page.title, "Backpropagation");
    assert!(page.tags.is_empty());
    assert_eq!(page.decay, None);
    assert!(page.body.contains("chain rule"));
}

#[test]
fn parse_missing_title_fails() {
    let md = r#"---
tags: [test]
---

No title here.
"#;
    assert!(Page::parse(md).is_err());
}

#[test]
fn round_trip() {
    let md = r#"---
title: Test Page
tags: [a, b]
decay: fast
sources: [raw/test.pdf]
author: alice
generator: claude
---

Body content here.
"#;
    let page = Page::parse(md).unwrap();
    let rendered = page.to_markdown();
    let reparsed = Page::parse(&rendered).unwrap();
    assert_eq!(page.title, reparsed.title);
    assert_eq!(page.tags, reparsed.tags);
    assert_eq!(page.decay, reparsed.decay);
    assert_eq!(page.body.trim(), reparsed.body.trim());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lw-core --test page_test`
Expected: FAIL — module `page` not found

- [ ] **Step 3: Implement page.rs**

```rust
// crates/lw-core/src/page.rs
use crate::{Result, WikiError};
use gray_matter::{Matter, engine::YAML};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub title: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub title: String,
    pub tags: Vec<String>,
    pub decay: Option<String>,
    pub sources: Vec<String>,
    pub author: Option<String>,
    pub generator: Option<String>,
    pub body: String,
}

impl Page {
    pub fn parse(markdown: &str) -> Result<Self> {
        let matter = Matter::<YAML>::new();
        let parsed = matter.parse(markdown);

        let yaml_str = parsed.matter.as_str();
        if yaml_str.is_empty() {
            return Err(WikiError::YamlParse("no frontmatter found".into()));
        }

        let fm: Frontmatter = serde_yml::from_str(yaml_str)
            .map_err(|e| WikiError::YamlParse(e.to_string()))?;

        if fm.title.is_empty() {
            return Err(WikiError::YamlParse("title is required".into()));
        }

        Ok(Self {
            title: fm.title,
            tags: fm.tags,
            decay: fm.decay,
            sources: fm.sources,
            author: fm.author,
            generator: fm.generator,
            body: parsed.content,
        })
    }

    pub fn frontmatter(&self) -> Frontmatter {
        Frontmatter {
            title: self.title.clone(),
            tags: self.tags.clone(),
            decay: self.decay.clone(),
            sources: self.sources.clone(),
            author: self.author.clone(),
            generator: self.generator.clone(),
        }
    }

    pub fn to_markdown(&self) -> String {
        let yaml = serde_yml::to_string(&self.frontmatter())
            .expect("frontmatter serialization should not fail");
        format!("---\n{}---\n\n{}", yaml, self.body.trim_start())
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
// crates/lw-core/src/lib.rs
pub mod error;
pub mod page;

pub use error::{Result, WikiError};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lw-core --test page_test`
Expected: All 4 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lw-core/src/page.rs crates/lw-core/src/lib.rs crates/lw-core/tests/page_test.rs
git commit -m "feat(core): add Page struct with frontmatter parsing and round-trip"
```

---

## Task 3: Schema Parsing

**Files:**

- Create: `crates/lw-core/src/schema.rs`
- Modify: `crates/lw-core/src/lib.rs`
- Create: `crates/lw-core/tests/schema_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/lw-core/tests/schema_test.rs
use lw_core::schema::WikiSchema;

#[test]
fn parse_full_schema() {
    let toml_str = r#"
[wiki]
name = "Acme AI Team Wiki"
default_review_days = 90

[tags]
categories = ["architecture", "training", "infra", "tools", "product", "ops"]

[tags.decay_defaults]
product = "fast"
architecture = "normal"
training = "normal"
infra = "normal"
tools = "fast"
ops = "normal"
"#;
    let schema = WikiSchema::parse(toml_str).unwrap();
    assert_eq!(schema.wiki.name, "Acme AI Team Wiki");
    assert_eq!(schema.wiki.default_review_days, 90);
    assert_eq!(schema.tags.categories.len(), 6);
    assert_eq!(
        schema.tags.decay_defaults.get("product").unwrap(),
        "fast"
    );
}

#[test]
fn default_schema_is_valid() {
    let schema = WikiSchema::default();
    assert_eq!(schema.wiki.default_review_days, 90);
    assert!(!schema.tags.categories.is_empty());
    // round-trip
    let toml_str = schema.to_toml();
    let reparsed = WikiSchema::parse(&toml_str).unwrap();
    assert_eq!(schema.wiki.name, reparsed.wiki.name);
    assert_eq!(schema.tags.categories, reparsed.tags.categories);
}

#[test]
fn decay_for_category() {
    let schema = WikiSchema::default();
    assert_eq!(schema.decay_for_category("product"), "fast");
    assert_eq!(schema.decay_for_category("architecture"), "normal");
    assert_eq!(schema.decay_for_category("unknown"), "normal"); // fallback
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lw-core --test schema_test`
Expected: FAIL — module `schema` not found

- [ ] **Step 3: Implement schema.rs**

```rust
// crates/lw-core/src/schema.rs
use crate::{Result, WikiError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSchema {
    pub wiki: WikiConfig,
    pub tags: TagsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiConfig {
    pub name: String,
    pub default_review_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagsConfig {
    pub categories: Vec<String>,
    #[serde(default)]
    pub decay_defaults: HashMap<String, String>,
}

impl WikiSchema {
    pub fn parse(toml_str: &str) -> Result<Self> {
        toml::from_str(toml_str).map_err(WikiError::TomlParse)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("schema serialization should not fail")
    }

    /// Get the default decay for a category. Falls back to "normal".
    pub fn decay_for_category(&self, category: &str) -> &str {
        self.tags
            .decay_defaults
            .get(category)
            .map(|s| s.as_str())
            .unwrap_or("normal")
    }

    /// List of category directory names, including _uncategorized.
    pub fn category_dirs(&self) -> Vec<String> {
        let mut dirs: Vec<String> = self.tags.categories.clone();
        dirs.push("_uncategorized".to_string());
        dirs
    }
}

impl Default for WikiSchema {
    fn default() -> Self {
        Self {
            wiki: WikiConfig {
                name: "LLM Wiki".to_string(),
                default_review_days: 90,
            },
            tags: TagsConfig {
                categories: vec![
                    "architecture".into(),
                    "training".into(),
                    "infra".into(),
                    "tools".into(),
                    "product".into(),
                    "ops".into(),
                ],
                decay_defaults: HashMap::from([
                    ("product".into(), "fast".into()),
                    ("architecture".into(), "normal".into()),
                    ("training".into(), "normal".into()),
                    ("infra".into(), "normal".into()),
                    ("tools".into(), "fast".into()),
                    ("ops".into(), "normal".into()),
                ]),
            },
        }
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
// crates/lw-core/src/lib.rs
pub mod error;
pub mod page;
pub mod schema;

pub use error::{Result, WikiError};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lw-core --test schema_test`
Expected: All 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lw-core/src/schema.rs crates/lw-core/src/lib.rs crates/lw-core/tests/schema_test.rs
git commit -m "feat(core): add WikiSchema parsing with category decay defaults"
```

---

## Task 4: Filesystem Operations

**Files:**

- Create: `crates/lw-core/src/fs.rs`
- Modify: `crates/lw-core/src/lib.rs`
- Create: `crates/lw-core/tests/fs_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/lw-core/tests/fs_test.rs
use lw_core::fs::{read_page, write_page, list_pages, init_wiki, discover_wiki_root};
use lw_core::page::Page;
use lw_core::schema::WikiSchema;
use tempfile::TempDir;

#[test]
fn init_creates_structure() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    assert!(root.join(".lw/schema.toml").exists());
    assert!(root.join("wiki/architecture").is_dir());
    assert!(root.join("wiki/training").is_dir());
    assert!(root.join("wiki/_uncategorized").is_dir());
    assert!(root.join("raw/papers").is_dir());
    assert!(root.join("raw/articles").is_dir());
    assert!(root.join("raw/assets").is_dir());
}

#[test]
fn write_and_read_page() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    let page = Page {
        title: "Test Page".to_string(),
        tags: vec!["architecture".to_string()],
        decay: None,
        sources: vec![],
        author: Some("alice".to_string()),
        generator: None,
        body: "Hello world.\n".to_string(),
    };

    let path = root.join("wiki/architecture/test-page.md");
    write_page(&path, &page).unwrap();
    assert!(path.exists());

    let loaded = read_page(&path).unwrap();
    assert_eq!(loaded.title, "Test Page");
    assert_eq!(loaded.body.trim(), "Hello world.");
}

#[test]
fn list_pages_finds_markdown() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    // Write two pages in different categories
    let p1 = Page {
        title: "Page A".into(),
        tags: vec![], decay: None, sources: vec![],
        author: None, generator: None,
        body: "A.\n".into(),
    };
    let p2 = Page {
        title: "Page B".into(),
        tags: vec![], decay: None, sources: vec![],
        author: None, generator: None,
        body: "B.\n".into(),
    };
    write_page(&root.join("wiki/architecture/a.md"), &p1).unwrap();
    write_page(&root.join("wiki/training/b.md"), &p2).unwrap();

    let pages = list_pages(&root.join("wiki")).unwrap();
    assert_eq!(pages.len(), 2);
    // Paths should be relative to wiki dir
    let names: Vec<String> = pages.iter().map(|p| p.display().to_string()).collect();
    assert!(names.iter().any(|n| n.contains("a.md")));
    assert!(names.iter().any(|n| n.contains("b.md")));
}

#[test]
fn read_nonexistent_page_errors() {
    let result = read_page(std::path::Path::new("/nonexistent/page.md"));
    assert!(result.is_err());
}

#[test]
fn discover_wiki_root_from_subdir() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    // Starting from a deeply nested wiki subdirectory
    let deep = root.join("wiki/architecture");
    let found = discover_wiki_root(&deep);
    assert_eq!(found, Some(root.to_path_buf()));

    // Starting from a file path within the wiki
    let file_path = root.join("wiki/architecture/transformer.md");
    std::fs::write(&file_path, "---\ntitle: T\n---\n").unwrap();
    let found = discover_wiki_root(&file_path);
    assert_eq!(found, Some(root.to_path_buf()));

    // Starting from outside any wiki
    let outside = TempDir::new().unwrap();
    let found = discover_wiki_root(outside.path());
    assert!(found.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lw-core --test fs_test`
Expected: FAIL — module `fs` not found

- [ ] **Step 3: Implement fs.rs**

```rust
// crates/lw-core/src/fs.rs
use crate::page::Page;
use crate::schema::WikiSchema;
use crate::{Result, WikiError};
use std::path::{Path, PathBuf};

/// Initialize a wiki directory with schema and skeleton.
pub fn init_wiki(root: &Path, schema: &WikiSchema) -> Result<()> {
    // .lw/
    let lw_dir = root.join(".lw");
    std::fs::create_dir_all(&lw_dir)?;
    let schema_path = lw_dir.join("schema.toml");
    std::fs::write(&schema_path, schema.to_toml())?;

    // wiki/ category dirs
    for cat in schema.category_dirs() {
        std::fs::create_dir_all(root.join("wiki").join(&cat))?;
    }

    // raw/ subdirs
    for sub in &["papers", "articles", "assets"] {
        std::fs::create_dir_all(root.join("raw").join(sub))?;
    }

    Ok(())
}

/// Read a markdown page from disk and parse it.
pub fn read_page(path: &Path) -> Result<Page> {
    let content = std::fs::read_to_string(path)
        .map_err(|_| WikiError::PageNotFound(path.to_path_buf()))?;
    Page::parse(&content).map_err(|e| match e {
        WikiError::YamlParse(reason) => WikiError::Frontmatter {
            path: path.to_path_buf(),
            reason,
        },
        other => other,
    })
}

/// Write a page to disk as markdown.
pub fn write_page(path: &Path, page: &Page) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, page.to_markdown())?;
    Ok(())
}

/// List all .md files under a directory (recursively).
/// Returns paths relative to the given root.
pub fn list_pages(wiki_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut pages = Vec::new();
    walk_md(wiki_dir, wiki_dir, &mut pages)?;
    pages.sort();
    Ok(pages)
}

fn walk_md(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_md(base, &path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_path_buf());
            }
        }
    }
    Ok(())
}

/// Load the WikiSchema from a wiki root directory.
pub fn load_schema(root: &Path) -> Result<WikiSchema> {
    let schema_path = root.join(".lw/schema.toml");
    if !schema_path.exists() {
        return Err(WikiError::NotAWiki(root.to_path_buf()));
    }
    let content = std::fs::read_to_string(&schema_path)?;
    WikiSchema::parse(&content)
}

/// Determine the category of a page from its path relative to wiki/.
/// e.g. "architecture/transformer.md" -> "architecture"
pub fn category_from_path(rel_path: &Path) -> Option<String> {
    rel_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
}

/// Walk up from `start` to find the wiki root (directory containing `.lw/schema.toml`).
/// Similar to how git finds `.git/`.
pub fn discover_wiki_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        if current.join(".lw/schema.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
// crates/lw-core/src/lib.rs
pub mod error;
pub mod fs;
pub mod page;
pub mod schema;

pub use error::{Result, WikiError};
```

- [ ] **Step 5: Implement git.rs**

```rust
// crates/lw-core/src/git.rs
use std::path::Path;
use std::process::Command;

/// Get the age of a page in days from `git log`.
/// Shells out to `git log --follow -1 --format=%aI -- <path>`, parses the ISO date,
/// and computes days since. Returns `None` if not a git repo or the file has no history.
pub fn page_age_days(path: &Path) -> Option<i64> {
    let output = Command::new("git")
        .args(["log", "--follow", "-1", "--format=%aI", "--"])
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let date_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if date_str.is_empty() {
        return None;
    }

    // Parse ISO 8601 date and compute days since
    let timestamp = chrono::DateTime::parse_from_rfc3339(&date_str).ok()?;
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(timestamp);
    Some(duration.num_days())
}
```

> **Note:** Add `chrono = "0.4"` to lw-core's `Cargo.toml` dependencies for date parsing.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p lw-core --test fs_test`
Expected: All 5 tests PASS (including discover_wiki_root test)

- [ ] **Step 7: Commit**

```bash
git add crates/lw-core/src/fs.rs crates/lw-core/src/git.rs crates/lw-core/src/lib.rs crates/lw-core/tests/fs_test.rs
git commit -m "feat(core): add filesystem ops, git age helper, wiki root discovery"
```

---

## Task 5: Tag Module

**Files:**

- Create: `crates/lw-core/src/tag.rs`
- Modify: `crates/lw-core/src/lib.rs`
- Create: `crates/lw-core/tests/tag_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/lw-core/tests/tag_test.rs
use lw_core::tag::Taxonomy;
use lw_core::page::Page;

fn make_page(title: &str, tags: &[&str]) -> Page {
    Page {
        title: title.to_string(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        body: String::new(),
    }
}

#[test]
fn collect_tags_from_pages() {
    let pages = vec![
        make_page("A", &["transformer", "attention"]),
        make_page("B", &["attention", "optimization"]),
        make_page("C", &["transformer"]),
    ];
    let tax = Taxonomy::from_pages(&pages);
    assert_eq!(tax.tag_count("transformer"), 2);
    assert_eq!(tax.tag_count("attention"), 2);
    assert_eq!(tax.tag_count("optimization"), 1);
    assert_eq!(tax.tag_count("nonexistent"), 0);
}

#[test]
fn all_tags_sorted() {
    let pages = vec![
        make_page("A", &["z-tag", "a-tag"]),
        make_page("B", &["m-tag"]),
    ];
    let tax = Taxonomy::from_pages(&pages);
    let all = tax.all_tags();
    assert_eq!(all, vec!["a-tag", "m-tag", "z-tag"]);
}

#[test]
fn pages_with_tag() {
    let pages = vec![
        make_page("A", &["shared"]),
        make_page("B", &["other"]),
        make_page("C", &["shared", "other"]),
    ];
    let tax = Taxonomy::from_pages(&pages);
    let titles = tax.pages_with_tag("shared");
    assert_eq!(titles, vec!["A", "C"]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lw-core --test tag_test`
Expected: FAIL — module `tag` not found

- [ ] **Step 3: Implement tag.rs**

```rust
// crates/lw-core/src/tag.rs
use crate::page::Page;
use std::collections::HashMap;

/// Collected tag information across all pages.
#[derive(Debug)]
pub struct Taxonomy {
    /// tag -> list of page titles that have this tag
    tag_to_pages: HashMap<String, Vec<String>>,
}

impl Taxonomy {
    pub fn from_pages(pages: &[Page]) -> Self {
        let mut tag_to_pages: HashMap<String, Vec<String>> = HashMap::new();
        for page in pages {
            for tag in &page.tags {
                tag_to_pages
                    .entry(tag.clone())
                    .or_default()
                    .push(page.title.clone());
            }
        }
        Self { tag_to_pages }
    }

    pub fn tag_count(&self, tag: &str) -> usize {
        self.tag_to_pages.get(tag).map(|v| v.len()).unwrap_or(0)
    }

    /// All unique tags, sorted alphabetically.
    pub fn all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self.tag_to_pages.keys().cloned().collect();
        tags.sort();
        tags
    }

    /// Page titles that have a given tag, in original insertion order.
    pub fn pages_with_tag(&self, tag: &str) -> Vec<String> {
        self.tag_to_pages.get(tag).cloned().unwrap_or_default()
    }

    /// All tags with their counts, sorted by count descending.
    pub fn tag_counts(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = self
            .tag_to_pages
            .iter()
            .map(|(k, v)| (k.clone(), v.len()))
            .collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        counts
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
// crates/lw-core/src/lib.rs
pub mod error;
pub mod fs;
pub mod page;
pub mod schema;
pub mod tag;

pub use error::{Result, WikiError};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lw-core --test tag_test`
Expected: All 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lw-core/src/tag.rs crates/lw-core/src/lib.rs crates/lw-core/tests/tag_test.rs
git commit -m "feat(core): add Taxonomy for tag collection and lookup"
```

---

## Task 6: Link Module

**Files:**

- Create: `crates/lw-core/src/link.rs`
- Modify: `crates/lw-core/src/lib.rs`
- Create: `crates/lw-core/tests/link_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/lw-core/tests/link_test.rs
use lw_core::link::{extract_wiki_links, resolve_link};
use std::path::{Path, PathBuf};

#[test]
fn extract_links_from_body() {
    let body = "See [[transformer]] for details. Also related: [[scaling-laws]] and [[attention-mechanism]].";
    let links = extract_wiki_links(body);
    assert_eq!(links, vec!["transformer", "scaling-laws", "attention-mechanism"]);
}

#[test]
fn extract_no_links() {
    let links = extract_wiki_links("No links in this text.");
    assert!(links.is_empty());
}

#[test]
fn extract_deduplicates() {
    let body = "See [[foo]] and then [[foo]] again.";
    let links = extract_wiki_links(body);
    assert_eq!(links, vec!["foo"]);
}

#[test]
fn resolve_link_finds_file() {
    // Create a temp wiki structure
    let tmp = tempfile::TempDir::new().unwrap();
    let wiki_dir = tmp.path().join("wiki");
    std::fs::create_dir_all(wiki_dir.join("architecture")).unwrap();
    std::fs::write(wiki_dir.join("architecture/transformer.md"), "---\ntitle: T\n---\n").unwrap();

    let result = resolve_link("transformer", &wiki_dir).unwrap();
    assert_eq!(result, PathBuf::from("architecture/transformer.md"));
}

#[test]
fn resolve_link_returns_none_for_missing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let wiki_dir = tmp.path().join("wiki");
    std::fs::create_dir_all(&wiki_dir).unwrap();

    let result = resolve_link("nonexistent", &wiki_dir);
    assert!(result.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lw-core --test link_test`
Expected: FAIL — module `link` not found

- [ ] **Step 3: Implement link.rs**

```rust
// crates/lw-core/src/link.rs
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static WIKI_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());

/// Extract all unique [[wiki-links]] from markdown body text.
/// Returns link targets in order of first appearance, deduplicated.
pub fn extract_wiki_links(body: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut links = Vec::new();
    for cap in WIKI_LINK_RE.captures_iter(body) {
        let target = cap[1].trim().to_string();
        if seen.insert(target.clone()) {
            links.push(target);
        }
    }
    links
}

/// Resolve a wiki-link target to a relative path within wiki_dir.
/// Searches all category subdirectories for `{target}.md`.
/// Returns None if not found.
pub fn resolve_link(target: &str, wiki_dir: &Path) -> Option<PathBuf> {
    let filename = format!("{}.md", target);
    let entries = std::fs::read_dir(wiki_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let candidate = path.join(&filename);
            if candidate.exists() {
                let cat = path.file_name()?;
                return Some(PathBuf::from(cat).join(&filename));
            }
        }
    }
    None
}

/// Find all broken links in a page body given the wiki directory.
/// Returns link targets that could not be resolved.
pub fn find_broken_links(body: &str, wiki_dir: &Path) -> Vec<String> {
    extract_wiki_links(body)
        .into_iter()
        .filter(|target| resolve_link(target, wiki_dir).is_none())
        .collect()
}
```

- [ ] **Step 4: Update lib.rs**

```rust
// crates/lw-core/src/lib.rs
pub mod error;
pub mod fs;
pub mod link;
pub mod page;
pub mod schema;
pub mod tag;

pub use error::{Result, WikiError};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lw-core --test link_test`
Expected: All 5 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lw-core/src/link.rs crates/lw-core/src/lib.rs crates/lw-core/tests/link_test.rs
git commit -m "feat(core): add wiki-link extraction and cross-category resolution"
```

---

## Task 7: Searcher Trait + Tantivy Implementation

**Files:**

- Create: `crates/lw-core/src/search.rs`
- Modify: `crates/lw-core/src/lib.rs`
- Create: `crates/lw-core/tests/search_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/lw-core/tests/search_test.rs
use lw_core::search::{Searcher, TantivySearcher, SearchQuery};
use lw_core::page::Page;
use tempfile::TempDir;

fn make_page(title: &str, tags: &[&str], body: &str) -> (String, Page) {
    let page = Page {
        title: title.to_string(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
        decay: None,
        sources: vec![],
        author: None,
        generator: None,
        body: body.to_string(),
    };
    let slug = title.to_lowercase().replace(' ', "-");
    (format!("architecture/{slug}.md"), page)
}

#[test]
fn index_and_search() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();

    let (path, page) = make_page(
        "Transformer",
        &["architecture"],
        "The transformer architecture uses self-attention mechanisms.",
    );
    searcher.index_page(&path, &page).unwrap();
    searcher.commit().unwrap();

    let query = SearchQuery {
        text: "attention".to_string(),
        tags: vec![],
        category: None,
        limit: 10,
    };
    let results = searcher.search(&query).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].title, "Transformer");
    assert!(results.hits[0].snippet.contains("attention"));
}

#[test]
fn search_filters_by_tag() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();

    let (p1, page1) = make_page("A", &["ml"], "Deep learning fundamentals.");
    let (p2, page2) = make_page("B", &["infra"], "Deep learning infrastructure.");
    searcher.index_page(&p1, &page1).unwrap();
    searcher.index_page(&p2, &page2).unwrap();
    searcher.commit().unwrap();

    let query = SearchQuery {
        text: "deep learning".to_string(),
        tags: vec!["ml".to_string()],
        category: None,
        limit: 10,
    };
    let results = searcher.search(&query).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].title, "A");
}

#[test]
fn search_filters_by_category() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();

    let (p1, page1) = make_page("A", &[], "Attention paper.");
    let (_, page2) = make_page("B", &[], "Attention in training.");
    searcher.index_page(&p1, &page1).unwrap();
    searcher.index_page("training/b.md", &page2).unwrap();
    searcher.commit().unwrap();

    let query = SearchQuery {
        text: "attention".to_string(),
        tags: vec![],
        category: Some("training".to_string()),
        limit: 10,
    };
    let results = searcher.search(&query).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.hits[0].title, "B");
}

#[test]
fn remove_page_from_index() {
    let tmp = TempDir::new().unwrap();
    let searcher = TantivySearcher::new(tmp.path()).unwrap();

    let (path, page) = make_page("Gone", &[], "This will be removed.");
    searcher.index_page(&path, &page).unwrap();
    searcher.commit().unwrap();

    searcher.remove_page(&path).unwrap();
    searcher.commit().unwrap();

    let query = SearchQuery {
        text: "removed".to_string(),
        tags: vec![],
        category: None,
        limit: 10,
    };
    let results = searcher.search(&query).unwrap();
    assert_eq!(results.total, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lw-core --test search_test`
Expected: FAIL — module `search` not found

- [ ] **Step 3: Implement search.rs**

```rust
// crates/lw-core/src/search.rs
use crate::page::Page;
use crate::{Result, WikiError};
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::*;
use tantivy::snippet::SnippetGenerator;
use tantivy::{doc, Index, IndexReader, IndexWriter, TantivyDocument};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: String,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub category: String,
    pub snippet: String,
}

#[derive(Debug)]
pub struct SearchResults {
    pub total: usize,
    pub hits: Vec<SearchHit>,
}

/// Search backend trait — tantivy now, grep-scan later.
pub trait Searcher: Send + Sync {
    fn search(&self, query: &SearchQuery) -> Result<SearchResults>;
    fn index_page(&self, rel_path: &str, page: &Page) -> Result<()>;
    fn remove_page(&self, rel_path: &str) -> Result<()>;
    fn commit(&self) -> Result<()>;
    fn rebuild(&self, wiki_dir: &Path) -> Result<()>;
}

pub struct TantivySearcher {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    // field handles
    f_path: Field,
    f_title: Field,
    f_body: Field,
    f_tags: Field,
    f_category: Field,
}

impl TantivySearcher {
    pub fn new(index_dir: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let f_path = schema_builder.add_text_field("path", STRING | STORED);
        let f_title = schema_builder.add_text_field("title", TEXT | STORED);
        let f_body = schema_builder.add_text_field("body", TEXT | STORED);
        let f_tags = schema_builder.add_text_field("tags", STRING | STORED);
        let f_category = schema_builder.add_text_field("category", STRING | STORED);
        let schema = schema_builder.build();

        std::fs::create_dir_all(index_dir)?;
        let index = Index::create_in_dir(index_dir, schema)
            .or_else(|_| Index::open_in_dir(index_dir))?;
        let reader = index.reader()?;
        let writer = index.writer(50_000_000)?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            f_path,
            f_title,
            f_body,
            f_tags,
            f_category,
        })
    }
}

impl Searcher for TantivySearcher {
    fn index_page(&self, rel_path: &str, page: &Page) -> Result<()> {
        let category = rel_path
            .split('/')
            .next()
            .unwrap_or("_uncategorized")
            .to_string();

        let mut writer = self.writer.lock().unwrap();
        // Delete existing doc with same path first
        let path_term = tantivy::Term::from_field_text(self.f_path, rel_path);
        writer.delete_term(path_term);

        // Build document with multi-value tags (each tag as separate field value)
        let mut doc = TantivyDocument::new();
        doc.add_text(self.f_path, rel_path);
        doc.add_text(self.f_title, &page.title);
        doc.add_text(self.f_body, &page.body);
        for tag in &page.tags {
            doc.add_text(self.f_tags, tag);
        }
        doc.add_text(self.f_category, &category);
        writer.add_document(doc)?;
        Ok(())
    }

    fn remove_page(&self, rel_path: &str) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        let term = tantivy::Term::from_field_text(self.f_path, rel_path);
        writer.delete_term(term);
        Ok(())
    }

    fn commit(&self) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    fn search(&self, query: &SearchQuery) -> Result<SearchResults> {
        let searcher = self.reader.searcher();
        let query_parser =
            QueryParser::for_index(&self.index, vec![self.f_title, self.f_body]);
        let text_query = query_parser
            .parse_query(&query.text)
            .map_err(|e| WikiError::Tantivy(tantivy::TantivyError::InvalidArgument(e.to_string())))?;

        // Build boolean query with optional filters
        let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = vec![
            (Occur::Must, text_query),
        ];

        for tag in &query.tags {
            let term = tantivy::Term::from_field_text(self.f_tags, tag);
            subqueries.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        if let Some(cat) = &query.category {
            let term = tantivy::Term::from_field_text(self.f_category, cat);
            subqueries.push((
                Occur::Must,
                Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            ));
        }

        let bool_query = BooleanQuery::new(subqueries);
        let top_docs = searcher.search(&bool_query, &TopDocs::with_limit(query.limit))?;

        let mut snippet_gen =
            SnippetGenerator::create(&searcher, &*query_parser.parse_query(&query.text)
                .map_err(|e| WikiError::Tantivy(tantivy::TantivyError::InvalidArgument(e.to_string())))?,
                self.f_body)?;
        snippet_gen.set_max_num_chars(200);

        let total = top_docs.len();
        let mut hits = Vec::new();
        for (_score, doc_addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_addr)?;
            let path = doc.get_first(self.f_path).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = doc.get_first(self.f_title).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let tags: Vec<String> = doc.get_all(self.f_tags)
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            let category = doc.get_first(self.f_category).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let snippet = snippet_gen.snippet_from_doc(&doc).to_html();

            hits.push(SearchHit {
                path,
                title,
                tags,
                category,
                snippet,
            });
        }

        Ok(SearchResults { total, hits })
    }

    fn rebuild(&self, wiki_dir: &Path) -> Result<()> {
        // Clear all documents
        {
            let mut writer = self.writer.lock().unwrap();
            writer.delete_all_documents()?;
            writer.commit()?;
        }

        // Re-index all pages
        let pages = crate::fs::list_pages(wiki_dir)?;
        for rel_path in &pages {
            let abs_path = wiki_dir.join(rel_path);
            if let Ok(page) = crate::fs::read_page(&abs_path) {
                self.index_page(&rel_path.display().to_string(), &page)?;
            }
        }
        self.commit()?;
        Ok(())
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
// crates/lw-core/src/lib.rs
pub mod error;
pub mod fs;
pub mod link;
pub mod page;
pub mod schema;
pub mod search;
pub mod tag;

pub use error::{Result, WikiError};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lw-core --test search_test`
Expected: All 4 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lw-core/src/search.rs crates/lw-core/src/lib.rs crates/lw-core/tests/search_test.rs
git commit -m "feat(core): add Searcher trait with TantivySearcher implementation"
```

---

## Task 8: LlmBackend Trait

**Files:**

- Create: `crates/lw-core/src/llm.rs`
- Modify: `crates/lw-core/src/lib.rs`
- Create: `crates/lw-core/tests/llm_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/lw-core/tests/llm_test.rs
use lw_core::llm::{LlmBackend, NoopLlm, CompletionRequest, CompletionResponse};

#[test]
fn noop_llm_is_unavailable() {
    let llm = NoopLlm;
    assert!(!llm.available());
}

#[tokio::test]
async fn noop_llm_returns_error() {
    let llm = NoopLlm;
    let req = CompletionRequest {
        system: None,
        prompt: "Summarize this paper.".to_string(),
        max_tokens: None,
    };
    let result = llm.complete(&req).await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lw-core --test llm_test`
Expected: FAIL — module `llm` not found

- [ ] **Step 3: Implement llm.rs**

```rust
// crates/lw-core/src/llm.rs
use crate::{Result, WikiError};

/// A request to generate a completion from an LLM.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub system: Option<String>,
    pub prompt: String,
    pub max_tokens: Option<u32>,
}

/// A response from an LLM completion.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
}

/// LLM abstraction — the core decoupling point between tool layer and intelligence.
/// Implementations: Claude API, OpenAI, Kimi, local ollama, subprocess.
///
/// Note: Rust edition 2024 supports native async traits, no need for async_trait crate.
pub trait LlmBackend: Send + Sync {
    /// Generate a completion. Returns Err if the backend is unavailable or fails.
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse>;

    /// Health check. Returns false if the LLM is not configured or unreachable.
    /// Tools should gracefully degrade when this returns false.
    fn available(&self) -> bool;
}

/// Fallback when no LLM is configured. Always returns unavailable.
pub struct NoopLlm;

impl LlmBackend for NoopLlm {
    async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse> {
        Err(WikiError::LlmUnavailable)
    }

    fn available(&self) -> bool {
        false
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
// crates/lw-core/src/lib.rs
pub mod error;
pub mod fs;
pub mod link;
pub mod llm;
pub mod page;
pub mod schema;
pub mod search;
pub mod tag;

pub use error::{Result, WikiError};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lw-core --test llm_test`
Expected: All 2 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lw-core/src/llm.rs crates/lw-core/src/lib.rs crates/lw-core/tests/llm_test.rs
git commit -m "feat(core): add LlmBackend trait with NoopLlm fallback"
```

---

## Task 9: Ingest Pipeline

**Files:**

- Create: `crates/lw-core/src/ingest.rs`
- Modify: `crates/lw-core/src/lib.rs`
- Create: `crates/lw-core/tests/ingest_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/lw-core/tests/ingest_test.rs
use lw_core::fs::init_wiki;
use lw_core::ingest::{IngestResult, ingest_source};
use lw_core::llm::NoopLlm;
use lw_core::schema::WikiSchema;
use tempfile::TempDir;
use std::path::Path;

#[tokio::test]
async fn ingest_copies_to_raw() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    // Create a source file outside the wiki
    let source = tmp.path().join("external/paper.md");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "# My Paper\n\nContent here.").unwrap();

    let llm = NoopLlm;
    let result = ingest_source(root, &source, "papers", &llm).await.unwrap();

    assert!(result.raw_path.exists());
    assert!(result.raw_path.starts_with(root.join("raw/papers")));
    // NoopLlm means no draft was generated
    assert!(result.draft.is_none());
}

#[tokio::test]
async fn ingest_with_mock_llm_generates_draft() {
    use lw_core::llm::{LlmBackend, CompletionRequest, CompletionResponse};

    struct MockLlm;
    impl LlmBackend for MockLlm {
        async fn complete(&self, _req: &CompletionRequest) -> lw_core::Result<CompletionResponse> {
            Ok(CompletionResponse {
                text: r#"---
title: My Paper
tags: [architecture, attention]
decay: normal
---

Summary of the paper content."#
                    .to_string(),
            })
        }
        fn available(&self) -> bool {
            true
        }
    }

    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let schema = WikiSchema::default();
    init_wiki(root, &schema).unwrap();

    let source = tmp.path().join("external/paper.md");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "# My Paper\n\nSome content.").unwrap();

    let llm = MockLlm;
    let result = ingest_source(root, &source, "papers", &llm).await.unwrap();

    assert!(result.draft.is_some());
    let draft = result.draft.unwrap();
    assert_eq!(draft.title, "My Paper");
    assert_eq!(draft.tags, vec!["architecture", "attention"]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lw-core --test ingest_test`
Expected: FAIL — module `ingest` not found

- [ ] **Step 3: Implement ingest.rs**

```rust
// crates/lw-core/src/ingest.rs
use crate::llm::LlmBackend;
use crate::page::Page;
use crate::Result;
use std::path::{Path, PathBuf};

/// Result of an ingest operation.
pub struct IngestResult {
    /// Where the source was copied to in raw/
    pub raw_path: PathBuf,
    /// LLM-generated draft page, if available
    pub draft: Option<Page>,
}

/// Ingest a source file: copy to raw/{subdir}/, optionally generate a wiki page draft.
///
/// - `wiki_root`: path to the wiki repo root
/// - `source`: path to the source file (will be copied)
/// - `raw_subdir`: subdirectory under raw/ (e.g. "papers", "articles")
/// - `llm`: LLM backend for draft generation (NoopLlm if unavailable)
pub async fn ingest_source(
    wiki_root: &Path,
    source: &Path,
    raw_subdir: &str,
    llm: &dyn LlmBackend,
) -> Result<IngestResult> {
    // Copy source to raw/
    let filename = source
        .file_name()
        .ok_or_else(|| crate::WikiError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "source has no filename",
        )))?;
    let dest_dir = wiki_root.join("raw").join(raw_subdir);
    std::fs::create_dir_all(&dest_dir)?;
    let raw_path = dest_dir.join(filename);
    std::fs::copy(source, &raw_path)?;

    // Try LLM draft generation
    let draft = if llm.available() {
        let source_content = std::fs::read_to_string(source).unwrap_or_default();
        let prompt = format!(
            "Read the following source material and generate a wiki page in markdown format.\n\
             The page MUST start with YAML frontmatter containing:\n\
             - title (required)\n\
             - tags (list of relevant tags)\n\
             - decay (fast/normal/evergreen)\n\n\
             Source:\n{}\n\n\
             Generate the wiki page:",
            source_content
        );

        let req = crate::llm::CompletionRequest {
            system: Some("You are a wiki page generator. Output only the markdown page with frontmatter.".to_string()),
            prompt,
            max_tokens: Some(2000),
        };

        match llm.complete(&req).await {
            Ok(resp) => Page::parse(&resp.text).ok(),
            Err(_) => None,
        }
    } else {
        None
    };

    Ok(IngestResult { raw_path, draft })
}
```

- [ ] **Step 4: Update lib.rs**

```rust
// crates/lw-core/src/lib.rs
pub mod error;
pub mod fs;
pub mod git;
pub mod ingest;
pub mod link;
pub mod llm;
pub mod page;
pub mod schema;
pub mod search;
pub mod tag;

pub use error::{Result, WikiError};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p lw-core --test ingest_test`
Expected: All 2 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/lw-core/src/ingest.rs crates/lw-core/src/lib.rs crates/lw-core/tests/ingest_test.rs
git commit -m "feat(core): add ingest pipeline with LLM draft generation"
```

---

## Task 10: CLI — init Command

**Files:**

- Create: `crates/lw-cli/src/init.rs`
- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Implement init.rs**

```rust
// crates/lw-cli/src/init.rs
use lw_core::fs::init_wiki;
use lw_core::schema::WikiSchema;
use std::path::Path;

pub fn run(root: &Path) -> anyhow::Result<()> {
    if root.join(".lw/schema.toml").exists() {
        anyhow::bail!("Wiki already initialized (found .lw/schema.toml)");
    }

    let schema = WikiSchema::default();
    init_wiki(root, &schema)?;

    println!("Created .lw/schema.toml");
    let cats: Vec<&str> = schema.tags.categories.iter().map(|s| s.as_str()).collect();
    println!("Created wiki/{{{},_uncategorized}}/", cats.join(","));
    println!("Created raw/{{papers,articles,assets}}/");
    println!("\nWiki initialized. Edit .lw/schema.toml to customize.");
    Ok(())
}
```

- [ ] **Step 2: Update main.rs with full CLI structure**

```rust
// crates/lw-cli/src/main.rs
mod init;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lw", about = "LLM Wiki — team knowledge base toolkit")]
struct Cli {
    /// Wiki root directory (default: current directory)
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize a new wiki in the current directory
    Init,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => init::run(&cli.root),
    }
}
```

- [ ] **Step 3: Test manually**

Run: `cargo run --bin lw -- init --root /tmp/test-wiki`
Expected:

```
Created .lw/schema.toml
Created wiki/{architecture,training,infra,tools,product,ops,_uncategorized}/
Created raw/{papers,articles,assets}/

Wiki initialized. Edit .lw/schema.toml to customize.
```

Run: `ls /tmp/test-wiki/wiki/`
Expected: `_uncategorized  architecture  infra  ops  product  tools  training`

Run: `cat /tmp/test-wiki/.lw/schema.toml`
Expected: Valid TOML with name, categories, decay_defaults

- [ ] **Step 4: Commit**

```bash
git add crates/lw-cli/src/init.rs crates/lw-cli/src/main.rs
git commit -m "feat(cli): add lw init command"
```

---

## Task 11: CLI — query Command

**Files:**

- Create: `crates/lw-cli/src/query.rs`
- Create: `crates/lw-cli/src/output.rs`
- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Implement output.rs**

```rust
// crates/lw-cli/src/output.rs
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

pub fn print_query_results(
    query: &str,
    hits: &[SearchHit],
    total: usize,
    format: &Format,
) {
    match format {
        Format::Json => {
            let envelope = QueryEnvelope {
                command: "query".to_string(),
                query: query.to_string(),
                total,
                returned: hits.len(),
                results: hits.iter().map(QueryResult::from).collect(),
            };
            println!("{}", serde_json::to_string_pretty(&envelope).unwrap());
        }
        Format::Human => {
            if hits.is_empty() {
                println!("No results for \"{}\"", query);
                return;
            }
            println!();
            for (i, hit) in hits.iter().enumerate() {
                let tags = if hit.tags.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", hit.tags.join(", "))
                };
                println!("  {}. {}{}", i + 1, hit.path, tags);
                if !hit.snippet.is_empty() {
                    // Strip HTML tags from snippet for terminal display
                    let clean = hit.snippet.replace("<b>", "").replace("</b>", "");
                    println!("     {}", clean.trim());
                }
            }
            println!();
            println!("  {} result(s)", total);
        }
        Format::Brief => {
            for hit in hits {
                println!("{}\t{}\t[{}]", hit.path, hit.title, hit.tags.join(","));
            }
        }
    }
}
```

- [ ] **Step 2: Implement query.rs**

```rust
// crates/lw-cli/src/query.rs
use crate::output::{self, Format};
use lw_core::fs::load_schema;
use lw_core::search::{SearchQuery, TantivySearcher, Searcher};
use std::path::Path;

pub fn run(
    root: &Path,
    text: &str,
    tag: &[String],
    category: &Option<String>,
    limit: usize,
    format: &Format,
) -> anyhow::Result<()> {
    let _schema = load_schema(root)?;
    let index_dir = root.join(".lw/search");
    let searcher = TantivySearcher::new(&index_dir)?;

    // Rebuild index from wiki files on every query for now.
    // Phase 2: incremental indexing.
    let wiki_dir = root.join("wiki");
    searcher.rebuild(&wiki_dir)?;

    let query = SearchQuery {
        text: text.to_string(),
        tags: tag.to_vec(),
        category: category.clone(),
        limit,
    };
    let results = searcher.search(&query)?;
    output::print_query_results(text, &results.hits, results.total, format);
    Ok(())
}
```

- [ ] **Step 3: Update main.rs**

```rust
// crates/lw-cli/src/main.rs
mod init;
mod output;
mod query;

use clap::Parser;
use output::Format;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lw", about = "LLM Wiki — team knowledge base toolkit")]
struct Cli {
    /// Wiki root directory (default: current directory)
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize a new wiki in the current directory
    Init,

    /// Search wiki pages
    Query {
        /// Search text
        text: String,

        /// Filter by tag (repeatable)
        #[arg(long)]
        tag: Vec<String>,

        /// Filter by category
        #[arg(long)]
        category: Option<String>,

        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Output format
        #[arg(short, long, default_value = "human")]
        format: Format,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => init::run(&cli.root),
        Commands::Query {
            text,
            tag,
            category,
            limit,
            format,
        } => query::run(&cli.root, &text, &tag, &category, limit, &format),
    }
}
```

- [ ] **Step 4: Test manually**

Run:

```bash
# Create a test wiki
cargo run --bin lw -- init --root /tmp/test-wiki2

# Add a test page
cat > /tmp/test-wiki2/wiki/architecture/transformer.md << 'EOF'
---
title: Transformer Architecture
tags: [architecture, attention, deep-learning]
decay: evergreen
---

The Transformer architecture, introduced in "Attention Is All You Need" (2017),
replaced recurrence with self-attention mechanisms.
EOF

# Search
cargo run --bin lw -- query "attention" --root /tmp/test-wiki2
```

Expected: Shows transformer.md in results with snippet containing "attention"

- [ ] **Step 5: Commit**

```bash
git add crates/lw-cli/src/query.rs crates/lw-cli/src/output.rs crates/lw-cli/src/main.rs
git commit -m "feat(cli): add lw query command with json/human/brief output"
```

---

## Task 12: CLI — ingest Command

**Files:**

- Create: `crates/lw-cli/src/ingest.rs`
- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Implement ingest.rs**

```rust
// crates/lw-cli/src/ingest.rs
use lw_core::fs::{load_schema, write_page};
use lw_core::ingest::ingest_source;
use lw_core::llm::NoopLlm;
use lw_core::page::Page;
use std::io::{self, BufRead, Write};
use std::path::Path;

pub async fn run(
    root: &Path,
    source: &Path,
    title: &Option<String>,
    category: &Option<String>,
    raw_subdir: &str,
) -> anyhow::Result<()> {
    let schema = load_schema(root)?;

    // For Phase 1: NoopLlm. Real LLM backends come from config.
    let llm = NoopLlm;
    let result = ingest_source(root, source, raw_subdir, &llm).await?;
    println!("Saved to {}", result.raw_path.display());

    // If LLM generated a draft, present it for approval
    let draft = if let Some(draft) = result.draft {
        draft
    } else {
        // No LLM available — build minimal page from filename
        let auto_title = title.clone().unwrap_or_else(|| {
            source
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string())
        });
        let cat = category.clone().unwrap_or_else(|| "_uncategorized".to_string());
        println!("\nNo LLM available. Creating minimal page.");
        Page {
            title: auto_title,
            tags: vec![],
            decay: Some(schema.decay_for_category(&cat).to_string()),
            sources: vec![format!("raw/{}/{}", raw_subdir, source.file_name().unwrap().to_string_lossy())],
            author: None,
            generator: None,
            body: format!("TODO: summarize {}\n", source.file_name().unwrap().to_string_lossy()),
        }
    };

    let cat = category.clone().unwrap_or_else(|| "_uncategorized".to_string());

    // Present draft for approval (P2: humans choose, never fill blanks)
    println!();
    println!("  Title: {}", draft.title);
    println!("  Tags: [{}]", draft.tags.join(", "));
    println!("  Category: {}", cat);
    println!("  Decay: {}", draft.decay.as_deref().unwrap_or("normal"));
    println!();

    if !confirm("Create wiki page?", true)? {
        println!("Skipped.");
        return Ok(());
    }

    let slug = slugify(&draft.title);
    let page_path = root.join("wiki").join(&cat).join(format!("{}.md", slug));
    write_page(&page_path, &draft)?;
    println!("Created {}", page_path.display());

    Ok(())
}

fn confirm(prompt: &str, default_yes: bool) -> io::Result<bool> {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("  {} {} ", prompt, suffix);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let trimmed = input.trim().to_lowercase();
    if trimmed.is_empty() {
        Ok(default_yes)
    } else {
        Ok(trimmed == "y" || trimmed == "yes")
    }
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
```

- [ ] **Step 2: Update main.rs**

```rust
// crates/lw-cli/src/main.rs
mod init;
mod ingest;
mod output;
mod query;

use clap::Parser;
use output::Format;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lw", about = "LLM Wiki — team knowledge base toolkit")]
struct Cli {
    /// Wiki root directory (default: current directory)
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize a new wiki in the current directory
    Init,

    /// Search wiki pages
    Query {
        /// Search text
        text: String,

        /// Filter by tag (repeatable)
        #[arg(long)]
        tag: Vec<String>,

        /// Filter by category
        #[arg(long)]
        category: Option<String>,

        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Output format
        #[arg(short, long, default_value = "human")]
        format: Format,
    },

    /// Ingest source material into the wiki
    Ingest {
        /// Path to source file
        source: PathBuf,

        /// Page title (auto-derived from filename if omitted)
        #[arg(long)]
        title: Option<String>,

        /// Target category
        #[arg(long)]
        category: Option<String>,

        /// Raw subdirectory (papers, articles, assets)
        #[arg(long, default_value = "articles")]
        raw_type: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => init::run(&cli.root),
        Commands::Query {
            text,
            tag,
            category,
            limit,
            format,
        } => query::run(&cli.root, &text, &tag, &category, limit, &format),
        Commands::Ingest {
            source,
            title,
            category,
            raw_type,
        } => ingest::run(&cli.root, &source, &title, &category, &raw_type),
    }
}
```

- [ ] **Step 3: Test manually**

Run:

```bash
# Setup
cargo run --bin lw -- init --root /tmp/test-wiki3

# Create a test source
echo "# Attention Is All You Need\n\nTransformer paper content." > /tmp/test-paper.md

# Ingest
cargo run --bin lw -- ingest /tmp/test-paper.md --root /tmp/test-wiki3 --category architecture --raw-type papers
# Press Enter to accept defaults
```

Expected:

```
Saved to /tmp/test-wiki3/raw/papers/test-paper.md

No LLM available. Creating minimal page.

  Title: test-paper
  Tags: []
  Category: architecture
  Decay: normal

  Create wiki page? [Y/n]
Created /tmp/test-wiki3/wiki/architecture/test-paper.md
```

- [ ] **Step 4: Commit**

```bash
git add crates/lw-cli/src/ingest.rs crates/lw-cli/src/main.rs
git commit -m "feat(cli): add lw ingest command with interactive approval"
```

---

## Task 13: MCP Server + CLI serve Command

**Files:**

- Modify: `crates/lw-mcp/src/lib.rs`
- Modify: `crates/lw-mcp/Cargo.toml`
- Create: `crates/lw-cli/src/serve.rs`
- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Implement lw-mcp/src/lib.rs**

```rust
// crates/lw-mcp/src/lib.rs
use lw_core::fs::{self, load_schema, read_page, write_page, list_pages};
use lw_core::page::Page;
use lw_core::search::{SearchQuery, Searcher, TantivySearcher};
use lw_core::tag::Taxonomy;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{schemars, tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiQueryArgs {
    /// Full-text search query
    pub query: String,
    /// Filter by tags (comma-separated)
    #[serde(default)]
    pub tags: Option<String>,
    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,
    /// Max results (default: 20)
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiReadArgs {
    /// Relative path within wiki/ (e.g. "architecture/transformer.md")
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiBrowseArgs {
    /// Filter by category
    #[serde(default)]
    pub category: Option<String>,
    /// Filter by tag
    #[serde(default)]
    pub tag: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiTagsArgs {
    /// Filter by category (omit to get tags across all categories)
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiWriteArgs {
    /// Relative path within wiki/ (e.g. "architecture/new-page.md")
    pub path: String,
    /// Full markdown content including frontmatter
    pub content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiIngestArgs {
    /// Path to source file (absolute or relative to wiki root)
    pub source_path: String,
    /// Target subdirectory under raw/ (papers, articles, assets)
    #[serde(default = "default_raw_type")]
    pub raw_type: String,
    /// Suggested title (auto-derived from filename if omitted)
    #[serde(default)]
    pub title: Option<String>,
    /// Suggested tags
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Target category directory
    #[serde(default)]
    pub category: Option<String>,
}

fn default_raw_type() -> String {
    "articles".to_string()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WikiLintArgs {
    /// Filter by category (omit to lint all)
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Clone)]
pub struct WikiMcpServer {
    wiki_root: PathBuf,
    searcher: Arc<TantivySearcher>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl WikiMcpServer {
    pub fn new(wiki_root: PathBuf) -> Self {
        let index_dir = wiki_root.join(".lw/search");
        let searcher = Arc::new(TantivySearcher::new(&index_dir).expect("failed to create search index"));
        let wiki_dir = wiki_root.join("wiki");
        searcher.rebuild(&wiki_dir).expect("failed to build initial index");
        Self {
            wiki_root,
            searcher,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(name = "wiki_query", description = "Full-text search across wiki pages. Returns matching pages with snippets.")]
    async fn wiki_query(
        &self,
        Parameters(args): Parameters<WikiQueryArgs>,
    ) -> Result<CallToolResult, McpError> {
        let tags = args.tags
            .map(|t| t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();

        let query = SearchQuery {
            text: args.query,
            tags,
            category: args.category,
            limit: args.limit.unwrap_or(20),
        };

        let results = self.searcher.search(&query)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let json = serde_json::json!({
            "total": results.total,
            "results": results.hits.iter().map(|h| serde_json::json!({
                "path": h.path,
                "title": h.title,
                "tags": h.tags,
                "category": h.category,
                "snippet": h.snippet,
            })).collect::<Vec<_>>()
        });

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&json).unwrap(),
        )]))
    }

    #[tool(name = "wiki_read", description = "Read a wiki page by its relative path. Returns full markdown content.")]
    async fn wiki_read(
        &self,
        Parameters(args): Parameters<WikiReadArgs>,
    ) -> Result<CallToolResult, McpError> {
        let path = self.wiki_root.join("wiki").join(&args.path);
        let content = std::fs::read_to_string(&path)
            .map_err(|e| McpError::internal_error(format!("Failed to read {}: {}", args.path, e), None))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(name = "wiki_browse", description = "List wiki pages, optionally filtered by category or tag.")]
    async fn wiki_browse(
        &self,
        Parameters(args): Parameters<WikiBrowseArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wiki_dir = self.wiki_root.join("wiki");
        let pages = list_pages(&wiki_dir)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut results: Vec<serde_json::Value> = Vec::new();
        for rel_path in &pages {
            let abs_path = wiki_dir.join(rel_path);
            if let Ok(page) = read_page(&abs_path) {
                let cat = lw_core::fs::category_from_path(rel_path)
                    .unwrap_or_else(|| "_uncategorized".to_string());

                // Apply filters
                if let Some(ref filter_cat) = args.category {
                    if &cat != filter_cat {
                        continue;
                    }
                }
                if let Some(ref filter_tag) = args.tag {
                    if !page.tags.iter().any(|t| t == filter_tag) {
                        continue;
                    }
                }

                results.push(serde_json::json!({
                    "path": rel_path.display().to_string(),
                    "title": page.title,
                    "tags": page.tags,
                    "category": cat,
                }));
            }
        }

        let json = serde_json::json!({
            "total": results.len(),
            "pages": results,
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&json).unwrap(),
        )]))
    }

    #[tool(name = "wiki_tags", description = "List all tags with usage counts. Returns tags sorted by frequency.")]
    async fn wiki_tags(
        &self,
        Parameters(args): Parameters<WikiTagsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let wiki_dir = self.wiki_root.join("wiki");
        let page_paths = list_pages(&wiki_dir)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut pages = Vec::new();
        for rel_path in &page_paths {
            let abs_path = wiki_dir.join(rel_path);
            if let Ok(page) = read_page(&abs_path) {
                if let Some(ref filter_cat) = args.category {
                    let cat = lw_core::fs::category_from_path(rel_path)
                        .unwrap_or_else(|| "_uncategorized".to_string());
                    if &cat != filter_cat { continue; }
                }
                pages.push(page);
            }
        }

        let taxonomy = lw_core::tag::Taxonomy::from_pages(&pages);
        let counts = taxonomy.tag_counts();

        let json = serde_json::json!({
            "total_tags": counts.len(),
            "tags": counts.iter().map(|(tag, count)| serde_json::json!({
                "tag": tag,
                "count": count,
            })).collect::<Vec<_>>()
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&json).unwrap(),
        )]))
    }

    #[tool(name = "wiki_write", description = "Write or update a wiki page. Content must include YAML frontmatter.")]
    async fn wiki_write(
        &self,
        Parameters(args): Parameters<WikiWriteArgs>,
    ) -> Result<CallToolResult, McpError> {
        let page = Page::parse(&args.content)
            .map_err(|e| McpError::internal_error(format!("Invalid page content: {}", e), None))?;
        let path = self.wiki_root.join("wiki").join(&args.path);
        write_page(&path, &page)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Incremental index update
        self.searcher.index_page(&args.path, &page)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        self.searcher.commit()
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(
            format!("Written to wiki/{}", args.path),
        )]))
    }

    #[tool(name = "wiki_ingest", description = "Ingest a source file: copy to raw/ and return metadata. After ingesting, use wiki_write to create the corresponding wiki page with frontmatter. This is a two-step process: ingest copies the source, then you write the wiki page.")]
    async fn wiki_ingest(
        &self,
        Parameters(args): Parameters<WikiIngestArgs>,
    ) -> Result<CallToolResult, McpError> {
        let source = std::path::Path::new(&args.source_path);
        if !source.exists() {
            return Err(McpError::internal_error(
                format!("Source not found: {}", args.source_path), None,
            ));
        }
        let filename = source.file_name()
            .ok_or_else(|| McpError::internal_error("Source has no filename".to_string(), None))?;
        let dest_dir = self.wiki_root.join("raw").join(&args.raw_type);
        std::fs::create_dir_all(&dest_dir)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let raw_path = dest_dir.join(filename);
        std::fs::copy(source, &raw_path)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let json = serde_json::json!({
            "raw_path": raw_path.display().to_string(),
            "filename": filename.to_string_lossy(),
            "suggested_title": args.title.unwrap_or_else(|| {
                source.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
            }),
            "suggested_tags": args.tags.unwrap_or_default(),
            "suggested_category": args.category.unwrap_or_else(|| "_uncategorized".to_string()),
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&json).unwrap(),
        )]))
    }

    #[tool(name = "wiki_lint", description = "Check wiki freshness. Returns pages grouped by status: stale, suspect, fresh.")]
    async fn wiki_lint(
        &self,
        Parameters(args): Parameters<WikiLintArgs>,
    ) -> Result<CallToolResult, McpError> {
        let schema = load_schema(&self.wiki_root)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let wiki_dir = self.wiki_root.join("wiki");
        let pages = list_pages(&wiki_dir)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut stale = Vec::new();
        let mut fresh = Vec::new();

        for rel_path in &pages {
            let cat = lw_core::fs::category_from_path(rel_path)
                .unwrap_or_else(|| "_uncategorized".to_string());
            if let Some(ref filter_cat) = args.category {
                if &cat != filter_cat { continue; }
            }
            let abs_path = wiki_dir.join(rel_path);
            if let Ok(page) = read_page(&abs_path) {
                let decay = page.decay.as_deref()
                    .unwrap_or_else(|| schema.decay_for_category(&cat));
                let threshold_days: i64 = match decay {
                    "fast" => 30,
                    "evergreen" => i64::MAX,
                    _ => schema.wiki.default_review_days as i64,
                };

                // Get last modified from git log (authoritative time source)
                let age_days = lw_core::git::page_age_days(&abs_path).unwrap_or(0);

                let entry = serde_json::json!({
                    "path": rel_path.display().to_string(),
                    "title": page.title,
                    "category": cat,
                    "decay": decay,
                    "age_days": age_days,
                });

                if age_days > threshold_days {
                    stale.push(entry);
                } else {
                    fresh.push(entry);
                }
            }
        }

        let json = serde_json::json!({
            "stale": stale,
            "fresh": fresh,
            "summary": {
                "total": stale.len() + fresh.len(),
                "stale_count": stale.len(),
                "fresh_count": fresh.len(),
            }
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&json).unwrap(),
        )]))
    }
}

#[tool_handler]
impl ServerHandler for WikiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::new("lw-mcp", env!("CARGO_PKG_VERSION")),
            instructions: Some(
                "LLM Wiki MCP server. Provides tools to query, read, browse, write, ingest, lint, and tag wiki pages."
                    .to_string(),
            ),
        }
    }
}

/// Start the MCP server on stdio. Call this from `lw serve`.
pub async fn run_stdio(wiki_root: PathBuf) -> anyhow::Result<()> {
    let server = WikiMcpServer::new(wiki_root);
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

- [ ] **Step 2: Implement serve.rs in lw-cli**

```rust
// crates/lw-cli/src/serve.rs
use std::path::Path;

pub fn run(root: &Path) -> anyhow::Result<()> {
    // Verify wiki exists
    if !root.join(".lw/schema.toml").exists() {
        anyhow::bail!(
            "Not a wiki directory: {} (missing .lw/schema.toml)\nRun `lw init` first.",
            root.display()
        );
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(lw_mcp::run_stdio(root.to_path_buf()))
}
```

- [ ] **Step 3: Update main.rs**

```rust
// crates/lw-cli/src/main.rs
mod init;
mod ingest;
mod output;
mod query;
mod serve;

use clap::Parser;
use output::Format;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lw", about = "LLM Wiki — team knowledge base toolkit")]
struct Cli {
    /// Wiki root directory (default: auto-discover from cwd, or LW_WIKI_ROOT env var)
    #[arg(long, global = true, env = "LW_WIKI_ROOT")]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize a new wiki in the current directory
    Init,

    /// Search wiki pages
    Query {
        /// Search text
        text: String,

        /// Filter by tag (repeatable)
        #[arg(long)]
        tag: Vec<String>,

        /// Filter by category
        #[arg(long)]
        category: Option<String>,

        /// Max results
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Output format
        #[arg(short, long, default_value = "human")]
        format: Format,
    },

    /// Ingest source material into the wiki
    Ingest {
        /// Path to source file
        source: PathBuf,

        /// Page title (auto-derived from filename if omitted)
        #[arg(long)]
        title: Option<String>,

        /// Target category
        #[arg(long)]
        category: Option<String>,

        /// Raw subdirectory (papers, articles, assets)
        #[arg(long, default_value = "articles")]
        raw_type: String,
    },

    /// Start MCP server (stdio)
    Serve,
}

/// Resolve the wiki root: explicit --root flag > LW_WIKI_ROOT env > auto-discover from cwd.
fn resolve_root(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(root) = explicit {
        return Ok(root);
    }
    let cwd = std::env::current_dir()?;
    lw_core::fs::discover_wiki_root(&cwd)
        .ok_or_else(|| anyhow::anyhow!(
            "Not inside a wiki. Use --root, set LW_WIKI_ROOT, or run from within a wiki directory."
        ))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // For init, default to cwd if no root specified
    let root = if matches!(cli.command, Commands::Init) {
        cli.root.unwrap_or_else(|| PathBuf::from("."))
    } else {
        resolve_root(cli.root)?
    };
    match cli.command {
        Commands::Init => init::run(&root),
        Commands::Query {
            text,
            tag,
            category,
            limit,
            format,
        } => query::run(&root, &text, &tag, &category, limit, &format),
        Commands::Ingest {
            source,
            title,
            category,
            raw_type,
        } => ingest::run(&root, &source, &title, &category, &raw_type).await,
        Commands::Serve => serve::run(&root),
    }
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles with no errors

- [ ] **Step 5: Test MCP server with a quick JSON-RPC probe**

Run:

```bash
# Init a test wiki
cargo run --bin lw -- init --root /tmp/test-wiki-mcp

# Send initialize request to MCP server
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}' | cargo run --bin lw -- serve --root /tmp/test-wiki-mcp 2>/dev/null | head -1
```

Expected: JSON response containing `"serverInfo":{"name":"lw-mcp",...}`

- [ ] **Step 6: Commit**

```bash
git add crates/lw-mcp/src/lib.rs crates/lw-cli/src/serve.rs crates/lw-cli/src/main.rs
git commit -m "feat(mcp): add MCP server with wiki_query, wiki_read, wiki_browse, wiki_write, wiki_tags tools"
```

---

## Task 14: Scripts + Final Polish

**Files:**

- Create: `scripts/cron/weekly-lint.sh`
- Create: `scripts/cron/daily-ingest.sh`
- Create: `scripts/agents/librarian.sh`
- Create: `CLAUDE.md`

- [ ] **Step 1: Create orchestration scripts**

```bash
#!/usr/bin/env bash
# scripts/cron/weekly-lint.sh
# Triggered by cron weekly. Runs Claude Code to triage stale pages.
# Usage: crontab -e → 0 9 * * 1 /path/to/weekly-lint.sh
set -euo pipefail
WIKI_ROOT="${WIKI_ROOT:-/path/to/team-wiki}"
cd "$WIKI_ROOT"
claude -p "Run 'lw query' to find pages. Check git log dates. \
Flag any page that hasn't been updated in 90+ days. \
For stale pages, read the page and its sources, update the content. \
Commit changes with descriptive messages."
```

```bash
#!/usr/bin/env bash
# scripts/cron/daily-ingest.sh
# Process new files dropped into raw/inbox/
set -euo pipefail
WIKI_ROOT="${WIKI_ROOT:-/path/to/team-wiki}"
cd "$WIKI_ROOT"
mkdir -p raw/inbox
for f in raw/inbox/*; do
  [ -f "$f" ] || continue
  lw ingest "$f" --category _uncategorized
  echo "Ingested: $f"
done
```

```bash
#!/usr/bin/env bash
# scripts/agents/librarian.sh
# Interactive Q&A agent powered by Kimi K2P5 (or any LLM with MCP support)
set -euo pipefail
WIKI_ROOT="${WIKI_ROOT:-/path/to/team-wiki}"
cd "$WIKI_ROOT"
# Configure your agent to use lw as MCP server:
# { "mcpServers": { "wiki": { "command": "lw", "args": ["serve"] } } }
echo "Librarian ready. Configure your LLM agent to connect via MCP."
echo "MCP command: lw serve --root $WIKI_ROOT"
```

- [ ] **Step 2: Make scripts executable**

Run: `chmod +x scripts/cron/*.sh scripts/agents/*.sh`

- [ ] **Step 3: Create CLAUDE.md for the tool repo**

````markdown
# CLAUDE.md

## What This Is

LLM Wiki (`lw`) — a team knowledge base toolkit. Rust workspace producing a single
binary `lw` with CLI commands and an MCP server.

## Build & Test

```bash
cargo build              # build all crates
cargo test               # run all tests
cargo run --bin lw       # run CLI
```
````

## Architecture

- `crates/lw-core/` — core library (search, page parsing, ingest, etc.)
- `crates/lw-cli/` — CLI binary (`lw`)
- `crates/lw-mcp/` — MCP server library (used by `lw serve`)
- `crates/lw-server/` — Phase 2 HTTP server (placeholder)

## Key Design Rules

- Wiki is a separate git repo, not in this repo
- Only two traits: `Searcher` and `LlmBackend`; everything else is concrete types
- All time-related data comes from `git log`, never stored in frontmatter
- Freshness is computed, never stored

````

- [ ] **Step 4: Commit**

```bash
git add scripts/ CLAUDE.md
git commit -m "feat: add orchestration scripts and CLAUDE.md"
````

---

## Task 15: Integration Test

**Files:**

- Create: `crates/lw-cli/tests/cli_test.rs`
- Modify: `crates/lw-cli/Cargo.toml` (add assert_cmd dev-dependency)

- [ ] **Step 1: Add dev-dependency**

Add to `crates/lw-cli/Cargo.toml`:

```toml
[dev-dependencies]
assert_cmd = "2"
tempfile = "3"
predicates = "3"
```

- [ ] **Step 2: Write integration tests**

```rust
// crates/lw-cli/tests/cli_test.rs
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn lw() -> Command {
    Command::cargo_bin("lw").unwrap()
}

#[test]
fn init_creates_wiki() {
    let tmp = TempDir::new().unwrap();
    lw()
        .args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Wiki initialized"));

    assert!(tmp.path().join(".lw/schema.toml").exists());
    assert!(tmp.path().join("wiki/architecture").is_dir());
    assert!(tmp.path().join("wiki/_uncategorized").is_dir());
    assert!(tmp.path().join("raw/papers").is_dir());
}

#[test]
fn init_twice_fails() {
    let tmp = TempDir::new().unwrap();
    lw()
        .args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    lw()
        .args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

#[test]
fn query_on_empty_wiki() {
    let tmp = TempDir::new().unwrap();
    lw()
        .args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();
    lw()
        .args(["query", "anything", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No results"));
}

#[test]
fn query_finds_page() {
    let tmp = TempDir::new().unwrap();
    lw()
        .args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();

    // Write a test page directly
    let page_dir = tmp.path().join("wiki/architecture");
    std::fs::write(
        page_dir.join("test.md"),
        "---\ntitle: Test Page\ntags: [test]\n---\n\nHello world of testing.\n",
    )
    .unwrap();

    lw()
        .args([
            "query", "testing", "--root", tmp.path().to_str().unwrap(),
            "--format", "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Page"));
}

#[test]
fn query_json_format() {
    let tmp = TempDir::new().unwrap();
    lw()
        .args(["init", "--root", tmp.path().to_str().unwrap()])
        .assert()
        .success();

    std::fs::write(
        tmp.path().join("wiki/architecture/t.md"),
        "---\ntitle: Transformer\ntags: [arch]\n---\n\nAttention mechanism.\n",
    )
    .unwrap();

    let output = lw()
        .args([
            "query", "attention", "--root", tmp.path().to_str().unwrap(),
            "--format", "json",
        ])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["command"], "query");
    assert!(json["total"].as_u64().unwrap() >= 1);
    assert_eq!(json["results"][0]["title"], "Transformer");
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p lw-cli --test cli_test`
Expected: All 5 tests PASS

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests across all crates PASS

- [ ] **Step 5: Commit**

```bash
git add crates/lw-cli/tests/cli_test.rs crates/lw-cli/Cargo.toml
git commit -m "test: add CLI integration tests for init, query"
```

---

## Verification Checklist

After all tasks are complete:

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` — all tests pass
- [ ] `lw init` creates correct directory structure
- [ ] `lw query` finds pages with correct output in all 3 formats
- [ ] `lw ingest` copies to raw/ and creates wiki page
- [ ] `lw serve` starts MCP server that responds to JSON-RPC
- [ ] `lw --help` shows all 4 commands
