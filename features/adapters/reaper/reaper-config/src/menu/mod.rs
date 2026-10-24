//! Parser and serializer for REAPER menu and toolbar customization files
//! (`reaper-menu.ini`, `.ReaperMenu`, `.ReaperMenuSet`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl MenuConfig {
    /// Parse the contents of a `reaper-menu.ini`, `.ReaperMenu`, or
    /// `.ReaperMenuSet` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of sections in this configuration.
    pub fn len(&self) -> usize {
        self.sections.len()
    }

    /// Return `true` if this configuration contains no sections.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// Iterate over all sections.
    pub fn iter(&self) -> impl Iterator<Item = &MenuSection> {
        self.sections.iter()
    }

    /// Find the first section with the given [`MenuSectionId`].
    pub fn section(&self, id: &MenuSectionId) -> Option<&MenuSection> {
        self.sections.iter().find(|s| &s.id == id)
    }

    /// Find the first section with the given [`MenuSectionId`], mutably.
    pub fn section_mut(&mut self, id: &MenuSectionId) -> Option<&mut MenuSection> {
        self.sections.iter_mut().find(|s| &s.id == id)
    }
}
