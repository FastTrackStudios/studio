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
pub use daw_view::changes;
pub use daw_view::edit_state;
pub use daw_view::engine;
pub use daw_view::folds;
pub use daw_view::jobs;
pub use daw_view::lanes;
pub use daw_view::leveler;
pub use daw_view::midi;
pub use daw_view::num;
#[cfg(any(feature = "native", feature = "web"))]
pub use engine_collab::collab;
#[cfg(feature = "native")]
pub use engine_collab::live_host;
#[cfg(feature = "native")]
pub use engine_core::autosave;
#[cfg(feature = "native")]
pub use engine_edit::comp;
#[cfg(feature = "native")]
pub use engine_edit::comp_rank;
#[cfg(feature = "native")]
pub use engine_edit::drum_group;
#[cfg(feature = "native")]
pub use engine_edit::grow;
#[cfg(feature = "native")]
pub use engine_mix::automation_actions;
#[cfg(feature = "native")]
pub use engine_mix::clap_fx;
#[cfg(feature = "native")]
pub use engine_mix::clap_host;
#[cfg(feature = "native")]
pub use engine_mix::fx_actions;
#[cfg(feature = "native")]
pub use engine_project::ingest;
#[cfg(feature = "native")]
pub use engine_project::instrument_tracks;
pub use engine_runtime::client::audio_mode;
pub use engine_runtime::folder;
pub use engine_runtime::guide_instrument;
#[cfg(feature = "native")]
pub mod open;
pub use daw_view::open_progress;
pub use daw_view::options;
pub use daw_view::play_return;
pub use engine_core::open_core;
#[cfg(feature = "native")]
pub use engine_core::project_file;
#[cfg(feature = "signal")]
pub use engine_mix::rig_actions;
#[cfg(feature = "native")]
pub use engine_project::organize_edit;
#[cfg(feature = "native")]
pub use engine_project::project_new;
#[cfg(feature = "native")]
pub use engine_project::projects;
#[cfg(feature = "show")]
pub use engine_project::show;
pub use engine_project::song_stream;
#[cfg(feature = "native")]
pub use engine_project::stream_in;
#[cfg(feature = "native")]
pub use engine_project::stream_set;
pub use engine_runtime::client::presence;
pub use engine_runtime::client::slices;
pub use engine_runtime::client::song_modes;
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
/// The engine as a window sees it: one vox service beside the daw facade.
pub use daw_view::view;
#[cfg(feature = "native")]
pub use engine_collab::agent;
#[cfg(feature = "native")]
pub use engine_core::task_account;
#[cfg(any(feature = "native", feature = "web"))]
pub use engine_core::task_set;
pub use engine_edit::track_regions;
/// A window's link to its engine: replicas and calls.
#[cfg(feature = "native")]
pub use engine_runtime::client::view_client;
#[cfg(feature = "native")]
pub mod listen;
#[cfg(feature = "native")]
pub mod mix_measure;
#[cfg(feature = "native")]
pub use engine_mix::mix_sends;
#[cfg(feature = "native")]
pub use engine_mix::mix_snapshot;
#[cfg(feature = "native")]
pub use engine_mix::mix_state;
#[cfg(feature = "native")]
pub use engine_mix::mix_variants;
#[cfg(all(test, feature = "native"))]
mod mix_variants_match_tests;
#[cfg(feature = "native")]
pub mod off_loop;
#[cfg(feature = "native")]
pub mod render_worker;
#[cfg(feature = "native")]
pub use engine_mix::signal_kit;
#[cfg(feature = "native")]
pub mod signal_offline;
#[cfg(feature = "native")]
pub use daw_view::wave_pool;
#[cfg(feature = "native")]
pub use engine_edit::vocal_level;
/// On the web there is no in-process tokio runtime: the engine clients
/// (`engine::Transport`, `Applier`, `Meters`, …) find none and run
/// read-only, the way they do natively before a session is open. The web
/// host drives the engine its own way (`web_host`).
#[cfg(not(feature = "native"))]
pub use engine_runtime::client::open;
#[cfg(feature = "native")]
pub mod actions;
#[cfg(any(feature = "native", feature = "web"))]
pub use daw_view::arrange_actions;
pub use daw_view::hit_overlay;
pub use daw_view::kit_gate;
#[cfg(feature = "native")]
pub use daw_view::notes_actions;
pub use editing::groove;
#[cfg(feature = "native")]
pub use engine_core::audio_device;
#[cfg(feature = "native")]
pub use engine_core::audio_window;
#[cfg(feature = "native")]
pub use engine_core::backup;
pub use engine_core::drum_reference;
#[cfg(feature = "native")]
pub use engine_core::edit_trace;
pub use engine_core::fundamental;
#[cfg(feature = "native")]
pub use engine_core::history;
#[cfg(feature = "native")]
pub use engine_core::history_marks;
#[cfg(feature = "native")]
pub use engine_core::hit_index;
#[cfg(feature = "native")]
pub use engine_edit::comp_keep;
#[cfg(feature = "native")]
pub use engine_edit::comp_song;
#[cfg(feature = "native")]
pub use engine_edit::drum_apply;
#[cfg(feature = "native")]
pub use engine_edit::drum_bench;
#[cfg(feature = "native")]
pub use engine_edit::drum_lanes;
pub use engine_edit::drum_level;
#[cfg(feature = "native")]
pub use engine_edit::drum_map_set;
#[cfg(feature = "native")]
pub use engine_edit::drum_plan;
#[cfg(feature = "signal")]
pub use engine_edit::drum_samples;
#[cfg(feature = "native")]
pub use engine_edit::edit_actions;
#[cfg(feature = "native")]
pub use engine_edit::edit_list;
#[cfg(feature = "native")]
pub use engine_edit::edit_reads;
#[cfg(feature = "native")]
pub use engine_edit::edit_revert;
pub use engine_edit::hit_flags;
#[cfg(feature = "native")]
pub use engine_edit::hit_ops;
#[cfg(feature = "native")]
pub use engine_edit::hit_place;
#[cfg(feature = "native")]
pub use engine_edit::hits;
#[cfg(feature = "native")]
pub use engine_edit::hits_midi;
#[cfg(feature = "native")]
pub use engine_edit::item_actions;
#[cfg(feature = "native")]
pub use engine_edit::key_change;
#[cfg(feature = "native")]
pub use engine_mix::algorithms;
#[cfg(feature = "signal")]
pub use engine_mix::bass_rig;
pub use engine_mix::mix_chains;
pub use engine_mix::mix_export;
#[cfg(feature = "native")]
pub use engine_project::library_actions;
pub use engine_runtime::client::drum_edit;
pub use engine_runtime::client::drum_settings;
pub use engine_runtime::client::gain_stage;
#[cfg(feature = "native")]
pub mod perf;
#[cfg(feature = "native")]
pub mod perf_meter;
/// The engine's telemetry on plain `tracing`: the span ring `PERF_RECENT`
/// reads, OTLP export, the panic hook.
#[cfg(feature = "native")]
pub use engine_core::telemetry;
pub use engine_runtime::client::plan;
pub mod poi;
pub use engine_core::prepare;
#[cfg(feature = "native")]
pub mod record;
pub use daw_view::routes;
pub use engine_core::reference;
#[cfg(feature = "native")]
pub use engine_core::render_ahead;
#[cfg(feature = "native")]
pub use engine_core::safety_net;
#[cfg(feature = "native")]
pub use engine_core::session_file;
#[cfg(feature = "native")]
pub use engine_edit::sample_align;
#[cfg(feature = "native")]
pub use engine_edit::seams;
#[cfg(feature = "native")]
pub use engine_project::reference_play;
pub use engine_project::setlist;
pub use engine_runtime::client::settings;
pub use engine_runtime::section_colors;
#[cfg(any(feature = "native", feature = "web"))]
pub mod shared_actions;
pub use daw_view::stretch;
#[cfg(feature = "native")]
pub use engine_core::song;
#[cfg(feature = "native")]
pub use engine_edit::take_rank;
pub use engine_runtime::client::song_bars;
pub use engine_runtime::client::song_chart;
pub use engine_runtime::client::song_model;
#[cfg(feature = "native")]
pub mod timeline_actions;
pub use daw_view::wave_detail;
#[cfg(feature = "native")]
pub use engine_edit::trigger_from_mics;
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
pub use body::install;
#[cfg(feature = "native")]
pub use engine_project::song_open;

#[cfg(test)]
mod web_clock_tests;
