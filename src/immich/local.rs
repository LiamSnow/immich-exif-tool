//! Reads a narrow view of `immich_data.json`
//! skippign fields which aren't needed (ones for backup purposes)

use anyhow::{Context, bail};
use jiff::{
    fmt::temporal::{DateTimeParser, PiecesOffset},
    tz::Offset,
};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::Path;

use crate::exif::{ExifDateTime, GPS};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImmichData {
    pub assets: FxHashMap<String, Asset>,
}

/// [Docs](https://api.immich.app/models/AssetResponseDto)
#[derive(Debug, Deserialize, Serialize)]
pub struct Asset {
    #[serde(rename = "livePhotoVideoId", deserialize_with = "option_is_some")]
    pub is_live_photo: bool,
    #[serde(rename = "exifInfo")]
    pub exif_info: Option<ImmichExif>,
}

/// [Docs](https://api.immich.app/models/ExifResponseDto)
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImmichExif {
    /// 2020-01-31T04:21:49+00:00
    /// 2018-10-07T15:54:57.356+00:00
    /// 2022-08-22T15:24:51+00:00
    pub date_time_original: Option<String>,

    /// America/Los_Angeles
    /// America/New_York
    /// UTC-5
    /// UTC
    pub time_zone: Option<String>,

    pub latitude: Option<f64>,
    pub longitude: Option<f64>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub description: Option<String>,
}

impl ImmichData {
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
                && entry.is_live_photo
            {
                return Some(entry);
            }
        }

        None
    }
}

static DATETIME_PARSER: DateTimeParser = DateTimeParser::new();

impl ImmichExif {
    pub fn date_time(&self) -> anyhow::Result<Option<ExifDateTime>> {
        let Some(dt) = &self.date_time_original else {
            return Ok(None);
        };

        let pieces = DATETIME_PARSER.parse_pieces(dt)?;
        let dt = pieces
            .date()
            .to_datetime(pieces.time().context("missing time")?);

        // always has blank offset (+00:00)
        if let Some(offset) = pieces.offset()
            && offset != PiecesOffset::Numeric(Offset::ZERO.into())
        {
            bail!("unexpected offset {offset:?}");
        }

        // never has time zone written out in dateTimeOriginal
        if let Some(tz) = pieces.time_zone_annotation() {
            bail!("unexpected time zone {tz:?}");
        }

        let Some(tz) = &self.time_zone else {
            return Ok(Some(ExifDateTime::nonstd(dt, None)));
        };

        let offset =
            if let Some(suffix) = tz.strip_prefix("UTC").filter(|s| s.starts_with(['+', '-'])) {
                let hours: i32 = suffix.parse().context("invalid UTC offset in time zone")?;
                Offset::from_seconds(hours * 3600)?
            } else {
                dt.in_tz(tz)?.offset()
            };

        Ok(Some(ExifDateTime::nonstd(dt, Some(offset))))
    }

    pub fn gps(&self) -> Option<GPS> {
        GPS::from_opts(self.latitude, self.longitude)
    }
}

fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    Ok(s.filter(|s| !s.is_empty()))
}

fn option_is_some<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<serde::de::IgnoredAny>::deserialize(d)?.is_some())
}

#[cfg(test)]
mod tests {
    use crate::{exif::ExifDateTime, immich::local::ImmichExif};
    use anyhow::Result;
    use jiff::{civil::DateTime, tz::Offset};

    fn parse_raw(dt: Option<&str>, tz: Option<&str>) -> Result<Option<ExifDateTime>> {
        ImmichExif {
            date_time_original: dt.map(String::from),
            time_zone: tz.map(String::from),
            ..Default::default()
        }
        .date_time()
    }

    #[test]
    fn datetime_empty() {
        let dt = parse_raw(None, None).unwrap();
        assert!(dt.is_none());
    }

    fn parse(dt: &str, tz: &str) -> Result<ExifDateTime> {
        parse_raw(Some(dt), Some(tz)).map(|o| o.unwrap())
    }

    fn parse_ntz(dt: &str) -> Result<ExifDateTime> {
        parse_raw(Some(dt), None).map(|o| o.unwrap())
    }

    fn edt(datetime: DateTime, offset_hours: i8) -> ExifDateTime {
        let offset = Offset::from_hours(offset_hours).unwrap();
        ExifDateTime::nonstd(datetime, Some(offset))
    }

    fn edt_ntz(datetime: DateTime) -> ExifDateTime {
        ExifDateTime::nonstd(datetime, None)
    }

    #[test]
    fn datetime_no_tz() {
        assert_eq!(
            parse_ntz("2020-01-31T04:21:49+00:00").unwrap(),
            edt_ntz(DateTime::new(2020, 1, 31, 4, 21, 49, 0).unwrap()),
        );
        assert_eq!(
            parse_ntz("2018-10-07T15:54:57.356+00:00").unwrap(),
            edt_ntz(DateTime::new(2018, 10, 7, 15, 54, 57, 356000000).unwrap()),
        );
        assert_eq!(
            parse_ntz("2022-08-22T15:24:51+00:00").unwrap(),
            edt_ntz(DateTime::new(2022, 8, 22, 15, 24, 51, 0).unwrap()),
        );
    }

    #[test]
    fn datetime_std_tz() {
        assert_eq!(
            parse("2020-01-31T04:21:49+00:00", "America/Los_Angeles").unwrap(),
            edt(DateTime::new(2020, 1, 31, 4, 21, 49, 0).unwrap(), -8),
        );
        assert_eq!(
            parse("2018-10-07T15:54:57.356+00:00", "America/Los_Angeles").unwrap(),
            edt(
                DateTime::new(2018, 10, 7, 15, 54, 57, 356000000).unwrap(),
                -7
            ),
        );
        assert_eq!(
            parse("2022-08-22T15:24:51+00:00", "America/Los_Angeles").unwrap(),
            edt(DateTime::new(2022, 8, 22, 15, 24, 51, 0).unwrap(), -7),
        );
    }

    #[test]
    fn datetime_utc_tz() {
        assert_eq!(
            parse("2020-01-31T04:21:49+00:00", "UTC").unwrap(),
            edt(DateTime::new(2020, 1, 31, 4, 21, 49, 0).unwrap(), 0),
        );
        assert_eq!(
            parse("2018-10-07T15:54:57.356+00:00", "UTC").unwrap(),
            edt(
                DateTime::new(2018, 10, 7, 15, 54, 57, 356000000).unwrap(),
                0
            ),
        );
        assert_eq!(
            parse("2022-08-22T15:24:51+00:00", "UTC").unwrap(),
            edt(DateTime::new(2022, 8, 22, 15, 24, 51, 0).unwrap(), 0),
        );
    }

    #[test]
    fn datetime_utc_minus_5() {
        assert_eq!(
            parse("2020-01-31T04:21:49+00:00", "UTC-5").unwrap(),
            edt(DateTime::new(2020, 1, 31, 4, 21, 49, 0).unwrap(), -5),
        );
        assert_eq!(
            parse("2018-10-07T15:54:57.356+00:00", "UTC-5").unwrap(),
            edt(
                DateTime::new(2018, 10, 7, 15, 54, 57, 356000000).unwrap(),
                -5
            ),
        );
        assert_eq!(
            parse("2022-08-22T15:24:51+00:00", "UTC-5").unwrap(),
            edt(DateTime::new(2022, 8, 22, 15, 24, 51, 0).unwrap(), -5),
        );
    }
}
