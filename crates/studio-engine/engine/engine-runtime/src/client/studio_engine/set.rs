//! The set as a window shows it: what it is (`SetView`), whether one is
//! opening, and what a window asks the engine to open from a library. The
//! set itself — installing, picking, opening — is the engine's
//! (`studio_engine::studio_engine::set`).

use crate::client::open_progress::Progress;
use crate::client::setlist::Setlist;

/// Where opening a set has got to.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Opening {
    /// Nothing is being opened.
    #[default]
    Idle,
    /// Opening: where it has got to.
    Busy(Progress),
    /// The last open failed, and why, for a person.
    Failed(String),
}

/// The set as the engine has it, as published.
#[derive(Clone, Default)]
pub struct SetView {
    /// Bumped by every change: a reader that saw this revision has seen
    /// everything up to it.
    pub revision: u64,
    /// Which set this is: bumped when one is installed or closed. Songs
    /// arriving for an older set are dropped, and a window remounts its
    /// views when it changes.
    pub epoch: u64,
    /// The set, when one is open.
    pub setlist: Option<Setlist>,
    /// An open under way, or how the last one went.
    pub opening: Opening,
}

/// A setlist of an org's Task library to open: its title, the org whose
/// library holds it, and one of its songs to open alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryOpen {
    /// The org whose library holds it; `FTS_TASK_ORG`, else the org last
    /// picked here, when `None`.
    pub org: Option<String>,
    /// Its title, as the library lists it.
    pub setlist: String,
    /// One of its songs (slug or title) to open alone; the whole set when
    /// `None`.
    pub song: Option<String>,
}
