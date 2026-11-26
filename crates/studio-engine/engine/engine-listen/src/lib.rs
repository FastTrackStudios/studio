//! Listening and measuring: listen, mix_measure, the render worker,
//! Signal's offline blocks, the audio's health and the performance meter.
//!
//! One of studio-engine's domains, on engine-core (and engine-mix, whose
//! FX, sends and Signal instances measuring reads: the one permitted edge
//! between domains). studio-engine re-exports each module from its old
//! path (docs/MONOREPO-MIGRATION.md, "Splitting studio-engine").

use architect::action::{ActionError, ActionList};

pub use engine_core::*;

/// The mix's modules this domain reads, under the paths it was written
/// against.
#[cfg(feature = "native")]
pub(crate) use engine_mix::{clap_fx, clap_host, mix_sends, mix_snapshot, signal_kit};

#[cfg(feature = "native")]
pub mod audio_health;
#[cfg(feature = "native")]
pub mod listen;
#[cfg(feature = "native")]
pub mod mix_measure;
#[cfg(feature = "native")]
pub mod perf_meter;
#[cfg(feature = "native")]
pub mod render_worker;
#[cfg(feature = "native")]
pub mod signal_offline;

/// This domain's actions, into the engine's Action List: what the engine
/// binary's collector (`studio_engine::actions::register_all`) calls.
/// Listening and measuring go through the render worker (an agent's
/// renders run apart from the person's session); Signal's offline blocks
/// and the audio's health are the engine's own.
///
/// # Errors
///
/// The first registration the list refuses ([`ActionError::Duplicate`]).
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    #[cfg(feature = "native")]
    {
        signal_offline::register(list)?;
        render_worker::register_routed(list, mix_measure::register)?;
        render_worker::register_routed(list, listen::register)?;
        render_worker::register(list)?;
        audio_health::register(list)?;
    }
    #[cfg(not(feature = "native"))]
    let _ = list;
    Ok(())
}
