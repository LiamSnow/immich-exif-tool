use crate::plan::datetime::ParsedDateTime;
use crate::progress::Row;
use crate::pull;
use anyhow::Result;
use indicatif::MultiProgress;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

pub mod file;
pub use file::*;
mod datetime;
mod exiftool;

const GPS_EPSILON: f64 = 0.0001;

#[derive(clap::Args)]
pub struct Args {
    /// Path to local photo library
    #[arg(long, env = "LOCAL_PATH")]
    pub local_path: String,

    /// Path to output from `Pull` step
    #[arg(short, long, default_value = "immich_data.json")]
    pub immich_data: PathBuf,

    /// Output file path
    #[arg(short, long, default_value = "plan.json")]
    pub output: PathBuf,
}

pub fn run(mut args: Args) -> Result<()> {
    args.local_path = format!("{}/", args.local_path.trim_end_matches('/'));

    let mp = MultiProgress::new();

    let file_name = args
        .immich_data
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let row_immich = Row::new(&mp, &file_name);
    let immich = pull::File::load(&args.immich_data)?;
    row_immich.finish(&format!("{} assets", immich.assets.len()));

    let row_exiftool = Row::new(&mp, "Exiftool");
    let assets = exiftool::run(&args.local_path, |done, total| {
        row_exiftool.update(&format!("{}/{} directories", done, total));
    })?;
    row_exiftool.finish(&format!("{} assets", assets.len()));

    let total_assets = assets
        .iter()
        .filter(|a| !a.source_file.ends_with(".xmp"))
        .count();

    let row_processing = Row::new(&mp, "Processing");
    let row_datetime = Row::new_stat(&mp, "Date Time");
    let row_gps = Row::new_stat(&mp, "GPS");
    let row_desc = Row::new_stat(&mp, "Description");
    let row_ext = Row::new_stat(&mp, "File Extension");
    let row_orphans = Row::new_stat(&mp, "Orphans");

    let mut out = FixesFile::default();

    for asset in assets {
        if asset.source_file.ends_with(".xmp") {
            continue;
        }

        out.summary.total += 1;
        row_processing.update(&format!("{}/{} assets", out.summary.total, total_assets));

        let rel_path = asset
            .source_file
            .strip_prefix(&args.local_path)
            .unwrap_or(&asset.source_file);

        let Some(immich_asset) = immich.resolve_asset(rel_path) else {
            out.summary.orphaned += 1;
            out.orphans.push(rel_path.to_string());
            row_orphans.update(&out.summary.orphaned.to_string());
            continue;
        };

        let state = asset.assess_state(immich_asset.exif_info.as_ref())?;
        out.summary.add_entry(&state);

        row_datetime.update(&out.summary.date_time.to_string());
        row_gps.update(&out.summary.gps.to_string());
        row_desc.update(&out.summary.description.to_string());
        row_ext.update(&out.summary.file_extension.to_string());

        if state.has_fixes() {
            out.assets.insert(rel_path.to_string(), state);
        }
    }

    out.orphans.sort();

    row_processing.finish(&format!("{} assets", out.summary.total));
    row_datetime.finish(&out.summary.date_time.to_string());
    row_gps.finish(&out.summary.gps.to_string());
    row_desc.finish(&out.summary.description.to_string());
    row_ext.finish(&out.summary.file_extension.to_string());
    row_orphans.finish(&out.summary.orphaned.to_string());

    let file = File::create(&args.output)?;
    serde_json::to_writer_pretty(file, &out)?;
    println!("\nWrote {:?}", args.output);

    Ok(())
}

impl exiftool::AssetExif {
    fn assess_state(&self, immich: Option<&pull::AssetExif>) -> Result<AssetState> {
        Ok(AssetState {
            date_time: self
                .assess_date_time_state(immich.and_then(|e| e.date_time_original.as_deref())),
            gps: self.assess_gps_state(immich.and_then(|e| e.gps())),
            description: self.assess_desc_state(immich.and_then(|e| e.description.as_deref())),
            file_extension: self.assess_file_ext_state()?,
        })
    }

    fn assess_date_time_state(&self, immich: Option<&str>) -> FieldState<String> {
        use FieldState::*;
        let local_str = self.date_time_original.as_deref();
        let local = local_str.and_then(ParsedDateTime::new);
        let immich = immich.and_then(ParsedDateTime::new);

        let target = match (&local, &immich) {
            (_, Some(immich)) => immich,
            (Some(local), None) => local,
            (None, None) => return Unfixable,
        };

        if let Some(local) = &local
            && local.is_exif
            && local == target
        {
            return Ok;
        }

        Fixable {
            is: local_str.map(|s| s.to_string()),
            fix: target.format(),
        }
    }

    fn assess_gps_state(&self, immich: Option<(f64, f64)>) -> FieldState<(f64, f64)> {
        use FieldState::*;
        match (self.gps(), immich) {
            (None, None) => Unfixable,
            (None, Some(imm)) => Fixable { is: None, fix: imm },
            (Some(_), None) => Ok,
            (Some(loc), Some(imm)) => {
                if (loc.0 - imm.0).abs() < GPS_EPSILON && (loc.1 - imm.1).abs() < GPS_EPSILON {
                    Ok
                } else {
                    Fixable {
                        is: Some(loc),
                        fix: imm,
                    }
                }
            }
        }
    }

    fn assess_desc_state(&self, immich: Option<&str>) -> FieldState<String> {
        use FieldState::*;
        match (&self.description, immich) {
            (None, None) => Unfixable,
            (None, Some(imm)) => Fixable {
                is: None,
                fix: imm.to_string(),
            },
            (Some(_), None) => Ok,
            (Some(loc), Some(imm)) => {
                if loc == imm {
                    Ok
                } else {
                    Fixable {
                        is: Some(loc.to_string()),
                        fix: imm.to_string(),
                    }
                }
            }
        }
    }

    // TODO replace with an actually good impl (crate?)
    fn assess_file_ext_state(&self) -> Result<FieldState<String>> {
        let ext = Path::new(&self.source_file)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let mut f = File::open(&self.source_file)?;
        let mut magic = [0u8; 12];
        f.read_exact(&mut magic)?;

        let is_jpeg = magic[0] == 0xFF && magic[1] == 0xD8;
        let is_riff = &magic[0..4] == b"RIFF";

        Ok(match ext.as_str() {
            "png" | "heic" if is_jpeg => FieldState::Fixable {
                is: Some(ext),
                fix: "jpg".to_string(),
            },
            "png" if is_riff => FieldState::Fixable {
                is: Some(ext),
                fix: "webp".to_string(),
            },
            _ => FieldState::Ok,
        })
    }
}
