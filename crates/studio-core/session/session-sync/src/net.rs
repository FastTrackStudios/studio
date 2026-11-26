//! Sharing a session over vox: one peer hosts, the others join.
//!
//! Built on [`live_doc`]: the host serves `DocSync` (the doc) and
//! `DocPresence` (who is doing what) for one session id; each joiner
//! holds both calls open for as long as it is connected. The transport is
//! whatever carries vox — iroh between native peers (`architect::iroh_link`),
//! a WebSocket from a browser, an in-process link in tests. It is hub and
//! spoke through the host, which is also a participant: its own engine
//! edits the very doc it serves.

#[cfg(not(target_arch = "wasm32"))]
use architect::LayerRouter;
use live_doc::{DocPresenceClient, DocSyncClient, PresenceDriver, PresencePeer, SyncedDoc};
// Hosting a set is a native thing (a desktop): a browser only joins.
#[cfg(not(target_arch = "wasm32"))]
use live_doc::{
    DocPresenceDispatcher, DocSyncDispatcher, DocSyncHost, Hosts, PresenceHost,
    doc_presence_service_descriptor, doc_sync_service_descriptor,
};
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::doc::SessionDoc;

/// How long a peer's presence outlives its last update.
pub const PRESENCE_TIMEOUT_MS: i64 = 30_000;

/// The id a song's session is shared under: the same on every machine
/// that has the song, so everyone who opens it meets in one doc. `song`
/// must be stable across machines and opens — a library song id, or the
/// song's name — never an engine project guid, which is minted per open.
#[must_use]
pub fn session_id(song: &str) -> Uuid {
    const NAMESPACE: Uuid = Uuid::from_u128(0x6a1f_0c3e_8d2b_4f5a_9e7c_1b3d_5f7a_9c2e);
    Uuid::new_v5(&NAMESPACE, song.as_bytes())
}

/// The doc a song of a shared setlist is synced under: every machine with
/// that setlist and that song meets in it.
#[must_use]
pub fn song_id(set: &str, song: &str) -> Uuid {
    session_id(&format!("{set}/{song}"))
}

/// A whole setlist, shared: every song's doc under its own id
/// ([`song_id`]), and one presence channel for the set under the set's
/// id — so people can be on different songs and still see where
/// everyone is.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub struct SetHost {
    id: Uuid,
    hosts: Hosts,
}

#[cfg(not(target_arch = "wasm32"))]
impl SetHost {
    /// A set shared under `id` (see [`session_id`]).
    #[must_use]
    pub fn new(id: Uuid) -> Self {
        Self {
            id,
            hosts: Hosts::new(PRESENCE_TIMEOUT_MS),
        }
    }

    /// Share a song's doc under `doc_id` that is kept in step with the
    /// song's own by a [`crate::relay::Relay`] — so this device's
    /// lock can hold the peers' edits back from the song, and its own from
    /// the peers. Returns the doc as the relay's side: what it is passed is
    /// imported and sent on to every peer attached (an import sends nothing
    /// by itself). Before anyone syncs it.
    #[must_use]
    pub fn add_relayed(&self, doc_id: Uuid, doc: &SessionDoc) -> crate::relay::Side {
        let host = self.hosts.add(doc_id, doc.loro().clone());
        let into = doc.loro().clone();
        crate::relay::Side {
            doc: doc.loro().clone(),
            take: Arc::new(move |bytes: Vec<u8>| {
                let (into, host) = (into.clone(), host.clone());
                Box::pin(async move {
                    into.import(&bytes)?;
                    host.publish(bytes);
                    Ok(())
                })
            }),
        }
    }

