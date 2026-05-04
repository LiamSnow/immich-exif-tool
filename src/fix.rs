use anyhow::Result;

#[derive(clap::Args)]
pub struct Args {
    /// Path to local photo library
    #[arg(long, env = "LOCAL_PATH")]
    pub local_path: String,

    /// Path to list of fixes (fixes.json)
    #[arg(short, long, default_value = "fixes.json")]
    pub fixes: String,
    // TODO allow specification of what fixes to apply?
}

pub fn run(args: Args) -> Result<()> {
    Ok(())
}
