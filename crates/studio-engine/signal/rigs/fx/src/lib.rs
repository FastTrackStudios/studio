//! signal's rigs as the session engine's built-in FX.
//!
//! signal runs its rigs as small daw projects whose effect slots hold
//! [`PluginInstance`]s — the same trait the session engine hosts. So a
//! rig's pieces can run in a session's own tracks, in-process, with no
//! second engine and no socket:
//!
//! - **`FTS Bass Rig`** ([`bass::BassRig`]): signal's bass chain — a NAM
//!   amp (or preamp) capture, an optional drive capture before it, an
//!   optional cab IR after — on a bass DI.
//! - **`FTS Kit Mic`** ([`kit::KitMic`]): the drum sampler's voices for
//!   one set of a kit's pieces and mics, played from the track's own MIDI
//!   — a piece's close mics on its Sample track, every piece's overhead or
//!   room mics on the kit's sample overhead or room track. The same notes
//!   reach each, so their round robins stay together, as in signal's own
//!   per-track kit.
//!
//! - **`FTS Signal Rig`** ([`signal_rig::SignalRig`]): signal's guitar or
//!   bass rig, whole — its profiles, stacks and patches — its service kept
//!   for the window and the actions to drive.
//!
//! Each keeps what it plays in its state (JSON): which model, which kit
//! preset, which pieces and mics. That is what a session saves, and what
//! prepare writes when it builds the rig.
//!
//! **In a browser** all three run in the page's engine, their files
//! streamed in from the rig store ([`files`]): the rigs' captures, IRs and
//! homes, the kit's preset, engines and the samples it plays, read out of
//! their packs by range. There the whole rig is its profile built in the
//! page (`signal_rig_page.rs`), with no rig service and no thread.

pub mod bass;
pub mod files;
pub mod kit;
pub mod profile;
pub mod rig;
#[cfg_attr(target_arch = "wasm32", path = "signal_rig_page.rs")]
pub mod signal_rig;
/// The page's whole rig, built natively too so its tests run on a desktop.
#[cfg(all(test, not(target_arch = "wasm32")))]
#[allow(dead_code, reason = "natively only its tests use it")]
mod signal_rig_page;

/// signal's rig crate, for what drives a rig from outside: its service's
/// calls, its homes and instruments.
pub use guitar as signal_guitar;

use daw_standalone::plugin::{FxFactory, PluginInstance};

/// The bass rig's name.
pub const BASS_RIG: &str = "FTS Bass Rig";
/// The kit voice's name.
pub const KIT_MIC: &str = "FTS Kit Mic";
/// signal's guitar or bass rig, whole ([`signal_rig::SignalRig`]).
pub const SIGNAL_RIG: &str = "FTS Signal Rig";

/// signal's FX, by name.
#[derive(Clone, Copy, Debug, Default)]
pub struct SignalFx;

/// The FX this build runs: every one, in a page too.
const RUNS: [&str; 3] = [BASS_RIG, KIT_MIC, SIGNAL_RIG];

impl SignalFx {
    fn bare(name: &str) -> Option<&'static str> {
        let name = name.trim();
        RUNS.into_iter()
            .find(|known| name.eq_ignore_ascii_case(known))
    }
}

impl FxFactory for SignalFx {
    fn installed(&self) -> Vec<daw_proto::fx::InstalledFx> {
        RUNS.into_iter()
            .map(|name| daw_proto::fx::InstalledFx {
                name: name.to_owned(),
                ident: name.to_owned(),
            })
            .collect()
    }

    fn create(&self, name_or_ident: &str, sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
        match Self::bare(name_or_ident)? {
            BASS_RIG => Some(Box::new(rig::Rig::new(sample_rate))),
            KIT_MIC => Some(Box::new(kit::KitMic::new(sample_rate))),
            SIGNAL_RIG => Some(Box::new(signal_rig::SignalRig::new(sample_rate))),
            _ => None,
        }
    }

    fn provides(&self, name_or_ident: &str) -> bool {
        Self::bare(name_or_ident).is_some()
    }
}
