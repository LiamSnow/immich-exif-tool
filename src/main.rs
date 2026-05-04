use anyhow::Result;
use clap::{Parser, Subcommand};

mod dump;
mod fix;
mod immich;
mod scan;

#[derive(Parser)]
#[command(name = "immich-exif-tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Dump all metadata from an Immich instance to a JSON file
    Dump(dump::Args),

    /// Scan local files, comparing against dumped metadata
    Scan(scan::Args),

    /// Apply fixes to local files
    Fix(fix::Args),
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    match cli.command {
        Command::Dump(args) => dump::run(args),
        Command::Scan(args) => scan::run(args),
        Command::Fix(args) => fix::run(args),
    }
}
