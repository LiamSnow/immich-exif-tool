use console::style;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Display};

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FixesFile {
    pub summary: Summary,
    pub assets: BTreeMap<String, AssetState>,
    pub orphans: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub total: u32,
    pub orphaned: u32,
    pub date_time: FieldStats,
    pub gps: FieldStats,
    pub description: FieldStats,
    pub file_extension: FieldStats,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FieldStats {
    pub ok: u32,
    pub fixable: u32,
    pub unfixable: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetState {
    pub date_time: FieldState<String>,
    pub gps: FieldState<(f64, f64)>,
    pub description: FieldState<String>,
    pub file_extension: FieldState<String>,
}

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldState<T> {
    Ok,
    /// Neither side has data
    Unfixable,
    /// Local copy is dirty, missing, or badly format
    Fixable {
        is: Option<T>,
        fix: T,
    },
}

impl AssetState {
    pub fn has_fixes(&self) -> bool {
        matches!(self.date_time, FieldState::Fixable { .. })
            || matches!(self.gps, FieldState::Fixable { .. })
            || matches!(self.description, FieldState::Fixable { .. })
            || matches!(self.file_extension, FieldState::Fixable { .. })
    }
}

impl Summary {
    pub fn add_entry(&mut self, entry: &AssetState) {
        self.date_time.add_entry(&entry.date_time);
        self.gps.add_entry(&entry.gps);
        self.description.add_entry(&entry.description);
        self.file_extension.add_entry(&entry.file_extension);
    }
}

impl FieldStats {
    pub fn add_entry<T>(&mut self, entry: &FieldState<T>) {
        match entry {
            FieldState::Ok => self.ok += 1,
            FieldState::Fixable { .. } => self.fixable += 1,
            FieldState::Unfixable => self.unfixable += 1,
        }
    }
}

impl Display for FieldStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ok, {} fixable, {} unfixable",
            style(self.ok).green(),
            style(self.fixable).yellow(),
            style(self.unfixable).dim(),
        )
    }
}
