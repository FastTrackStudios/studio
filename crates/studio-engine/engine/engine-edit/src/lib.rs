//! Editing a song's data: the hits and their MIDI, placing them, drum editing, comping, takes, edits, growing a song, the fundamental, the vocal leveler.
//!
//! One of studio-engine's domains, on engine-core. Empty until its modules
//! move here from studio-engine (docs/MONOREPO-MIGRATION.md, "Splitting
//! studio-engine", the handoff); studio-engine re-exports each from its old
//! path.

use architect::action::{ActionError, ActionList};

/// This domain's actions, into the engine's Action List: what the engine
/// binary's collector (`studio_engine::actions::register_all`) calls.
///
/// # Errors
///
/// The first registration the list refuses ([`ActionError::Duplicate`]).
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    let _ = list;
    Ok(())
}
