//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod effect_filters;
mod effects_add;
#[cfg(all(feature = "engine"))]
mod engine;
mod eq_suite;
mod eq_transparency;
#[cfg(all(feature = "engine"))]
mod mic;
mod parallel_level;
mod preamp;
#[cfg(all(feature = "engine"))]
mod reverb_time;
mod saturate_engine;
#[cfg(all(feature = "engine"))]
mod sidechain;
mod stability;
#[cfg(all(feature = "engine"))]
mod state_round_trip;
mod tune_block;
