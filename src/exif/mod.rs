pub mod datetime;
pub mod exiftool;
pub use datetime::*;
use serde::{Deserialize, Serialize};

const GPS_EPSILON: f64 = 0.0001;

#[derive(Default)]
pub struct AssetExif {
    pub source_file: String,
    /// parsed, original
    pub date_time: Option<(ExifDateTime, String)>,
    pub gps: Option<GPS>,
    pub description: Option<String>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GPS {
    pub latitude: f64,
    pub longitude: f64,
}

impl GPS {
    pub fn from_opts(latitude: Option<f64>, longitude: Option<f64>) -> Option<Self> {
        Some(Self {
            latitude: latitude?,
            longitude: longitude?,
        })
    }
}

impl PartialEq for GPS {
    fn eq(&self, other: &Self) -> bool {
        (self.latitude - other.latitude).abs() < GPS_EPSILON
            && (self.longitude - other.longitude).abs() < GPS_EPSILON
    }
}
