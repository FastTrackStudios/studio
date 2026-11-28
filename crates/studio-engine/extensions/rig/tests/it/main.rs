//! The crate's one integration binary: the rig engine's every action,
//! driven as Studio's MCP and control socket drive it (studio#560).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]

mod actions;
