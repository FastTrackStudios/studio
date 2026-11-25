//! Daslight 5 / myDMX 5 showfiles (`.dvc`): read them, and write new ones.
//!
//! A `.dvc` is plain XML with its bulk payloads zlib-packed into attributes
//! ([`container`]). This crate reads it into an owned tree ([`xml`]), models
//! the parts Ignition edits (the [`patch`] and its embedded [`profile`]s, the
//! fixture groups) and writes everything else back as the desk wrote it.
//!
//! Scenes are written ([`scene`]) but not read back into values yet; a
//! repatched showfile starts from empty banks
//! ([`show::Showfile::clear_programming`]) and new ones are added with
//! [`show::Showfile::add_bank`]. [`cook`] turns DMX recorded off another
//! controller into Steps scenes; [`fx`] writes the desk's own live effects.

pub mod container;
pub mod cook;
pub mod easyview;
pub mod fixups;
pub mod fx;
pub mod library;
pub mod patch;
pub mod profile;
pub mod scene;
pub mod show;
pub mod timeline;
pub mod xml;

pub use patch::{Fixture, Patch};
pub use scene::{Scene, Step};
pub use show::Showfile;

/// Everything that can go wrong reading or writing a showfile.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("XML: {0}")]
    Xml(String),
    #[error("packed payload: {0}")]
    Container(&'static str),
    #[error("patch: {0}")]
    Patch(String),
    #[error("{0}: {1}")]
    Io(String, #[source] std::io::Error),
}
