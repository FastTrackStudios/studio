//! Session's engine: everything that runs whether or not a window is open —
//! the set and its songs (`studio_engine`), the Action List (`actions`),
//! opening and saving (`open`, `open_core`, `project_file`), streaming
//! (`stream_set`, `song_stream`), collaboration (`collab`), the song's data
//! and its edits (hits, comps, fx, automation, timeline), and the engine
//! with no window (`studio_engine::headless`, what `studio engine` runs).
//!
//! Nothing here draws: the window is `session-ui`, which attaches to this
//! engine (`window_queue::attach`, `actions::list`) and reads what it
//! publishes. See docs/design/engine-boundary.md.

#[cfg(feature = "native")]
pub mod audio_health;
pub mod audio_mode;
pub mod automation;
#[cfg(feature = "native")]
pub mod automation_actions;
pub mod changes;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab;
#[cfg(feature = "native")]
pub mod comp;
#[cfg(feature = "native")]
pub mod comp_rank;
#[cfg(feature = "native")]
pub mod drum_group;
pub mod edit_state;
pub mod engine;
pub mod folder;
pub mod folds;
#[cfg(feature = "native")]
pub mod fx_actions;
#[cfg(feature = "native")]
pub mod grow;
pub mod guide_instrument;
#[cfg(feature = "native")]
pub mod instrument_tracks;
pub mod jobs;
pub mod lanes;
pub mod leveler;
#[cfg(feature = "native")]
pub mod live_host;
pub mod midi;
pub mod num;
#[cfg(feature = "native")]
pub mod open;
pub mod open_core;
pub mod open_progress;
pub mod options;
#[cfg(feature = "native")]
pub mod organize_edit;
pub mod play_return;
pub mod presence;
#[cfg(feature = "native")]
pub mod project_file;
#[cfg(feature = "native")]
pub mod project_new;
#[cfg(feature = "native")]
pub mod projects;
#[cfg(feature = "signal")]
pub mod rig_actions;
#[cfg(feature = "show")]
pub mod show;
pub mod slices;
pub mod song_modes;
pub mod song_stream;
#[cfg(feature = "native")]
pub mod stream_in;
#[cfg(feature = "native")]
pub mod stream_set;
// The engine side, behind its own boundary: the set, its actions, the
// Action List's MCP server, and the engine with no window (`studio engine`).
pub mod stream_quality;
#[cfg(feature = "native")]
pub mod studio_engine;
pub mod take_colors;
#[cfg(feature = "native")]
pub mod task_account;
#[cfg(any(feature = "native", feature = "web"))]
pub mod task_set;
pub mod track_regions;
#[cfg(feature = "native")]
pub mod vocal_level;
#[cfg(feature = "native")]
pub mod wave_pool;
/// On the web there is no in-process tokio runtime: the engine clients
/// (`engine::Transport`, `Applier`, `Meters`, …) find none and run
/// read-only, the way they do natively before a session is open. The web
/// host drives the engine its own way (`web_host`).
#[cfg(not(feature = "native"))]
pub mod open {
    pub use crate::open_core::*;

    /// Never constructed: [`runtime`] has none to give.
    pub struct Runtime(());

    impl Runtime {
        pub fn block_on<F: core::future::Future>(&self, _future: F) -> F::Output {
            unreachable!("no runtime is ever handed out on the web")
        }
    }

    #[must_use]
    pub fn runtime() -> Option<&'static Runtime> {
        None
    }

    /// The song on screen: a page shows one ([`set_current_song`]).
    #[must_use]
    pub fn current_song() -> Option<String> {
        CURRENT.read().ok().and_then(|slot| slot.clone())
    }

    static CURRENT: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

    /// The page shows `project` now.
    pub fn set_current_song(project: &str) {
        if let Ok(mut slot) = CURRENT.write() {
            *slot = Some(project.to_owned());
        }
    }

    /// Make `project_guid` the song on screen — what picking a setlist tab
    /// does; on a page, [`crate::web_engine::switch_song`].
    pub fn switch_song(project_guid: &str) {
        #[cfg(feature = "web")]
        crate::web_engine::switch_song(project_guid);
        #[cfg(not(feature = "web"))]
        set_current_song(project_guid);
    }

    /// What needs the local engine's own handle (as natively).
    #[derive(Clone, Copy, Debug)]
    pub enum LocalOnly {
        ChartRebuild,
        SessionSave,
        /// daw-transport-sync's per-buffer backend: a page's audio loop has
        /// none yet, so it follows a shared transport by the coarse
        /// commands (a play, a seek) rather than to the sample.
        TransportSync,
    }

    /// The local engine's handle, for what needs it — none on a page yet.
    pub fn with_local_engine<R>(
        _what: LocalOnly,
        _f: impl FnOnce(&daw::standalone::Standalone) -> R,
    ) -> Option<R> {
        None
    }
}
#[cfg(feature = "native")]
pub mod actions;
#[cfg(feature = "signal")]
pub mod bass_rig;
pub mod drum_edit;
#[cfg(feature = "native")]
pub mod drum_lanes;
pub mod drum_level;
pub mod drum_reference;
#[cfg(feature = "signal")]
pub mod drum_samples;
pub mod drum_settings;
#[cfg(feature = "native")]
pub mod edit_actions;
pub mod fundamental;
pub mod gain_stage;
pub use editing::groove;
#[cfg(feature = "native")]
pub mod audio_window;
#[cfg(feature = "native")]
pub mod comp_keep;
#[cfg(feature = "native")]
pub mod comp_song;
#[cfg(feature = "native")]
pub mod hit_index;
pub mod hit_overlay;
#[cfg(feature = "native")]
pub mod hit_place;
#[cfg(feature = "native")]
pub mod hits;
#[cfg(feature = "native")]
pub mod hits_midi;
#[cfg(feature = "native")]
pub mod item_actions;
pub mod kit_gate;
pub mod mix_chains;
pub mod mix_export;
#[cfg(feature = "native")]
pub mod perf;
pub mod plan;
pub mod poi;
pub mod prepare;
#[cfg(feature = "native")]
pub mod record;
pub mod reference;
#[cfg(feature = "native")]
pub mod reference_play;
#[cfg(feature = "native")]
pub mod render_ahead;
pub mod routes;
#[cfg(feature = "native")]
pub mod sample_align;
#[cfg(feature = "native")]
pub mod seams;
pub mod section_colors;
#[cfg(feature = "native")]
pub mod session_file;
pub mod setlist;
pub mod settings;
#[cfg(feature = "native")]
pub mod song;
pub mod song_bars;
pub mod song_chart;
pub mod song_model;
pub mod stages;
pub mod stretch;
#[cfg(feature = "native")]
pub mod take_rank;
#[cfg(feature = "native")]
pub mod timeline_actions;
pub mod wave_detail;
#[cfg(feature = "web")]
pub mod web_audio;
#[cfg(feature = "web")]
pub mod web_engine;
pub mod window_queue;
pub use editing::written;
pub mod zoom;

// The library reads a clock a browser has (#235).
#[cfg(test)]
mod web_clock_tests;
