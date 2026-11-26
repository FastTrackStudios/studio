//! What the core asks of the engine around it.
//!
//! The core is the bottom of the engine: the domains (listen, edit, mix,
//! project, collab) depend on it, never it on them. Where opening, a step
//! or preparing a song has a domain's part to play (the CLAP host offered
//! the engine's actions, a step's hits followed by the kit's MIDI, the mix
//! chains a template adds), the core calls it through [`Assembly`]: a table
//! the engine binary's crate (studio-engine) fills once, at
//! `studio_engine::install`, before the engine opens anything.
//!
//! With none installed (a core test, a tool with the core alone) each call
//! does what a core with no domains does: nothing, and says so once.

use std::sync::{Arc, OnceLock};

use daw::service::ProjectContext;
use daw::standalone::Standalone;
use daw::standalone::audio_engine::EngineStats;

/// How a step's hits and the kit's MIDI are brought together after it
/// (the edit domain's `hits_midi::Mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepSync {
    /// An edit step: a view edited since it was written is the hits' edit,
    /// carried over to them first.
    Reconcile,
    /// An undo or a redo, or an edit about to be made: the views written
    /// from the hits.
    Follow,
}

/// The engine's parts the core calls. Each is a plain function, so the
/// table is a `static` in the crate that fills it.
pub struct Assembly {
    /// The engine's body, the window's host and the Action List, installed
    /// in this process (`studio_engine::install`). Idempotent.
    pub install: fn(),
    /// The Action List served over MCP (`studio_engine::mcp::start`).
    pub serve_mcp: fn(),
    /// Every CLAP the engine loads offered the engine's actions
    /// (`clap_host::install`).
    pub clap_host: fn(&'static Standalone),
    /// The engine's view service, as a router (`view_host::router`).
    pub view_router: fn(&Standalone) -> daw::LayerRouter,
    /// The set's audio device attached: its health and its meter watched
    /// (`audio_health::watch`, `perf_meter::watch`).
    pub audio_attached: fn(&Standalone, Arc<EngineStats>, u32),
    /// A step finished, labelled `label`: a CLAP's subscriptions and the
    /// agent's revision follow it (`clap_host`, `agent::step_finished`).
    pub step_finished: fn(&str),
    /// The kit's MIDI and the hits brought together after a step
    /// (`actions::samples_follow`).
    pub samples_follow: fn(&Standalone, &ProjectContext, StepSync),
    /// The mix's processing a prepared song gets (`mix_chains::add`).
    pub mix_chains: fn(&daw_standalone::sync::Standalone, &ProjectContext) -> eyre::Result<()>,
    /// A prepared song's stereo reference filed, and the song heard by it
    /// when that is how it came (`instrument_tracks`).
    pub reference: fn(&daw_standalone::sync::Standalone, &ProjectContext) -> eyre::Result<()>,
    /// Song `guid` saved as it now is (`projects::mark_saved`).
    pub mark_saved: fn(&Standalone, &str),
    // A domain that the core (or another domain, through the core) must call
    // adds its fields under its own heading.
    // ── engine-listen's parts ──
    // ── engine-edit's parts ──
    // ── engine-mix's parts ──
    // ── engine-project's parts ──
    // ── engine-collab's parts ──
}

static ASSEMBLY: OnceLock<&'static Assembly> = OnceLock::new();

/// Install the engine's parts: once, by studio-engine, as the engine comes
/// up. A second call is ignored.
pub fn install(assembly: &'static Assembly) {
    let _ = ASSEMBLY.set(assembly);
}

/// The engine's parts, or `None` with none installed (said once).
#[must_use]
pub fn get() -> Option<&'static Assembly> {
    let got = ASSEMBLY.get().copied();
    if got.is_none() {
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| {
            tracing::warn!(
                "engine-core: no engine installed around the core; its domains' parts do nothing"
            );
        });
    }
    got
}
