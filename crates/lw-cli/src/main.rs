mod config;
mod import;
mod ingest;
mod init;
mod install_prefix;
mod integrate;
mod integrations;
mod lint;
mod output;
mod query;
mod read;
mod serve;
mod status;
mod templates;
mod uninstall;
mod upgrade;
mod version_file;
mod workspace;
mod write;

use clap::Parser;
use output::Format;
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(
    name = "lw",
    version,
    about = "LLM Wiki — team knowledge base toolkit",
    after_help = "Examples:\n  lw init\n  lw query \"attention mechanism\" --format json\n  lw ingest paper.pdf --category architecture --yes"
)]
struct Cli {
    /// Wiki root directory (default: auto-discover from cwd, or LW_WIKI_ROOT env)
    #[arg(long, global = true, env = "LW_WIKI_ROOT")]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize a new wiki in the current directory
    #[command(after_help = "Examples:\n  lw init\n  lw init --root /path/to/wiki")]
    Init,

    /// Search wiki pages
    #[command(
        after_help = "Examples:\n  lw query \"attention mechanism\"\n  lw query \"transformer\" --tag architecture --format json\n  lw query \"gpu\" --stale --format brief\n  lw query \"\" --tag training"
    )]
    Query {
        /// Search text (use "" for tag/category-only queries)
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
        /// Only show stale pages (freshness == Stale)
        #[arg(long)]
        stale: bool,
    },

    /// Ingest source material into the wiki
    #[command(
        after_help = "Examples:\n  lw ingest paper.pdf --category architecture --raw-type papers\n  lw ingest https://arxiv.org/abs/2405.12345 --category architecture --yes\n  lw ingest notes.md --title \"Meeting Notes\" --category ops --yes\n  cat article.md | lw ingest --stdin --title \"Article\" --yes"
    )]
    Ingest {
        /// Source file path or URL (omit if using --stdin)
        source: Option<String>,
        /// Read from stdin
        #[arg(long)]
        stdin: bool,
        /// Page title (auto-derived from filename if omitted)
        #[arg(long)]
        title: Option<String>,
        /// Target category
        #[arg(long)]
        category: Option<String>,
        /// Tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,
        /// Raw subdirectory (papers, articles, assets)
        #[arg(long, default_value = "articles")]
        raw_type: String,
        /// Skip interactive prompts (agent mode)
        #[arg(long)]
        yes: bool,
        /// Preview what would be created without writing files or downloading URLs
        #[arg(long)]
        dry_run: bool,
        /// Output format (human or json)
        #[arg(short = 'o', long, default_value = "human")]
        output_format: Format,
    },

    /// Import batch sources into the wiki
    #[command(
        after_help = "Examples:\n  lw import tweets.json --format twitter-json\n  lw import tweets.json --format twitter-json --limit 10 --dry-run\n  lw import tweets.json --format twitter-json --category architecture"
    )]
    Import {
        /// Path to source file
        file: PathBuf,
        /// Source format (twitter-json)
        #[arg(long)]
        format: String,
        /// Target category
        #[arg(long, default_value = "_uncategorized")]
        category: String,
        /// Max entries to import
        #[arg(long)]
        limit: Option<usize>,
        /// Preview without writing
        #[arg(long)]
        dry_run: bool,
        /// Output format (human or json)
        #[arg(short = 'o', long, default_value = "human")]
        output_format: Format,
    },

    /// Read a wiki page by path
    ///
    /// # Examples
    ///
    ///     lw read architecture/transformer.md
    ///     lw read architecture/transformer.md --format json
    #[command(
        after_help = "Examples:\n  lw read architecture/transformer.md\n  lw read architecture/transformer.md --format json\n  lw query \"attention\" --format brief | head -1 | cut -f1 | xargs lw read"
    )]
    Read {
        /// Wiki-relative path (e.g., architecture/transformer.md)
        path: String,
        /// Output format
        #[arg(short, long, default_value = "human")]
        format: Format,
    },

    /// Show wiki health status and freshness report
    #[command(after_help = "Examples:\n  lw status\n  lw status --format json")]
    Status {
        /// Output format
        #[arg(short, long, default_value = "human")]
        format: Format,
    },

    /// Check wiki health: stale pages, broken links, orphans, TODO stubs
    #[command(
        after_help = "Examples:\n  lw lint\n  lw lint --format json\n  lw lint --category architecture"
    )]
    Lint {
        /// Filter by category
        #[arg(long)]
        category: Option<String>,
        /// Output format (human or json)
        #[arg(short, long, default_value = "human")]
        format: Format,
    },

    /// Start MCP server (stdio)
    #[command(after_help = "Examples:\n  lw serve\n  lw serve --root /path/to/wiki")]
    Serve,

    /// Write or update a wiki page (overwrite, append to section, or upsert section)
    #[command(
        after_help = "Examples:\n  echo 'full content' | lw write tools/page.md\n  lw write tools/page.md --mode append --section References --content '- [[link]]'\n  echo 'new docs' | lw write tools/page.md --mode upsert --section Usage"
    )]
    Write {
        /// Wiki-relative path (e.g. tools/page.md)
        path: String,
        /// Write mode: overwrite (default), append, upsert
        #[arg(long, default_value = "overwrite")]
        mode: String,
        /// Section name for append/upsert modes
        #[arg(long)]
        section: Option<String>,
        /// Content to write (alternative to stdin)
        #[arg(long)]
        content: Option<String>,
    },

    /// Manage registered wiki workspaces (Obsidian-style vaults)
    #[command(
        after_help = "Examples:\n  lw workspace add personal ~/Documents/MyWiki --init\n  lw workspace list\n  lw workspace use work\n  lw workspace current -v\n  lw workspace remove old-vault"
    )]
    Workspace {
        #[command(subcommand)]
        action: WorkspaceCmd,
    },

    /// Wire llm-wiki into your agent tool(s)
    #[command(
        after_help = "Examples:\n  lw integrate --auto\n  lw integrate claude-code\n  lw integrate claude-code --uninstall\n  lw integrate --auto --yes  # non-interactive"
    )]
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

    /// Check for or apply a newer llm-wiki release
    #[command(after_help = "Examples:\n  lw upgrade --check\n  lw upgrade\n  lw upgrade --yes")]
    Upgrade {
        /// Only check; do not download/replace
        #[arg(long)]
        check: bool,
        /// Pass --yes to the installer (auto-integrate)
        #[arg(short, long)]
        yes: bool,
    },

    /// Remove llm-wiki from this machine (vault data preserved)
    #[command(
        after_help = "Examples:\n  lw uninstall\n  lw uninstall --yes\n  lw uninstall --keep-config\n  lw uninstall --yes --purge"
    )]
    Uninstall {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        /// Keep ~/.llm-wiki/config.toml in place
        #[arg(long)]
        keep_config: bool,
        /// Also delete .bak files left by past integration writes
        #[arg(long)]
        purge: bool,
    },
}

