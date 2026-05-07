//! Raw and flexible interface to the Immich API
//! Only provides structure where needed for version compatibility
//! [Docs](https://api.immich.app/introduction)

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sonic_rs::{JsonValueMutTrait, JsonValueTrait, OwnedLazyValue};

pub struct ImmichClient {
    client: Client,
    base_url: String,
    api_key: String,
}

impl ImmichClient {
    pub fn new(base_url: &str, api_key: String) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
        }
    }

    fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        let url = format!("{}/api{}", self.base_url, endpoint);
        let resp = self
            .client
            .get(&url)
            .header("x-api-key", &self.api_key)
            .send()
            .with_context(|| format!("request failed: GET {}", endpoint))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("GET {} returned {}: {}", endpoint, status, body);
        }

        let bytes = resp
            .bytes()
            .with_context(|| format!("failed to read response from GET {}", endpoint))?;
        sonic_rs::from_slice(&bytes)
            .with_context(|| format!("failed to parse response from GET {}", endpoint))
    }

    /// [Docs](https://api.immich.app/endpoints/system-config/getConfig)
    pub fn system_config(&self) -> Result<OwnedLazyValue> {
        self.get("/system-config")
    }

    /// [Docs](https://api.immich.app/endpoints/users/searchUsers)
    pub fn users(&self) -> Result<Vec<OwnedLazyValue>> {
        self.get("/users")
    }

    /// [Docs](https://api.immich.app/endpoints/tags/getAllTags)
    pub fn tags(&self) -> Result<Vec<OwnedLazyValue>> {
        self.get("/tags")
    }

    /// [Docs](https://api.immich.app/endpoints/stacks/searchStacks)
    pub fn stacks(&self) -> Result<Vec<OwnedLazyValue>> {
        self.get("/stacks")
    }

    /// [Docs](https://api.immich.app/endpoints/map/getMapMarkers)
    pub fn map_markers(&self) -> Result<Vec<OwnedLazyValue>> {
        self.get("/map/markers")
    }
}

// ---- Assets

/// [Docs](https://api.immich.app/models/AssetResponseDto)
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Asset(OwnedLazyValue);

impl Asset {
    pub fn original_path(&self) -> Result<&str> {
        self.0
            .get("originalPath")
            .and_then(|v| v.as_str())
            .context("Immich asset missing `originalPath`")
    }
}

impl ImmichClient {
    /// Fetch metadata of every asset
    pub fn assets(&self) -> Result<impl ExactSizeIterator<Item = Result<Vec<Asset>>> + '_> {
        let dirs = self.dirs()?;
        Ok(dirs.into_iter().map(|dir| {
            self.assets_in_dir(&dir)
                .with_context(|| format!("assets in directory '{dir}'"))
        }))
    }

    /// Fetch every directory which contains an asset
    /// [Docs](https://api.immich.app/endpoints/views/getUniqueOriginalPaths)
    fn dirs(&self) -> Result<Vec<String>> {
        self.get("/view/folder/unique-paths")
    }

    /// Fetch metadata of every asset in a directory
    /// [Docs](https://api.immich.app/endpoints/views/getAssetsByOriginalPath)
    fn assets_in_dir(&self, path: &str) -> Result<Vec<Asset>> {
        let encoded = urlencoding::encode(path);
        self.get(&format!("/view/folder?path={}", encoded))
    }
}

// ---- People

/// [Docs](https://api.immich.app/endpoints/people/getAllPeople)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeopleResponse {
    pub has_next_page: bool,
    pub people: Vec<OwnedLazyValue>,
}

impl ImmichClient {
    /// [Docs](https://api.immich.app/endpoints/people/getAllPeople)
    pub fn people(&self) -> Result<Vec<OwnedLazyValue>> {
        let mut all = Vec::new();
        let mut page = 1;

        loop {
            let endpoint = format!("/people?page={}", page);
            let response: PeopleResponse = self.get(&endpoint)?;
            all.extend(response.people);

            if !response.has_next_page {
                break;
            }
            page += 1;
        }

        Ok(all)
    }
}

// ---- Server About

/// [Docs](https://api.immich.app/endpoints/server/getAboutInfo)
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServerAbout(OwnedLazyValue);

impl ServerAbout {
    pub fn version(&self) -> Result<&str> {
        self.0
            .get("version")
            .and_then(|v| v.as_str())
            .context("Immich server about missing `version`")
    }
}

impl ImmichClient {
    /// [Docs](https://api.immich.app/endpoints/server/getAboutInfo)
    pub fn server_about(&self) -> Result<ServerAbout> {
        self.get("/server/about")
    }
}

// ---- Albums

/// [Docs](https://api.immich.app/models/AlbumResponseDto)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlbumMetadata {
    pub id: String,
    pub album_name: String,
}

/// [Docs](https://api.immich.app/endpoints/albums/getAlbumInfo)
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Album(sonic_rs::Value);

impl Album {
    pub fn id(&self) -> Result<&str> {
        self.0
            .get("id")
            .and_then(|v| v.as_str())
            .context("Immich album missing `id`")
    }

    fn set_activities(&mut self, activities: sonic_rs::Value) {
        if let Some(obj) = self.0.as_object_mut() {
            obj.insert("activities", activities);
        }
    }
}

impl ImmichClient {
    /// Fetch all albums, with their full data
    /// [Docs](https://api.immich.app/endpoints/albums/getAllAlbums)
    pub fn albums(&self) -> Result<impl ExactSizeIterator<Item = Result<Album>> + '_> {
        let index: Vec<AlbumMetadata> = self.get("/albums")?;
        Ok(index.into_iter().map(|meta| {
            self.album(&meta.id)
                .with_context(|| format!("album '{}'", meta.album_name))
        }))
    }

    /// Fetch full data of an album
    /// [Docs](https://api.immich.app/endpoints/albums/getAlbumInfo)
    /// [Docs](https://api.immich.app/endpoints/activities/getActivities)
    pub fn album(&self, id: &str) -> Result<Album> {
        let mut album: Album = self.get(&format!("/albums/{}", id))?;
        album.strip_asset_details();
        if let Ok(activities) = self.get::<sonic_rs::Value>(&format!(
            "/activities?albumId={}",
            album.id().unwrap_or_default()
        )) {
            album.set_activities(activities);
        }
        Ok(album)
    }
}

impl Album {
    fn strip_asset_details(&mut self) {
        let Some(assets) = self.0.get_mut("assets") else {
            return;
        };
        let Some(arr) = assets.as_array_mut() else {
            return;
        };
        for asset in arr.iter_mut() {
            let id = asset
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_owned());
            if let Some(id) = id {
                *asset = sonic_rs::json!({ "id": id });
            }
        }
    }
}
