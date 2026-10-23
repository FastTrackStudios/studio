//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

#[cfg(all(feature = "audio"))]
mod adapter;
mod adopt_identity_keeps_lane;
#[cfg(all(feature = "audio"))]
mod align;
#[cfg(all(feature = "audio"))]
mod align_hits;
#[cfg(all(feature = "audio"))]
mod analyze;
#[cfg(all(feature = "audio", feature = "audio-daw", feature = "render"))]
mod apply_quantize;
#[cfg(all(feature = "standalone"))]
mod arrange_scroll;
#[cfg(all(feature = "ui"))]
mod blobs;
#[cfg(all(feature = "ui"))]
mod cc;
#[cfg(all(feature = "chords", feature = "daw"))]
mod chords_reaper_chord_insert;
mod core;
mod cursor;
#[cfg(all(feature = "daw"))]
mod daw_session;
#[cfg(all(
    feature = "audio",
    feature = "audio-daw",
    feature = "host",
    feature = "standalone",
    feature = "ui",
    feature = "ui-host"
))]
mod drums;
#[cfg(all(feature = "standalone"))]
mod drums_real;
#[cfg(all(feature = "audio"))]
mod dynamics;
#[cfg(all(feature = "daw"))]
mod editor_state;
#[cfg(all(feature = "ui"))]
mod envelope_panel;
#[cfg(all(feature = "standalone", feature = "ui"))]
mod fills_real;
mod flam;
#[cfg(all(feature = "audio", feature = "corpus"))]
mod flam_recall;
#[cfg(all(feature = "audio"))]
mod gate;
#[cfg(all(feature = "ui"))]
mod geometry;
#[cfg(all(feature = "standalone"))]
mod hit_after_mutation;
#[cfg(all(feature = "guitarpro"))]
mod import;
mod kit_scoring;
#[cfg(all(feature = "audio"))]
mod lanes;
#[cfg(all(feature = "standalone"))]
mod load;
mod memagic;
#[cfg(all(feature = "ui"))]
mod mode_drums;
#[cfg(all(feature = "ui"))]
mod mode_guitar;
#[cfg(all(feature = "ui"))]
mod mode_mpe;
#[cfg(all(feature = "ui"))]
mod modes;
#[cfg(all(feature = "ops", feature = "ui"))]
mod mouse_actions;
mod mouse_stack;
#[cfg(all(feature = "daw"))]
mod mpe_fixture;
#[cfg(all(feature = "ui"))]
mod mpe_gestures;
#[cfg(all(feature = "daw"))]
mod mpe_roundtrip;
mod multitrack;
#[cfg(all(feature = "ops"))]
mod ops_align;
mod page_bars;
#[cfg(all(feature = "audio"))]
mod percussive;
#[cfg(all(feature = "ui"))]
mod perf;
#[cfg(all(feature = "audio", feature = "standalone"))]
mod practice_real;
#[cfg(all(feature = "audio"))]
mod quantize;
#[cfg(all(feature = "ops"))]
mod quantize_midi;
#[cfg(all(feature = "ops", feature = "ui"))]
mod quantize_panel;
#[cfg(all(feature = "audio", feature = "audio-daw", feature = "render"))]
mod quantize_roundtrip;
#[cfg(all(feature = "ui", feature = "ui-webview"))]
mod raster_offthread;
#[cfg(all(feature = "guitarpro"))]
mod real_file;
#[cfg(all(feature = "ui"))]
mod render;
#[cfg(all(feature = "daw"))]
mod roundtrip;
#[cfg(all(feature = "standalone"))]
mod rpp_save_sources;
#[cfg(all(feature = "ui"))]
mod ruler_geometry;
#[cfg(all(
    feature = "audio",
    feature = "corpus",
    feature = "daw",
    feature = "demo",
    feature = "guitarpro",
    feature = "ops"
))]
mod scenarios;
#[cfg(all(feature = "ui", feature = "ui-webview"))]
mod scene_image_cost;
#[cfg(all(feature = "guitarpro", feature = "ui"))]
mod screenshots;
#[cfg(all(feature = "audio"))]
mod session;
#[cfg(all(feature = "ui"))]
mod slider_drag;
#[cfg(all(feature = "audio", feature = "audio-daw", feature = "render"))]
mod slip;
#[cfg(all(feature = "ui"))]
mod stack;
#[cfg(all(feature = "ui"))]
mod stack_slip;
#[cfg(all(feature = "audio", feature = "audio-daw", feature = "render"))]
mod standalone;
#[cfg(all(feature = "ui"))]
mod tools;
#[cfg(all(feature = "ui"))]
mod ui_cursor;
#[cfg(all(feature = "ui"))]
mod undo_routing;
#[cfg(all(feature = "audio", feature = "audio-daw", feature = "render"))]
mod warp;
#[cfg(all(feature = "corpus"))]
mod wav_probe;
#[cfg(all(feature = "ui"))]
mod webview_seam;
#[cfg(all(feature = "standalone"))]
mod workstation;
