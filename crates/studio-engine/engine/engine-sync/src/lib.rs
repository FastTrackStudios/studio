//! The live session document ([`session_sync`]) bridged to a DAW engine.
//!
//! Each replica is the source of truth for its own engine: [`bridge`]
//! reads the engine's project back as a [`SessionModel`] and writes it into
//! the document, and applies what the document gained from others to the
//! engine ([`engine`] through `daw_control`, [`whole`] straight onto a
//! `daw-standalone` project, [`direct`] for the reads the bridge needs).
//!
//! Everything engine-free (the schema, model, diff, relay, presence, the
//! network lanes and the shared clock) is `session-sync`'s, re-exported
//! here so a caller names one crate.

pub use session_sync::*;

pub mod bridge;
pub mod direct;
pub mod engine;
pub mod whole;

pub use bridge::{Bridge, LocalRead, StepWriter};
