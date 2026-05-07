use jiff::{
    civil::DateTime,
    fmt::strtime::{self, BrokenDownTime},
    tz::Offset,
};
use std::fmt::{self, Display, Formatter};

/// Technically the only stardard, but many platforms support offsets
const STD_FMT_NO_OFFSET: &str = "%Y:%m:%d %H:%M:%S";
const STD_FMT_OFFSET: &str = "%Y:%m:%d %H:%M:%S%:z";

#[derive(Clone, Debug, PartialEq)]
pub struct ExifDateTime {
    pub datetime: DateTime,
    pub offset: Option<Offset>,
    /// was parsed from a format above
    pub was_std: bool,
}

impl ExifDateTime {
    pub fn nonstd(datetime: DateTime, offset: Option<Offset>) -> Self {
        Self {
            datetime,
            offset,
            was_std: false,
        }
    }

    pub fn parse_std(input: &str) -> Option<Self> {
        Self::parse_std_offset(input).or_else(|| Self::parse_std_no_offset(input))
    }

    fn parse_std_offset(input: &str) -> Option<Self> {
        let r = strtime::parse(STD_FMT_OFFSET, input).ok()?;
        Some(Self {
            // crash if this doesn't work as expected
            offset: Some(r.offset().unwrap()),
            datetime: r.to_datetime().unwrap(),
            was_std: true,
        })
    }

    fn parse_std_no_offset(input: &str) -> Option<Self> {
        let r = strtime::parse(STD_FMT_NO_OFFSET, input).ok()?;
        Some(Self {
            offset: None,
            // crash if this doesn't work as expected
            datetime: r.to_datetime().unwrap(),
            was_std: true,
        })
    }

    pub fn broken(&self) -> BrokenDownTime {
        let mut bdt: BrokenDownTime = self.datetime.into();
        bdt.set_offset(self.offset);
        bdt
    }

    pub fn fmt_datetime(&self) -> Result<String, jiff::Error> {
        let fms = match self.offset {
            Some(_) => STD_FMT_OFFSET,
            None => STD_FMT_NO_OFFSET,
        };
        let mut out = String::new();
        self.broken().format(fms, &mut out)?;
        Ok(out)
    }
}

impl Display for ExifDateTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.fmt_datetime().map_err(|_| fmt::Error)?)
    }
}
