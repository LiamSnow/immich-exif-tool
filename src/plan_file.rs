use comfy_table::{
    Attribute, Cell, CellAlignment, Color, Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    ops::AddAssign,
};

use crate::exif::GPS;

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub summary: Summary,
    pub assets: BTreeMap<String, AssetState>,
    pub orphans: BTreeSet<String>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// \# assets local
    pub total: u32,
    /// \# assets local & immich
    pub linked: u32,
    /// \# assets only local
    pub orphaned: u32,
    pub date_time: FieldStats,
    pub gps: FieldStats,
    pub description: FieldStats,
    pub file_extension: FieldStats,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
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
    pub gps: FieldState<GPS>,
    pub description: FieldState<String>,
    pub file_extension: FieldState<String>,
}

#[derive(PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "status")]
pub enum FieldState<T> {
    Good,
    Unfixable(Reason<T>),
    Fixable(Fix<T>),
}

#[derive(PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fix<T> {
    AddImmich(T),
    ReplaceWithImmich { old: T, new: T },
    Repair { old: T, new: T },
}

#[derive(PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason<T> {
    NoSources,
    Incomplete(T),
    Corrupted(T),
}

impl AssetState {
    pub fn is_perfect(&self) -> bool {
        matches!(self.date_time, FieldState::Good)
            && matches!(self.gps, FieldState::Good)
            && matches!(self.description, FieldState::Good)
            && matches!(self.file_extension, FieldState::Good)
    }
}

impl<T> AddAssign<&FieldState<T>> for FieldStats {
    fn add_assign(&mut self, rhs: &FieldState<T>) {
        match rhs {
            FieldState::Good => self.ok += 1,
            FieldState::Fixable { .. } => self.fixable += 1,
            FieldState::Unfixable { .. } => self.unfixable += 1,
        };
    }
}

impl AddAssign for FieldStats {
    fn add_assign(&mut self, rhs: Self) {
        self.ok += rhs.ok;
        self.fixable += rhs.fixable;
        self.unfixable += rhs.unfixable;
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new(""),
                Cell::new("OK").fg(Color::Green),
                Cell::new("Fixable").fg(Color::Yellow),
                Cell::new("Unfixable").add_attribute(Attribute::Dim),
            ]);

        for col in 1..=3 {
            table
                .column_mut(col)
                .unwrap()
                .set_cell_alignment(CellAlignment::Right);
        }

        for (name, stats) in [
            ("Date Time", &self.date_time),
            ("GPS", &self.gps),
            ("Description", &self.description),
            ("File Extension", &self.file_extension),
        ] {
            table.add_row(vec![
                Cell::new(name),
                Cell::new(stats.ok).fg(Color::Green),
                Cell::new(stats.fixable).fg(Color::Yellow),
                Cell::new(stats.unfixable).add_attribute(Attribute::Dim),
            ]);
        }

        writeln!(f, "{table}")
    }
}
