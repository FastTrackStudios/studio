//! A Loro document shared live over vox: one peer hosts it, the others
//! join and edit it with it.
//!
//! - [`sync`]: the doc. A host ([`DocSyncHost`], or [`Hosts`] for many)
//!   serves it; a joiner's [`SyncedDoc`] catches up by version vector and
//!   then exchanges updates both ways. A joiner that has nothing yet is
//!   seeded with a shallow snapshot (the state now, without its history),
//!   sent in pieces a link takes, so a song of any age joins in one go.
//! - [`presence`]: who is here and what they are doing, on Loro's
//!   `EphemeralStore` ([`PresenceHost`], [`PresencePeer`]), and the data
//!   that rides it ([`Here`], [`PeerState`], [`PlayState`], [`Roster`]).
//! - [`clock`]: the host's clock, which a playhead is stamped in.
//!
//! Studio's own, on Loro directly. It began as a trimmed port of
//! Architect's `crdt::sync`, `crdt::presence` and `crdt::clock` (MIT OR
//! Apache-2.0, `FastTrackStudios`), which studio no longer uses.

pub mod clock;
pub mod presence;
pub mod sync;

pub use clock::SharedClock;
pub use loro;
pub use presence::{Here, PeerState, Place, PlayState, PresenceSink, Roster, clock_ms};
pub use sync::{
    DocPresence, DocPresenceClient, DocPresenceDispatcher, DocSync, DocSyncClient,
    DocSyncDispatcher, Error, MAX_FRAME, PresenceDriver, PresencePeer, SyncDown, SyncError, SyncUp,
    SyncedDoc, doc_presence_service_descriptor, doc_sync_service_descriptor,
};
// Hosting is a native thing (a desktop): a browser only joins.
#[cfg(not(target_arch = "wasm32"))]
pub use sync::{DocSyncHost, Hosts, Joiner, JoinerOf, PresenceHost};

/// A lock, taken even when a holder panicked: what it guards is plain
/// data that a panic leaves whole.
pub(crate) fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
