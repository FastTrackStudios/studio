//! Projects: ingest, the project list, new projects, instrument tracks,
//! streaming a set in, a song streamed, a show split, and the set's openers.
//!
//! One of studio-engine's domains, on engine-core (docs/MONOREPO-MIGRATION.md,
//! "Splitting studio-engine"). studio-engine re-exports each module from its
//! old path. `crate::` reaches the core's modules through the glob below;
//! this crate's own modules shadow it.

use architect::action::{ActionError, ActionList};

pub use engine_core::*;

#[cfg(feature = "native")]
pub mod ingest;
#[cfg(feature = "native")]
pub mod instrument_tracks;
#[cfg(feature = "native")]
pub mod library_actions;
#[cfg(feature = "native")]
pub mod organize_edit;
#[cfg(feature = "native")]
pub mod project_new;
#[cfg(feature = "native")]
pub mod projects;
#[cfg(feature = "native")]
pub mod reference_play;
#[cfg(feature = "native")]
mod set_open;
pub mod setlist;
#[cfg(feature = "show")]
pub mod show;
#[cfg(feature = "native")]
pub mod song_open;
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

/// This domain's actions, into the engine's Action List: new projects and
/// instrument tracks, the library, organizing, ingest. What the engine
/// binary's collector (`studio_engine::actions::register_all`) calls.
///
/// # Errors
///
/// The first registration the list refuses ([`ActionError::Duplicate`]).
#[cfg(feature = "native")]
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    library_actions::register(list)?;
    project_new::register(list)?;
    instrument_tracks::register(list)?;
    organize_edit::register(list)?;
    ingest::actions::register(list)?;
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
