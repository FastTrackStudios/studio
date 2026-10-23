//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod audio_accessor;
mod automation_write;
mod aux_sends;
mod clap_host;
mod comped_take_selection;
mod create_with_guid;
mod duplicate_midi;
mod edit_steps;
mod fx_automation;
mod grouping_switch;
mod hit_markers;
mod in_process_daw;
mod item_ids;
mod lanes;
mod materialize_audio;
mod media_bay;
mod media_fetch;
mod media_seed;
mod meters_follow_tracks;
mod midi_full_types;
mod open_kit_project;
mod project_loader;
mod project_loader_fx;
mod reapeaks_sidecar;
mod recording;
mod remote_stream;
mod render_midi;
mod render_muted_fx;
mod render_routing;
mod ruler_lanes;
mod save_as;
mod save_as_real;
mod session_file;
mod song_files;
mod standalone_facade;
mod streamed_playback;
mod stretch_marker;
mod take_envelope;
mod tempo_map_midi;
mod tempo_reaches_plugins;
mod track_groups;
mod transport_engine;
mod transport_sync;
mod transport_sync_facade;
mod vst3_host;
