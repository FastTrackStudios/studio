//! A doc's sync and presence over vox.
//!
//! ```text
//!  joiner A                       host                        joiner B
//!  ┌──────────┐  sync(vv, up, down)  ┌──────────┐   sync    ┌──────────┐
//!  │ LoroDoc  │ ───────────────────▶ │ LoroDoc  │ ◀──────── │ LoroDoc  │
//!  │ (replica)│ ◀── what A lacks ─── │ + PubSub │ ────────▶ │ (replica)│
//!  │ edits ───┼───── up ───────────▶ │          │ ── down ─▶│          │
//!  └──────────┘                      └──────────┘           └──────────┘
//! ```
//!
//! One [`DocSync::sync`] call per joiner, held open for as long as it is
//! connected: it carries the joiner's version vector, an **up** channel of
//! its edits and a **down** channel of [`SyncDown`] frames. The host sends
//! what the joiner lacks, then every update from anyone, its own edits
//! included; what a joiner sends up is imported and sent on to the rest.
//! Concurrent edits merge; there is no rollback.
//!
//! **A joiner with nothing** (an empty version vector) is seeded with a
//! shallow snapshot of the host's doc: its state now, without the history
//! behind it, which a joiner never needs and which a song holds millions
//! of ops of (a song's snapshot is ~170 MB). A host whose doc is itself
//! shallow (a song folded at its open) can only seed this way. A joiner
//! that reconnects gets the updates since its version vector.
//!
//! **Any blob goes in frames** of at most [`MAX_FRAME`], both ways: a
//! seed, a long catch-up, a set seeded from one peer's song. The pieces go
//! as `Part`s and the frame after completes them; they are imported as
//! one. A host's joiner has its own queue, filled from the moment it
//! attaches, so a seed that takes seconds to export holds up nothing.
//!
//! **Presence** ([`DocPresence`]) has the same shape, over Loro's
//! `EphemeralStore`: last-writer-wins per key, expiring, never stored. The
//! host also serves its clock ([`DocPresence::now`]), which each peer's
//! [`PresenceDriver`] pings into its [`crate::SharedClock`].

use std::collections::{BTreeSet, HashMap};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use loro::awareness::EphemeralStore;
use loro::{ExportMode, LoroDoc, VersionVector};
use uuid::Uuid;

use crate::lock;

/// Why an attach was refused.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet, thiserror::Error)]
#[repr(u8)]
pub enum SyncError {
    #[error("unknown doc")]
    UnknownDoc,
    #[error("bad version vector: {0}")]
    BadVersion(String),
    #[error("internal error: {0}")]
    Internal(String),
}

/// Why a joiner's session ended in error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The host refused the attach, or the call failed.
    #[error("attach: {0}")]
    Attach(String),
    /// What arrived would not import.
    #[error("import: {0}")]
    Import(#[from] loro::LoroError),
    /// The host's version vector would not decode.
    #[error("the host's version vector: {0}")]
    BadVersion(String),
    /// What the host lacks would not export.
    #[error("export: {0}")]
    Export(String),
    /// What arrived could not be written through ([`SyncedDoc::persist`]).
    #[error("persist: {0}")]
    Persist(String),
}

/// A frame on the sync **down** channel.
///
/// vox scopes channels to their request: delivering the response ends
/// them, so the attach cannot return a value and keep streaming. The
/// host's version vector rides the first frame instead, and the call
/// itself stays in flight for the whole session.
///
/// A blob over a frame (a shallow snapshot, a long history) goes as
/// [`SyncDown::Part`]s, and the next `Attached` or `Update` completes it:
/// the pieces and its bytes are imported as one.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum SyncDown {
    /// After a successful attach: the host's version vector (so the
    /// joiner sends back what the host lacks: catch-up goes both ways)
    /// and what the joiner lacks (empty when nothing).
    Attached { host_vv: Vec<u8>, backlog: Vec<u8> },
    /// A Loro update, live.
    Update(Vec<u8>),
    /// A piece of the blob the next frame completes.
    Part(Vec<u8>),
}

/// A frame on the sync **up** channel: [`SyncDown`]'s framing, up.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
pub enum SyncUp {
    /// A Loro update (with the parts before it, when there were any).
    Update(Vec<u8>),
    /// A piece of the update the next frame completes.
    Part(Vec<u8>),
}

/// The sync wire. `from` is the joiner's encoded version vector, empty for
/// a replica that has nothing. The call is the session: a successful
/// attach returns only once the joiner's connection closes (or it cancels
/// the call); a refused one returns the error at once.
#[vox::service]
pub trait DocSync {
    async fn sync(
        &self,
        doc_id: Uuid,
        from: Vec<u8>,
        up: vox::Rx<SyncUp>,
        down: vox::Tx<SyncDown>,
    ) -> Result<(), SyncError>;
}

