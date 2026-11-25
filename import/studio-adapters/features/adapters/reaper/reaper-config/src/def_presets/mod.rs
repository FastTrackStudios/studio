//! Parser and serializer for REAPER default preset assignment files
//! (`reaper-defpresets.ini`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl DefPresets {
    /// Parse the contents of a `reaper-defpresets.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Serialize back to the `.ini` file format.
    pub fn serialize(&self) -> String {
        serialize::serialize(self)
    }

    /// Look up the default preset name for a plugin identity string.
    pub fn get(&self, plugin_id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.plugin_id == plugin_id)
            .map(|e| e.preset_name.as_str())
    }

    /// Set (or insert) the default preset for a plugin identity string.
    pub fn set(&mut self, plugin_id: &str, preset_name: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.plugin_id == plugin_id) {
            entry.preset_name = preset_name.to_string();
        } else {
            self.entries.push(DefPresetEntry {
                plugin_id: plugin_id.to_string(),
                preset_name: preset_name.to_string(),
            });
        }
    }
}
