//! Parser and serializer for REAPER plugin preset files.

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl PluginPresets {
    /// Parse the contents of a plugin preset `.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Serialize back to the `.ini` file format.
    pub fn serialize(&self) -> String {
        serialize::serialize(self)
    }
}
