//! The mix: its state, snapshots, variants, chains and sends, the FX and automation actions, the Signal kit, the CLAP host and its FX.
//!
//! One of studio-engine's domains, on engine-core
//! (docs/MONOREPO-MIGRATION.md, "Splitting studio-engine"); studio-engine
//! re-exports each module from its old path. engine-listen reads the mix
//! (the one permitted edge between domains: listen → mix), so the modules
//! it reads are `pub`. The mix itself reads only the core.

// The core's modules under the paths these were written against; the
// modules here shadow the glob.
pub use engine_core::*;

#[cfg(feature = "native")]
pub mod algorithms;
#[cfg(feature = "native")]
pub mod automation_actions;
#[cfg(feature = "signal")]
pub mod bass_rig;
#[cfg(feature = "native")]
pub mod clap_fx;
#[cfg(feature = "native")]
pub mod clap_host;
#[cfg(feature = "native")]
pub mod fx_actions;
#[cfg(feature = "native")]
mod mix_actions;
pub mod mix_chains;
pub mod mix_export;
#[cfg(feature = "native")]
pub mod mix_sends;
#[cfg(feature = "native")]
pub mod mix_snapshot;
#[cfg(feature = "native")]
pub mod mix_state;
#[cfg(feature = "native")]
pub mod mix_variants;
#[cfg(feature = "signal")]
pub mod rig_actions;
#[cfg(feature = "native")]
pub mod signal_kit;

/// A song of known signals, for the mix's tests and the engine's.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod test_song;

#[cfg(feature = "native")]
pub use mix_actions::register;
