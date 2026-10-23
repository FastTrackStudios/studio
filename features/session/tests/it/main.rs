//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod chart_import_praise;
mod comping;
mod coverage;
mod demo_setlist_tests;
#[cfg(all(feature = "engine"))]
mod doc_undo;
mod edge_case_tests;
#[cfg(all(feature = "engine"))]
mod engine_roundtrip;
mod guide_engine;
mod guitar_grow_actions;
mod integration_tests;
mod loop_tests;
mod marker_region_tests;
mod merge;
#[cfg(all(feature = "ui"))]
mod navigation;
mod navigation_tests;
#[cfg(all(feature = "net"))]
mod net;
mod presence;
mod project_tests;
mod reaper_drum_recording;
mod reaper_key_track;
mod reaper_lan_engine;
mod reaper_rockstars_album;
mod reaper_session_extension;
mod reaper_setlist_recording_mode;
mod region_name_parsing;
mod repro_inprocess;
mod seek_semantics_tests;
mod song_builder_tests;
mod standalone_setlist_harness;
mod subscription_tests;
#[cfg(all(feature = "ui"))]
mod take_review;
mod tempo_tests;
mod track_manager_actions;
mod transport;
#[cfg(all(feature = "ui"))]
mod transport_stats;
mod transport_tests;
#[cfg(all(feature = "engine"))]
mod whole;
mod wire_roundtrip;
