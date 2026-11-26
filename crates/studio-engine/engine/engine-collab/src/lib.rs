//! Working together: the live session, the agent, Task's account and set, hosting a live set.
//!
//! One of studio-engine's domains, on engine-core
//! (docs/MONOREPO-MIGRATION.md, "Splitting studio-engine"):
//!
//! - [`collab`]: the live session, hosted over iroh or kept by Task, its
//!   presence and the shared transport;
//! - [`agent`]: an agent's edits beside the person's (`AGENT_RUN`,
//!   `SESSION_CHANGES`, …) on the core's journal;
//! - [`live_host`]: what is open hosted in Task by itself, signed in.
//!
//! Task's account and set (`task_account`, `task_set`), the song's journal
//! and what other domains ask of the live session (`collab_calls`) are the
//! core's: other domains read them too. studio-engine re-exports each
//! module from its old path.

use architect::action::{ActionError, ActionList};

pub use engine_core::*;

#[cfg(feature = "native")]
pub mod agent;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab;
#[cfg(feature = "native")]
pub mod live_host;

/// This domain's actions, into the engine's Action List: what the engine
/// binary's collector (`studio_engine::actions::register_all`) calls.
///
/// # Errors
///
/// The first registration the list refuses ([`ActionError::Duplicate`]).
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    #[cfg(feature = "native")]
    agent::register(list)?;
    #[cfg(not(feature = "native"))]
    let _ = list;
    Ok(())
}
