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
