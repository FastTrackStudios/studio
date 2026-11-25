//! Parser and serializer for REAPER VST/DX plugin cache files
//! (`reaper-vstplugins*.ini`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl VstPluginsCache {
    /// Parse the contents of a `reaper-vstplugins*.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Serialize back to the `.ini` file format.
    pub fn serialize(&self) -> String {
        serialize::serialize(self)
    }
}
