//! # reaper-kbd
//!
//! Parser and types for REAPER keyboard mapping (`reaper-kb.ini`) files.
//!
//! This crate can parse the three line types found in REAPER's keyboard
//! configuration files:
//!
//! - **KEY** -- keyboard and MIDI key bindings (with optional global scope)
//! - **ACT** -- custom/composite actions
//! - **SCR** -- script bindings
//!
//! # Example
//!
//! ```
//! use reaper_kbd::KeyMap;
//!
//! let input = "KEY 1 65 40001 0\r\n";
//! let keymap = KeyMap::parse(input).unwrap();
//! let output = keymap.serialize();
//! assert_eq!(input, output);
//! ```

mod parse;
mod serialize;
mod types;

pub use parse::ParseError;
pub use types::*;

impl KeyMap {
    /// Parse the contents of a `reaper-kb.ini` file.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Return the number of entries in this key map.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true if this key map has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// Iterate over only KEY entries.
    pub fn key_bindings(&self) -> impl Iterator<Item = &KeyBinding> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Key(kb) => Some(kb),
            _ => None,
        })
    }

    /// Iterate over only ACT entries.
    pub fn custom_actions(&self) -> impl Iterator<Item = &CustomAction> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Action(a) => Some(a),
            _ => None,
        })
    }

    /// Iterate over only SCR entries.
    pub fn script_bindings(&self) -> impl Iterator<Item = &ScriptBinding> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Script(s) => Some(s),
            _ => None,
        })
    }
}