    /// Share a song's doc under `doc_id`. Before anyone syncs it.
    pub fn add(&self, doc_id: Uuid, doc: &SessionDoc) {
        let _ = self.hosts.add(doc_id, doc.loro().clone());
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    /// Add the set's services to `router`.
    #[must_use]
    pub fn mount(&self, router: LayerRouter) -> LayerRouter {
        router
            .with(
                doc_sync_service_descriptor(),
                DocSyncDispatcher::new(self.hosts.clone()),
            )
            .with(
                doc_presence_service_descriptor(),
                DocPresenceDispatcher::new(self.hosts.clone()),
            )
            // The session's shared clock: this host's (see `clock`).
            .with(
                crate::clock::session_clock_service_descriptor(),
                crate::clock::SessionClockDispatcher::new(crate::clock::ClockHost),
            )
    }

    /// This host's own presence, as a peer of its own set — over an
    /// in-process link, the same path every joiner takes.
    ///
    /// # Errors
    /// When the in-process link cannot be established.
    pub async fn own_presence(&self) -> eyre::Result<PresencePeer> {
        let server =
            architect::LocalServer::serve(self.mount(LayerRouter::new()), architect::Scope::new());
        let client: DocPresenceClient = server
            .establish()
            .await
            .map_err(|e| eyre::eyre!("in-process presence: {e:?}"))?;
        let (peer, mut driver) = PresencePeer::new(self.id, PRESENCE_TIMEOUT_MS);
        architect::platform::spawn(async move {
            let _server = server;
            if let Err(e) = driver.run(&client).await {
                tracing::warn!(collab.error = %e, "collab: the host's own presence ended");
            }
        });
        Ok(peer)
    }
}

/// A joiner's replicas of a shared set: one doc per song, each synced by
/// its own held-open call, and the set's presence.
pub struct SetPeer {
    id: Uuid,
    presence: PresencePeer,
    driver: Option<PresenceDriver>,
}

impl SetPeer {
    #[must_use]
    pub fn new(id: Uuid) -> Self {
        let (presence, driver) = PresencePeer::new(id, PRESENCE_TIMEOUT_MS);
        Self {
            id,
            presence,
            driver: Some(driver),
        }
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    #[must_use]
    pub const fn presence(&self) -> &PresencePeer {
        &self.presence
    }

    /// Start the set's presence session — and keep it: when its
    /// connection goes (the server restarting, a network blip), `dial`
    /// is asked for a new one and the session attaches again, re-announcing
    /// this peer.
    pub fn run_presence(&mut self, dial: Dial<DocPresenceClient>) {
        if let Some(mut driver) = self.driver.take() {
            architect::platform::spawn(async move {
                let mut backoff = Backoff::default();
                loop {
                    let began = architect::platform::now();
                    match dial().await {
                        Ok(client) => {
                            if let Err(e) = driver.run(&client).await {
                                tracing::debug!(collab.error = %e, "collab: presence ended; reconnecting");
                            }
                        }
                        Err(e) => {
                            tracing::debug!(collab.error = %e, "collab: presence could not reconnect yet")
                        }
                    }
                    backoff.wait(began.elapsed() >= HELD).await;
                }
            });
        }
    }

    /// Replicate one song's doc: an empty replica, filled from the host,
    /// and kept in sync across reconnects the same way as
    /// [`Self::run_presence`] — the replica catches up by version vector
    /// each time it attaches, both ways.
    #[must_use]
    pub fn sync_song(doc_id: Uuid, dial: Dial<DocSyncClient>) -> SessionDoc {
        Self::sync_song_relayed(doc_id, dial).0
    }

    /// [`Self::sync_song`], and the replica as a side of a
    /// [`crate::relay::Relay`]: what it is passed is imported and
    /// sent up its connection too (an import sends nothing up by itself).
    #[must_use]
    pub fn sync_song_relayed(
        doc_id: Uuid,
        dial: Dial<DocSyncClient>,
    ) -> (SessionDoc, crate::relay::Side) {
        let doc = SessionDoc::new();
        let mut synced = SyncedDoc::new(doc_id, doc.loro().clone());
        let outbox = synced.outbox();
        let into = doc.loro().clone();
        let side = crate::relay::Side {
            doc: doc.loro().clone(),
            take: Arc::new(move |bytes: Vec<u8>| {
                let (into, outbox) = (into.clone(), outbox.clone());
                Box::pin(async move {
                    into.import(&bytes)?;
                    outbox
                        .send(bytes)
                        .map_err(|_| eyre::eyre!("the replica's connection is gone"))?;
                    Ok(())
                })
            }),
        };
        architect::platform::spawn(async move {
            let mut backoff = Backoff::default();
            loop {
                let began = architect::platform::now();
                match dial().await {
                    Ok(client) => {
                        if let Err(e) = synced.run(&client).await {
                            tracing::debug!(collab.error = %e, "collab: a song's sync ended; reconnecting");
                        }
                    }
                    Err(e) => {
                        tracing::debug!(collab.error = %e, "collab: a song's sync could not reconnect yet")
                    }
                }
                backoff.wait(began.elapsed() >= HELD).await;
            }
        });
        (doc, side)
    }
}

/// Makes a client afresh: what a replica reconnects with once its
/// connection has gone. A dial for a connection that cannot be made again
/// (an in-process link, an iroh connection already open) is
/// [`dial_fixed`].
pub type Dial<C> =
    Arc<dyn Fn() -> architect::platform::BoxFuture<'static, eyre::Result<C>> + Send + Sync>;

/// A [`Dial`] that hands back the one client it was given.
pub fn dial_fixed<C>(client: C) -> Dial<C>
where
    C: Clone + Send + Sync + 'static,
{
    Arc::new(move || {
        let client = client.clone();
        Box::pin(async move { Ok(client) })
    })
}

/// A session that lasted this long was a working connection, not a
/// failed attempt: the next wait starts short again.
const HELD: Duration = Duration::from_secs(10);

/// How long to wait before dialling again: from a second, doubling to
/// fifteen while attempts keep failing, back to a second after one held.
struct Backoff {
    wait: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            wait: Backoff::FIRST,
        }
    }
}

