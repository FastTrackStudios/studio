//! Parser and serializer for REAPER render preset (`reaper-render.ini`) files.
//!
//! This module can parse and round-trip the four entry types found in REAPER's
//! render preset configuration file:
//!
//! - **`<RENDERPRESET>`** — primary format/quality settings for a named preset
//! - **`<RENDERPRESET2>`** — secondary output format for a named preset
//! - **`RENDERPRESET_OUTPUT`** — output path and bounds settings
//! - **`RENDERPRESET_EXT`** — normalization, fade and silence-trim settings

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl RenderPresetFile {
    /// Parse the contents of a `reaper-render.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of entries in this file.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if this file has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = &RenderPresetEntry> {
        self.entries.iter()
    }

    /// Iterate over `<RENDERPRESET>` entries only.
    pub fn presets(&self) -> impl Iterator<Item = &RenderPreset> {
        self.entries.iter().filter_map(|e| match e {
            RenderPresetEntry::Preset(p) => Some(p),
            _ => None,
        })
    }

    /// Iterate over `<RENDERPRESET2>` entries only.
    pub fn presets2(&self) -> impl Iterator<Item = &RenderPreset2> {
        self.entries.iter().filter_map(|e| match e {
            RenderPresetEntry::Preset2(p) => Some(p),
            _ => None,
        })
    }

    /// Iterate over `RENDERPRESET_OUTPUT` entries only.
    pub fn outputs(&self) -> impl Iterator<Item = &RenderPresetOutput> {
        self.entries.iter().filter_map(|e| match e {
            RenderPresetEntry::Output(o) => Some(o),
            _ => None,
        })
    }

    /// Iterate over `RENDERPRESET_EXT` entries only.
    pub fn exts(&self) -> impl Iterator<Item = &RenderPresetExt> {
        self.entries.iter().filter_map(|e| match e {
            RenderPresetEntry::Ext(e) => Some(e),
            _ => None,
        })
    }
}

// ---------------------------------------------------------------------------
// Integration / round-trip tests
// ---------------------------------------------------------------------------
