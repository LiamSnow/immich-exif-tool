use jiff::{Zoned, civil::DateTime, fmt::rfc2822};

const EXIF_NO_TZ: &str = "%Y:%m:%d %H:%M:%S";
const EXIF_WITH_TZ: &str = "%Y:%m:%d %H:%M:%S%:z";
const ISO_WITH_TZ: &str = "%Y-%m-%dT%H:%M:%S%.f%:z";

#[derive(Debug, Clone)]
pub struct ParsedDateTime {
    pub civil: DateTime,
    pub zoned: Option<Zoned>,
    pub is_exif: bool,
}

impl ParsedDateTime {
    pub fn new(input: &str) -> Option<ParsedDateTime> {
        let input = input.trim();

        // EXIF with offset
        if let Ok(zoned) = Zoned::strptime(EXIF_WITH_TZ, input) {
            return Some(ParsedDateTime {
                civil: truncate_to_seconds(zoned.datetime()),
                zoned: Some(zoned),
                is_exif: true,
            });
        }

        // EXIF without offset
        if let Ok(dt) = DateTime::strptime(EXIF_NO_TZ, input) {
            return Some(ParsedDateTime {
                civil: truncate_to_seconds(dt),
                zoned: None,
                is_exif: true,
            });
        }

        // RFC 9557 with IANA annotation
        if let Ok(zoned) = input.parse::<Zoned>() {
            return Some(ParsedDateTime {
                civil: truncate_to_seconds(zoned.datetime()),
                zoned: Some(zoned),
                is_exif: false,
            });
        }

        // ISO 8601 / RFC 3339 with offset
        if let Some(parsed) = Self::try_iso_with_offset(input) {
            return Some(parsed);
        }

        // ISO 8601 without offset
        if let Ok(dt) = input.parse::<DateTime>() {
            return Some(ParsedDateTime {
                civil: truncate_to_seconds(dt),
                zoned: None,
                is_exif: false,
            });
        }

        // RFC 2822
        if let Ok(zoned) = rfc2822::parse(input) {
            return Some(ParsedDateTime {
                civil: truncate_to_seconds(zoned.datetime()),
                zoned: Some(zoned),
                is_exif: false,
            });
        }

        None
    }

    /// Parse ISO 8601 with UTC offset but no IANA time zone
    fn try_iso_with_offset(input: &str) -> Option<ParsedDateTime> {
        let mut buf = input.to_string();

        // "2024-07-11 01:14:00+02:00" -> "2024-07-11T01:14:00+02:00"
        if buf.len() > 10 && buf.as_bytes()[10] == b' ' {
            buf.replace_range(10..11, "T");
        }

        // "2024-07-11T01:14:00Z" -> "2024-07-11T01:14:00+00:00"
        if buf.ends_with('Z') {
            buf.pop();
            buf.push_str("+00:00");
        }

        let zoned = Zoned::strptime(ISO_WITH_TZ, &buf).ok()?;
        Some(ParsedDateTime {
            civil: truncate_to_seconds(zoned.datetime()),
            zoned: Some(zoned),
            is_exif: false,
        })
    }

    /// Format as an EXIF datetime string
    pub fn format(&self) -> String {
        self.zoned
            .as_ref()
            .map(|z| z.strftime(EXIF_WITH_TZ))
            .unwrap_or(self.civil.strftime(EXIF_NO_TZ))
            .to_string()
    }
}

impl PartialEq for ParsedDateTime {
    fn eq(&self, other: &Self) -> bool {
        match (&self.zoned, &other.zoned) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.civil == other.civil,
            _ => false,
        }
    }
}

