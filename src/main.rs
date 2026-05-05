use anyhow::Result;
use clap::{Parser, Subcommand};

mod apply;
mod plan;
mod progress;
mod pull;

#[derive(Parser)]
#[command(name = "immich-exif-tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pull all metadata from an Immich instance to a JSON file
    Pull(pull::Args),

    /// Plan changes to local assets from Immich metadata
    Plan(plan::Args),

    /// Apply changes local assets
    Apply(apply::Args),
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    match cli.command {
        Command::Pull(args) => pull::run(args),
        Command::Plan(args) => plan::run(args),
        Command::Apply(args) => apply::run(args),
    }
}
