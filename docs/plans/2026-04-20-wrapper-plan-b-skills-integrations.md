# Plan B — Skills, Templates, Integrations Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the canonical `llm-wiki:import` skill, three starter vault templates (`general` / `research-papers` / `engineering-notes`), the TOML-driven integration adapter engine, and `lw integrate` command. Wire Claude Code as the first integration target with safe MCP config merging (atomic write + backup + version marker + diff-prompt on user-edited entries).

**Architecture:** Templates and skills are static asset trees in the repo, embedded into release tarballs by Plan C. The integrations engine is a small Rust subsystem in `lw-cli` that loads `*.toml` descriptors at runtime, knows how to read/merge per-tool config files (JSON or TOML), and manages skills as symlinks (copy-mode fallback for filesystems without symlink support). One adapter trait + descriptor loader covers the 90% case; per-tool logic only when truly needed.

**Tech Stack:** Rust 2024, `serde_json` (already in deps) for JSON merge, `toml` for descriptors, `dirs` for path resolution. Skills are markdown + YAML frontmatter.

**Working dir:** `tool/llm-wiki/`. Depends on Plan A (workspace registry must exist).

**Spec reference:** `docs/superpowers/specs/2026-04-19-llm-wiki-product-wrapper-design.md` §6, §9, §10, §10.5.

---

## File structure

| File                                                                                                 | Status | Responsibility                  |
| ---------------------------------------------------------------------------------------------------- | ------ | ------------------------------- |
| `templates/general/{.lw/schema.toml,SCOPE.md,wiki/_uncategorized/welcome.md,raw/.gitkeep}`           | create | Generic starter vault           |
| `templates/research-papers/{.lw/schema.toml,SCOPE.md,wiki/_uncategorized/welcome.md,raw/.gitkeep}`   | create | Paper-curation starter          |
| `templates/engineering-notes/{.lw/schema.toml,SCOPE.md,wiki/_uncategorized/welcome.md,raw/.gitkeep}` | create | Eng-notes starter               |
| `skills/llm-wiki-import/SKILL.md`                                                                    | create | Canonical import skill prompt   |
| `integrations/claude-code.toml`                                                                      | create | Claude Code adapter descriptor  |
| `crates/lw-cli/src/templates.rs`                                                                     | create | Locate templates dir, copy tree |
| `crates/lw-cli/src/workspace.rs`                                                                     | modify | Add `--template` to `add`       |
| `crates/lw-cli/src/integrations/mod.rs`                                                              | create | Module aggregator               |
| `crates/lw-cli/src/integrations/descriptor.rs`                                                       | create | `Descriptor` types + loader     |
| `crates/lw-cli/src/integrations/mcp.rs`                                                              | create | JSON config atomic merge        |
| `crates/lw-cli/src/integrations/skills.rs`                                                           | create | Symlink / copy management       |
| `crates/lw-cli/src/integrate.rs`                                                                     | create | `lw integrate` command body     |
| `crates/lw-cli/src/main.rs`                                                                          | modify | Wire `Integrate` + `--template` |
| `crates/lw-cli/tests/integrate_cli.rs`                                                               | create | E2E with fake config dirs       |

Asset directories (`templates/`, `skills/`, `integrations/`) live at the **repo root**, not inside `crates/`. They are copied into release tarballs by Plan C.

---

## Task 1: Create three starter template trees

**Files:**

- Create: `templates/general/.lw/schema.toml`
- Create: `templates/general/SCOPE.md`
- Create: `templates/general/wiki/_uncategorized/welcome.md`
- Create: `templates/general/raw/.gitkeep`
- Create: `templates/research-papers/.lw/schema.toml`
- Create: `templates/research-papers/SCOPE.md`
- Create: `templates/research-papers/wiki/_uncategorized/welcome.md`
- Create: `templates/research-papers/raw/.gitkeep`
- Create: `templates/engineering-notes/.lw/schema.toml`
- Create: `templates/engineering-notes/SCOPE.md`
- Create: `templates/engineering-notes/wiki/_uncategorized/welcome.md`
- Create: `templates/engineering-notes/raw/.gitkeep`

- [ ] **Step 1: Create general template**

`templates/general/.lw/schema.toml`:

```toml
[tags]
categories = ["notes", "links", "people", "_uncategorized"]
```

`templates/general/SCOPE.md`:

```markdown
# Scope

## Purpose

A general-purpose knowledge base. Capture interesting links, articles, notes, and references that you want to come back to later.

## Includes

- Articles and blog posts you found valuable
- Notes from books, talks, podcasts
- Links worth saving with context for why
- People and organizations worth tracking

## Excludes

- Private credentials, API keys, secrets
- Daily journal entries (use a separate journal tool)
- Code snippets that belong in a real codebase
```

`templates/general/wiki/_uncategorized/welcome.md`:

```markdown
---
title: Welcome
tags: [meta]
---

# Welcome to your wiki

This is a starter page. Edit or delete it.

To add content, ask your agent: _"add this link to my wiki"_ with a URL or paste the text directly. The `llm-wiki:import` skill will handle fetching, scope-checking, and filing.

To customize what gets accepted, edit `SCOPE.md` at the vault root.
```

`templates/general/raw/.gitkeep`: empty file.

- [ ] **Step 2: Create research-papers template**

`templates/research-papers/.lw/schema.toml`:

```toml
[tags]
categories = ["architecture", "training", "evaluation", "applications", "_uncategorized"]
```

`templates/research-papers/SCOPE.md`:

```markdown
# Scope

## Purpose

A curated collection of machine learning research papers, with summaries, key takeaways, and cross-references.

## Includes

- arXiv papers and their summaries
- Conference / workshop publications
- Survey papers and benchmark studies
- Reference works on ML architecture, training methods, evaluation

## Excludes

- News articles about papers (link in the paper page if relevant)
- Marketing posts and product announcements
- Twitter threads (capture the underlying paper instead)
```

`templates/research-papers/wiki/_uncategorized/welcome.md`:

```markdown
---
title: Welcome
tags: [meta]
---

# Research Papers

Drop arXiv URLs into your agent and ask it to import. The `llm-wiki:import` skill will fetch, summarize, and file under the right category (architecture / training / evaluation / applications).

Categories are configured in `.lw/schema.toml`. Edit them to match your research focus.
```

`templates/research-papers/raw/.gitkeep`: empty.

- [ ] **Step 3: Create engineering-notes template**

`templates/engineering-notes/.lw/schema.toml`:

```toml
[tags]
categories = ["systems", "tools", "patterns", "incidents", "_uncategorized"]
```

`templates/engineering-notes/SCOPE.md`:

