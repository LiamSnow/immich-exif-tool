use crate::immich::ImmichClient;
use crate::immich::types::{AlbumDetail, AssetResponse, ServerAbout};
use anyhow::{Context, Result};
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::File;
use std::time::Duration;

const LABEL_WIDTH: usize = 13;

#[derive(clap::Args)]
pub struct Args {
    /// Immich server URL
    #[arg(long, env = "SERVER_URL")]
    pub server_url: String,

    /// Immich API key
    #[arg(long, env = "API_KEY")]
    pub api_key: String,

    /// Remote library path prefix (on the Immich server)
    #[arg(long, env = "REMOTE_PATH")]
    pub remote_path: String,

    /// Output file path
    #[arg(short, long, default_value = "dump.json")]
    pub output: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DumpFile {
    pub dumped_at: String,
    pub server_url: String,
    pub remote_path: String,

    pub server: ServerAbout,
    pub config: Value,
    pub users: Vec<Value>,
    pub tags: Vec<Value>,
    pub people: Vec<Value>,
    pub stacks: Vec<Value>,
    pub map_markers: Vec<Value>,

    pub albums: Vec<AlbumDump>,

    /// Keyed by relative path (original path with `remote_path` stripped)
    pub assets: BTreeMap<String, AssetResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumDump {
    #[serde(flatten)]
    pub album: AlbumDetail,
    pub activities: Vec<Value>,
}

pub fn run(args: Args) -> Result<()> {
    let remote_path = args.remote_path.trim_end_matches('/');
    let prefix = format!("{}/", remote_path);
    let client = ImmichClient::new(&args.server_url, args.api_key);
    let mp = MultiProgress::new();

    let row_server = Row::new(&mp, "Server");
    let row_config = Row::new(&mp, "Config");
    let row_users = Row::new(&mp, "Users");
    let row_tags = Row::new(&mp, "Tags");
    let row_people = Row::new(&mp, "People");
    let row_stacks = Row::new(&mp, "Stacks");
    let row_markers = Row::new(&mp, "Map markers");
    let row_folders = Row::new(&mp, "Folders");
    let row_assets = Row::new(&mp, "Assets");
    let row_albums = Row::new(&mp, "Albums");
    let row_writing = Row::new(&mp, "Writing");

    let server = client.server_about()?;
    row_server.finish(&format!("Immich {}", server.version));

    let config = client.system_config()?;
    row_config.finish("fetched");

    let users = client.users()?;
    row_users.finish(&users.len().to_string());

    let tags = client.tags()?;
    row_tags.finish(&tags.len().to_string());

    let people = client.people()?;
    row_people.finish(&people.len().to_string());

    let stacks = client.stacks()?;
    row_stacks.finish(&stacks.len().to_string());

    let map_markers = client.map_markers()?;
    row_markers.finish(&map_markers.len().to_string());

    let all_paths = client.folder_paths()?;
    let paths: Vec<&String> = all_paths
        .iter()
        .filter(|p| p.starts_with(remote_path))
        .collect();
    row_folders.finish(&format!("{} (from {})", paths.len(), all_paths.len()));

    let (assets, asset_errors) = fetch_assets(&client, &mp, &paths, &prefix, row_assets);
    let (albums, album_errors) = fetch_albums(&client, &mp, row_albums);

    let dump = DumpFile {
        dumped_at: jiff::Zoned::now().to_string(),
        server_url: args.server_url,
        remote_path: remote_path.to_string(),
        server,
        config,
        users,
        tags,
        people,
        stacks,
        map_markers,
        albums,
        assets,
    };

    let file =
        File::create(&args.output).with_context(|| format!("failed to create {}", &args.output))?;
    serde_json::to_writer_pretty(file, &dump)
        .with_context(|| format!("failed to write {}", &args.output))?;

    row_writing.finish(&args.output);

    let total_errors = asset_errors + album_errors;
    println!();
    if total_errors > 0 {
        println!(
            "  {} with {} errors",
            style("Done").green().bold(),
            style(total_errors).red().bold(),
        );
    } else {
        println!("  {}", style("Done!").green().bold());
    }

    Ok(())
}

fn fetch_assets(
    client: &ImmichClient,
    mp: &MultiProgress,
    paths: &[&String],
    prefix: &str,
    row: Row,
) -> (BTreeMap<String, AssetResponse>, u32) {
    let mut assets = BTreeMap::new();
    let mut errors = 0u32;

    for (i, path) in paths.iter().enumerate() {
        row.update(&format!(
            "{}/{} ({} assets)",
            i + 1,
            paths.len(),
            assets.len()
        ));

        let folder_assets = match client.folder_assets(path) {
            Ok(a) => a,
            Err(e) => {
                let _ = mp.println(format!("  {} {}: {}", style("✗").red(), path, e));
                errors += 1;
                continue;
            }
        };

        for asset in folder_assets {
            let Some(rel) = asset.original_path.strip_prefix(prefix) else {
                let _ = mp.println(format!(
                    "  {} bad prefix: {}",
                    style("✗").yellow(),
                    asset.original_path
                ));
                errors += 1;
                continue;
            };
            assets.insert(rel.to_string(), asset);
        }
    }

    row.finish(&assets.len().to_string());
    (assets, errors)
}

fn fetch_albums(client: &ImmichClient, mp: &MultiProgress, row: Row) -> (Vec<AlbumDump>, u32) {
    let album_list = match client.albums() {
        Ok(list) => list,
        Err(e) => {
            let _ = mp.println(format!("  {} album list: {}", style("✗").red(), e));
            row.finish("error");
            return (vec![], 1);
        }
    };

    let mut albums = Vec::new();
    let mut errors = 0u32;

    for (i, summary) in album_list.iter().enumerate() {
        row.update(&format!("{}/{}", i + 1, album_list.len()));

        let detail = match client.album_detail(&summary.id) {
            Ok(d) => d,
            Err(e) => {
                let _ = mp.println(format!(
                    "  {} album '{}': {}",
                    style("✗").red(),
                    summary.album_name,
                    e
                ));
                errors += 1;
                continue;
            }
        };

        let activities = client.activities(&summary.id).unwrap_or_default();
        albums.push(AlbumDump {
            album: detail,
            activities,
        });
    }

    row.finish(&albums.len().to_string());
    (albums, errors)
}

struct Row {
    label: String,
    sp: ProgressBar,
}

impl Row {
    fn new(mp: &MultiProgress, label: &str) -> Self {
        let bar = mp.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::default_spinner()
                .template("  {spinner:.cyan} {msg}")
                .unwrap(),
        );
        bar.enable_steady_tick(Duration::from_millis(80));
        bar.set_message(format!("{:<LABEL_WIDTH$}", label));
        Self {
            label: label.to_string(),
            sp: bar,
        }
    }

    fn update(&self, value: &str) {
        self.sp
            .set_message(format!("{:<LABEL_WIDTH$} {}", self.label, value,));
    }

    fn finish(self, value: &str) {
        self.sp.set_style(
            ProgressStyle::default_spinner()
                .template("  {msg}")
                .unwrap(),
        );
        self.sp.finish_with_message(format!(
            "{} {:<LABEL_WIDTH$} {}",
            style("✓").green(),
            self.label,
            value,
        ));
    }
}
