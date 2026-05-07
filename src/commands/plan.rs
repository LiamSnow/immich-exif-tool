use crate::exif::exiftool::Exiftool;
use crate::exif::{self, AssetExif, ExifDateTime, GPS};
use crate::immich::local as immich;
use crate::immich::local::ImmichData;
use crate::plan_file::{self, AssetState, FieldState, FieldStats, Fix, Reason};
use anyhow::{Context, Result};
use indicatif::{HumanDuration, ProgressBar, ProgressStyle};
use sonic_rs::writer::BufferedWriter;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::thread;
use std::time::{Duration, Instant};
use std::{fs, io::Read};
use walkdir::WalkDir;

const IGNORED_EXTS: &[&str] = &["xmp"];
const BATCH_SIZE: usize = 20;

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

struct AtomicState<'a> {
    local_path: Arc<String>,
    immich_data: Arc<ImmichData>,
    batches: Arc<Vec<&'a [PathBuf]>>,
    next_batch: AtomicUsize,
    progress_bar: ProgressBar,
    progress: (AtomicUsize, usize),
}

pub fn run(mut args: Args) -> Result<()> {
    let start = Instant::now();
    args.local_path = format!("{}/", args.local_path.trim_end_matches('/'));

    let immich_data: ImmichData = {
        let file = fs::File::open(&args.immich_data)
            .with_context(|| format!("failed to open {:?}", args.immich_data))?;
        let reader = BufReader::new(file);

        let bar = ProgressBar::new_spinner()
            .with_prefix("Loading")
            .with_message(args.immich_data_file_name())
            .with_style(ProgressStyle::with_template(
                "    {spinner:.green} {prefix} {msg}",
            )?);
        bar.enable_steady_tick(Duration::from_millis(80));

        let data: ImmichData = sonic_rs::from_reader(reader)
            .with_context(|| format!("failed to parse {:?}", args.immich_data))?;

        bar.finish_and_clear();

        Ok::<ImmichData, anyhow::Error>(data)
    }?;

    let assets = find_local_assets(&args.local_path);

    let progress_bar = ProgressBar::new(assets.len() as u64).with_style(
        ProgressStyle::with_template("{pos:>5}/{len:5} {bar:40.cyan/blue} [{elapsed} ETA {eta}]")?,
    );
    progress_bar.set_position(0);

    let workers = num_workers();
    let state = AtomicState {
        local_path: args.local_path.clone().into(),
        immich_data: immich_data.into(),
        progress_bar,
        progress: (0.into(), assets.len()),
        batches: Arc::new(assets.chunks(BATCH_SIZE).collect()),
        next_batch: 0.into(),
    };

    let mut out = plan_file::File::default();
    thread::scope(|s| -> Result<()> {
        let handles: Vec<_> = (0..workers).map(|_| s.spawn(|| state.worker())).collect();

        for handle in handles {
            out.add_result(handle.join().unwrap()?);
        }
        Ok(())
    })?;

    state.progress_bar.finish_and_clear();

    let bar = ProgressBar::new_spinner()
        .with_message("Writing")
        .with_prefix(args.output_file_name())
        .with_style(ProgressStyle::with_template(
            "    {spinner:.green} {msg} {prefix}",
        )?);
    bar.enable_steady_tick(Duration::from_millis(80));

    let file = fs::File::create(&args.output)?;
    let writer = BufferedWriter::new(file);
    sonic_rs::to_writer(writer, &out)?;

    bar.finish_and_clear();

    println!(" Done in {}", HumanDuration(Instant::now() - start),);
    print!("{}", out.summary);
    println!(
        " {} Linked   {} Orphaned",
        out.summary.linked, out.summary.orphaned,
    );
    println!(" Output {}", args.output.display());

    Ok(())
}

#[derive(Default)]
struct WorkerResult {
    linked: u32,
    issued: Vec<(String, AssetState)>,
    orphaned: Vec<String>,
    date_time: FieldStats,
    gps: FieldStats,
    description: FieldStats,
    file_extension: FieldStats,
}

impl<'a> AtomicState<'a> {
    fn worker(&self) -> Result<WorkerResult> {
        let mut out = WorkerResult::default();
        let mut exiftool = Exiftool::spawn()?;

        loop {
            let idx = self.next_batch.fetch_add(1, Relaxed);
            if idx >= self.batches.len() {
                break;
            }

            let batch = self.batches[idx];
            for asset in exiftool.read_batch(batch)? {
                self.process_asset(&mut out, asset)?;
            }
        }

        Ok(out)
    }

    fn process_asset(&self, out: &mut WorkerResult, local: AssetExif) -> Result<()> {
        let rel_path = local
            .source_file
            .strip_prefix(&*self.local_path)
            .unwrap_or(&local.source_file)
            .to_string();

        let immich = self.immich_data.resolve_asset(&rel_path);

        if immich.is_none() {
            out.orphaned.push(rel_path.to_string());
        } else {
            out.linked += 1;
        }

        let state = local.assess_state(immich.and_then(|i| i.exif_info.as_ref()))?;
        out.add_entry(&state);

        if !state.is_perfect() {
            out.issued.push((rel_path.to_string(), state));
        }

        self.progress();
        Ok(())
    }
}

impl<'a> AtomicState<'a> {
    fn progress(&self) {
        let res = self.progress.0.fetch_add(1, Relaxed);
        self.progress_bar.set_position(res as u64);
    }
}

impl plan_file::File {
    fn add_result(&mut self, res: WorkerResult) {
        self.summary.add_result(&res);

        for (k, v) in res.issued {
            self.assets.insert(k, v);
        }
        self.orphans.extend(res.orphaned);
    }
}

