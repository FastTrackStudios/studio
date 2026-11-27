//! The engine's core: the bottom of studio-engine, what every domain
//! (listen, edit, mix, project, collab) builds on.
//!
//! - the set the engine has open ([`studio_engine::set`]);
//! - opening and saving a song ([`open`], [`open_core`], [`project_file`],
//!   [`session_file`], [`prepare`], [`reference`]) and the song as an action
//!   reads it ([`song`]);
//! - its history ([`history`], [`history_marks`], [`edit_trace`]),
//!   [`autosave`] and [`backup`]s;
//! - the audio device ([`audio_device`]) and rendering ahead
//!   ([`render_ahead`]);
//! - a checkpoint before a rewrite ([`safety_net`]) and the engine's
//!   telemetry ([`telemetry`]);
//! - what every domain's actions share ([`actions`]);
//! - what the core asks of the engine around it ([`assembly`]).
//!
//! The engine's shared half with a window (`engine_runtime::client`) is
//! re-exported here under the paths the modules were written against.
//! studio-engine re-exports every module of this crate from its old path.

pub use engine_runtime::client::*;

#[cfg(feature = "native")]
pub mod actions;
#[cfg(feature = "native")]
pub mod assembly;
#[cfg(feature = "native")]
pub mod audio_device;
#[cfg(feature = "native")]
pub mod autosave;
#[cfg(feature = "native")]
pub mod backup;
/// Where the song's chart is at a time (`SESSION_CHART_AT`).
pub mod chart_at;
#[cfg(feature = "native")]
pub mod edit_trace;
#[cfg(feature = "native")]
pub mod history;
#[cfg(feature = "native")]
pub mod history_marks;
#[cfg(feature = "native")]
pub mod mix_states;
#[cfg(feature = "native")]
pub mod open;
pub mod open_core;
pub mod prepare;
#[cfg(feature = "native")]
pub mod project_file;
pub mod reference;
#[cfg(feature = "native")]
pub mod render_ahead;
#[cfg(feature = "native")]
pub mod safety_net;
#[cfg(feature = "native")]
pub mod session_file;
#[cfg(feature = "native")]
pub mod song;
#[cfg(feature = "native")]
pub mod studio_engine;
/// The engine's telemetry on plain `tracing`: the span ring `PERF_RECENT`
/// reads, OTLP export, the panic hook.
#[cfg(feature = "native")]
pub mod telemetry;

// What a domain moves down into the core (shared by more than one domain),
// each under its own heading, so the domains never edit the same lines.

// ── from engine-listen ──
/// A span rendered offline and measured (MIX_MEASURE's numbers).
#[cfg(feature = "native")]
pub mod measure;
/// Engine actions and blocking work off the async thread that asked for them.
#[cfg(feature = "native")]
pub mod off_loop;
/// Signal's offline blocks, as another domain asks for them.
#[cfg(feature = "native")]
pub mod signal_offline;

// ── from engine-edit ──
/// A stretch of an audio file read straight off the disk.
#[cfg(feature = "native")]
pub mod audio_window;
/// The drum MIDI as written, mapped to the kit (engine-edit's, read by
/// the kit).
pub mod drum_reference;
/// A drum's fundamental, off its own hits (the mix tunes its Fund/Sub to it).
pub mod fundamental;
/// Where every hit of the kit plays now, kept per song.
#[cfg(feature = "native")]
pub mod hit_index;
/// The kit and its hits as the song plays them, and their notes.
#[cfg(feature = "native")]
pub mod kit;
/// `TRACK_SHOW_LANES`'s work, and an action asked of the window too.
#[cfg(any(feature = "native", feature = "web"))]
pub mod lane_actions;
/// The vocal leveler's lanes as a track keeps them.
#[cfg(feature = "native")]
pub mod vocal_lanes;

// ── from engine-mix ──
#[cfg(feature = "native")]
pub mod fx_target;
#[cfg(feature = "native")]
pub mod mix_calls;
#[cfg(feature = "native")]
pub mod moment;

// ── from engine-project ──

// ── from engine-collab ──
/// What other domains ask of the live session (engine-collab's `collab`),
/// through the engine around the core.
#[cfg(feature = "native")]
pub mod collab_calls;
/// The song's journal and revision, and the in-flight tracking every door
/// into the Action List uses (engine-collab's `agent` acts on it).
#[cfg(feature = "native")]
pub mod journal;
/// Task's account on this machine: signing in, the org libraries, the org
/// picked.
#[cfg(feature = "native")]
pub mod task_account;
/// A set Task keeps: which, joined, tried again until Task answers.
#[cfg(any(feature = "native", feature = "web"))]
pub mod task_set;
/// Work on a page cut into slices, the page given its turn between them.
#[cfg(feature = "web")]
pub mod web_slices;
