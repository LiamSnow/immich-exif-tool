use anyhow::Result;

mod commands;
mod exif;
mod immich;
mod plan_file;

pub fn main() -> Result<()> {
    commands::run()
}
