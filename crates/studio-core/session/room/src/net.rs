//! The room over vox.
//!
//! The leader hosts the room ([`RoomHost`]): live-doc's `DocSync` (the
//! room's doc, what is loaded) and `DocPresence` (who is here, the
//! transport, and the leader's clock, which the transport is stamped in),
//! both under the room's id. A member ([`RoomPeer`]) attaches once (the
//! doc's call and the presence's held open, two streams to the leader),
//! and follows everything the
//! leader does through it: another setlist loaded is a change to the doc,
//! never a new connection. Only a lost connection attaches again
//! ([`RoomPeer::attaches`] counts them).
//!
//! The transport under it is the caller's ([`Attach`]): iroh between
//! devices (engine-collab's `room`), an in-process link in tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use live_doc::{
    DocPresenceClient, DocPresenceDispatcher, DocSyncClient, DocSyncDispatcher, Hosts,
    PresencePeer, SyncedDoc, doc_presence_service_descriptor, doc_sync_service_descriptor,
};
use loro::LoroValue;
use session_sync::transport::{KEY, SharedTransport};

use crate::record::RoomId;
use crate::state::{RoomDoc, here_key};

/// How long a person's presence outlives their last word.
pub const PRESENCE_TIMEOUT_MS: i64 = 30_000;

/// One attachment to a room's leader: its doc's sync and its presence.
pub type Attach = Arc<
    dyn Fn() -> architect::platform::BoxFuture<
            'static,
            eyre::Result<(DocSyncClient, DocPresenceClient)>,
        > + Send
        + Sync,
>;

/// What a person says about themselves in the room.
fn here(name: &str) -> LoroValue {
    let mut map = std::collections::HashMap::new();
    map.insert("name".to_owned(), LoroValue::from(name));
    LoroValue::from(map)
}

