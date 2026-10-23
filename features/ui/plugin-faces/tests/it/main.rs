//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod comp_fet;
mod comp_lane;
mod comp_tube;
mod comp_vca_eq;
mod delay_saturate;
mod frame_assets;
mod motion;
mod reverb_modulation;
mod stable_ids;
mod suite_boutique_clean;
mod suite_funk_rhythm;
mod suite_poly_clean;
mod suite_studio_blues;
mod suites_forge_rhythm_blackout;
mod verb_480;
