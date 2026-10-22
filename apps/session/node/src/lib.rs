//! A Signal or Session install as a Task device node.
//!
//! Task's `offline.peers` says every native install is a node: it holds
//! the orgs it was admitted to, has one iroh endpoint per org, and syncs
//! documents peer to peer with no hub. ADR 0002 (STA-362) decides how a
//! studio app becomes one: by embedding Task's own
//! `task_server::local_node::LocalNode` (option A) rather than extracting
//! a thin device-node library from it (option B).
//!
//! With the `task-node` feature off the crate holds only [`embedded`], so
//! an app that does not opt in pays nothing.
//! With it on, [`Node::start`] boots the node over a data root, and the
//! app reaches the node's lanes through [`Node::local`] exactly as Task's
//! own apps do (`task-node-host`).

#[cfg(feature = "task-node")]
mod node;

#[cfg(feature = "task-node")]
pub use node::{Node, NodeOptions, PERSONAL_SLUG};

/// Whether this build carries the embedded Task node.
#[must_use]
pub const fn embedded() -> bool {
    cfg!(feature = "task-node")
}