/// Everyone in the room, by name, sorted: what `presence` holds.
#[must_use]
pub fn people(presence: &PresencePeer) -> Vec<String> {
    let mut names: Vec<String> = presence
        .states()
        .into_iter()
        .filter(|(k, _)| k.ends_with("/here"))
        .filter_map(|(_, v)| match v {
            LoroValue::Map(m) => match m.get("name") {
                Some(LoroValue::String(s)) => Some(s.to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    names.sort();
    names
}

/// The leader's transport, as `presence` holds it.
#[must_use]
pub fn transport(presence: &PresencePeer) -> Option<SharedTransport> {
    presence.states().get(KEY).and_then(SharedTransport::decode)
}

/// The leader's end of a room: its doc, served with its presence.
#[derive(Clone)]
pub struct RoomHost {
    id: uuid::Uuid,
    hosts: Hosts,
    doc: RoomDoc,
    /// Ends the leader's own presence task ([`Self::close`]).
    stop: Arc<tokio::sync::watch::Sender<bool>>,
}

impl RoomHost {
    /// Host room `id`, nothing loaded yet.
    #[must_use]
    pub fn new(id: &RoomId) -> Self {
        let hosts = Hosts::new(PRESENCE_TIMEOUT_MS);
        let doc = RoomDoc::new();
        // The doc's host stays in `hosts`, which the dispatchers serve.
        let _host = hosts.add(id.doc_id(), doc.loro().clone());
        Self {
            id: id.doc_id(),
            hosts,
            doc,
            stop: Arc::new(tokio::sync::watch::channel(false).0),
        }
    }

    /// The room's doc: what the leader loads into.
    #[must_use]
    pub const fn doc(&self) -> &RoomDoc {
        &self.doc
    }

    /// Stop hosting: the leader's own presence ends. What serves the room
    /// (the caller's endpoint) is the caller's to close.
    pub fn close(&self) {
        let _ = self.stop.send(true);
    }

    /// Members attached now.
    #[must_use]
    pub fn members(&self) -> usize {
        self.hosts.doc(self.id).active_sessions()
    }

    /// Add the room's services to `router`.
    #[must_use]
    pub fn mount(&self, router: architect::LayerRouter) -> architect::LayerRouter {
        router
            .with(
                doc_sync_service_descriptor(),
                DocSyncDispatcher::new(self.hosts.clone()),
            )
            .with(
                doc_presence_service_descriptor(),
                DocPresenceDispatcher::new(self.hosts.clone()),
            )
    }

    /// The leader's own presence in the room, as `me` named `name`: over
    /// an in-process link, the same path every member takes. Its clock is
    /// the room's.
    ///
    /// # Errors
    ///
    /// The in-process link could not be made.
    pub async fn own_presence(&self, me: &str, name: &str) -> eyre::Result<PresencePeer> {
        let server = architect::LocalServer::serve(
            self.mount(architect::LayerRouter::new()),
            architect::Scope::new(),
        );
        let client: DocPresenceClient = server.establish().await?;
        let (peer, mut driver) = PresencePeer::new(self.id, PRESENCE_TIMEOUT_MS);
        peer.set(&here_key(me), here(name));
        let mut stopped = self.stop.subscribe();
        architect::platform::spawn(async move {
            let _server = server;
            tokio::select! {
                res = driver.run(&client) => {
                    if let Err(e) = res {
                        tracing::warn!(room.error = %e, "room: the leader's own presence ended");
                    }
                }
                _ = stopped.wait_for(|s| *s) => {}
            }
        });
        Ok(peer)
    }
}

/// A member's end of a room: replicas of its doc and presence, kept on
/// one attachment for as long as the member is in the room.
pub struct RoomPeer {
    doc: RoomDoc,
    presence: PresencePeer,
    attaches: Arc<AtomicUsize>,
    stop: tokio::sync::watch::Sender<bool>,
}

impl RoomPeer {
    /// Join room `id` as `me` named `name`, through `attach`. Returns at
    /// once; the doc fills as the attachment comes up.
    #[must_use]
    pub fn join(id: &RoomId, me: &str, name: &str, attach: Attach) -> Self {
        let doc = RoomDoc::new();
        let mut synced = SyncedDoc::new(id.doc_id(), doc.loro().clone());
        let (presence, mut driver) = PresencePeer::new(id.doc_id(), PRESENCE_TIMEOUT_MS);
        presence.set(&here_key(me), here(name));
        let attaches = Arc::new(AtomicUsize::new(0));
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        let counted = Arc::clone(&attaches);
        architect::platform::spawn(async move {
            let mut wait = FIRST_WAIT;
            loop {
                let began = architect::platform::now();
                let session = async {
                    let (sync, presence) = attach().await?;
                    counted.fetch_add(1, Ordering::SeqCst);
                    tokio::select! {
                        res = synced.run(&sync) => res.map_err(|e| eyre::eyre!("the room's doc: {e}")),
                        res = driver.run(&presence) => res.map_err(|e| eyre::eyre!("the room's presence: {e}")),
                    }
                };
                tokio::select! {
                    res = session => {
                        if let Err(e) = res {
                            tracing::debug!(room.error = %e, "room: the attachment ended; attaching again");
                        }
                    }
                    _ = stopped.wait_for(|s| *s) => return,
                }
                if began.elapsed() >= HELD {
                    wait = FIRST_WAIT;
                }
                tokio::select! {
                    () = architect::platform::sleep(wait) => {}
                    _ = stopped.wait_for(|s| *s) => return,
                }
                wait = wait.saturating_mul(2).min(LONGEST_WAIT);
            }
        });
        Self {
            doc,
            presence,
            attaches,
            stop,
        }
    }

    /// The member's replica of the room's doc.
    #[must_use]
    pub const fn doc(&self) -> &RoomDoc {
        &self.doc
    }

    /// The member's replica of the room's presence (its clock is the
    /// leader's, estimated).
    #[must_use]
    pub fn presence(&self) -> PresencePeer {
        self.presence.clone()
    }

    /// How many times the member has attached to the leader: once, unless
    /// a connection was lost.
    #[must_use]
    pub fn attaches(&self) -> usize {
        self.attaches.load(Ordering::SeqCst)
    }
}

impl Drop for RoomPeer {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

/// An attachment that lasted this long worked: the next wait starts short.
const HELD: Duration = Duration::from_secs(10);
const FIRST_WAIT: Duration = Duration::from_secs(1);
const LONGEST_WAIT: Duration = Duration::from_secs(15);
