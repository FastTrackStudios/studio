//! Studio engines as peers of one account (docs/design/peers.md).
//!
//! Every Studio engine — the desk playing a set's tracks, an iPad running
//! the guitar rig, one doing both, one that changes role mid-evening — is
//! the same kind of node on the account's mesh. None leads by design:
//!
//! - **Who is let in** is Architect's: `architect_devices::account::pass`
//!   puts this engine in the issuer's address book and admits the
//!   account's other devices; iroh finds them (n0 and, on the LAN, by
//!   mDNS). Nothing is typed.
//! - **What each runs** is [`About`]: its [`Roles`] (tracks, which rigs),
//!   asked of any peer over [`StudioPeer`].
//! - **What each plays now** is [`NowPlaying`]: the set and song up, the
//!   section, the transport. Every peer publishes its own; any peer may
//!   follow any other's ([`follow`]), and switch or stop at will.
//! - **The rig books** (presets, scenes, songs, setlists) are
//!   `crdt::docs` documents; peers sync them with each other directly
//!   (`DocsDriver::host` / `run_peer`), with no hub.
//!
//! The default set is the wire and the follow rules, wasm-clean. `mesh`
//! (native) is the node itself.

pub mod follow;
pub mod wire;

#[cfg(feature = "mesh")]
pub mod mesh;

pub use follow::{Follow, FollowAction, Here};
pub use wire::{About, Cue, NowPlaying, PeerRow, PeersStatus, RIG_CUES, RigCues, Roles, SongCues};
#[cfg(feature = "vox")]
pub use wire::control::{PeersControl, PeersControlClient};
#[cfg(feature = "vox")]
pub use wire::service::{
    StudioPeer, StudioPeerClient, StudioPeerStreamClient, StudioPeerStreamSource,
};