impl plan_file::Summary {
    fn add_result(&mut self, res: &WorkerResult) {
        self.linked += res.linked;
        self.orphaned += res.orphaned.len() as u32;
        self.total = self.linked + self.orphaned;
        self.date_time += res.date_time;
        self.gps += res.gps;
        self.description += res.description;
        self.file_extension += res.file_extension;
    }
}

impl WorkerResult {
    fn add_entry(&mut self, entry: &AssetState) {
        self.date_time += &entry.date_time;
        self.gps += &entry.gps;
        self.description += &entry.description;
        self.file_extension += &entry.file_extension;
    }
}

impl exif::AssetExif {
    fn assess_state(self, immich: Option<&immich::ImmichExif>) -> Result<AssetState> {
        Ok(AssetState {
            date_time: assess_date_time(
                self.date_time,
                immich.and_then(|e| e.date_time().transpose()).transpose()?,
            )?,
            gps: assess_gps_state(self.gps, immich.and_then(|e| e.gps())),
            description: assess_description_state(
                self.description,
                immich.and_then(|e| e.description.as_deref()),
            ),
            file_extension: assess_file_ext_state(self.source_file)?,
        })
    }
}

fn assess_date_time(
    local: Option<(ExifDateTime, String)>,
    immich: Option<ExifDateTime>,
) -> Result<FieldState<String>> {
    use FieldState::*;
    Ok(match (local, immich) {
        (None, None) => Unfixable(Reason::NoSources),
        (None, Some(dt)) => Fixable(Fix::AddImmich(dt.to_string())),
        (Some((local, old)), None) => {
            if local.was_std {
                Good
            } else {
                Fixable(Fix::Repair {
                    new: local.to_string(),
                    old,
                })
            }
        }
        (Some((local, old)), Some(immich)) => assess_date_time_d(local, old, immich)?,
    })
}

fn assess_date_time_d(
    local: ExifDateTime,
    old: String,
    immich: ExifDateTime,
) -> Result<FieldState<String>> {
    use FieldState::*;
    Ok(match (local.offset, immich.offset) {
        (None, None) => {
            if local.datetime == immich.datetime {
                if local.was_std {
                    Good
                } else {
                    Fixable(Fix::Repair {
                        new: local.to_string(),
                        old,
                    })
                }
            } else {
                Fixable(Fix::ReplaceWithImmich {
                    old,
                    new: immich.fmt_datetime()?,
                })
            }
        }
        (None, Some(_)) => Fixable(Fix::ReplaceWithImmich {
            old,
            new: immich.fmt_datetime()?,
        }),
        (Some(_), None) => {
            if local.was_std {
                Good
            } else {
                Fixable(Fix::Repair {
                    new: local.to_string(),
                    old,
                })
            }
        }
        (Some(_), Some(_)) => {
            if local.offset == immich.offset && local.datetime == immich.datetime {
                if local.was_std {
                    Good
                } else {
                    Fixable(Fix::Repair {
                        new: local.to_string(),
                        old,
                    })
                }
            } else {
                Fixable(Fix::ReplaceWithImmich {
                    old,
                    new: immich.fmt_datetime()?,
                })
            }
        }
    })
}

fn assess_gps_state(local: Option<GPS>, immich: Option<GPS>) -> FieldState<GPS> {
    use FieldState::*;
    match (local, immich) {
        (None, None) => Unfixable(Reason::NoSources),
        (None, Some(imm)) => Fixable(Fix::AddImmich(imm)),
        (Some(_), None) => Good,
        (Some(loc), Some(imm)) => {
            if loc == imm {
                Good
            } else {
                Fixable(Fix::ReplaceWithImmich { old: loc, new: imm })
            }
        }
    }
}

fn assess_description_state(local: Option<String>, immich: Option<&str>) -> FieldState<String> {
    use FieldState::*;
    match (local, immich) {
        (None, None) => Unfixable(Reason::NoSources),
        (None, Some(i)) => Fixable(Fix::AddImmich(i.to_string())),
        (Some(_), None) => Good,
        (Some(loc), Some(imm)) => {
            if loc == imm {
                Good
            } else {
                Fixable(Fix::ReplaceWithImmich {
                    old: loc.to_string(),
                    new: imm.to_string(),
                })
            }
        }
    }
}

// TODO replace with an actually good impl (crate?)
fn assess_file_ext_state(path: impl AsRef<Path>) -> Result<FieldState<String>> {
    let ext = path
        .as_ref()
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let mut f = fs::File::open(path)?;
    let mut magic = [0u8; 12];
    f.read_exact(&mut magic)?;

    let is_jpeg = magic[0] == 0xFF && magic[1] == 0xD8;
    let is_riff = &magic[0..4] == b"RIFF";

    Ok(match ext.as_str() {
        "png" | "heic" if is_jpeg => FieldState::Fixable(Fix::Repair {
            old: ext,
            new: "jpg".to_string(),
        }),
        "png" if is_riff => FieldState::Fixable(Fix::Repair {
            old: ext,
            new: "webp".to_string(),
        }),
        _ => FieldState::Good,
    })
}

fn find_local_assets(base: &str) -> Vec<PathBuf> {
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
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| !IGNORED_EXTS.contains(&s))
                .unwrap_or(false)
        })
        .map(|e| e.path().to_owned())
        .collect()
}

fn num_workers() -> usize {
    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4)
}

impl Args {
    fn immich_data_file_name(&self) -> String {
        self.immich_data
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default()
            .to_string()
    }

    fn output_file_name(&self) -> String {
        self.output
            .file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default()
            .to_string()
    }
}
