//! The engine's half a window shares in-process: what was `engine-client`
//! (studio-connect), moved down so the engine depends on no UI crate.
//!
//! The engine (`studio-engine`) runs in a process of its own (`studio
//! engine`, Studio's `daw` extension); a window reaches it over a link
//! (`view_client`, the `EngineView` service in `view`). Here is what both
//! sides of that link share: the protocol and its link, the window's queue
//! (`window_queue`), the read-back model (`song_model`, `midi`,
//! `wave_detail`), the overlays the engine publishes (`hit_overlay`, `poi`,
//! `track_regions`), the Action List as a window runs it (`actions`), the
//! process's engine handle (`open`), and the plain metadata the UI shows
//! (`plan`, `settings`, `options`, …). Nothing here draws.
//!
//! `engine-client` re-exports every module from its old path
//! (`engine_client::view`, …) beside the few that need Dioxus
//! (`setlist::ReadOnlySet`, `song_modes::use_song_modes`); `studio-engine`
//! re-exports them from its old paths too. What a window module needs of
//! the engine's body when the engine runs in the same process goes through
//! [`body`].

/// What a window is built from that is any DAW's, not Studio's: the
/// views' contract (`daw-view`), under the paths this crate always had.
pub use daw_view::{
    automation, automation_actions, changes, edit_actions, edit_state, engine, folds, hit_overlay,
    jobs, kit_gate, lanes, leveler, midi, num, open_progress, options, play_return, poi, routes,
    scale, stretch, take_colors, track_regions, view, wake, wave_detail, window_queue,
};
#[cfg(feature = "native")]
pub use daw_view::{page, wave_pool};
pub mod actions;
#[cfg(any(feature = "native", feature = "web"))]
pub use daw_view::arrange_actions;
pub mod audio_mode;
pub mod body;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab;
#[cfg(feature = "native")]
pub mod comp;
pub mod drum_edit;
pub mod drum_settings;
pub use crate::folder;
pub mod gain_stage;
pub mod host;
pub use crate::guide_instrument;
pub mod instrument_tracks;
#[cfg(feature = "native")]
pub use daw_view::notes_actions;
/// Opening's window half: the process's engine runtime and handle, the
/// song on screen, the audio mode, and attaching to an engine or a REAPER.
#[cfg(feature = "native")]
#[path = "open_native.rs"]
pub mod open;
#[cfg(not(feature = "native"))]
#[path = "open_web.rs"]
pub mod open;
pub mod organize_edit;
pub mod plan;
pub mod prepare;
pub mod presence;
pub mod projects;
#[cfg(feature = "native")]
pub mod render_ahead;
pub use crate::section_colors;
pub mod setlist;
pub mod settings;
pub mod slices;
pub mod song_bars;
pub mod song_chart;
pub mod song_model;
pub mod song_modes;
pub mod stream_quality;
pub mod stream_set;
#[cfg(feature = "native")]
pub mod studio_engine;
/// A window's link to its engine: replicas and calls.
#[cfg(feature = "native")]
pub mod view_client;
#[cfg(feature = "web")]
pub use crate::web_audio;
pub use editing::groove;
pub use editing::written;
pub mod zoom;
