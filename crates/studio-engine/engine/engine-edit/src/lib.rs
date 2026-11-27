//! Editing a song's data: the hits and their MIDI, placing them, drum editing, comping, takes, edits, growing a song, the fundamental, the vocal leveler.
//!
//! One of studio-engine's domains, on engine-core. Empty until its modules
//! move here from studio-engine (docs/MONOREPO-MIGRATION.md, "Splitting
//! studio-engine", the handoff); studio-engine re-exports each from its old
//! path.

// The core, under the paths the modules were written against (its own
// modules and the window's shared half); this crate's modules shadow it.
pub use engine_core::*;

#[cfg(feature = "native")]
pub mod actions;

#[cfg(feature = "native")]
pub mod comp;
#[cfg(feature = "native")]
pub mod comp_keep;
#[cfg(feature = "native")]
pub mod comp_rank;
#[cfg(feature = "native")]
pub mod comp_song;
#[cfg(feature = "native")]
pub mod drum_apply;
#[cfg(feature = "native")]
pub mod drum_bench;
#[cfg(feature = "native")]
pub mod drum_group;
#[cfg(feature = "native")]
pub mod drum_lanes;
pub mod drum_level;
#[cfg(feature = "native")]
pub mod drum_map_set;
#[cfg(feature = "native")]
pub mod drum_plan;
#[cfg(feature = "signal")]
pub mod drum_samples;
#[cfg(feature = "native")]
pub mod edit_actions;
#[cfg(feature = "native")]
pub mod edit_list;
#[cfg(feature = "native")]
pub mod edit_reads;
#[cfg(feature = "native")]
pub mod edit_revert;
#[cfg(feature = "native")]
pub mod grow;
pub mod hit_flags;
#[cfg(feature = "native")]
pub mod hit_ops;
#[cfg(feature = "native")]
pub mod hit_place;
#[cfg(feature = "native")]
pub mod hits;
#[cfg(feature = "native")]
pub mod hits_midi;
#[cfg(feature = "native")]
pub mod item_actions;
#[cfg(feature = "native")]
pub mod key_change;
#[cfg(feature = "native")]
pub mod sample_align;
#[cfg(feature = "native")]
pub mod seams;
#[cfg(feature = "native")]
pub mod take_rank;
pub mod track_regions;
#[cfg(feature = "native")]
pub mod trigger_from_mics;
#[cfg(feature = "native")]
pub mod vocal_level;

use architect::action::{ActionError, ActionList};

/// This domain's actions, into the engine's Action List: what the engine
/// binary's collector (`studio_engine::actions::register_all`) calls.
///
/// # Errors
///
/// The first registration the list refuses ([`ActionError::Duplicate`]).
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    // The desktop engine's: none off it.
    #[cfg(feature = "native")]
    actions::edit::register(list)?;
    #[cfg(not(feature = "native"))]
    let _ = list;
    Ok(())
}
