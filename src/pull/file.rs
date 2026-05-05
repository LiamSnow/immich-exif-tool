//! Both describes the format for the pull file (immich_data.json)
//! and for deserializing Immich API responses
//! Only provides structure where needed for version compatibility

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::Path,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub pulled_at: String,
    pub server_url: String,
    pub remote_path: String,

    pub server: ServerAbout,
    pub config: Value,
    pub users: Vec<Value>,
    pub tags: Vec<Value>,
    pub people: Vec<Value>,
    pub stacks: Vec<Value>,
    pub map_markers: Vec<Value>,

    pub albums: Vec<Album>,

    /// Keyed by relative path (original path with `remote_path` stripped)
    pub assets: BTreeMap<String, Asset>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    #[serde(flatten)]
    pub album: AlbumDetail,
    pub activities: Vec<Value>,
}

/// [Docs](https://api.immich.app/models/AssetResponseDto)
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub original_path: String,
    pub original_file_name: String,
    #[serde(rename = "type")]
    pub asset_type: AssetType,
    pub live_photo_video_id: Option<String>,
    pub is_favorite: bool,
    pub is_trashed: bool,
    pub is_archived: bool,
    pub exif_info: Option<AssetExif>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// [Docs](https://api.immich.app/models/AssetTypeEnum)
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum AssetType {
    Image,
    Video,
    Audio,
    #[serde(other)]
    Other,
}

/// [Docs](https://api.immich.app/models/ExifResponseDto)
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetExif {
    pub date_time_original: Option<String>,

    pub latitude: Option<f64>,
    pub longitude: Option<f64>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub description: Option<String>,

    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,

    pub make: Option<String>,
    pub model: Option<String>,

    pub rating: Option<f64>,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// [Docs](https://api.immich.app/models/AlbumResponseDto)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumResponse {
    pub id: String,
    pub album_name: String,
    #[allow(dead_code)]
    pub asset_count: u32,
}

/// [Docs](https://api.immich.app/endpoints/albums/getAlbumInfo)
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumDetail {
    pub id: String,
    pub album_name: String,
    pub description: String,
    pub assets: Vec<AlbumAssetRef>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// [Docs](https://api.immich.app/models/AssetResponseDto)
/// Same as above, but kept minimal so albums only contain a list of images that they contain (instead of their full data)
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumAssetRef {
    pub id: String,
}

/// [Docs](https://api.immich.app/endpoints/server/getAboutInfo)
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAbout {
    pub version: String,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// [Docs](https://api.immich.app/endpoints/people/getAllPeople)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeopleResponse {
    #[allow(dead_code)]
    pub total: u64,
    #[allow(dead_code)]
    pub hidden: u64,
    pub has_next_page: bool,
    pub people: Vec<Value>,
}

impl AssetExif {
    pub fn gps(&self) -> Option<(f64, f64)> {
        match (self.latitude, self.longitude) {
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

impl File {
    pub fn load(path: &Path) -> Result<File> {
        let file = fs::File::open(path).with_context(|| format!("failed to open {path:?}"))?;
        serde_json::from_reader(file).with_context(|| format!("failed to parse {path:?}"))
    }

    pub fn resolve_asset(&self, rel_path: &str) -> Option<&Asset> {
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
