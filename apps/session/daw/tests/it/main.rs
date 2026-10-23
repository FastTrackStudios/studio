//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod automation_by_section;
mod automation_item_edits;
mod chart_live;
mod comp_cycle_real;
mod comp_lanes;
mod comp_real;
mod compose_start;
mod default_view;
mod diagnose_playback;
mod drum_quantize;
mod drum_samples_follow;
mod drums_full_real;
mod edit_restore;
mod edit_selection;
mod editor_undo_live;
mod editor_view;
mod fader_writes_automation;
mod folder_items;
mod fundamental_finder;
mod golden_chart;
mod golden_mix;
mod golden_scenes;
mod grow_parts;
mod guide_instrument;
mod hit_index_real;
mod language_dimension;
mod mcp_in_process;
mod organize_live;
mod patch_list_view;
mod perf_slip;
mod perf_waves;
mod place_real;
mod prepare_live;
mod reaper_panel;
mod record_takes;
mod remote_lanes;
mod rig_actions;
mod session_file_live;
mod setlist_live;
mod song_compaction;
mod split_keeps_audio;
mod take_align;
mod tone_binding;
mod track_regions;
mod vocal_level;