fn truncate_to_seconds(dt: DateTime) -> DateTime {
    DateTime::new(
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second(),
        0,
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exif_with_offset() {
        let dt = ParsedDateTime::new("2024:07:11 01:14:00+02:00").unwrap();
        assert_eq!(dt.civil.to_string(), "2024-07-11T01:14:00");
        assert!(dt.zoned.is_some());
        assert!(dt.is_exif);
    }

    #[test]
    fn parse_exif_no_offset() {
        let dt = ParsedDateTime::new("2024:07:11 01:14:00").unwrap();
        assert_eq!(dt.civil.to_string(), "2024-07-11T01:14:00");
        assert!(dt.zoned.is_none());
        assert!(dt.is_exif);
    }

    #[test]
    fn parse_iso_utc_z() {
        let dt = ParsedDateTime::new("2024-07-11T01:14:00.123Z").unwrap();
        assert_eq!(dt.civil.to_string(), "2024-07-11T01:14:00");
        assert!(dt.zoned.is_some());
        assert!(!dt.is_exif);
    }

    #[test]
    fn parse_iso_with_offset() {
        let dt = ParsedDateTime::new("2024-07-11T01:14:00+02:00").unwrap();
        assert_eq!(dt.civil.to_string(), "2024-07-11T01:14:00");
        assert!(dt.zoned.is_some());
        assert!(!dt.is_exif);
    }

    #[test]
    fn parse_iso_no_offset() {
        let dt = ParsedDateTime::new("2024-07-11T01:14:00").unwrap();
        assert_eq!(dt.civil.to_string(), "2024-07-11T01:14:00");
        assert!(dt.zoned.is_none());
        assert!(!dt.is_exif);
    }

    #[test]
    fn parse_iso_space_separator_with_offset() {
        let dt = ParsedDateTime::new("2024-07-11 01:14:00+02:00").unwrap();
        assert_eq!(dt.civil.to_string(), "2024-07-11T01:14:00");
        assert!(dt.zoned.is_some());
    }

    #[test]
    fn parse_immich_formats() {
        let cases = [
            "2020-01-31T04:21:49+00:00",
            "2018-10-07T15:54:57.356+00:00",
            "2022-08-22T15:24:51+00:00",
        ];
        for input in cases {
            let dt = ParsedDateTime::new(input)
                .unwrap_or_else(|| panic!("failed to parse Immich date: {input}"));
            assert!(dt.zoned.is_some(), "expected zoned for {input}");
            assert!(!dt.is_exif, "expected non-exif for {input}");
        }
    }

    #[test]
    fn parse_rfc9557_iana() {
        let dt = ParsedDateTime::new("2024-07-11T01:14:00-04:00[America/New_York]").unwrap();
        assert!(dt.zoned.is_some());
        assert!(!dt.is_exif);
        assert_eq!(dt.format(), "2024:07:11 01:14:00-04:00");
    }

    #[test]
    fn parse_invalid() {
        assert!(ParsedDateTime::new("not a date").is_none());
    }

    #[test]
    fn format_exif_with_offset() {
        let dt = ParsedDateTime::new("2024:07:11 01:14:00+02:00").unwrap();
        assert_eq!(dt.format(), "2024:07:11 01:14:00+02:00");
    }

    #[test]
    fn format_iso_utc() {
        let dt = ParsedDateTime::new("2024-07-11T01:14:00Z").unwrap();
        assert_eq!(dt.format(), "2024:07:11 01:14:00+00:00");
    }

    #[test]
    fn format_iso_no_offset() {
        let dt = ParsedDateTime::new("2024-07-11T01:14:00").unwrap();
        assert_eq!(dt.format(), "2024:07:11 01:14:00");
    }

    #[test]
    fn eq_same_instant_different_offsets() {
        let a = ParsedDateTime::new("2024:07:11 01:14:00+02:00").unwrap();
        let b = ParsedDateTime::new("2024:07:11 01:14:00+02:00").unwrap();
        let c = ParsedDateTime::new("2024:07:11 03:14:00+04:00").unwrap();
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn eq_both_unzoned() {
        let a = ParsedDateTime::new("2024:07:11 01:14:00").unwrap();
        let b = ParsedDateTime::new("2024-07-11T01:14:00").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn ne_zoned_vs_unzoned() {
        let zoned = ParsedDateTime::new("2024:07:11 01:14:00+02:00").unwrap();
        let unzoned = ParsedDateTime::new("2024:07:11 01:14:00").unwrap();
        assert_ne!(zoned, unzoned);
    }
}
