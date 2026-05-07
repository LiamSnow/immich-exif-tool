use crate::exif::{AssetExif, GPS, datetime::ExifDateTime};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer};
use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

const EXIF_TAGS: &[&str] = &[
    "DateTimeOriginal",
    "GPSLatitude",
    "GPSLongitude",
    "Description",
];

pub struct Exiftool {
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    child: Child,
}

#[derive(Deserialize, Default)]
struct Response {
    #[serde(rename = "SourceFile")]
    source_file: String,
    #[serde(rename = "DateTimeOriginal")]
    date_time_original: Option<String>,
    #[serde(rename = "GPSLatitude")]
    gps_latitude: Option<f64>,
    #[serde(rename = "GPSLongitude")]
    gps_longitude: Option<f64>,
    #[serde(
        rename = "Description",
        default,
        deserialize_with = "empty_string_as_none"
    )]
    description: Option<String>,
}

impl From<Response> for AssetExif {
    fn from(value: Response) -> Self {
        AssetExif {
            source_file: value.source_file,
            date_time: value
                .date_time_original
                .and_then(|s| Some((ExifDateTime::parse_std(&s)?, s))),
            gps: GPS::from_opts(value.gps_latitude, value.gps_longitude),
            description: value.description,
        }
    }
}

impl Exiftool {
    pub fn spawn() -> Result<Self> {
        let mut child = Command::new("exiftool")
            .args(["-stay_open", "True", "-@", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to start exiftool")?;

        let stdin = child.stdin.take().context("failed to capture stdin")?;
        let stdout = child.stdout.take().context("failed to capture stdout")?;

        Ok(Self {
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            child,
        })
    }

    pub fn read_batch(&mut self, paths: &[PathBuf]) -> Result<impl Iterator<Item = AssetExif>> {
        self.send_command(paths)?;
        let output = self.read_until_ready()?;

        let res: Vec<Response> = if output.is_empty() {
            vec![]
        } else {
            sonic_rs::from_slice(&output).context("failed to parse exiftool output")?
        };

        Ok(res.into_iter().map(|r| r.into()))
    }

    pub fn send_command(&mut self, paths: &[PathBuf]) -> Result<()> {
        writeln!(self.stdin, "-json")?;
        writeln!(self.stdin, "-n")?;
        for tag in EXIF_TAGS {
            writeln!(self.stdin, "-{tag}")?;
        }
        for path in paths {
            writeln!(self.stdin, "{}", path.display())?;
        }
        writeln!(self.stdin, "-execute")?;
        self.stdin.flush()?;
        Ok(())
    }

    pub fn read_until_ready(&mut self) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        let mut line = String::new();

        loop {
            line.clear();
            if self.stdout.read_line(&mut line)? == 0 {
                bail!("exiftool process terminated unexpectedly");
            }
            if line.trim_end() == "{ready}" {
                return Ok(output);
            }
            output.extend_from_slice(line.as_bytes());
        }
    }
}

impl Drop for Exiftool {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "-stay_open");
        let _ = writeln!(self.stdin, "False");
        let _ = self.stdin.flush();
        let _ = self.child.wait();
    }
}

fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = Option::<String>::deserialize(deserializer)?;
    Ok(s.filter(|s| !s.is_empty()))
}