#[derive(clap::Subcommand)]
enum WorkspaceCmd {
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
    /// List all registered workspaces
    List,
    /// Print the current workspace name and path
    Current {
        /// Show the full root resolution chain for debugging
        #[arg(short, long)]
        verbose: bool,
    },
    /// Set the current workspace
    #[command(name = "use")]
    UseCmd {
        /// Name of the workspace to switch to
        name: String,
    },
    /// Remove a workspace from the registry (does not touch the directory)
    Remove {
        /// Name of the workspace to unregister
        name: String,
    },
}

fn resolve_root(cli_root: Option<PathBuf>) -> Result<PathBuf, String> {
    // Priority: --root flag > LW_WIKI_ROOT env (already merged into cli_root by clap) > current workspace > cwd
    if let Some(root) = cli_root {
        return Ok(root);
    }
    // Try current workspace from ~/.llm-wiki/config.toml.
    // If the user has a current workspace registered but its path is gone
    // (vault moved/deleted), surface a distinct, actionable error rather
    // than silently falling through to cwd discovery.
    //
    // Note: when `current` is set but its name has no corresponding entry
    // in `workspaces` (corrupt config), the outer let-chain short-circuits
    // and we fall through to cwd discovery. `workspace::current(verbose)`
    // is the diagnostic for that case.
    if let Ok(cfg_path) = config::config_path()
        && let Ok(cfg) = config::Config::load_from(&cfg_path)
        && let Some(name) = &cfg.workspace.current
        && let Some(entry) = cfg.workspaces.get(name)
    {
        if entry.path.exists() {
            return Ok(entry.path.clone());
        }
        return Err(format!(
            "current workspace '{name}' points to {} which no longer exists\n  Run: lw workspace remove {name}  (to forget it)\n  Or restore the directory at {}",
            entry.path.display(),
            entry.path.display()
        ));
    }
    // Final fallback: cwd auto-discover
    let cwd = std::env::current_dir().map_err(|e| format!("Cannot get cwd: {e}"))?;
    lw_core::fs::discover_wiki_root(&cwd).ok_or_else(|| {
        format!(
            "Not a wiki directory (or any parent): {}\n  Run: lw init --root <path>\n  Or: lw workspace add <name> <path> --init\n  Or set LW_WIKI_ROOT environment variable",
            cwd.display()
        )
    })
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init => {
            let root = cli
                .root
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            init::run(&root)
        }
        Commands::Query {
            text,
            tag,
            category,
            limit,
            format,
            stale,
        } => match resolve_root(cli.root) {
            Ok(root) => query::run(&root, &text, &tag, &category, limit, &format, stale),
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        Commands::Ingest {
            source,
            stdin,
            title,
            category,
            tags,
            raw_type,
            yes,
            dry_run,
            output_format,
        } => match resolve_root(cli.root) {
            Ok(root) => ingest::run(
                &root,
                source.as_deref(),
                stdin,
                &title,
                &category,
                &tags,
                &raw_type,
                yes,
                dry_run,
                &output_format,
            ),
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        Commands::Import {
            file,
            format,
            category,
            limit,
            dry_run,
            output_format,
        } => match resolve_root(cli.root) {
            Ok(root) => import::run(
                &root,
                &file,
                &format,
                &category,
                limit,
                dry_run,
                &output_format,
            ),
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        Commands::Read { path, format } => match resolve_root(cli.root) {
            Ok(root) => read::run(&root, &path, &format),
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        Commands::Status { format } => match resolve_root(cli.root) {
            Ok(root) => status::run(&root, &format),
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        Commands::Lint { category, format } => match resolve_root(cli.root) {
            Ok(root) => lint::run(&root, &category, &format),
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        Commands::Serve => match resolve_root(cli.root) {
            Ok(root) => serve::run(&root),
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        Commands::Write {
            path,
            mode,
            section,
            content,
        } => match resolve_root(cli.root) {
            Ok(root) => {
                // Detect if stdin has data (is not a terminal)
                use std::io::IsTerminal;
                let stdin_available = !std::io::stdin().is_terminal();
                write::run(&root, &path, &mode, &section, &content, stdin_available)
            }
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
        Commands::Workspace { action } => match action {
            WorkspaceCmd::Add {
                name,
                path,
                init,
                template,
            } => workspace::add(&name, &path, init, template.as_deref()),
            WorkspaceCmd::List => workspace::list(),
            WorkspaceCmd::Current { verbose } => workspace::current(verbose),
            WorkspaceCmd::UseCmd { name } => workspace::use_(&name),
            WorkspaceCmd::Remove { name } => workspace::remove(&name),
        },
        Commands::Integrate {
            tool,
            auto,
            yes,
            uninstall,
        } => {
            let target = if auto { None } else { tool.as_deref() };
            integrate::run(target, integrate::IntegrateOpts { yes, uninstall })
        }
        Commands::Upgrade { check, yes } => {
            if check {
                upgrade::check()
            } else {
                upgrade::apply(yes)
            }
        }
        Commands::Uninstall {
            yes,
            keep_config,
            purge,
        } => uninstall::run(uninstall::UninstallOpts {
            yes,
            keep_config,
            purge,
        }),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
