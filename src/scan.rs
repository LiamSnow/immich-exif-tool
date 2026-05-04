use anyhow::Result;

#[derive(clap::Args)]
pub struct Args {
    /// Path to local photo library
    #[arg(long, env = "LOCAL_PATH")]
    pub local_path: String,

    /// Path to the Immich dump (dump.json)
    #[arg(short, long, default_value = "dump.json")]
    pub dump: String,

    /// Output file path
    #[arg(short, long, default_value = "fixes.json")]
    pub output: String,
}

pub fn run(args: Args) -> Result<()> {
    Ok(())
}