impl Backoff {
    const FIRST: Duration = Duration::from_secs(1);
    const LONGEST: Duration = Duration::from_secs(15);

    async fn wait(&mut self, held: bool) {
        if held {
            self.wait = Self::FIRST;
        }
        architect::platform::sleep(self.wait).await;
        self.wait = (self.wait * 2).min(Self::LONGEST);
    }
}

/// Something that can be asked to tell everyone about this peer.
pub trait PresenceSink: Send + Sync {
    fn set(&self, key: &str, value: loro::LoroValue);
    fn delete(&self, key: &str);
    /// Every peer's entries, this one's included.
    fn states(&self) -> std::collections::HashMap<String, loro::LoroValue>;
}

/// This peer, serving its session to others.
#[derive(Clone)]
#[cfg(not(target_arch = "wasm32"))]
pub struct CollabHost {
    id: Uuid,
    sync: DocSyncHost,
    presence: PresenceHost,
}

#[cfg(not(target_arch = "wasm32"))]
impl CollabHost {
    /// Serve `doc` — the same Loro doc this peer's own bridge edits.
    #[must_use]
    pub fn new(id: Uuid, doc: &SessionDoc) -> Self {
        Self {
            id,
            sync: DocSyncHost::new(id, doc.loro().clone()),
            presence: PresenceHost::new(id, PRESENCE_TIMEOUT_MS),
        }
    }

    /// The session id joiners ask for.
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    /// Add the session's two services to `router`.
    #[must_use]
    pub fn mount(&self, router: LayerRouter) -> LayerRouter {
        router
            .with(
                doc_sync_service_descriptor(),
                DocSyncDispatcher::new(self.sync.clone()),
            )
            .with(
                doc_presence_service_descriptor(),
                DocPresenceDispatcher::new(self.presence.clone()),
            )
    }

    /// How many peers are connected.
    #[must_use]
    pub fn peers(&self) -> usize {
        self.sync.active_sessions()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PresenceSink for CollabHost {
    fn set(&self, key: &str, value: loro::LoroValue) {
        self.presence.store().set(key, value);
    }
    fn delete(&self, key: &str) {
        self.presence.store().delete(key);
    }
    fn states(&self) -> std::collections::HashMap<String, loro::LoroValue> {
        self.presence.store().get_all_states().into_iter().collect()
    }
}

/// This peer, joined to someone else's session.
pub struct CollabPeer {
    id: Uuid,
    doc: SessionDoc,
    synced: SyncedDoc,
    presence: PresencePeer,
    driver: PresenceDriver,
}

impl CollabPeer {
    /// Prepare to join session `id`. The doc starts empty and fills from
    /// the host on [`Self::run`]; wire the bridge to [`Self::doc`] only
    /// after the first sync (see `Bridge::join`).
    #[must_use]
    pub fn new(id: Uuid) -> Self {
        let doc = SessionDoc::new();
        let synced = SyncedDoc::new(id, doc.loro().clone());
        let (presence, driver) = PresencePeer::new(id, PRESENCE_TIMEOUT_MS);
        Self {
            id,
            doc,
            synced,
            presence,
            driver,
        }
    }

    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    /// The replica — the same Loro doc the sync driver fills.
    #[must_use]
    pub const fn doc(&self) -> &SessionDoc {
        &self.doc
    }

    /// This peer's presence handle (cheap to clone).
    #[must_use]
    pub const fn presence(&self) -> &PresencePeer {
        &self.presence
    }

    /// Split into what the UI keeps (doc, presence) and the two session
    /// drivers, each to be run on a connection until it drops.
    #[must_use]
    pub fn into_parts(self) -> (SessionDoc, PresencePeer, SyncedDoc, PresenceDriver) {
        (self.doc, self.presence, self.synced, self.driver)
    }

    /// Run both sessions until the connection drops. Two clients, each
    /// established on its own link: a vox caller is bound to one service
    /// once constructed.
    ///
    /// # Errors
    /// When either session fails to attach.
    pub async fn run(
        &mut self,
        sync: &DocSyncClient,
        presence: &DocPresenceClient,
    ) -> eyre::Result<()> {
        let (a, b) = futures::join!(self.synced.run(sync), self.driver.run(presence));
        a.map_err(|e| eyre::eyre!("session sync: {e}"))?;
        b.map_err(|e| eyre::eyre!("session presence: {e}"))?;
        Ok(())
    }
}

impl PresenceSink for PresencePeer {
    fn set(&self, key: &str, value: loro::LoroValue) {
        Self::set(self, key, value);
    }
    fn delete(&self, key: &str) {
        Self::delete(self, key);
    }
    fn states(&self) -> std::collections::HashMap<String, loro::LoroValue> {
        Self::states(self)
    }
}
