mod import;
mod ingest;
mod init;
mod output;
mod query;
mod read;
mod serve;
mod status;

use clap::Parser;
use output::Format;
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(
    name = "lw",
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
        after_help = "Examples:\n  lw ingest paper.pdf --category architecture --raw-type papers\n  lw ingest notes.md --title \"Meeting Notes\" --category ops --yes\n  cat article.md | lw ingest --stdin --title \"Article\" --yes"
    )]
    Ingest {
        /// Path to source file (omit if using --stdin)
        source: Option<PathBuf>,
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

    /// Start MCP server (stdio)
    #[command(after_help = "Examples:\n  lw serve\n  lw serve --root /path/to/wiki")]
    Serve,
}

fn resolve_root(cli_root: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(root) = cli_root {
        return Ok(root);
    }
    let cwd = std::env::current_dir().map_err(|e| format!("Cannot get cwd: {e}"))?;
    lw_core::fs::discover_wiki_root(&cwd).ok_or_else(|| {
        format!(
            "Not a wiki directory (or any parent): {}\n  Run: lw init --root <path>\n  Or set LW_WIKI_ROOT environment variable",
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
        } => match resolve_root(cli.root) {
            Ok(root) => import::run(&root, &file, &format, &category, limit, dry_run),
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
        Commands::Serve => match resolve_root(cli.root) {
            Ok(root) => serve::run(&root),
            Err(e) => {
                eprintln!("Error: {e}");
                process::exit(1);
            }
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
