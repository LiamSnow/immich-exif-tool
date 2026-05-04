use anyhow::{Context, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};
use walkdir::WalkDir;

#[derive(Deserialize, Default)]
pub struct AssetExif {
    #[serde(rename = "SourceFile")]
    pub source_file: String,
    #[serde(rename = "DateTimeOriginal")]
    pub date_time_original: Option<String>,
    #[serde(rename = "GPSLatitude")]
    pub gps_latitude: Option<f64>,
    #[serde(rename = "GPSLongitude")]
    pub gps_longitude: Option<f64>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
}

/// Runs `exiftool` in parallel on all cores by running
/// it on each subdirectory which contains assets
pub fn run(dir: &str) -> Result<Vec<AssetExif>> {
    let subdirs = find_subdirs_with_files(dir);
    Ok(subdirs
        .par_iter()
        .map(|subdir| run_once(subdir))
        .collect::<Result<Vec<Vec<AssetExif>>>>()?
        .into_iter()
        .flatten()
        .collect())
}

/// Runs `exiftool` for a given file or directory
/// Returns date, location, and description in a structured response
/// If `path` is a directory, it **will not** recursively follow subdirectories
fn run_once(path: &Path) -> Result<Vec<AssetExif>> {
    let output = Command::new("exiftool")
        .args([
            "-json",
            "-n",
            "-DateTimeOriginal",
            "-GPSLatitude",
            "-GPSLongitude",
            "-Description",
        ])
        .arg(path)
        .output()
        .context("failed to run exiftool")?;

    if output.stdout.is_empty() {
        return Ok(vec![]);
    }

    serde_json::from_slice(&output.stdout).context("failed to parse exiftool output")
}

/// Returns every subdirectory in a directory which contains a file
fn find_subdirs_with_files(base: &str) -> Vec<PathBuf> {
    WalkDir::new(base)
        .into_iter()
        .filter_map(|res| match res {
            Ok(e) => Some(e),
            Err(err) => {
                eprintln!("WARN failed to access item in {base}: {err}");
                None
            }
        })
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.path().parent().map(|p| p.to_path_buf()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

impl AssetExif {
    pub fn gps(&self) -> Option<(f64, f64)> {
        match (self.gps_latitude, self.gps_longitude) {
            (Some(lat), Some(long)) => Some((lat, long)),
            (_, _) => None,
        }
    }
}
