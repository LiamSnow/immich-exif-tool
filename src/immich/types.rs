use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// [Docs](https://api.immich.app/models/AssetResponseDto)
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetResponse {
    pub id: String,
    pub original_path: String,
    pub original_file_name: String,
    #[serde(rename = "type")]
    pub asset_type: AssetType,
    pub live_photo_video_id: Option<String>,
    pub is_favorite: bool,
    pub is_trashed: bool,
    pub is_archived: bool,
    pub exif_info: Option<ExifInfo>,
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
pub struct ExifInfo {
    pub date_time_original: Option<String>,

    pub latitude: Option<f64>,
    pub longitude: Option<f64>,

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
