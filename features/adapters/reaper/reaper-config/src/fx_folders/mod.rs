//! Parser and serializer for REAPER FX browser folder files
//! (`reaper-fxfolders.ini`).

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::{FxFolder, FxFolderItem, FxFolders, PluginType};

impl FxFolders {
    /// Parse the contents of a `reaper-fxfolders.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of folders.
    pub fn len(&self) -> usize {
        self.folders.len()
    }

    /// Return `true` if there are no folders.
    pub fn is_empty(&self) -> bool {
        self.folders.is_empty()
    }

    /// Iterate over all folders.
    pub fn iter(&self) -> impl Iterator<Item = &FxFolder> {
        self.folders.iter()
    }

    /// Look up a folder by display name (case-sensitive, returns the first match).
    pub fn folder_by_name(&self, name: &str) -> Option<&FxFolder> {
        self.folders.iter().find(|f| f.name == name)
    }
}
