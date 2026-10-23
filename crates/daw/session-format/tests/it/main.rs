//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod compact;
mod composite_envelope;
mod dawproject_bridge;
mod dawproject_overlap;
mod fixed_lanes;
mod oplog;
mod rpp_roundtrip;
mod session_roundtrip;
mod store;
mod template;
