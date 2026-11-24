//! What a window needs of Session's engine.
//!
//! The engine (`session-engine`) runs in a process of its own (`studio
//! engine`, Studio's `daw` extension); a window reaches it over a link
//! (`view_client`, the `EngineView` service in `view`). This crate is
//! everything a window is built from: that protocol and its link, the
//! window's queue (`window_queue`), the read-back model it draws
//! (`song_model`, `midi`, `wave_detail`), the overlays the engine publishes
//! (`hit_overlay`, `poi`, `track_regions`), the Action List as a window runs
//! it (`actions`), and the plain metadata the UI shows (`plan`, `settings`,
//! `options`, …).
//!
//! The engine depends on this crate and re-exports every module from its
//! old path (`session_engine::view`, …), so an edit to the engine's body
//! rebuilds the engine and nothing a window is built from. What a window
//! module needs of the body when the engine runs in the same process goes
//! through [`body`].

pub mod actions;
#[cfg(any(feature = "native", feature = "web"))]
pub mod arrange_actions;
pub mod audio_mode;
pub mod automation;
#[cfg(feature = "native")]
pub mod automation_actions;
pub mod body;
pub mod changes;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab;
#[cfg(feature = "native")]
pub mod comp;
pub mod drum_edit;
pub mod drum_settings;
#[cfg(feature = "native")]
pub mod edit_actions;
pub mod edit_state;
pub mod engine;
pub use session_runtime::folder;
pub mod folds;
pub mod gain_stage;
pub use session_runtime::guide_instrument;
pub mod hit_overlay;
pub mod instrument_tracks;
pub mod jobs;
pub mod kit_gate;
pub mod lanes;
pub mod leveler;
pub mod midi;
#[cfg(feature = "native")]
pub mod notes_actions;
pub mod num;
/// Opening's window half: the process's engine runtime and handle, the
/// song on screen, the audio mode, and attaching to an engine or a REAPER.
#[cfg(feature = "native")]
#[path = "open_native.rs"]
pub mod open;
#[cfg(not(feature = "native"))]
#[path = "open_web.rs"]
pub mod open;
pub mod open_progress;
pub mod options;
pub mod organize_edit;
#[cfg(feature = "native")]
pub mod page;
pub mod plan;
pub mod play_return;
pub mod poi;
pub mod prepare;
pub mod presence;
pub mod projects;
#[cfg(feature = "native")]
pub mod render_ahead;
pub mod routes;
pub use session_runtime::section_colors;
pub mod setlist;
pub mod settings;
pub mod slices;
pub mod song_bars;
pub mod song_chart;
pub mod song_model;
pub mod song_modes;
pub mod stages;
pub mod stream_quality;
pub mod stream_set;
pub mod stretch;
#[cfg(feature = "native")]
pub mod studio_engine;
pub mod take_colors;
pub mod track_regions;
/// The engine as a window sees it: one vox service beside the daw facade.
pub mod view;
/// A window's link to its engine: replicas and calls.
#[cfg(feature = "native")]
pub mod view_client;
/// Asking the windows for a frame when the engine changed what they draw.
pub mod wake;
pub mod wave_detail;
#[cfg(feature = "native")]
pub mod wave_pool;
#[cfg(feature = "web")]
pub use session_runtime::web_audio;
pub mod window_queue;
pub use editing::groove;
pub use editing::written;
pub mod zoom;
