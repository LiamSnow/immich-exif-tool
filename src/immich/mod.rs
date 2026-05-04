//! Raw and flexible interface to the Immich API
//! Only provides structure where needed for version compatibility
//! [Docs](https://api.immich.app/introduction)

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub mod types;
use types::*;

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

        resp.json::<T>()
            .with_context(|| format!("failed to parse response from GET {}", endpoint))
    }

    /// [Docs](https://api.immich.app/endpoints/views/getUniqueOriginalPaths)
    pub fn folder_paths(&self) -> Result<Vec<String>> {
        self.get("/view/folder/unique-paths")
    }

    /// [Docs](https://api.immich.app/endpoints/views/getAssetsByOriginalPath)
    pub fn folder_assets(&self, path: &str) -> Result<Vec<AssetResponse>> {
        let encoded = urlencoding::encode(path);
        self.get(&format!("/view/folder?path={}", encoded))
    }

    /// [Docs](https://api.immich.app/endpoints/albums/getAllAlbums)
    pub fn albums(&self) -> Result<Vec<AlbumResponse>> {
        self.get("/albums")
    }

    /// [Docs](https://api.immich.app/endpoints/albums/getAlbumInfo)
    pub fn album_detail(&self, id: &str) -> Result<AlbumDetail> {
        self.get(&format!("/albums/{}", id))
    }

    /// [Docs](https://api.immich.app/endpoints/server/getAboutInfo)
    pub fn server_about(&self) -> Result<ServerAbout> {
        self.get("/server/about")
    }

    /// [Docs](https://api.immich.app/endpoints/system-config/getConfig)
    pub fn system_config(&self) -> Result<Value> {
        self.get("/system-config")
    }

    /// [Docs](https://api.immich.app/endpoints/users/searchUsers)
    pub fn users(&self) -> Result<Vec<Value>> {
        self.get("/users")
    }

    /// [Docs](https://api.immich.app/endpoints/tags/getAllTags)
    pub fn tags(&self) -> Result<Vec<Value>> {
        self.get("/tags")
    }

    /// [Docs](https://api.immich.app/endpoints/people/getAllPeople)
    pub fn people(&self) -> Result<Vec<Value>> {
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

    /// [Docs](https://api.immich.app/endpoints/stacks/searchStacks)
    pub fn stacks(&self) -> Result<Vec<Value>> {
        self.get("/stacks")
    }

    /// [Docs](https://api.immich.app/endpoints/map/getMapMarkers)
    pub fn map_markers(&self) -> Result<Vec<Value>> {
        self.get("/map/markers")
    }

    /// [Docs](https://api.immich.app/endpoints/activities/getActivities)
    pub fn activities(&self, album_id: &str) -> Result<Vec<Value>> {
        self.get(&format!("/activities?albumId={}", album_id))
    }
}
