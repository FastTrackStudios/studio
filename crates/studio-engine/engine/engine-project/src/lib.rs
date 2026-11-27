//! Projects: the project list, new projects, instrument tracks, songs made
//! ready to travel, a set's streamed songs, a song streamed, a show split,
//! and the set's openers.
//!
//! One of studio-engine's domains, on engine-core (docs/MONOREPO-MIGRATION.md,
//! "Splitting studio-engine"). studio-engine re-exports each module from its
//! old path. `crate::` reaches the core's modules through the glob below;
//! this crate's own modules shadow it.

use architect::action::{ActionError, ActionList};

pub use engine_core::*;

#[cfg(feature = "native")]
pub mod instrument_tracks;
#[cfg(feature = "native")]
pub mod organize_edit;
#[cfg(feature = "native")]
pub mod project_new;
#[cfg(feature = "native")]
pub mod projects;
#[cfg(feature = "native")]
pub mod reference_play;
#[cfg(feature = "native")]
pub mod session_new;
#[cfg(feature = "native")]
mod set_open;
pub mod setlist;
#[cfg(feature = "native")]
pub mod setlist_download;
#[cfg(feature = "show")]
pub mod show;
#[cfg(feature = "native")]
pub mod song_open;
#[cfg(feature = "native")]
pub mod song_prep;
pub mod song_stream;
#[cfg(feature = "native")]
pub mod stream_in;
#[cfg(feature = "native")]
pub mod stream_set;

#[cfg(feature = "native")]
/// The core's `studio_engine`, with the set's openers beside the core's set
/// (`crate::studio_engine::set`, as in studio-engine).
pub mod studio_engine {
    pub use engine_core::studio_engine::*;
    /// The set: the core's, with its openers (`set_open`).
    pub mod set {
        #[cfg(feature = "native")]
        pub use crate::set_open::*;
        pub use engine_core::studio_engine::set::*;
    }
}

/// This domain's actions, into the engine's Action List: new projects,
/// instrument tracks and organizing. What the engine
/// binary's collector (`studio_engine::actions::register_all`) calls.
///
/// # Errors
///
/// The first registration the list refuses ([`ActionError::Duplicate`]).
#[cfg(feature = "native")]
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    session_new::register(list)?;
    setlist_download::register(list)?;
    instrument_tracks::register(list)?;
    organize_edit::register(list)?;
    Ok(())
}

/// No actions off the desktop engine.
///
/// # Errors
///
/// None.
#[cfg(not(feature = "native"))]
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    let _ = list;
    Ok(())
}
