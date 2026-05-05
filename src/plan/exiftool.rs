use anyhow::{Context, Result};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Deserializer};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU32, Ordering},
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
    #[serde(
        rename = "Description",
        default,
        deserialize_with = "empty_string_as_none"
    )]
    pub description: Option<String>,
}

/// Runs `exiftool` in parallel across all subdirectories containing files.
/// Calls `on_progress(completed, total)` after each directory finishes.
pub fn run(dir: &str, on_progress: impl Fn(u32, u32) + Sync) -> Result<Vec<AssetExif>> {
    let subdirs = find_subdirs_with_files(dir);
    let total = subdirs.len() as u32;
    let completed = AtomicU32::new(0);

    Ok(subdirs
        .par_iter()
        .map(|subdir| {
            let result = run_once(subdir);
            let n = completed.fetch_add(1, Ordering::Relaxed) + 1;
            on_progress(n, total);
            result
        })
        .collect::<Result<Vec<Vec<AssetExif>>>>()?
        .into_iter()
        .flatten()
        .collect())
}

/// Runs `exiftool` for a given file or directory (non-recursive)
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

/// Returns every subdirectory that contains at least one file
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

pub fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    Ok(s.filter(|s| !s.is_empty()))
}
