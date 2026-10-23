//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod apply;
mod apply_state;
mod discovery;
mod headphones;
mod lowering;
mod performer_row;
mod profile;
mod round_trip;
mod schema;
#[path = "override.rs"]
mod t_override;
mod validation;
