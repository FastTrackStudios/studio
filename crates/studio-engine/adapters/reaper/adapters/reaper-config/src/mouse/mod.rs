//! Parser and serializer for REAPER mouse modifier configuration files
//! (`reaper-mouse.ini` and `.ReaperMouseMap`).

mod parse;
mod serialize;
pub mod types;

pub use parse::ParseError;
pub use types::*;

impl MouseConfig {
    /// Parse the contents of a `reaper-mouse.ini` or `.ReaperMouseMap` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of context sections in this config.
    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    /// Return `true` if there are no context sections.
    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }

    /// Iterate over all context sections.
    pub fn iter(&self) -> impl Iterator<Item = &MouseContext> {
        self.contexts.iter()
    }
}
