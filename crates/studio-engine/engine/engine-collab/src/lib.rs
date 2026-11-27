//! Working together: the live session, the agent and the room.
//!
//! One of studio-engine's domains, on engine-core
//! (docs/MONOREPO-MIGRATION.md, "Splitting studio-engine"):
//!
//! - [`collab`]: the live session, hosted over iroh, its presence and the
//!   shared transport;
//! - [`agent`]: an agent's edits beside the person's (`AGENT_RUN`,
//!   `SESSION_CHANGES`, …) on the core's journal;
//! - [`room`]: an organization's room, led or followed (`ROOM_*`).
//!
//! The song's journal and what other domains ask of the live session
//! (`collab_calls`) are the core's: other domains read them too.
//! studio-engine re-exports each module from its old path.

use architect::action::{ActionError, ActionList};

pub use engine_core::*;

#[cfg(feature = "native")]
pub mod agent;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab;
#[cfg(feature = "native")]
pub mod room;

/// This domain's actions, into the engine's Action List: what the engine
/// binary's collector (`studio_engine::actions::register_all`) calls.
///
/// # Errors
///
/// The first registration the list refuses ([`ActionError::Duplicate`]).
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    #[cfg(feature = "native")]
    agent::register(list)?;
    #[cfg(feature = "native")]
    room::register(list)?;
    #[cfg(not(feature = "native"))]
    let _ = list;
    Ok(())
}