```markdown
# Scope

## Purpose

A working engineer's reference: tools you use, systems you've debugged, patterns you've found useful, incidents you've learned from.

## Includes

- Tool docs and gotchas (CLIs, libraries, infra)
- Architecture write-ups for systems you maintain or depend on
- Reusable patterns: code, config, debugging methodology
- Incident postmortems (yours or notable public ones)

## Excludes

- Production credentials or sensitive infra details
- Code that should be in a repo with version control
- Personal task lists (use an issue tracker)
```

`templates/engineering-notes/wiki/_uncategorized/welcome.md`:

```markdown
---
title: Welcome
tags: [meta]
---

# Engineering Notes

Working engineer's reference. Categories: systems, tools, patterns, incidents.

Customize categories in `.lw/schema.toml`. Adjust `SCOPE.md` to reflect what you actually want to capture.
```

`templates/engineering-notes/raw/.gitkeep`: empty.

- [ ] **Step 4: Verify tree**

```bash
find templates -type f | sort
```

Expected:

```
templates/engineering-notes/.lw/schema.toml
templates/engineering-notes/SCOPE.md
templates/engineering-notes/raw/.gitkeep
templates/engineering-notes/wiki/_uncategorized/welcome.md
templates/general/.lw/schema.toml
templates/general/SCOPE.md
templates/general/raw/.gitkeep
templates/general/wiki/_uncategorized/welcome.md
templates/research-papers/.lw/schema.toml
templates/research-papers/SCOPE.md
templates/research-papers/raw/.gitkeep
templates/research-papers/wiki/_uncategorized/welcome.md
```

- [ ] **Step 5: Commit**

```bash
git add templates/
git commit -m "feat(templates): add general/research-papers/engineering-notes starter vaults"
```

---

## Task 2: Templates discovery + copy + `--template` flag on `workspace add`

**Files:**

- Create: `crates/lw-cli/src/templates.rs`
- Modify: `crates/lw-cli/src/workspace.rs`
- Modify: `crates/lw-cli/src/main.rs`

- [ ] **Step 1: Write tests**

Create `crates/lw-cli/src/templates.rs`:

```rust
use std::path::{Path, PathBuf};

/// Resolution order for the templates root:
/// 1. $LW_TEMPLATES_DIR (explicit override, mostly for tests)
/// 2. $LW_HOME/templates/ (set by installer)
/// 3. ~/.llm-wiki/templates/ (default install location)
/// 4. <exe_dir>/../share/llm-wiki/templates/ (cargo-installed layout)
/// 5. <repo>/templates/ (development checkout — exe at target/debug/lw)
pub fn templates_root() -> anyhow::Result<PathBuf> {
    if let Ok(p) = std::env::var("LW_TEMPLATES_DIR") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(home) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(home).join("templates"));
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".llm-wiki").join("templates");
        if p.exists() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe()?;
    if let Some(exe_dir) = exe.parent() {
        let share = exe_dir.join("../share/llm-wiki/templates");
        if share.exists() {
            return Ok(share);
        }
        // Dev fallback: walk up looking for templates/
        let mut cur = exe_dir.to_path_buf();
        for _ in 0..6 {
            let candidate = cur.join("templates");
            if candidate.exists() {
                return Ok(candidate);
            }
            if !cur.pop() {
                break;
            }
        }
    }
    anyhow::bail!("Cannot locate templates directory")
}

pub fn list_available() -> anyhow::Result<Vec<String>> {
    let root = templates_root()?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Copy a template tree into `dest`. Dest must be empty or non-existent.
/// Skips `.gitkeep` placeholder files (they only exist to ship empty dirs).
pub fn copy_template(template_name: &str, dest: &Path) -> anyhow::Result<()> {
    let root = templates_root()?;
    let src = root.join(template_name);
    if !src.exists() {
        let avail = list_available().unwrap_or_default().join(", ");
        anyhow::bail!(
            "template '{template_name}' not found in {} (available: {avail})",
            root.display()
        );
    }
    if dest.exists() && std::fs::read_dir(dest)?.next().is_some() {
        anyhow::bail!("destination {} is not empty", dest.display());
    }
    std::fs::create_dir_all(dest)?;
    copy_recursive(&src, dest)
}

fn copy_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let name = entry.file_name();
        if name == ".gitkeep" {
            // Materialize the parent directory but don't copy the placeholder
            std::fs::create_dir_all(dst)?;
            continue;
        }
        let to = dst.join(&name);
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&to)?;
            copy_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_fake_templates(dir: &Path) {
        let t = dir.join("templates").join("demo");
        std::fs::create_dir_all(t.join(".lw")).unwrap();
        std::fs::create_dir_all(t.join("wiki/_uncategorized")).unwrap();
        std::fs::write(t.join(".lw/schema.toml"), "[tags]\ncategories = [\"_uncategorized\"]\n").unwrap();
        std::fs::write(t.join("SCOPE.md"), "# Scope\n").unwrap();
        std::fs::write(t.join("wiki/_uncategorized/welcome.md"), "# Hi\n").unwrap();
        std::fs::write(t.join(".gitkeep"), "").unwrap();
        std::fs::create_dir_all(t.join("raw")).unwrap();
        std::fs::write(t.join("raw/.gitkeep"), "").unwrap();
    }

    #[test]
    fn list_available_finds_dirs() {
        let dir = TempDir::new().unwrap();
        make_fake_templates(dir.path());
        // SAFETY: tests serialized via --test-threads=1
        unsafe { std::env::set_var("LW_TEMPLATES_DIR", dir.path().join("templates")) };
        let avail = list_available().unwrap();
        assert_eq!(avail, vec!["demo".to_string()]);
        unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };
    }

    #[test]
    fn copy_template_writes_tree() {
        let templates_dir = TempDir::new().unwrap();
        make_fake_templates(templates_dir.path());
        let dest = TempDir::new().unwrap();
        unsafe { std::env::set_var("LW_TEMPLATES_DIR", templates_dir.path().join("templates")) };
        copy_template("demo", &dest.path().join("vault")).unwrap();
        unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };

        assert!(dest.path().join("vault/.lw/schema.toml").exists());
        assert!(dest.path().join("vault/SCOPE.md").exists());
        assert!(dest.path().join("vault/wiki/_uncategorized/welcome.md").exists());
        assert!(dest.path().join("vault/raw").exists());
        // .gitkeep must NOT be copied
        assert!(!dest.path().join("vault/raw/.gitkeep").exists());
        assert!(!dest.path().join("vault/.gitkeep").exists());
    }

    #[test]
    fn copy_template_unknown_errors() {
        let templates_dir = TempDir::new().unwrap();
        make_fake_templates(templates_dir.path());
        let dest = TempDir::new().unwrap();
        unsafe { std::env::set_var("LW_TEMPLATES_DIR", templates_dir.path().join("templates")) };
        let result = copy_template("ghost", &dest.path().join("vault"));
        unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };
        assert!(result.is_err());
    }

    #[test]
    fn copy_template_rejects_nonempty_dest() {
        let templates_dir = TempDir::new().unwrap();
        make_fake_templates(templates_dir.path());
        let dest = TempDir::new().unwrap();
        let vault = dest.path().join("vault");
        std::fs::create_dir(&vault).unwrap();
        std::fs::write(vault.join("stranger.txt"), "hi").unwrap();
        unsafe { std::env::set_var("LW_TEMPLATES_DIR", templates_dir.path().join("templates")) };
        let result = copy_template("demo", &vault);
        unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };
        assert!(result.is_err());
    }
}
```

