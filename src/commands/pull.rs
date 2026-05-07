use crate::immich::remote::{Album, Asset, ImmichClient, ServerAbout};
use anyhow::{Context, Result};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Attribute, Cell, CellAlignment, Color, Table};
use indicatif::{HumanDuration, ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use sonic_rs::OwnedLazyValue;
use sonic_rs::writer::BufferedWriter;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;
use std::{fmt, fs};

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
    #[arg(short, long, default_value = "immich_data.json")]
    pub output: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    pulled_at: String,
    server_url: String,
    remote_path: String,

    server: ServerAbout,
    config: OwnedLazyValue,
    users: Vec<OwnedLazyValue>,
    tags: Vec<OwnedLazyValue>,
    people: Vec<OwnedLazyValue>,
    stacks: Vec<OwnedLazyValue>,
    map_markers: Vec<OwnedLazyValue>,

    albums: Vec<Album>,
    album_errors: Vec<String>,

    /// Keyed by relative path (original path with `remote_path` stripped)
    /// B-Tree sorts the output just for readability
    assets: BTreeMap<String, Asset>,
    asset_errors: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    let start = Instant::now();
    let remote_path = args.remote_path.trim_end_matches('/');
    let prefix = format!("{}/", remote_path);
    let client = ImmichClient::new(&args.server_url, args.api_key);

    let bar = ProgressBar::new(9).with_style(ProgressStyle::with_template(
        "{pos:>4}/{len:4} {bar:40.cyan/blue} {msg} [{elapsed} ETA {eta}]",
    )?);
    bar.set_position(0);

    bar.set_message("Server About");
    let server = client.server_about()?;
    bar.inc(1);

    bar.set_message("Server Config");
    let config = client.system_config()?;
    bar.inc(1);

    bar.set_message("Assets Index");
    let assets_fetcher = client.assets()?;
    bar.inc(1);
    bar.inc_length(assets_fetcher.len() as u64);

    bar.set_message("Albums Index");
    let albums_fetcher = client.albums()?;
    bar.inc(1);
    bar.inc_length(albums_fetcher.len() as u64);

    bar.set_message("Users");
    let users = client.users()?;
    bar.inc(1);

    bar.set_message("Tags");
    let tags = client.tags()?;
    bar.inc(1);

    bar.set_message("People");
    let people = client.people()?;
    bar.inc(1);

    bar.set_message("Stacks");
    let stacks = client.stacks()?;
    bar.inc(1);

    bar.set_message("Map Markers");
    let map_markers = client.map_markers()?;
    bar.inc(1);

    bar.set_message("Assets");
    let mut assets: BTreeMap<String, Asset> = BTreeMap::new();
    let mut asset_errors = Vec::new();
    for result in assets_fetcher {
        bar.inc(1);
        match result {
            Ok(res) => {
                for asset in res {
                    if let Some(rel) = asset.original_path()?.strip_prefix(&prefix) {
                        assets.insert(rel.to_string(), asset);
                    }
                }
            }
            Err(e) => {
                asset_errors.push(e.to_string());
            }
        }
    }

    bar.set_message("Albums");
    let mut albums: Vec<Album> = Vec::new();
    let mut album_errors = Vec::new();
    for result in albums_fetcher {
        bar.inc(1);
        match result {
            Ok(album) => albums.push(album),
            Err(e) => {
                album_errors.push(e.to_string());
            }
        }
    }

    bar.finish_and_clear();

    let out = Output {
        pulled_at: jiff::Zoned::now().to_string(),
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
        album_errors,
        assets,
        asset_errors,
    };

    let file = fs::File::create(&args.output)
        .with_context(|| format!("failed to create {:?}", &args.output))?;
    let writer = BufferedWriter::new(file);
    sonic_rs::to_writer(writer, &out)
        .with_context(|| format!("failed to write {:?}", &args.output))?;

    println!(" Done in {}", HumanDuration(start.elapsed()),);

    print!("{out}");

    println!(" Output {}", args.output.display());

    Ok(())
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS);

        let dim = |s: &str| Cell::new(s).add_attribute(Attribute::Dim);
        let green = |n: usize| Cell::new(n).fg(Color::Green);

        table.add_row(vec![
            dim("Server"),
            Cell::new(self.server.version().unwrap_or("?")),
        ]);
        table.add_row(vec![dim("Assets"), green(self.assets.len())]);
        table.add_row(vec![dim("Albums"), green(self.albums.len())]);
        table.add_row(vec![dim("Users"), green(self.users.len())]);
        table.add_row(vec![dim("Tags"), green(self.tags.len())]);
        table.add_row(vec![dim("People"), green(self.people.len())]);
        table.add_row(vec![dim("Stacks"), green(self.stacks.len())]);
        table.add_row(vec![dim("Map Markers"), green(self.map_markers.len())]);

        let total_errors = self.asset_errors.len() + self.album_errors.len();
        if total_errors > 0 {
            table.add_row(vec![dim("Errors"), Cell::new(total_errors).fg(Color::Red)]);
        }

        table
            .column_mut(1)
            .unwrap()
            .set_cell_alignment(CellAlignment::Right);

        writeln!(f, "{table}")
    }
}
