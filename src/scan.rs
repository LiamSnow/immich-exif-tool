use crate::dump::DumpFile;
use crate::exiftool;
use crate::immich;
use anyhow::{Context, Result};
use jiff::Zoned;
use jiff::civil::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

const GPS_EPSILON: f64 = 0.0001;

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

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FixesFile {
    pub summary: Summary,
    pub assets: BTreeMap<String, AssetState>,
    pub orphans: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub total: u32,
    pub orphaned: u32,
    pub date_time: FieldStats,
    pub gps: FieldStats,
    pub description: FieldStats,
    pub file_extension: FieldStats,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FieldStats {
    pub ok: u32,
    pub fixable: u32,
    pub unfixable: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetState {
    date_time: FieldState<String>,
    gps: FieldState<(f64, f64)>,
    description: FieldState<String>,
    file_extension: FieldState<String>,
}

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldState<T> {
    Ok,
    /// Neither side has data
    Unfixable,
    /// Local copy is dirty, missing, or badly format
    Fixable {
        is: Option<T>,
        fix: T,
    },
}

pub fn run(mut args: Args) -> Result<()> {
    args.local_path = format!("{}/", args.local_path.trim_end_matches('/'));

    let mut out = FixesFile::default();
    let dump = DumpFile::load(&args.dump)?;

    let assets = exiftool::run(&args.local_path)?;
    println!("Found {} assets", assets.len());

    for asset in assets {
        // ignore Immich sidecars
        if asset.source_file.ends_with(".xmp") {
            continue;
        }

        out.summary.total += 1;

        let rel_path = asset
            .source_file
            .strip_prefix(&args.local_path)
            .unwrap_or(&asset.source_file);

        let Some(immich) = dump.resolve_asset(rel_path) else {
            out.summary.orphaned += 1;
            out.orphans.push(rel_path.to_string());
            continue;
        };

        let state = asset.assess_state(immich.exif_info.as_ref())?;
        out.summary.add_entry(&state);

        if state.has_fixes() {
            out.assets.insert(rel_path.to_string(), state);
        }
    }

    out.orphans.sort();

    print_summary(&out);

    let file = File::create(&args.output)?;
    serde_json::to_writer_pretty(file, &out)?;
    println!("\nWrote {}", args.output);

    Ok(())
}

impl exiftool::AssetExif {
    fn assess_state(&self, immich: Option<&immich::AssetExif>) -> Result<AssetState> {
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
        let local_dt = local_str.and_then(parse_date_time);
        let immich_dt = immich.and_then(parse_date_time);

        let target_dt = match (local_dt, immich_dt) {
            (_, Some((imm, _))) => imm,
            (Some((loc, _)), None) => loc,
            (None, None) => return Unfixable,
        };

        if local_dt.is_some_and(|(local_dt, is_exif)| is_exif && local_dt == target_dt) {
            Ok
        } else {
            Fixable {
                is: local_str.map(|s| s.to_string()),
                fix: target_dt.strftime(EXIF_DATETIME_FORMAT).to_string(),
            }
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

    // TODO verify this function
    // The goal is to find images with the wrong extension (Google Photos rejects them)
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

const EXIF_DATETIME_FORMAT: &str = "%Y:%m:%d %H:%M:%S";

/// EXIF Date -> Some((dt, true))
/// Other Date -> Some((_), false))
/// Unknown Date -> None
fn parse_date_time(s: &str) -> Option<(DateTime, bool)> {
    // 2024:07:11 01:14:00
    if let Ok(dt) = DateTime::strptime(EXIF_DATETIME_FORMAT, s) {
        return Some((dt, true));
    }
    // 2024-07-11T01:14:00Z
    if let Ok(zoned) = s.parse::<Zoned>() {
        return Some((zoned.datetime(), false));
    }
    // 2024-07-11T01:14:00
    if let Ok(dt) = s.parse::<DateTime>() {
        return Some((dt, false));
    }
    None
}

fn print_summary(out: &FixesFile) {
    println!("\n--- Summary ---");
    println!("Total Assets:     {}", out.summary.total);
    println!("Orphan Assets:    {}", out.summary.orphaned);
    out.summary.date_time.print("Date Time");
    out.summary.gps.print("GPS");
    out.summary.description.print("Description");
    out.summary.file_extension.print("File Extension");

    if !out.orphans.is_empty() {
        println!("\nOrphan files (no dump entry):");
        for o in out.orphans.iter().take(20) {
            println!("  {}", o);
        }
        if out.orphans.len() > 20 {
            println!("  ... and {} more", out.orphans.len() - 20);
        }
    }
}

impl AssetState {
    fn has_fixes(&self) -> bool {
        matches!(self.date_time, FieldState::Fixable { .. })
            || matches!(self.gps, FieldState::Fixable { .. })
            || matches!(self.description, FieldState::Fixable { .. })
            || matches!(self.file_extension, FieldState::Fixable { .. })
    }
}

impl Summary {
    fn add_entry(&mut self, entry: &AssetState) {
        self.date_time.add_entry(&entry.date_time);
        self.gps.add_entry(&entry.gps);
        self.description.add_entry(&entry.description);
        self.file_extension.add_entry(&entry.file_extension);
    }
}

impl FieldStats {
    fn add_entry<T>(&mut self, entry: &FieldState<T>) {
        match entry {
            FieldState::Ok => self.ok += 1,
            FieldState::Fixable { .. } => self.fixable += 1,
            FieldState::Unfixable => self.unfixable += 1,
        }
    }

    fn print(&self, label: &str) {
        println!(
            "  {:<18} {} ok, {} fixable, {} unfixable",
            label, self.ok, self.fixable, self.unfixable
        );
    }
}

impl DumpFile {
    fn load(path: &str) -> Result<DumpFile> {
        let file = File::open(path).with_context(|| format!("failed to open {}", path))?;
        serde_json::from_reader(file).with_context(|| format!("failed to parse {}", path))
    }

    fn resolve_asset(&self, rel_path: &str) -> Option<&immich::AssetResponse> {
        if let Some(entry) = self.assets.get(rel_path) {
            return Some(entry);
        }

        // Apple splits live photos into two photos
        // this handles grabbing metadata from parent
        let path = Path::new(rel_path);
        let stem = path.file_stem()?.to_str()?;
        let dir = path.parent()?.to_str()?;

        for ext in ["heic", "HEIC", "jpg", "JPG", "jpeg", "JPEG"] {
            let candidate = format!("{}/{}.{}", dir, stem, ext);
            if let Some(entry) = self.assets.get(&candidate)
                && entry.live_photo_video_id.is_some()
            {
                return Some(entry);
            }
        }

        None
    }
}