/// The presence wire: `EphemeralStore` encodings both ways, the current
/// picture first. Held open like [`DocSync::sync`].
#[vox::service]
pub trait DocPresence {
    async fn presence(
        &self,
        doc_id: Uuid,
        up: vox::Rx<Vec<u8>>,
        down: vox::Tx<Vec<u8>>,
    ) -> Result<(), SyncError>;

    /// The host's clock now ([`crate::clock_ms`]), milliseconds.
    async fn now(&self) -> f64;
}

/// The most a frame down or up holds, bytes: a quarter of a WebSocket
/// link's 64 MiB message limit. A frame over a link's limit is refused,
/// and a joiner would retry it forever.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

// ── Host ────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
/// Who is attaching, as a host sees them: what they may change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Joiner {
    /// Edits the doc with everyone, and writes any presence.
    Editor,
    /// Follows: reads every doc and changes nothing in one (each update it
    /// sends is dropped), and writes only its own presence, the keys that
    /// begin with `own` (anything else it sends is dropped).
    Follower { own: String },
}

#[cfg(not(target_arch = "wasm32"))]
impl Joiner {
    const fn edits(&self) -> bool {
        matches!(self, Self::Editor)
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Asked as a joiner attaches, on its call: who they are.
pub type JoinerOf = Arc<dyn Fn() -> Joiner + Send + Sync>;

#[cfg(not(target_arch = "wasm32"))]
/// What goes to every joiner: an update, shared rather than copied.
type Out = Arc<Vec<u8>>;

#[cfg(not(target_arch = "wasm32"))]
/// The joiners' queues. Each joiner has its own, unbounded (an update
/// dropped is an edit lost until a reconnect), drained by its session.
#[derive(Clone, Default)]
struct Fanout(Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<Out>>>>);

#[cfg(not(target_arch = "wasm32"))]
impl Fanout {
    fn join(&self) -> tokio::sync::mpsc::UnboundedReceiver<Out> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        lock(&self.0).push(tx);
        rx
    }

    fn publish(&self, bytes: &Out) {
        lock(&self.0).retain(|tx| tx.send(Arc::clone(bytes)).is_ok());
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// The host of one doc: the doc, and the fan-out to everyone attached.
///
/// Every update reaches every joiner, whether a joiner sent it up or the
/// host's own engine made it in the doc. Echoing a sender its own update
/// is harmless: imports are idempotent.
#[derive(Clone)]
pub struct DocSyncHost {
    doc_id: Uuid,
    doc: LoroDoc,
    fanout: Fanout,
    /// Sessions attached: from a successful attach until the joiner's
    /// connection closes.
    sessions: Arc<AtomicUsize>,
    /// The most a frame holds ([`MAX_FRAME`] unless set).
    max_frame: usize,
    /// Keeps the host's own edits going out.
    _local: Arc<loro::Subscription>,
}

#[cfg(not(target_arch = "wasm32"))]
impl DocSyncHost {
    /// Serve `doc` under `doc_id`. Edits made to it in this process go out
    /// to every joiner on their own.
    #[must_use]
    pub fn new(doc_id: Uuid, doc: LoroDoc) -> Self {
        let fanout = Fanout::default();
        let out = fanout.clone();
        let local = doc.subscribe_local_update(Box::new(move |bytes| {
            out.publish(&Arc::new(bytes.clone()));
            true
        }));
        Self {
            doc_id,
            doc,
            fanout,
            sessions: Arc::default(),
            max_frame: MAX_FRAME,
            _local: Arc::new(local),
        }
    }

    /// Send frames of at most `bytes` instead of [`MAX_FRAME`]: a link
    /// with a smaller limit, or a test.
    #[must_use]
    pub const fn with_max_frame(mut self, bytes: usize) -> Self {
        self.max_frame = bytes;
        self
    }

    /// Send `bytes` (an update that reached the doc another way: imported,
    /// which fires no local-update subscription) to every joiner. Importing
    /// them is the caller's. Any size: a joiner's session cuts it.
    pub fn publish(&self, bytes: Vec<u8>) {
        self.fanout.publish(&Arc::new(bytes));
    }

    /// The doc served.
    #[must_use]
    pub const fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    /// How many joiners are attached.
    #[must_use]
    pub fn active_sessions(&self) -> usize {
        self.sessions.load(Ordering::Acquire)
    }

    /// What a joiner at `from` lacks, as one blob: the updates since its
    /// version vector, or, for one with nothing, a shallow snapshot.
    fn intro(&self, from: Option<&VersionVector>) -> Result<Vec<u8>, loro::LoroEncodeError> {
        from.map_or_else(
            || seed(&self.doc),
            |from| self.doc.export(ExportMode::updates(from)),
        )
    }

    /// The joiner's edits into the doc and out to everyone, until its
    /// connection closes. A [`Joiner::Follower`] has each one dropped: it
    /// reads the doc and changes nothing in it.
    async fn pump(&self, mut up: vox::Rx<SyncUp>, joiner: &Joiner) {
        let mut parts: Vec<u8> = Vec::new();
        let mut dropped = 0usize;
        while let Ok(Some(frame)) = up.recv().await {
            let mut owned: Option<SyncUp> = None;
            let _ = frame.map(|f| owned = Some(f));
            let bytes = match owned {
                Some(SyncUp::Part(part)) => {
                    parts.extend(part);
                    continue;
                }
                Some(SyncUp::Update(bytes)) => whole(&mut parts, bytes),
                None => continue,
            };
            if !joiner.edits() {
                dropped = dropped.saturating_add(1);
                if dropped == 1 {
                    tracing::warn!(sync.doc = %self.doc_id, sync.bytes = bytes.len(), "live-doc: a joiner that only follows sent an update; dropped, as every one after it is");
                }
                continue;
            }
            if let Err(e) = self.doc.import(&bytes) {
                tracing::warn!(sync.doc = %self.doc_id, sync.bytes = bytes.len(), sync.error = %e, "live-doc: dropping an update that would not import");
                continue;
            }
            self.fanout.publish(&Arc::new(bytes));
        }
    }

    /// Everything published to this joiner, in order, cut to frames,
    /// until its channel or the fan-out closes.
    async fn forward(
        &self,
        mut queue: tokio::sync::mpsc::UnboundedReceiver<Out>,
        down: &vox::Tx<SyncDown>,
    ) {
        while let Some(bytes) = queue.recv().await {
            let (parts, last) = cut(&bytes, self.max_frame);
            for part in parts {
                if down.send(SyncDown::Part(part)).await.is_err() {
                    return;
                }
            }
            if down.send(SyncDown::Update(last)).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl DocSync for DocSyncHost {
    async fn sync(
        &self,
        doc_id: Uuid,
        from: Vec<u8>,
        up: vox::Rx<SyncUp>,
        down: vox::Tx<SyncDown>,
    ) -> Result<(), SyncError> {
        self.attach(doc_id, from, up, down, &Joiner::Editor).await
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl DocSyncHost {
    /// One joiner's session: what [`DocSync::sync`] does, the joiner's
    /// updates taken in only from an editor. A follower follows the doc as
    /// everyone else does, and changes nothing in it.
    async fn attach(
        &self,
        doc_id: Uuid,
        from: Vec<u8>,
        up: vox::Rx<SyncUp>,
        down: vox::Tx<SyncDown>,
        joiner: &Joiner,
    ) -> Result<(), SyncError> {
        if doc_id != self.doc_id {
            return Err(SyncError::UnknownDoc);
        }
        let from = if from.is_empty() {
            None
        } else {
            Some(VersionVector::decode(&from).map_err(|e| SyncError::BadVersion(e.to_string()))?)
        };
        // Joined first, so whatever is published while the intro is made
        // waits in the queue behind it. (Overlap is fine: imports merge.)
        let queue = self.fanout.join();
        let host_vv = self.doc.oplog_vv().encode();
        let intro = self
            .intro(from.as_ref())
            .map_err(|e| SyncError::Internal(format!("export: {e}")))?;
        let _session = SessionGuard::new(Arc::clone(&self.sessions));
        let (parts, backlog) = cut(&intro, self.max_frame);
        drop(intro);
        tracing::debug!(
            sync.doc = %doc_id,
            sync.fresh = from.is_none(),
            sync.parts = parts.len(),
            sync.edits = joiner.edits(),
            "live-doc: a joiner attached"
        );
        for part in parts {
            if down.send(SyncDown::Part(part)).await.is_err() {
                return Ok(());
            }
        }
        if down
            .send(SyncDown::Attached { host_vv, backlog })
            .await
            .is_err()
        {
            return Ok(());
        }
        tokio::select! {
            () = self.pump(up, joiner) => {}
            () = self.forward(queue, &down) => {}
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// A fresh joiner's seed: a shallow snapshot of `doc` now, its state
/// without the history behind it. Nothing for a doc with nothing in it.
fn seed(doc: &LoroDoc) -> Result<Vec<u8>, loro::LoroEncodeError> {
    if doc.oplog_vv().is_empty() {
        return Ok(Vec::new());
    }
    doc.export(ExportMode::shallow_snapshot(&doc.oplog_frontiers()))
}

/// `bytes` as the pieces ahead and the last, each at most `max` (at least
/// one byte).
fn cut(bytes: &[u8], max: usize) -> (Vec<Vec<u8>>, Vec<u8>) {
    let mut pieces: Vec<Vec<u8>> = bytes.chunks(max.max(1)).map(<[u8]>::to_vec).collect();
    let last = pieces.pop().unwrap_or_default();
    (pieces, last)
}

/// The pieces kept, then `last`: one blob. Empties the pieces.
fn whole(parts: &mut Vec<u8>, last: Vec<u8>) -> Vec<u8> {
    if parts.is_empty() {
        return last;
    }
    let mut whole = std::mem::take(parts);
    whole.extend(last);
    whole
}

#[cfg(not(target_arch = "wasm32"))]
/// Counts a session for as long as it lives.
struct SessionGuard(Arc<AtomicUsize>);

#[cfg(not(target_arch = "wasm32"))]
impl SessionGuard {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self(counter)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

// ── Presence host ───────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
/// The host of one presence channel: a mirror store (so a late joiner gets
/// the current picture) and a sliding fan-out (under pressure a stale
/// pointer may go for a newer one).
#[derive(Clone)]
pub struct PresenceHost {
    doc_id: Uuid,
    store: EphemeralStore,
    /// How long an entry outlives its last write: a follower's updates are
    /// read in a store of their own first, with the same.
    timeout_ms: i64,
    hub: architect::PubSub<Vec<u8>>,
    sessions: Arc<AtomicUsize>,
    /// The clock it serves: [`crate::clock_ms`] unless given another.
    clock: Arc<dyn Fn() -> f64 + Send + Sync>,
    /// Keeps the host's own presence going out (a desktop that hosts the
    /// set it is in).
    _local: Arc<loro::Subscription>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PresenceHost {
    /// A channel whose entries go `timeout_ms` after their last write.
    #[must_use]
    pub fn new(doc_id: Uuid, timeout_ms: i64) -> Self {
        let store = EphemeralStore::new(timeout_ms);
        let hub = architect::PubSub::sliding(64);
        let out = hub.clone();
        let local = store.subscribe_local_updates(Box::new(move |bytes| {
            out.publish(bytes.clone());
            true
        }));
        Self {
            doc_id,
            store,
            timeout_ms,
            hub,
            sessions: Arc::default(),
            clock: Arc::new(crate::clock_ms),
            _local: Arc::new(local),
        }
    }

    /// Serve `clock` (milliseconds) as the shared clock instead of this
    /// process's.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Fn() -> f64 + Send + Sync>) -> Self {
        self.clock = clock;
        self
    }

    /// The mirror store (who is here, read on the host).
    #[must_use]
    pub const fn store(&self) -> &EphemeralStore {
        &self.store
    }

    /// How many peers are attached.
    #[must_use]
    pub fn active_sessions(&self) -> usize {
        self.sessions.load(Ordering::Acquire)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl DocPresence for PresenceHost {
    async fn presence(
        &self,
        doc_id: Uuid,
        up: vox::Rx<Vec<u8>>,
        down: vox::Tx<Vec<u8>>,
    ) -> Result<(), SyncError> {
        self.attach(doc_id, up, down, &Joiner::Editor).await
    }

    fn now(&self) -> impl Future<Output = f64> + Send {
        std::future::ready((self.clock)())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PresenceHost {
    /// One peer's presence session: what [`DocPresence::presence`] does,
    /// a follower's updates kept only for its own keys.
    async fn attach(
        &self,
        doc_id: Uuid,
        mut up: vox::Rx<Vec<u8>>,
        down: vox::Tx<Vec<u8>>,
        joiner: &Joiner,
    ) -> Result<(), SyncError> {
        if doc_id != self.doc_id {
            return Err(SyncError::UnknownDoc);
        }
        let pending = self.hub.begin_attach(down);
        self.store.remove_outdated();
        let picture = self.store.encode_all();
        self.hub
            .complete_attach(pending, (!picture.is_empty()).then_some(picture));
        let _session = SessionGuard::new(Arc::clone(&self.sessions));
        while let Ok(Some(update)) = up.recv().await {
            let mut owned: Option<Vec<u8>> = None;
            let _ = update.map(|u| owned = Some(u));
            let Some(bytes) = owned else { continue };
            match joiner {
                Joiner::Editor => {
                    if let Err(e) = self.store.apply(&bytes) {
                        tracing::warn!(sync.doc = %self.doc_id, sync.error = %e, "live-doc: dropping a presence update that would not apply");
                        continue;
                    }
                    self.hub.publish(bytes);
                }
                Joiner::Follower { own } => self.keep_own(&bytes, own),
            }
        }
        Ok(())
    }

    /// A follower's presence update, kept only for the keys it owns (that
    /// begin with `own`): read in a store of its own, each owned entry set
    /// here (which goes out to everyone), anything else dropped. What it
    /// deletes goes when it times out.
    fn keep_own(&self, bytes: &[u8], own: &str) {
        let theirs = EphemeralStore::new(self.timeout_ms);
        if let Err(e) = theirs.apply(bytes) {
            tracing::warn!(sync.doc = %self.doc_id, sync.error = %e, "live-doc: dropping a presence update that would not apply");
            return;
        }
        for key in theirs.keys() {
            match theirs.get(&key) {
                Some(value) if key.starts_with(own) => self.store.set(&key, value),
                Some(_) => {
                    tracing::debug!(sync.doc = %self.doc_id, sync.key = %key, "live-doc: a follower wrote presence not its own; dropped")
                }
                None => {}
            }
        }
    }
}

// ── Many docs ───────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
/// The host of many docs and presence channels behind one pair of
/// services: a set's songs, each under its own id, and its presence.
///
/// A doc is [`Self::insert`]ed (or [`Self::add`]ed) before anyone syncs
/// it; asked for one it does not have, it serves an empty doc under that
/// id. A presence channel opens on first use.
#[derive(Clone)]
pub struct Hosts {
    docs: Arc<Mutex<HashMap<Uuid, DocSyncHost>>>,
    presence: Arc<Mutex<HashMap<Uuid, PresenceHost>>>,
    timeout_ms: i64,
    /// Who the joiner attaching now is ([`Self::joiners`]).
    joiner_of: Option<JoinerOf>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Hosts {
    /// Presence entries go `timeout_ms` after their last write.
    #[must_use]
    pub fn new(timeout_ms: i64) -> Self {
        Self {
            docs: Arc::default(),
            presence: Arc::default(),
            timeout_ms,
            joiner_of: None,
        }
    }

    /// Ask `who` as each joiner attaches, on its call (where a gate in
    /// front has said who is calling): a [`Joiner::Follower`] reads every
    /// doc and changes none, and writes only its own presence. A guest in
    /// a room, say, who plays along and edits nothing. Unasked, everyone
    /// is an editor.
    #[must_use]
    pub fn joiners(mut self, who: JoinerOf) -> Self {
        self.joiner_of = Some(who);
        self
    }

    fn joiner(&self) -> Joiner {
        self.joiner_of.as_ref().map_or(Joiner::Editor, |who| who())
    }

    /// Serve `doc` under `doc_id`; the host made for it.
    #[must_use]
    pub fn add(&self, doc_id: Uuid, doc: LoroDoc) -> DocSyncHost {
        let host = DocSyncHost::new(doc_id, doc);
        self.insert(doc_id, host.clone());
        host
    }

    /// Serve `host` under `doc_id`.
    pub fn insert(&self, doc_id: Uuid, host: DocSyncHost) {
        lock(&self.docs).insert(doc_id, host);
    }

    /// The host of `doc_id`, made empty when there is none.
    #[must_use]
    pub fn doc(&self, doc_id: Uuid) -> DocSyncHost {
        lock(&self.docs)
            .entry(doc_id)
            .or_insert_with(|| DocSyncHost::new(doc_id, LoroDoc::new()))
            .clone()
    }

    /// The presence channel `doc_id`, opened when it is not.
    #[must_use]
    pub fn presence(&self, doc_id: Uuid) -> PresenceHost {
        lock(&self.presence)
            .entry(doc_id)
            .or_insert_with(|| PresenceHost::new(doc_id, self.timeout_ms))
            .clone()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl DocSync for Hosts {
    async fn sync(
        &self,
        doc_id: Uuid,
        from: Vec<u8>,
        up: vox::Rx<SyncUp>,
        down: vox::Tx<SyncDown>,
    ) -> Result<(), SyncError> {
        let joiner = self.joiner();
        self.doc(doc_id)
            .attach(doc_id, from, up, down, &joiner)
            .await
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl DocPresence for Hosts {
    async fn presence(
        &self,
        doc_id: Uuid,
        up: vox::Rx<Vec<u8>>,
        down: vox::Tx<Vec<u8>>,
    ) -> Result<(), SyncError> {
        let joiner = self.joiner();
        self.presence(doc_id)
            .attach(doc_id, up, down, &joiner)
            .await
    }

    fn now(&self) -> impl Future<Output = f64> + Send {
        std::future::ready(crate::clock_ms())
    }
}

// ── Presence peer ───────────────────────────────────────────────────────

/// One peer's presence: a cloneable handle on its store, and the
/// [`PresenceDriver`] that runs it over a connection.
///
/// Each peer writes its own keys; everyone reads [`Self::states`]. An
/// entry goes on its own `timeout_ms` after its last write.
#[derive(Clone)]
pub struct PresencePeer {
    store: EphemeralStore,
    /// The keys this peer owns, announced again on every attach.
    local_keys: Arc<Mutex<BTreeSet<String>>>,
    /// When each owned key was last written, on the store's clock: a
    /// delete waits until that millisecond has passed. Locked after
    /// `local_keys`.
    written: Arc<Mutex<HashMap<String, i64>>>,
    /// The host's clock, as the driver estimates it.
    clock: crate::SharedClock,
}

/// The store's clock: `EphemeralStore` stamps entries with the wall
/// clock's milliseconds since the epoch.
fn store_ms() -> i64 {
    architect::platform::SystemTime::now()
        .duration_since(architect::platform::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

impl PresencePeer {
    /// The handle and its driver: run `driver.run(&client)` (again on a
    /// reconnect), keep the handle.
    #[must_use]
    pub fn new(doc_id: Uuid, timeout_ms: i64) -> (Self, PresenceDriver) {
        let store = EphemeralStore::new(timeout_ms);
        let (tx, outbox) = tokio::sync::mpsc::unbounded_channel();
        let local = store.subscribe_local_updates(Box::new(move |bytes| {
            let _ = tx.send(bytes.clone());
            true
        }));
        let peer = Self {
            store,
            local_keys: Arc::default(),
            written: Arc::default(),
            clock: crate::SharedClock::default(),
        };
        let driver = PresenceDriver {
            doc_id,
            peer: peer.clone(),
            timeout_ms,
            outbox,
            _local: local,
        };
        (peer, driver)
    }

    /// Publish this peer's `key`.
    pub fn set(&self, key: &str, value: impl Into<loro::LoroValue>) {
        // Held across the write so a refresh never sets again a key
        // withdrawn while it ran.
        let mut keys = lock(&self.local_keys);
        keys.insert(key.to_string());
        self.store.set(key, value);
        lock(&self.written).insert(key.to_string(), store_ms());
        drop(keys);
    }

    /// Withdraw `key`.
    ///
    /// The store is last-write-wins by millisecond and a tie keeps what a
    /// receiver has, so a delete stamped in the millisecond of the key's
    /// last write would be dropped over there: it waits, at most a
    /// millisecond, until the clock has moved past that write.
    pub fn delete(&self, key: &str) {
        let mut keys = lock(&self.local_keys);
        keys.remove(key);
        let last = lock(&self.written).remove(key);
        if let Some(last) = last {
            loop {
                let now = store_ms();
                if now > last || now < last.saturating_sub(1_000) {
                    break;
                }
                std::hint::spin_loop();
            }
        }
        self.store.delete(key);
        drop(keys);
    }

    /// The host's clock, as this peer's driver estimates it.
    #[must_use]
    pub const fn clock(&self) -> &crate::SharedClock {
        &self.clock
    }

    /// Everyone's entries, this peer's included. Never prunes (it is read
    /// from render paths; the driver prunes on a timer).
    #[must_use]
    pub fn states(&self) -> HashMap<String, loro::LoroValue> {
        self.store.get_all_states().into_iter().collect()
    }

    /// Drop the entries past their timeout. From a task on a timer, never
    /// a render path: it fires the store's subscribers.
    pub fn sweep(&self) {
        self.store.remove_outdated();
    }

    /// Write this peer's keys again, unchanged, so the others keep it
    /// while it sits still.
    fn refresh(&self) {
        let keys = lock(&self.local_keys);
        for key in keys.iter() {
            if let Some(value) = self.store.get(key) {
                self.store.set(key, value);
                lock(&self.written).insert(key.clone(), store_ms());
            }
        }
        drop(keys);
    }

    /// The store, for change subscriptions.
    #[must_use]
    pub const fn store(&self) -> &EphemeralStore {
        &self.store
    }
}

/// A [`PresencePeer`]'s session: one [`Self::run`] per connection.
pub struct PresenceDriver {
    doc_id: Uuid,
    peer: PresencePeer,
    timeout_ms: i64,
    outbox: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    _local: loro::Subscription,
}

impl PresenceDriver {
    /// Attach, announce this peer's keys, then exchange updates (and ping
    /// the host's clock) until the connection drops or the future is
    /// dropped.
    ///
    /// # Errors
    /// When the host refuses the attach or the call fails.
    pub async fn run(&mut self, client: &DocPresenceClient) -> Result<(), Error> {
        let (up_tx, up_rx) = vox::channel::<Vec<u8>>();
        let (down_tx, mut down_rx) = vox::channel::<Vec<u8>>();
        let call = std::pin::pin!(client.presence(self.doc_id, up_rx, down_tx));

        // A reconnect: the host may have let this peer's entries go.
        let announce: Vec<Vec<u8>> = lock(&self.peer.local_keys)
            .iter()
            .map(|key| self.peer.store.encode(key))
            .filter(|encoded| !encoded.is_empty())
            .collect();
        let outbox = &mut self.outbox;
        let up = async move {
            for encoded in announce {
                if up_tx.send(encoded).await.is_err() {
                    return;
                }
            }
            while let Some(update) = outbox.recv().await {
                if up_tx.send(update).await.is_err() {
                    return;
                }
            }
            // The handle is gone; the session stays up for reads.
            std::future::pending::<()>().await;
        };
        let store = self.peer.store.clone();
        let down = async move {
            while let Ok(Some(update)) = down_rx.recv().await {
                let mut owned: Option<Vec<u8>> = None;
                let _ = update.map(|u| owned = Some(u));
                if let Some(bytes) = owned
                    && let Err(e) = store.apply(&bytes)
                {
                    tracing::warn!(sync.error = %e, "live-doc: dropping a presence update that would not apply");
                }
            }
        };
        // This peer's keys written again well inside the timeout; the
        // others' pruned once past it.
        let peer = self.peer.clone();
        let period = std::time::Duration::from_millis(
            u64::try_from(self.timeout_ms / 3).unwrap_or(0).max(1_000),
        );
        let housekeeping = async move {
            loop {
                architect::platform::sleep(period).await;
                peer.refresh();
                peer.sweep();
            }
        };
        // The host's clock, ten times a second.
        let clock = self.peer.clock.clone();
        let pings = async move {
            loop {
                let sent = crate::clock_ms();
                match client.now().await {
                    Ok(there) => clock.record(sent, there, crate::clock_ms()),
                    Err(_) => std::future::pending::<()>().await,
                }
                architect::platform::sleep(std::time::Duration::from_millis(100)).await;
            }
        };
        tokio::select! {
            res = call => res.map_err(|e| Error::Attach(e.to_string())),
            () = up => Ok(()),
            () = down => Ok(()),
            () = housekeeping => Ok(()),
            () = pings => Ok(()),
        }
    }
}

// ── Joiner ──────────────────────────────────────────────────────────────

/// What [`SyncedDoc::persist`] writes what arrives through.
pub type Persist =
    dyn Fn(Vec<u8>) -> architect::platform::BoxFuture<'static, Result<(), String>> + Send + Sync;

/// A joiner's replica: its doc's edits go up, the others' merge in, and a
/// dropped connection only means the next [`Self::run`] catches up by
/// version vector.
pub struct SyncedDoc {
    doc: LoroDoc,
    doc_id: Uuid,
    outbox: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    outbox_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// Writes what arrives, when the replica keeps it ([`Self::persist`]).
    persist: Option<Arc<Persist>>,
    /// Imported but not yet written: written before the next attach.
    unpersisted: Vec<Vec<u8>>,
    /// The most a frame up holds ([`MAX_FRAME`] unless set).
    max_frame: usize,
    _local: loro::Subscription,
}

/// How a session's held-open call ended.
fn ended<E: std::fmt::Display>(res: Result<(), E>) -> Result<(), Error> {
    res.map_err(|e| Error::Attach(e.to_string()))
}

impl SyncedDoc {
    /// Sync `doc` under `doc_id`. Once per replica, before it is edited,
    /// so every edit lands in the outbox.
    #[must_use]
    pub fn new(doc_id: Uuid, doc: LoroDoc) -> Self {
        let (tx, outbox) = tokio::sync::mpsc::unbounded_channel();
        let outbox_tx = tx.clone();
        let local = doc.subscribe_local_update(Box::new(move |bytes| {
            let _ = tx.send(bytes.clone());
            true
        }));
        Self {
            doc,
            doc_id,
            outbox,
            outbox_tx,
            persist: None,
            unpersisted: Vec::new(),
            max_frame: MAX_FRAME,
            _local: local,
        }
    }

    /// Send frames of at most `bytes` instead of [`MAX_FRAME`].
    #[must_use]
    pub const fn with_max_frame(mut self, bytes: usize) -> Self {
        self.max_frame = bytes;
        self
    }

    /// The outbox: bytes sent here go up like the doc's own edits (what
    /// relays an update that arrived over another connection; an import
    /// fires no local-update subscription).
    #[must_use]
    pub fn outbox(&self) -> tokio::sync::mpsc::UnboundedSender<Vec<u8>> {
        self.outbox_tx.clone()
    }

    /// Write everything that arrives through `persist` too, not only the
    /// doc's own edits: a replica on disk keeps what it received across a
    /// restart offline.
    #[must_use]
    pub fn persist(mut self, persist: Arc<Persist>) -> Self {
        self.persist = Some(persist);
        self
    }

    /// The replica.
    #[must_use]
    pub const fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    async fn import(&mut self, bytes: Vec<u8>) -> Result<(), Error> {
        self.doc.import(&bytes)?;
        if self.persist.is_some() {
            self.unpersisted.push(bytes);
            self.write_pending().await?;
        }
        Ok(())
    }

    async fn write_pending(&mut self) -> Result<(), Error> {
        let Some(persist) = self.persist.clone() else {
            return Ok(());
        };
        while let Some(bytes) = self.unpersisted.first() {
            persist(bytes.clone()).await.map_err(Error::Persist)?;
            self.unpersisted.remove(0);
        }
        Ok(())
    }

    /// One session: catch up both ways, then exchange updates until the
    /// connection drops or the future is dropped. Run it again with a
    /// fresh client on a disconnect.
    ///
    /// # Errors
    /// When the host refuses the attach, the call fails, or what arrives
    /// will not import or be written.
    pub async fn run(&mut self, client: &DocSyncClient) -> Result<(), Error> {
        self.write_pending().await?;
        let (up_tx, up_rx) = vox::channel::<SyncUp>();
        let (down_tx, mut down_rx) = vox::channel::<SyncDown>();
        let ours = self.doc.oplog_vv();
        let from = if ours.is_empty() {
            Vec::new()
        } else {
            ours.encode()
        };
        let mut call = std::pin::pin!(client.sync(self.doc_id, from, up_rx, down_tx));
        // The pieces of a blob, until the frame that completes it.
        let mut parts: Vec<u8> = Vec::new();
        loop {
            tokio::select! {
                // The call first: its request binds `up_tx`, and nothing
                // goes out until it is polled.
                biased;
                res = &mut call => return ended(res),
                Some(update) = self.outbox.recv() => {
                    if !send_up(&up_tx, &update, self.max_frame).await {
                        return ended(call.await);
                    }
                }
                incoming = down_rx.recv() => {
                    let mut owned: Option<SyncDown> = None;
                    match incoming {
                        Ok(Some(frame)) => {
                            let _ = frame.map(|f| owned = Some(f));
                        }
                        // A refused attach closes the channels too: the
                        // call says why.
                        _ => return ended(call.await),
                    }
                    match owned {
                        Some(SyncDown::Part(part)) => parts.extend(part),
                        Some(SyncDown::Attached { host_vv, backlog }) => {
                            let backlog = whole(&mut parts, backlog);
                            if !backlog.is_empty() {
                                self.import(backlog).await?;
                            }
                            // What this replica holds that the host lacks
                            // (edited offline): sent up.
                            let host_vv = VersionVector::decode(&host_vv)
                                .map_err(|e| Error::BadVersion(e.to_string()))?;
                            if !host_vv.includes_vv(&ours) {
                                let missing = self
                                    .doc
                                    .export(ExportMode::updates(&host_vv))
                                    .map_err(|e| Error::Export(e.to_string()))?;
                                if !send_up(&up_tx, &missing, self.max_frame).await {
                                    return ended(call.await);
                                }
                            }
                        }
                        Some(SyncDown::Update(bytes)) => {
                            let bytes = whole(&mut parts, bytes);
                            self.import(bytes).await?;
                        }
                        None => {}
                    }
                }
            }
        }
    }
}

/// `bytes` up, cut to frames of at most `max`; `false` when the channel is
/// gone.
async fn send_up(up: &vox::Tx<SyncUp>, bytes: &[u8], max: usize) -> bool {
    let (parts, last) = cut(bytes, max);
    for part in parts {
        if up.send(SyncUp::Part(part)).await.is_err() {
            return false;
        }
    }
    up.send(SyncUp::Update(last)).await.is_ok()
}
