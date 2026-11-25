//! Parser and serializer for REAPER external state persistence files
//! (`reaper-extstate.ini`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl ExtState {
    /// Parse the contents of a `reaper-extstate.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Serialize back to the `.ini` file format.
    pub fn serialize(&self) -> String {
        serialize::serialize(self)
    }

    /// Look up a value by section and key. Returns `None` if either is absent.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|s| s.name == section)
            .and_then(|s| s.entries.iter().find(|e| e.key == key))
            .map(|e| e.value.as_str())
    }

    /// Set (or insert) a value in the given section, creating the section if
    /// it does not already exist.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        if let Some(sec) = self.sections.iter_mut().find(|s| s.name == section) {
            if let Some(entry) = sec.entries.iter_mut().find(|e| e.key == key) {
                entry.value = value.to_string();
            } else {
                sec.entries.push(ExtStateEntry {
                    key: key.to_string(),
                    value: value.to_string(),
                });
            }
        } else {
            self.sections.push(ExtStateSection {
                name: section.to_string(),
                entries: vec![ExtStateEntry {
                    key: key.to_string(),
                    value: value.to_string(),
                }],
            });
        }
    }
}