Add `mod templates;` to `crates/lw-cli/src/main.rs`.

- [ ] **Step 2: Update workspace::add to support --template**

In `crates/lw-cli/src/workspace.rs`, change the signature and body of `add`:

```rust
pub fn add(name: &str, path: &Path, init: bool, template: Option<&str>) -> anyhow::Result<()> {
    validate_name(name)?;
    let abs = resolve_path(path)?;

    let cfg_path = config_path()?;
    let mut cfg = Config::load_from(&cfg_path)?;

    if cfg.workspaces.contains_key(name) {
        anyhow::bail!("workspace '{name}' already exists");
    }

    if init && template.is_some() {
        anyhow::bail!("--init and --template are mutually exclusive");
    }

    if let Some(tpl) = template {
        if abs.exists() && std::fs::read_dir(&abs)?.next().is_some() {
            anyhow::bail!("--template requires an empty or non-existent directory");
        }
        crate::templates::copy_template(tpl, &abs)?;
    } else if init {
        if !abs.exists() {
            std::fs::create_dir_all(&abs)?;
        }
        let is_empty = std::fs::read_dir(&abs)?.next().is_none();
        if !is_empty && !abs.join(".lw/schema.toml").exists() {
            anyhow::bail!(
                "--init requires an empty directory or an existing wiki (got non-empty non-wiki at {})",
                abs.display()
            );
        }
        if !abs.join(".lw/schema.toml").exists() {
            let schema = lw_core::schema::WikiSchema::default();
            lw_core::fs::init_wiki(&abs, &schema)?;
        }
    }

    let first_workspace = cfg.workspaces.is_empty();
    cfg.workspaces.insert(name.into(), WorkspaceEntry { path: abs.clone() });
    if first_workspace {
        cfg.workspace.current = Some(name.into());
    }
    cfg.save_to(&cfg_path)?;

    println!("Added workspace '{name}' at {}", abs.display());
    if let Some(tpl) = template {
        println!("  initialized from template '{tpl}'");
    }
    if first_workspace {
        println!("  set as current (first workspace)");
    }
    Ok(())
}
```

In the existing `tests` mod inside `workspace.rs`, update each call to `add(...)` from 3 args to 4 (pass `None` for template):

```rust
add("personal", vault.path(), false, None).unwrap();
// ... and so on for all existing test sites
```

Add a new test at the bottom of the `tests` module:

```rust
    #[test]
    fn add_with_template_copies_tree() {
        use std::path::Path;
        let home = TempDir::new().unwrap();
        let templates_dir = TempDir::new().unwrap();
        // Stub templates
        let demo = templates_dir.path().join("templates").join("demo");
        std::fs::create_dir_all(demo.join(".lw")).unwrap();
        std::fs::create_dir_all(demo.join("wiki/_uncategorized")).unwrap();
        std::fs::write(demo.join(".lw/schema.toml"), "[tags]\ncategories = [\"_uncategorized\"]\n").unwrap();
        std::fs::write(demo.join("SCOPE.md"), "# Scope\n").unwrap();
        std::fs::write(demo.join("wiki/_uncategorized/welcome.md"), "# Hi\n").unwrap();

        let vault = TempDir::new().unwrap();
        let target: &Path = &vault.path().join("v");
        with_lw_home(home.path(), || {
            unsafe { std::env::set_var("LW_TEMPLATES_DIR", templates_dir.path().join("templates")) };
            add("foo", target, false, Some("demo")).unwrap();
            unsafe { std::env::remove_var("LW_TEMPLATES_DIR") };
            assert!(target.join("SCOPE.md").exists());
            assert!(target.join(".lw/schema.toml").exists());
        });
    }
```

In `crud_tests` and `workspace_cli.rs` integration tests from Plan A, update to 4-arg `add` calls. (Plan A's integration tests use the CLI, not direct calls, so they only need updating if CLI changes — see Step 3.)

- [ ] **Step 3: Wire `--template` into the CLI**

In `crates/lw-cli/src/main.rs`, change the `WorkspaceCmd::Add` variant:

```rust
    /// Register a new workspace
    Add {
        /// Workspace name (lowercase alphanumeric + dashes)
        name: String,
        /// Path to the vault directory
        path: PathBuf,
        /// Initialize an empty wiki at the path if it does not exist
        #[arg(long)]
        init: bool,
        /// Initialize from a starter template (general | research-papers | engineering-notes)
        #[arg(long)]
        template: Option<String>,
    },
```

Update the dispatch:

```rust
            WorkspaceCmd::Add { name, path, init, template } => {
                workspace::add(&name, &path, init, template.as_deref())
            }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p lw-cli -- --test-threads=1
```

Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add crates/lw-cli/src/templates.rs crates/lw-cli/src/workspace.rs crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): add templates module and --template flag on workspace add"
```

---

## Task 3: Canonical `llm-wiki:import` skill

**Files:**

- Create: `skills/llm-wiki-import/SKILL.md`

- [ ] **Step 1: Write the skill**

Create `skills/llm-wiki-import/SKILL.md`:

```markdown
---
name: llm-wiki:import
description: Use when the user wants to add a URL, pasted text, or local file to their llm-wiki. Fetches content if needed, checks against the vault's SCOPE.md, and ingests via `wiki_ingest`.
when-to-use: User shares a link, paste, or path with intent to save. Trigger phrases include "add this to my wiki", "save this", "remember this article", "ingest this paper", or any case where the user supplies content with archival intent.
---

You are helping the user maintain an llm-wiki vault. The user has shared content (a URL, pasted text, or a file path) and wants it added to the wiki.

## Step 1 — Identify the input type

- **URL**: a web link → fetch it (use `wiki_ingest` with the URL directly; the tool handles fetching and parsing).
- **Pasted text**: raw markdown or prose → use `wiki_write` for full pages, or `wiki_ingest` from stdin.
- **Local file path**: a file the user already has → pass the path to `wiki_ingest`.

## Step 2 — Check scope

Read `SCOPE.md` from the vault root using `wiki_read SCOPE.md`. If `SCOPE.md` does not exist, **skip the scope check entirely** — proceed permissively.

If `SCOPE.md` exists, judge whether the new content fits the documented Purpose / Includes / Excludes. The judgment is yours as the agent — `SCOPE.md` is guidance, not a strict rule list.

## Step 3 — Route the content

