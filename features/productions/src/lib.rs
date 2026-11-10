//! Productions in other DAWs: an Architect extension that keeps track of
//! the Ableton sets, Pro Tools sessions, REAPER projects and Logic
//! projects a studio has, as project notes with a stage (studio#396).
//!
//! **The model**, as notes of an Architect vault:
//! - a **production** is a project of Architect's built-in `project` kind
//!   (`Projects/<Name>/project.md`, `type: project`) of one of this
//!   extension's project types ([`model::Daw`]: `ableton-set`,
//!   `protools-session`, `reaper-session`, `logic-project`), with a
//!   [`model::Stage`], a key, a tempo, a client and a due date, and what
//!   its adapter reads from the file ([`read`]);
//! - an **Ableton show set** (its locators mark songs) also gets a
//!   setlist note in the shape `setlists` keeps, its songs in a library
//!   named for the set ([`show`]).
//!
//! **Found, not filed.** [`find`] walks a folder for project files and
//! project folders (an Ableton Project folder or a Pro Tools session
//! folder is one production); the scan action
//! (`STUDIO_PRODUCTIONS_SCAN`, [`ext`]) makes or updates one note each.
//! It stands in for Architect's ingest rules (architect#505), which the
//! pinned Architect does not have. A re-scan rewrites what was read and
//! leaves what a person set.
//!
//! **Read-only to the foreign file.** Nothing here calls an adapter's
//! writer. Notes are reached only through the app's `VAULT_*` and
//! `BASES_*` actions ([`host`]), so the copy compiled into an Architect
//! build and the one the Architect app runs as a process do the same.

pub mod bases;
pub mod find;
pub mod model;
pub mod note;
pub mod open;
pub mod read;
pub mod show;

#[cfg(feature = "extension")]
pub mod ext;
#[cfg(feature = "extension")]
pub mod host;

#[cfg(feature = "cli")]
pub mod cli;

pub use find::{Found, find};
pub use model::{Daw, Stage};
pub use read::{Facts, read};
