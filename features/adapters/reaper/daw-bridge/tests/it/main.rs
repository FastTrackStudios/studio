//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod audio_sync_peers;
mod audio_sync_snapshot;
mod csurf_inproc_latency;
mod csurf_subscribe_latency;
mod sync_full_session;
mod sync_multi_transport;
mod sync_network_latency;
mod sync_position;