- **Clearly in scope**: ingest immediately.
  - For URLs and files: `wiki_ingest` with `--raw-type articles` (or `papers` for arXiv-style links, `assets` for binary files).
  - For pasted markdown that looks like a finished article: `wiki_ingest --stdin`.
  - Suggest a category from the wiki's known categories (read `.lw/schema.toml` if needed).
- **Clearly out of scope**: do not silently drop. Tell the user:
  > "This looks out of scope for your wiki (Purpose: <quote>). Want me to add it anyway, or skip?"
  > Wait for an answer.
- **Ambiguous**: ask before ingesting.
  > "I'm not sure this fits the scope. The vault is for <Purpose>; this looks like <observation>. Add it?"

## Step 4 — Confirm

After ingestion, print a one-line confirmation: file path written, category, freshness (raw vs full page).

## Hard rules

- Never silent-fail. If you cannot complete the import, tell the user why.
- Never overwrite an existing wiki page without confirmation; use `wiki_read` first to check.
- For URLs that fail to fetch (404, paywall, login wall), report the failure and ask whether to file the URL with a note rather than the content.
- Do not invent metadata (publication date, author) that you cannot verify from the source.

## MCP tools available

- `wiki_query` — search existing pages
- `wiki_read` — read a page
- `wiki_browse` — browse categories / tags
- `wiki_ingest` — file new raw content (URL, file, or stdin)
- `wiki_write` — write or update a wiki page
- `wiki_lint` — health check
- `wiki_tags` — list tags
```

- [ ] **Step 2: Verify file exists**

```bash
ls -la skills/llm-wiki-import/SKILL.md
```

Expected: file present, non-empty.

- [ ] **Step 3: Commit**

```bash
git add skills/
git commit -m "feat(skills): add canonical llm-wiki:import skill"
```

---

## Task 4: IntegrationDescriptor types + loader + `claude-code.toml`

**Files:**

- Create: `crates/lw-cli/src/integrations/mod.rs`
- Create: `crates/lw-cli/src/integrations/descriptor.rs`
- Create: `integrations/claude-code.toml`

- [ ] **Step 1: Create descriptor types**

Create `crates/lw-cli/src/integrations/descriptor.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Descriptor {
    pub name: String,
    pub detect: Detect,
    pub mcp: Option<McpConfig>,
    pub skills: Option<SkillsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Detect {
    pub config_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpConfig {
    pub config_path: String,
    pub format: McpFormat,
    pub key_path: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpFormat {
    Json,
    Toml,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillsConfig {
    pub target_dir: String,
    pub mode: SkillsMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillsMode {
    Symlink,
    Copy,
}

/// Tilde-expand a path string. `~` and `~/` resolve to $HOME.
pub fn expand_tilde(s: &str) -> PathBuf {
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(s)
}

impl Descriptor {
    pub fn detect_present(&self) -> bool {
        expand_tilde(&self.detect.config_dir).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_claude_code_descriptor() {
        let toml_str = r#"
name = "Claude Code"

[detect]
config_dir = "~/.claude"

[mcp]
config_path = "~/.claude/settings.json"
format = "json"
key_path = "mcpServers.llm-wiki"
command = "lw"
args = ["serve"]

[skills]
target_dir = "~/.claude/skills/llm-wiki/"
mode = "symlink"
"#;
        let d: Descriptor = toml::from_str(toml_str).unwrap();
        assert_eq!(d.name, "Claude Code");
        assert_eq!(d.mcp.as_ref().unwrap().format, McpFormat::Json);
        assert_eq!(d.skills.as_ref().unwrap().mode, SkillsMode::Symlink);
    }

    #[test]
    fn skills_only_descriptor_no_mcp() {
        let toml_str = r#"
name = "Skill-only Tool"

[detect]
config_dir = "~/.someothertool"

[skills]
target_dir = "~/.someothertool/skills/llm-wiki/"
mode = "copy"
"#;
        let d: Descriptor = toml::from_str(toml_str).unwrap();
        assert!(d.mcp.is_none());
        assert_eq!(d.skills.as_ref().unwrap().mode, SkillsMode::Copy);
    }

    #[test]
    fn tilde_expansion() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~/foo"), home.join("foo"));
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
        assert_eq!(expand_tilde("rel"), PathBuf::from("rel"));
    }
}
```

- [ ] **Step 2: Create module aggregator + loader**

Create `crates/lw-cli/src/integrations/mod.rs`:

```rust
pub mod descriptor;
pub mod mcp;
pub mod skills;

use descriptor::Descriptor;
use std::path::PathBuf;

/// Resolution order for the integrations descriptor dir:
/// 1. $LW_INTEGRATIONS_DIR
/// 2. $LW_HOME/integrations
/// 3. ~/.llm-wiki/integrations
/// 4. <exe_dir>/../share/llm-wiki/integrations
/// 5. <repo>/integrations  (dev fallback)
pub fn integrations_root() -> anyhow::Result<PathBuf> {
    if let Ok(p) = std::env::var("LW_INTEGRATIONS_DIR") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(home) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(home).join("integrations"));
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".llm-wiki").join("integrations");
        if p.exists() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe()?;
    if let Some(exe_dir) = exe.parent() {
        let share = exe_dir.join("../share/llm-wiki/integrations");
        if share.exists() {
            return Ok(share);
        }
        let mut cur = exe_dir.to_path_buf();
        for _ in 0..6 {
            let candidate = cur.join("integrations");
            if candidate.exists() {
                return Ok(candidate);
            }
            if !cur.pop() {
                break;
            }
        }
    }
    anyhow::bail!("Cannot locate integrations directory")
}

pub fn load_all() -> anyhow::Result<Vec<(String, Descriptor)>> {
    let root = integrations_root()?;
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("toml") {
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("invalid descriptor filename"))?
                .to_string();
            let body = std::fs::read_to_string(&path)?;
            let desc: Descriptor = toml::from_str(&body)
                .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?;
            out.push((id, desc));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Stubs — populated by Tasks 5-7
pub struct IntegrateOpts {
    pub yes: bool,
    pub interactive: bool,
}
```

Create stub files so the module compiles:

`crates/lw-cli/src/integrations/mcp.rs`:

```rust
// Populated by Task 5
```

`crates/lw-cli/src/integrations/skills.rs`:

```rust
// Populated by Task 6
```

Add `mod integrations;` to `crates/lw-cli/src/main.rs`.

- [ ] **Step 3: Create the Claude Code descriptor**

Create `integrations/claude-code.toml`:

```toml
name = "Claude Code"

[detect]
config_dir = "~/.claude"

[mcp]
config_path = "~/.claude/settings.json"
format = "json"
key_path = "mcpServers.llm-wiki"
command = "lw"
args = ["serve"]

[skills]
target_dir = "~/.claude/skills/llm-wiki/"
mode = "symlink"
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p lw-cli integrations -- --test-threads=1
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/lw-cli/src/integrations/ crates/lw-cli/src/main.rs integrations/
git commit -m "feat(lw-cli): add integration descriptor types and Claude Code adapter"
```

---

## Task 5: MCP config atomic merge with version markers + backup

**Files:**

- Modify: `crates/lw-cli/src/integrations/mcp.rs`

- [ ] **Step 1: Implement merge logic with tests**

Replace `crates/lw-cli/src/integrations/mcp.rs`:

```rust
use serde_json::{Map, Value};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const VERSION_MARKER: &str = "_lw_version";

/// Result of a merge attempt.
#[derive(Debug, PartialEq)]
pub enum MergeOutcome {
    /// Entry inserted (was absent).
    Inserted,
    /// Entry updated; previous version matched expected (clean upgrade).
    Updated,
    /// Entry exists but appears user-edited; not modified.
    Conflict { existing: Value },
    /// Entry already matches desired; no-op.
    NoOp,
}

/// Merge a managed entry into a JSON config.
///
/// `key_path` is dot-separated (e.g., "mcpServers.llm-wiki").
/// `entry` MUST contain a `_lw_version` field; it will be added if missing.
/// `expected_prev_version` is the version we last shipped; if the existing entry's
/// `_lw_version` matches, we treat it as an unmodified upgrade and replace silently.
/// If it does not match (or `_lw_version` is absent), we treat it as user-edited
/// and return `Conflict` without modifying.
pub fn merge_entry(
    config: &mut Value,
    key_path: &str,
    mut entry: Value,
    expected_prev_version: Option<&str>,
) -> anyhow::Result<MergeOutcome> {
    // Ensure entry has a version marker
    if !entry
        .as_object()
        .map(|o| o.contains_key(VERSION_MARKER))
        .unwrap_or(false)
    {
        anyhow::bail!("entry must include '{VERSION_MARKER}' field");
    }

    let parts: Vec<&str> = key_path.split('.').collect();
    let (last, parents) = parts.split_last().unwrap();

    // Walk / create parents
    let mut cursor = config;
    for p in parents {
        if !cursor.is_object() {
            *cursor = Value::Object(Map::new());
        }
        let obj = cursor.as_object_mut().unwrap();
        cursor = obj.entry((*p).to_string()).or_insert(Value::Object(Map::new()));
    }
    if !cursor.is_object() {
        *cursor = Value::Object(Map::new());
    }
    let obj = cursor.as_object_mut().unwrap();

    match obj.get(*last) {
        None => {
            obj.insert((*last).to_string(), entry);
            Ok(MergeOutcome::Inserted)
        }
        Some(existing) if existing == &entry => Ok(MergeOutcome::NoOp),
        Some(existing) => {
            let existing_ver = existing
                .get(VERSION_MARKER)
                .and_then(|v| v.as_str());
            match (existing_ver, expected_prev_version) {
                (Some(ev), Some(pv)) if ev == pv => {
                    // Clean upgrade
                    if let Some(obj_entry) = entry.as_object_mut() {
                        obj_entry.insert(
                            VERSION_MARKER.into(),
                            entry
                                .get(VERSION_MARKER)
                                .cloned()
                                .unwrap_or_else(|| Value::String("unknown".into())),
                        );
                    }
                    obj.insert((*last).to_string(), entry);
                    Ok(MergeOutcome::Updated)
                }
                _ => Ok(MergeOutcome::Conflict { existing: existing.clone() }),
            }
        }
    }
}

/// Atomically write a JSON file: backup → temp → fsync → rename.
/// Returns the backup path so callers can report it.
pub fn atomic_write_with_backup(
    path: &Path,
    body: &str,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    let backup_path = if path.exists() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();
        let bak = path.with_extension(format!(
            "{}.bak.{ts}",
            path.extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
        ));
        std::fs::copy(path, &bak)?;
        Some(bak)
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        None
    };
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|s| s.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, body)?;
    let f = std::fs::File::open(&tmp)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(backup_path)
}

/// Remove an entry from JSON config by key_path. Returns true if removed.
pub fn remove_entry(config: &mut Value, key_path: &str) -> bool {
    let parts: Vec<&str> = key_path.split('.').collect();
    let (last, parents) = parts.split_last().unwrap();
    let mut cursor = config;
    for p in parents {
        match cursor.get_mut(*p) {
            Some(child) => cursor = child,
            None => return false,
        }
    }
    cursor
        .as_object_mut()
        .map(|o| o.remove(*last).is_some())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn entry(version: &str) -> Value {
        json!({
            "command": "lw",
            "args": ["serve"],
            VERSION_MARKER: version
        })
    }

    #[test]
    fn merge_inserts_when_absent() {
        let mut cfg = json!({});
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0"), None).unwrap();
        assert_eq!(outcome, MergeOutcome::Inserted);
        assert_eq!(
            cfg["mcpServers"]["llm-wiki"][VERSION_MARKER],
            json!("0.2.0")
        );
    }

    #[test]
    fn merge_noop_when_identical() {
        let mut cfg = json!({"mcpServers": {"llm-wiki": entry("0.2.0")}});
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0"), Some("0.2.0")).unwrap();
        assert_eq!(outcome, MergeOutcome::NoOp);
    }

    #[test]
    fn merge_updates_when_prev_version_matches() {
        let mut cfg = json!({"mcpServers": {"llm-wiki": entry("0.1.0")}});
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0"), Some("0.1.0")).unwrap();
        assert_eq!(outcome, MergeOutcome::Updated);
        assert_eq!(
            cfg["mcpServers"]["llm-wiki"][VERSION_MARKER],
            json!("0.2.0")
        );
    }

    #[test]
    fn merge_conflict_when_user_edited() {
        let mut user_edited = entry("0.1.0");
        user_edited["args"] = json!(["serve", "--root", "/custom"]);
        let mut cfg = json!({"mcpServers": {"llm-wiki": user_edited.clone()}});
        let outcome = merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0"), Some("0.0.1")).unwrap();
        match outcome {
            MergeOutcome::Conflict { existing } => assert_eq!(existing, user_edited),
            _ => panic!("expected Conflict"),
        }
        // Config must be unchanged
        assert_eq!(cfg["mcpServers"]["llm-wiki"], user_edited);
    }

    #[test]
    fn merge_preserves_sibling_entries() {
        let mut cfg = json!({
            "mcpServers": {
                "other-tool": {"command": "other"},
                "llm-wiki": entry("0.1.0"),
            },
            "permissions": {"allow": ["foo"]},
        });
        merge_entry(&mut cfg, "mcpServers.llm-wiki", entry("0.2.0"), Some("0.1.0")).unwrap();
        assert_eq!(cfg["mcpServers"]["other-tool"], json!({"command": "other"}));
        assert_eq!(cfg["permissions"]["allow"], json!(["foo"]));
    }

    #[test]
    fn merge_rejects_entry_without_version_marker() {
        let mut cfg = json!({});
        let bad = json!({"command": "lw", "args": ["serve"]});
        let result = merge_entry(&mut cfg, "mcpServers.llm-wiki", bad, None);
        assert!(result.is_err());
    }

    #[test]
    fn atomic_write_creates_backup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{\"old\": true}").unwrap();
        let backup = atomic_write_with_backup(&path, "{\"new\": true}").unwrap();
        assert!(backup.is_some());
        let bak = backup.unwrap();
        assert!(bak.exists());
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), "{\"old\": true}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"new\": true}");
    }

    #[test]
    fn atomic_write_no_backup_when_file_absent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        let backup = atomic_write_with_backup(&path, "{}").unwrap();
        assert!(backup.is_none());
        assert!(path.exists());
    }

    #[test]
    fn remove_entry_returns_true_when_present() {
        let mut cfg = json!({"mcpServers": {"llm-wiki": entry("0.2.0"), "other": {}}});
        assert!(remove_entry(&mut cfg, "mcpServers.llm-wiki"));
        assert!(cfg["mcpServers"]["llm-wiki"].is_null());
        assert_eq!(cfg["mcpServers"]["other"], json!({}));
    }

    #[test]
    fn remove_entry_returns_false_when_absent() {
        let mut cfg = json!({});
        assert!(!remove_entry(&mut cfg, "mcpServers.llm-wiki"));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p lw-cli mcp:: -- --test-threads=1
```

Expected: 9 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/src/integrations/mcp.rs
git commit -m "feat(integrations): atomic JSON config merge with version markers and backup"
```

---

## Task 6: Skills symlink / copy installer

**Files:**

- Modify: `crates/lw-cli/src/integrations/skills.rs`

- [ ] **Step 1: Implement skills installer**

Replace `crates/lw-cli/src/integrations/skills.rs`:

```rust
use crate::integrations::descriptor::{SkillsConfig, SkillsMode, expand_tilde};
use std::path::{Path, PathBuf};

pub fn skills_root() -> anyhow::Result<PathBuf> {
    if let Ok(p) = std::env::var("LW_SKILLS_DIR") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(home) = std::env::var("LW_HOME") {
        return Ok(PathBuf::from(home).join("skills"));
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".llm-wiki").join("skills");
        if p.exists() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe()?;
    if let Some(exe_dir) = exe.parent() {
        let share = exe_dir.join("../share/llm-wiki/skills");
        if share.exists() {
            return Ok(share);
        }
        let mut cur = exe_dir.to_path_buf();
        for _ in 0..6 {
            let candidate = cur.join("skills");
            if candidate.exists() {
                return Ok(candidate);
            }
            if !cur.pop() {
                break;
            }
        }
    }
    anyhow::bail!("Cannot locate skills directory")
}

pub fn install(cfg: &SkillsConfig) -> anyhow::Result<PathBuf> {
    let target = expand_tilde(&cfg.target_dir);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let source = skills_root()?;
    if !source.exists() {
        anyhow::bail!(
            "skills source not found at {} — install layout broken",
            source.display()
        );
    }
    // If target already exists, replace it.
    if target.exists() || target.symlink_metadata().is_ok() {
        if target.is_symlink() || target.is_file() {
            std::fs::remove_file(&target)?;
        } else if target.is_dir() {
            std::fs::remove_dir_all(&target)?;
        }
    }
    match cfg.mode {
        SkillsMode::Symlink => link_dir(&source, &target)?,
        SkillsMode::Copy => copy_recursive(&source, &target)?,
    }
    Ok(target)
}

pub fn uninstall(cfg: &SkillsConfig) -> anyhow::Result<bool> {
    let target = expand_tilde(&cfg.target_dir);
    if !target.exists() && target.symlink_metadata().is_err() {
        return Ok(false);
    }
    if target.is_symlink() || target.is_file() {
        std::fs::remove_file(&target)?;
    } else if target.is_dir() {
        std::fs::remove_dir_all(&target)?;
    }
    Ok(true)
}

#[cfg(unix)]
fn link_dir(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::os::unix::fs::symlink(src, dst)?;
    Ok(())
}

#[cfg(not(unix))]
fn link_dir(_src: &Path, _dst: &Path) -> anyhow::Result<()> {
    anyhow::bail!("symlink mode requires Unix; use mode = \"copy\" on this platform")
}

fn copy_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_skills_root(dir: &Path) {
        let s = dir.join("skills").join("llm-wiki-import");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(s.join("SKILL.md"), "---\nname: test\n---\nbody").unwrap();
    }

    #[test]
    fn install_symlink_creates_link() {
        let src_dir = TempDir::new().unwrap();
        make_skills_root(src_dir.path());
        let target_dir = TempDir::new().unwrap();
        let cfg = SkillsConfig {
            target_dir: target_dir.path().join("llm-wiki").display().to_string(),
            mode: SkillsMode::Symlink,
        };
        unsafe { std::env::set_var("LW_SKILLS_DIR", src_dir.path().join("skills")) };
        let target = install(&cfg).unwrap();
        unsafe { std::env::remove_var("LW_SKILLS_DIR") };
        assert!(target.exists());
        assert!(target.is_symlink() || target.read_link().is_ok());
        assert!(target.join("llm-wiki-import/SKILL.md").exists());
    }

    #[test]
    fn install_copy_writes_files() {
        let src_dir = TempDir::new().unwrap();
        make_skills_root(src_dir.path());
        let target_dir = TempDir::new().unwrap();
        let cfg = SkillsConfig {
            target_dir: target_dir.path().join("llm-wiki").display().to_string(),
            mode: SkillsMode::Copy,
        };
        unsafe { std::env::set_var("LW_SKILLS_DIR", src_dir.path().join("skills")) };
        let target = install(&cfg).unwrap();
        unsafe { std::env::remove_var("LW_SKILLS_DIR") };
        assert!(target.join("llm-wiki-import/SKILL.md").exists());
        assert!(!target.is_symlink());
    }

    #[test]
    fn uninstall_removes_symlink() {
        let src_dir = TempDir::new().unwrap();
        make_skills_root(src_dir.path());
        let target_dir = TempDir::new().unwrap();
        let cfg = SkillsConfig {
            target_dir: target_dir.path().join("llm-wiki").display().to_string(),
            mode: SkillsMode::Symlink,
        };
        unsafe { std::env::set_var("LW_SKILLS_DIR", src_dir.path().join("skills")) };
        install(&cfg).unwrap();
        let removed = uninstall(&cfg).unwrap();
        unsafe { std::env::remove_var("LW_SKILLS_DIR") };
        assert!(removed);
        assert!(!target_dir.path().join("llm-wiki").exists());
    }

    #[test]
    fn install_replaces_existing_symlink() {
        let src_dir = TempDir::new().unwrap();
        make_skills_root(src_dir.path());
        let target_dir = TempDir::new().unwrap();
        let cfg = SkillsConfig {
            target_dir: target_dir.path().join("llm-wiki").display().to_string(),
            mode: SkillsMode::Symlink,
        };
        unsafe { std::env::set_var("LW_SKILLS_DIR", src_dir.path().join("skills")) };
        install(&cfg).unwrap();
        // Re-install must succeed (replaces existing)
        install(&cfg).unwrap();
        unsafe { std::env::remove_var("LW_SKILLS_DIR") };
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p lw-cli skills:: -- --test-threads=1
```

Expected: 4 passed.

- [ ] **Step 3: Commit**

```bash
git add crates/lw-cli/src/integrations/skills.rs
git commit -m "feat(integrations): add skills symlink/copy installer with replace semantics"
```

---

## Task 7: `lw integrate` command (install + auto + uninstall)

**Files:**

- Create: `crates/lw-cli/src/integrate.rs`
- Modify: `crates/lw-cli/src/integrations/mod.rs` (export helper, drop stub)
- Modify: `crates/lw-cli/src/main.rs` (wire subcommand)

- [ ] **Step 1: Write integrate command**

Create `crates/lw-cli/src/integrate.rs`:

```rust
use crate::integrations::{
    descriptor::{Descriptor, McpFormat, expand_tilde},
    integrations_root, load_all, mcp, skills,
};
use serde_json::{Value, json};
use std::io::IsTerminal;

pub struct IntegrateOpts {
    pub yes: bool,
    pub uninstall: bool,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run(target: Option<&str>, opts: IntegrateOpts) -> anyhow::Result<()> {
    let descriptors = load_all()?;
    let to_process: Vec<(String, Descriptor)> = match target {
        Some(name) => descriptors
            .into_iter()
            .filter(|(id, _)| id == name)
            .collect(),
        None => {
            // --auto: only those whose detect.config_dir exists
            descriptors
                .into_iter()
                .filter(|(_, d)| d.detect_present())
                .collect()
        }
    };

    if to_process.is_empty() {
        match target {
            Some(t) => anyhow::bail!(
                "no integration descriptor named '{t}' (check {})",
                integrations_root()?.display()
            ),
            None => {
                println!("No supported agent tools detected. Install Claude Code, Codex, or OpenClaw first.");
                return Ok(());
            }
        }
    }

    for (id, desc) in to_process {
        if opts.uninstall {
            uninstall_one(&id, &desc)?;
        } else {
            let proceed = if opts.yes || target.is_some() {
                true
            } else if std::io::stdout().is_terminal() {
                prompt_yes_no(&format!("Integrate llm-wiki with {} ({}?)", desc.name, id))?
            } else {
                println!("Detected {} ({id}). Run `lw integrate {id}` or `lw integrate --auto --yes` to install.", desc.name);
                false
            };
            if proceed {
                install_one(&id, &desc)?;
            }
        }
    }
    Ok(())
}

fn install_one(id: &str, desc: &Descriptor) -> anyhow::Result<()> {
    println!("Installing integration: {} ({id})", desc.name);

    if let Some(mcp_cfg) = &desc.mcp {
        if mcp_cfg.format != McpFormat::Json {
            anyhow::bail!("only JSON MCP format is supported in this version");
        }
        let path = expand_tilde(&mcp_cfg.config_path);
        let mut config: Value = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&path)?)
                .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?
        } else {
            json!({})
        };
        let entry = json!({
            "command": mcp_cfg.command,
            "args": mcp_cfg.args,
            mcp::VERSION_MARKER: VERSION,
        });
        let outcome = mcp::merge_entry(&mut config, &mcp_cfg.key_path, entry, Some(VERSION))?;
        match outcome {
            mcp::MergeOutcome::Inserted => println!("  MCP entry inserted at {}", path.display()),
            mcp::MergeOutcome::NoOp => println!("  MCP entry already current at {}", path.display()),
            mcp::MergeOutcome::Updated => println!("  MCP entry updated at {}", path.display()),
            mcp::MergeOutcome::Conflict { existing } => {
                eprintln!(
                    "  MCP entry at {} appears user-edited; not overwriting.",
                    path.display()
                );
                eprintln!("  Existing: {}", serde_json::to_string_pretty(&existing)?);
                eprintln!(
                    "  To force, remove the entry manually or run with `--force` (not yet supported)."
                );
                return Ok(());
            }
        }
        let body = serde_json::to_string_pretty(&config)? + "\n";
        let backup = mcp::atomic_write_with_backup(&path, &body)?;
        if let Some(b) = backup {
            println!("  backup: {}", b.display());
        }
    }

    if let Some(skills_cfg) = &desc.skills {
        let target = skills::install(skills_cfg)?;
        println!("  skills installed at {}", target.display());
    }

    Ok(())
}

fn uninstall_one(id: &str, desc: &Descriptor) -> anyhow::Result<()> {
    println!("Uninstalling integration: {} ({id})", desc.name);

    if let Some(mcp_cfg) = &desc.mcp {
        let path = expand_tilde(&mcp_cfg.config_path);
        if path.exists() {
            let mut config: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)
                .map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))?;
            let removed = mcp::remove_entry(&mut config, &mcp_cfg.key_path);
            if removed {
                let body = serde_json::to_string_pretty(&config)? + "\n";
                let backup = mcp::atomic_write_with_backup(&path, &body)?;
                println!("  MCP entry removed from {}", path.display());
                if let Some(b) = backup {
                    println!("  backup: {}", b.display());
                }
            } else {
                println!("  MCP entry not present at {}", path.display());
            }
        }
    }

    if let Some(skills_cfg) = &desc.skills {
        let removed = skills::uninstall(skills_cfg)?;
        if removed {
            println!("  skills removed from {}", expand_tilde(&skills_cfg.target_dir).display());
        }
    }

    Ok(())
}

fn prompt_yes_no(question: &str) -> anyhow::Result<bool> {
    use std::io::Write;
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    Ok(matches!(buf.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}
```

- [ ] **Step 2: Wire CLI subcommand**

In `crates/lw-cli/src/main.rs`, add to `enum Commands`:

```rust
    /// Wire llm-wiki into your agent tool(s)
    #[command(after_help = "Examples:\n  lw integrate --auto\n  lw integrate claude-code\n  lw integrate claude-code --uninstall\n  lw integrate --auto --yes  # non-interactive")]
    Integrate {
        /// Specific integration id (omit for --auto detection)
        tool: Option<String>,
        /// Detect installed tools and prompt per tool
        #[arg(long, conflicts_with = "tool")]
        auto: bool,
        /// Skip prompts (assume yes)
        #[arg(short, long)]
        yes: bool,
        /// Reverse the integration
        #[arg(long)]
        uninstall: bool,
    },
```

Add `mod integrate;` near top of `main.rs`.

In the `match cli.command { ... }` block, add:

```rust
        Commands::Integrate { tool, auto, yes, uninstall } => {
            let target = if auto { None } else { tool.as_deref() };
            integrate::run(target, integrate::IntegrateOpts { yes, uninstall })
        }
```

- [ ] **Step 3: Build and smoke-test**

```bash
cargo build -p lw-cli
./target/debug/lw integrate --help
```

Expected: help text shows all flags.

```bash
LW_INTEGRATIONS_DIR=$PWD/integrations LW_SKILLS_DIR=$PWD/skills ./target/debug/lw integrate --auto
```

Expected on a system without `~/.claude/`: prints "No supported agent tools detected." and exits 0.

- [ ] **Step 4: Commit**

```bash
git add crates/lw-cli/src/integrate.rs crates/lw-cli/src/main.rs
git commit -m "feat(lw-cli): add lw integrate command (auto/single/uninstall)"
```

---

## Task 8: End-to-end integration test

**Files:**

- Create: `crates/lw-cli/tests/integrate_cli.rs`

- [ ] **Step 1: Write tests**

Create `crates/lw-cli/tests/integrate_cli.rs`:

```rust
use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn lw(env_home: &std::path::Path, integrations: &std::path::Path, skills: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("lw").unwrap();
    cmd.env("LW_HOME", env_home);
    cmd.env("LW_INTEGRATIONS_DIR", integrations);
    cmd.env("LW_SKILLS_DIR", skills);
    cmd.env_remove("LW_WIKI_ROOT");
    cmd
}

fn make_descriptor(integrations: &std::path::Path, fake_home: &std::path::Path) {
    std::fs::create_dir_all(integrations).unwrap();
    let claude_dir = fake_home.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let settings_path = claude_dir.join("settings.json");
    let skills_target = claude_dir.join("skills/llm-wiki/");

    let toml = format!(
        r#"
name = "Claude Code"

[detect]
config_dir = "{}"

[mcp]
config_path = "{}"
format = "json"
key_path = "mcpServers.llm-wiki"
command = "lw"
args = ["serve"]

[skills]
target_dir = "{}"
mode = "symlink"
"#,
        claude_dir.display(),
        settings_path.display(),
        skills_target.display()
    );
    std::fs::write(integrations.join("claude-code.toml"), toml).unwrap();
}

fn make_skills(skills_dir: &std::path::Path) {
    let s = skills_dir.join("llm-wiki-import");
    std::fs::create_dir_all(&s).unwrap();
    std::fs::write(s.join("SKILL.md"), "---\nname: llm-wiki:import\n---\nbody").unwrap();
}

#[test]
fn integrate_install_writes_settings_and_links_skills() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    make_descriptor(integrations.path(), fake_home.path());
    make_skills(skills.path());

    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code"])
        .assert()
        .success();

    let settings_path = fake_home.path().join(".claude/settings.json");
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(settings["mcpServers"]["llm-wiki"]["command"], "lw");
    assert_eq!(settings["mcpServers"]["llm-wiki"]["args"], serde_json::json!(["serve"]));
    assert!(settings["mcpServers"]["llm-wiki"]["_lw_version"].is_string());

    let skills_link = fake_home.path().join(".claude/skills/llm-wiki/");
    assert!(skills_link.join("llm-wiki-import/SKILL.md").exists());
}

#[test]
fn integrate_uninstall_removes_entry_and_skills() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    make_descriptor(integrations.path(), fake_home.path());
    make_skills(skills.path());

    // Install first
    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code"])
        .assert()
        .success();
    // Then uninstall
    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code", "--uninstall"])
        .assert()
        .success();

    let settings_path = fake_home.path().join(".claude/settings.json");
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert!(settings["mcpServers"].get("llm-wiki").is_none());
    assert!(!fake_home.path().join(".claude/skills/llm-wiki/").exists());
}

#[test]
fn integrate_preserves_other_mcp_entries() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    make_descriptor(integrations.path(), fake_home.path());
    make_skills(skills.path());

    let settings_path = fake_home.path().join(".claude/settings.json");
    std::fs::write(
        &settings_path,
        r#"{"mcpServers":{"other":{"command":"other"}},"permissions":{"allow":["foo"]}}"#,
    )
    .unwrap();

    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "claude-code"])
        .assert()
        .success();

    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
    assert_eq!(settings["mcpServers"]["other"]["command"], "other");
    assert_eq!(settings["permissions"]["allow"], serde_json::json!(["foo"]));
    assert_eq!(settings["mcpServers"]["llm-wiki"]["command"], "lw");

    // Backup must exist
    let entries: Vec<_> = std::fs::read_dir(fake_home.path().join(".claude"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|n| n.contains(".bak."))
        .collect();
    assert!(!entries.is_empty(), "expected at least one backup file");
}

#[test]
fn integrate_auto_with_no_tools_succeeds() {
    let env_home = TempDir::new().unwrap();
    let integrations = TempDir::new().unwrap();
    let skills = TempDir::new().unwrap();
    let fake_home = TempDir::new().unwrap();
    // Descriptor present but its detect.config_dir does NOT exist
    let phantom = fake_home.path().join("never-installed");
    std::fs::create_dir_all(integrations.path()).unwrap();
    let toml = format!(
        r#"
name = "Phantom"

[detect]
config_dir = "{}"

[mcp]
config_path = "{}"
format = "json"
key_path = "mcpServers.llm-wiki"
command = "lw"
args = ["serve"]

[skills]
target_dir = "{}"
mode = "symlink"
"#,
        phantom.display(),
        phantom.join("settings.json").display(),
        phantom.join("skills/").display()
    );
    std::fs::write(integrations.path().join("phantom.toml"), toml).unwrap();
    make_skills(skills.path());

    lw(env_home.path(), integrations.path(), skills.path())
        .args(["integrate", "--auto", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No supported agent tools detected"));
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p lw-cli --test integrate_cli -- --test-threads=1
```

Expected: 4 passed.

- [ ] **Step 3: Final verification**

```bash
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Expected: all green.

- [ ] **Step 4: Commit**

```bash
git add crates/lw-cli/tests/integrate_cli.rs
git commit -m "test(lw-cli): end-to-end integrate install/uninstall against fake config"
```

---

## Done criteria

- 3 templates copyable via `lw workspace add --template <name>`
- `llm-wiki:import` skill present in canonical `skills/`
- `lw integrate claude-code` writes a valid mcpServers entry, links skills, backs up prior config
- `lw integrate claude-code --uninstall` reverses cleanly, leaves sibling entries intact
- `lw integrate --auto` skips when no tools detected, prompts (or `--yes`) when present
- User-edited entries trigger Conflict and are NOT overwritten
- All tests green, clippy clean, fmt clean
