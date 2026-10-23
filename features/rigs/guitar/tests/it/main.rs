//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod config_reload;
mod hosted_rig;
mod instrument_switch;
mod macro_rpc;
#[cfg(all(feature = "ui"))]
mod param_flood;
mod part_sound;
mod perform_rpc;
mod pick_rpc;
mod rig_homes;
mod setlist_walk;
#[cfg(all(feature = "mcp"))]
mod signal_mcp;
mod todays_setlist;
mod tone_rpc;
