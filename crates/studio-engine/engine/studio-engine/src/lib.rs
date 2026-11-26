//! Session's engine: everything that runs whether or not a window is open —
//! the set and its songs (`studio_engine`), the Action List (`actions`),
//! opening and saving (`open`, `open_core`, `project_file`), streaming
//! (`stream_set`, `song_stream`), collaboration (`collab`), the song's data
//! and its edits (hits, comps, fx, automation, timeline), and the engine
//! with no window (`studio_engine::headless`, what `studio engine` runs).
//!
//! Nothing here draws: the window is `daw-view-kit`, which attaches to this
//! engine (`window_queue::attach`, `actions::list`) and reads what it
//! publishes. See docs/design/engine-boundary.md.

#[cfg(feature = "native")]
pub mod audio_health;
pub use daw_view::automation;
pub use engine_runtime::client::audio_mode;
#[cfg(feature = "native")]
pub mod automation_actions;
#[cfg(feature = "native")]
pub mod autosave;
pub use daw_view::changes;
#[cfg(any(feature = "native", feature = "web"))]
pub mod collab;
#[cfg(feature = "native")]
pub mod comp;
#[cfg(feature = "native")]
pub mod comp_rank;
#[cfg(feature = "native")]
pub mod drum_group;
pub use daw_view::edit_state;
pub use daw_view::engine;
pub use daw_view::folds;
pub use engine_runtime::folder;
#[cfg(feature = "native")]
pub mod clap_fx;
#[cfg(feature = "native")]
pub mod clap_host;
#[cfg(feature = "native")]
pub mod fx_actions;
#[cfg(feature = "native")]
pub mod grow;
pub use engine_runtime::guide_instrument;
#[cfg(feature = "native")]
pub mod ingest;
#[cfg(feature = "native")]
pub mod instrument_tracks;
pub use daw_view::jobs;
pub use daw_view::lanes;
pub use daw_view::leveler;
#[cfg(feature = "native")]
pub mod live_host;
pub use daw_view::midi;
pub use daw_view::num;
#[cfg(feature = "native")]
pub mod open;
pub mod open_core;
pub use daw_view::open_progress;
pub use daw_view::options;
#[cfg(feature = "native")]
pub mod organize_edit;
pub use daw_view::play_return;
pub use engine_runtime::client::presence;
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
pub use engine_runtime::client::slices;
pub use engine_runtime::client::song_modes;
pub mod song_stream;
#[cfg(feature = "native")]
pub mod stream_in;
#[cfg(feature = "native")]
pub mod stream_set;
// The engine side, behind its own boundary: the set, its actions, the
// Action List's MCP server, and the engine with no window (`studio engine`).
/// A page of bars, as a window steps through the song (`SESSION_PAGE`).
#[cfg(feature = "native")]
pub mod page;
pub use engine_runtime::client::stream_quality;
/// The engine with no window, serving windows over its socket: what
/// `studio engine`, `studio-desktop --session-engine` and Studio's `daw`
/// extension run.
#[cfg(all(feature = "native", unix, not(target_os = "ios")))]
pub mod engine_host;
#[cfg(feature = "native")]
pub mod studio_engine;
pub use daw_view::take_colors;
#[cfg(feature = "native")]
pub mod task_account;
#[cfg(any(feature = "native", feature = "web"))]
pub mod task_set;
pub mod track_regions;
/// The engine as a window sees it: one vox service beside the daw facade.
pub use daw_view::view;
/// A window's link to its engine: replicas and calls.
#[cfg(feature = "native")]
pub use engine_runtime::client::view_client;
#[cfg(feature = "native")]
pub mod agent;
#[cfg(feature = "native")]
pub mod listen;
#[cfg(feature = "native")]
pub mod mix_measure;
#[cfg(feature = "native")]
pub mod mix_sends;
#[cfg(feature = "native")]
pub mod mix_snapshot;
#[cfg(feature = "native")]
pub mod mix_state;
#[cfg(feature = "native")]
pub mod mix_variants;
#[cfg(feature = "native")]
pub mod off_loop;
#[cfg(feature = "native")]
pub mod render_worker;
#[cfg(feature = "native")]
pub mod signal_kit;
#[cfg(feature = "native")]
pub mod signal_offline;
#[cfg(feature = "native")]
pub mod vocal_level;
#[cfg(feature = "native")]
pub use daw_view::wave_pool;
/// On the web there is no in-process tokio runtime: the engine clients
/// (`engine::Transport`, `Applier`, `Meters`, …) find none and run
/// read-only, the way they do natively before a session is open. The web
/// host drives the engine its own way (`web_host`).
#[cfg(not(feature = "native"))]
pub use engine_runtime::client::open;
#[cfg(feature = "native")]
pub mod actions;
#[cfg(feature = "signal")]
pub mod bass_rig;
pub use engine_runtime::client::drum_edit;
#[cfg(feature = "native")]
pub mod drum_lanes;
pub mod drum_level;
pub mod drum_reference;
#[cfg(feature = "signal")]
pub mod drum_samples;
pub use engine_runtime::client::drum_settings;
#[cfg(feature = "native")]
pub mod backup;
#[cfg(feature = "native")]
pub mod drum_map_set;
#[cfg(feature = "native")]
pub mod edit_actions;
#[cfg(feature = "native")]
pub mod edit_reads;
#[cfg(feature = "native")]
pub mod edit_revert;
#[cfg(feature = "native")]
pub mod edit_trace;
pub mod fundamental;
#[cfg(feature = "native")]
pub mod history_marks;
#[cfg(any(feature = "native", feature = "web"))]
pub use daw_view::arrange_actions;
pub use editing::groove;
pub use engine_runtime::client::gain_stage;
#[cfg(feature = "native")]
pub mod algorithms;
#[cfg(feature = "native")]
pub mod audio_device;
#[cfg(feature = "native")]
pub mod audio_window;
#[cfg(feature = "native")]
pub mod comp_keep;
#[cfg(feature = "native")]
pub mod comp_song;
#[cfg(feature = "native")]
pub mod history;
#[cfg(feature = "native")]
pub mod hit_index;
pub use daw_view::hit_overlay;
#[cfg(feature = "native")]
pub mod drum_apply;
#[cfg(feature = "native")]
pub mod drum_bench;
#[cfg(feature = "native")]
pub mod drum_plan;
#[cfg(feature = "native")]
pub mod edit_list;
pub mod hit_flags;
#[cfg(feature = "native")]
pub mod hit_ops;
#[cfg(feature = "native")]
pub mod hit_place;
#[cfg(feature = "native")]
pub mod hits;
#[cfg(feature = "native")]
pub mod hits_midi;
#[cfg(feature = "native")]
pub mod item_actions;
#[cfg(feature = "native")]
pub mod key_change;
pub use daw_view::kit_gate;
#[cfg(feature = "native")]
pub mod library_actions;
pub mod mix_chains;
pub mod mix_export;
#[cfg(feature = "native")]
pub use daw_view::notes_actions;
#[cfg(feature = "native")]
pub mod perf;
#[cfg(feature = "native")]
pub mod perf_meter;
/// The engine's telemetry on plain `tracing`: the span ring `PERF_RECENT`
/// reads, OTLP export, the panic hook.
#[cfg(feature = "native")]
pub mod telemetry;
pub use engine_runtime::client::plan;
pub mod poi;
pub mod prepare;
#[cfg(feature = "native")]
pub mod record;
pub mod reference;
#[cfg(feature = "native")]
pub mod reference_play;
#[cfg(feature = "native")]
pub mod render_ahead;
pub use daw_view::routes;
#[cfg(feature = "native")]
pub mod safety_net;
#[cfg(feature = "native")]
pub mod sample_align;
#[cfg(feature = "native")]
pub mod seams;
pub use engine_runtime::section_colors;
#[cfg(feature = "native")]
pub mod session_file;
pub mod setlist;
pub use engine_runtime::client::settings;
#[cfg(any(feature = "native", feature = "web"))]
pub mod shared_actions;
#[cfg(feature = "native")]
pub mod song;
pub use daw_view::stretch;
pub use engine_runtime::client::song_bars;
pub use engine_runtime::client::song_chart;
pub use engine_runtime::client::song_model;
#[cfg(feature = "native")]
pub mod take_rank;
#[cfg(feature = "native")]
pub mod timeline_actions;
#[cfg(feature = "native")]
pub mod trigger_from_mics;
pub use daw_view::wave_detail;
#[cfg(all(feature = "web", not(feature = "native")))]
pub mod web_actions;
#[cfg(feature = "web")]
pub use engine_runtime::web_audio;
#[cfg(feature = "web")]
pub mod web_engine;
pub use daw_view::window_queue;
pub use editing::written;
pub use engine_runtime::client::zoom;

// The library reads a clock a browser has (#235).
/// The engine's body, installed for the window modules
/// (`engine_runtime::client::body`).
mod body;
#[cfg(feature = "native")]
pub mod song_open;
pub use body::install;

#[cfg(test)]
mod web_clock_tests;
