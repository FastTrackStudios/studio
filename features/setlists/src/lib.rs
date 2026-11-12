//! Song libraries and setlists: an Architect extension, and the tools
//! that bring songs into a library.
//!
//! **The model**, as notes of an Architect vault ([`layout`]):
//! - a **library** (`Libraries/<Library>/`) is a collection of songs,
//!   grouped by category, kept whether or not anyone is working on them;
//! - a **song** (`Libraries/<Library>/<Song>/<Song>.md`, `type: song`)
//!   keeps everything of its own in its folder: its chart and engraved
//!   pages and PDF, LRC, session, media, and arrangements (each with a
//!   chart of its own); `categories` names its categories ([`song`]);
//! - a **setlist** (`Setlists/<Set>.md`, `type: setlist`) is songs from
//!   any library in running order, an arrangement included
//!   (`Song#Acoustic`; [`setlist`]), shared by an Architect share link.
//!
//! **Architect does the rest.** The extension ([`ext`], feature
//! `extension`) fills the navigator's two-stage sidebar ([`nav`]):
//! setlists, or libraries and their categories, on the left; their songs
//! on the right; each song a scope tab in its "Song" screenset. It
//! reaches notes and share links only through the app's actions
//! ([`host`]), so the same code is compiled into an Architect build
//! (Studio's app, a deployment's own web build) or installed into the
//! Architect app as a process. Bases group a library by category; the
//! shell brings sign-in, orgs, ⌘K and settings.
//!
//! **What only Studio does**: engraving a Keyflow chart, with its lyric
//! lane, into its pages and PDF ([`chart`], feature `engrave`), and
//! bringing a song's files from outside the vault into its folder
//! ([`vault`], `studio setlist import`; [`cli`], feature `cli`).
//!
//! **The share page** (layer 0 of a share link: the page, no engine) is
//! Architect's site of the setlist's notes, with Studio's blocks drawn
//! into it ([`share`], feature `share`): each song's chart as an SVG whose
//! bars follow the song's recording ([`follow`]), its lyrics as timed
//! lines (its LRC, else its chart's lyric lane: one source a song), each
//! set's songs as rows that play, the link's queue playing the setlist
//! through.

pub mod frontmatter;
pub mod layout;
pub mod lyrics;
pub mod media;
pub mod setlist;
pub mod song;
pub mod vault;

#[cfg(feature = "engrave")]
pub mod chart;
#[cfg(feature = "engrave")]
pub mod follow;
#[cfg(feature = "share")]
pub mod share;
#[cfg(feature = "workspace")]
pub mod workspace;

#[cfg(feature = "extension")]
pub mod ext;
#[cfg(feature = "extension")]
pub mod host;
#[cfg(feature = "extension")]
pub mod nav;
#[cfg(feature = "extension")]
pub mod workspace_view;

#[cfg(feature = "cli")]
pub mod cli;

pub use layout::{Library, SongRef};
pub use setlist::Setlist;
pub use song::Song;

/// Why an operation failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{0}: not a {1} note")]
    NotA(String, &'static str),
    #[error("no song `{0}`")]
    NoSong(String),
    #[error("`{0}` is not a usable name (empty, or has `/`, `\\`, `[`, `]`, `|`, `#`, `^` or `:`)")]
    BadName(String),
    #[error("position {at} is outside a setlist of {len} songs")]
    Position { at: usize, len: usize },
    #[error("the chart did not engrave: {0}")]
    Engrave(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A name usable as a file and a wikilink target.
///
/// # Errors
///
/// [`Error::BadName`] when it is empty or has a character a path or a
/// wikilink would read as structure.
pub fn check_name(name: &str) -> Result<&str> {
    let name = name.trim();
    let bad = name.is_empty()
        || name.starts_with('.')
        || name
            .chars()
            .any(|c| matches!(c, '/' | '\\' | '[' | ']' | '|' | '#' | '^' | ':'));
    if bad {
        Err(Error::BadName(name.to_owned()))
    } else {
        Ok(name)
    }
}
