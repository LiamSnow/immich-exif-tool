use anyhow::Result;
use clap::{Parser, Subcommand};

mod apply;
mod plan;
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

pub fn run() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    use Command::*;
    match cli.command {
        Pull(args) => pull::run(args),
        Plan(args) => plan::run(args),
        Apply(args) => apply::run(args),
    }
}
