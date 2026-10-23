//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod chunk_tree_fidelity;
mod combine_rpl;
mod combine_trimmed;
mod diff_setlist_sync;
mod fixed_lanes;
mod full_project_audit;
mod item_take_roundtrip;
mod organize_project;
mod perf_guardrails;
mod roundtrip;
mod setlist_concatenation;
mod setlist_real_rpl;
mod tempo_map_advanced;
mod vst3_header_roundtrip;
