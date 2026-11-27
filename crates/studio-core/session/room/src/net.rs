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
//! devices ([`attach_to`], which a desktop's engine and a browser's page
//! both join by), an in-process link in tests.
//!
//! Every call a member makes to the leader presents the room's pass
//! ([`Presents`]): the leader's door refuses one that does not
//! (`crate::gate`).
//!
//! It builds for the browser too: a page is a member as a desktop is, its
//! endpoint relay-only ([`bind_in_browser`]).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use architect::iroh_link::iroh;

use live_doc::{
    DocPresenceClient, DocPresenceDispatcher, DocSyncClient, DocSyncDispatcher, Hosts,
    PresencePeer, SyncedDoc, doc_presence_service_descriptor, doc_sync_service_descriptor,
};
use loro::LoroValue;
use session_sync::transport::{KEY, SharedTransport};

use crate::record::{Live, Pass, RoomId};
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

/// What a person says about themselves in the room: their name, and the
/// load they have opened (a member, once it has: what the leader sees it
/// follow).
fn here(name: &str, opened: Option<u64>) -> LoroValue {
    let mut map = std::collections::HashMap::new();
    map.insert("name".to_owned(), LoroValue::from(name));
    if let Some(load) = opened.and_then(|l| i64::try_from(l).ok()) {
        map.insert("opened".to_owned(), LoroValue::I64(load));
    }
    LoroValue::from(map)
}

/// What an agent working through a member's MCP says about itself in
/// the room: its name, and that it is an agent.
fn agent_here(name: &str) -> LoroValue {
    let mut map = std::collections::HashMap::new();
    map.insert("name".to_owned(), LoroValue::from(name));
    map.insert("kind".to_owned(), LoroValue::from("agent"));
    LoroValue::from(map)
}

/// The presence key agent `name` is here under, written by the member
/// whose MCP it works through (`by`): one entry per agent per member.
#[must_use]
pub fn agent_key(by: &str, name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    here_key(&format!("{by}~agent-{slug}"))
}

/// Say in `presence` that agent `name` is here, through member `by`.
pub fn agent_arrived(presence: &PresencePeer, by: &str, name: &str) {
    presence.set(&agent_key(by, name), agent_here(name));
}

/// Say in `presence` that agent `name`, through member `by`, has gone.
pub fn agent_left(presence: &PresencePeer, by: &str, name: &str) {
    presence.delete(&agent_key(by, name));
}

/// Someone in the room, as they say in its presence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub name: String,
    /// An agent working through someone's MCP.
    pub agent: bool,
    /// The load they have opened: a member following the leader says so
    /// once what the leader loaded is open on their device.
    pub opened: Option<u64>,
}

/// Everyone in the room, from `presence`, sorted by name.
#[must_use]
pub fn here_now(presence: &PresencePeer) -> Vec<Person> {
    let mut people: Vec<Person> = presence
        .states()
        .into_iter()
        .filter(|(k, _)| k.ends_with("/here"))
        .filter_map(|(_, v)| match v {
            LoroValue::Map(m) => match m.get("name") {
                Some(LoroValue::String(s)) => Some(Person {
                    name: s.to_string(),
                    agent: matches!(m.get("kind"), Some(LoroValue::String(k)) if &**k == "agent"),
                    opened: match m.get("opened") {
                        Some(LoroValue::I64(n)) => u64::try_from(*n).ok(),
                        _ => None,
                    },
                }),
                _ => None,
            },
            _ => None,
        })
        .collect();
    people.sort_by(|a, b| a.name.cmp(&b.name));
    people
}

/// Who is here, from `presence`: each one's name, and whether they are an
/// agent.
fn here_entries(presence: &PresencePeer) -> Vec<(String, bool)> {
    here_now(presence)
        .into_iter()
        .map(|p| (p.name, p.agent))
        .collect()
}

/// Everyone in the room, by name, sorted: what `presence` holds. An
/// agent is in it as a person is.
#[must_use]
pub fn people(presence: &PresencePeer) -> Vec<String> {
    let mut names: Vec<String> = here_entries(presence).into_iter().map(|(n, _)| n).collect();
    names.sort();
    names
}

/// The agents in the room, by name, sorted.
#[must_use]
pub fn agents(presence: &PresencePeer) -> Vec<String> {
    let mut names: Vec<String> = here_entries(presence)
        .into_iter()
        .filter(|(_, agent)| *agent)
        .map(|(n, _)| n)
        .collect();
    names.sort();
    names.dedup();
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
        peer.set(&here_key(me), here(name, None));
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
    /// Who this member is in the room's presence, and their name.
    me: String,
    name: String,
    attaches: Arc<AtomicUsize>,
    /// Why the last attachment ended, or could not be made (the leader's
    /// door refusing it, say); cleared once one is held.
    error: Arc<Mutex<Option<String>>>,
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
        presence.set(&here_key(me), here(name, None));
        let attaches = Arc::new(AtomicUsize::new(0));
        let error = Arc::new(Mutex::new(None));
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        let (counted, said) = (Arc::clone(&attaches), Arc::clone(&error));
        architect::platform::spawn(async move {
            let mut wait = FIRST_WAIT;
            loop {
                let began = architect::platform::now();
                let session = async {
                    let (sync, presence) = attach().await?;
                    counted.fetch_add(1, Ordering::SeqCst);
                    // Held this long, it works: what went wrong before is
                    // no longer so.
                    let held = async {
                        architect::platform::sleep(HELD).await;
                        *lock(&said) = None;
                        std::future::pending::<()>().await;
                    };
                    tokio::select! {
                        res = synced.run(&sync) => res.map_err(|e| eyre::eyre!("the room's doc: {e}")),
                        res = driver.run(&presence) => res.map_err(|e| eyre::eyre!("the room's presence: {e}")),
                        () = held => Ok(()),
                    }
                };
                tokio::select! {
                    res = session => {
                        if let Err(e) = res {
                            tracing::debug!(room.error = %e, "room: the attachment ended; attaching again");
                            *lock(&said) = Some(format!("{e:#}"));
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
            me: me.to_owned(),
            name: name.to_owned(),
            attaches,
            error,
            stop,
        }
    }

    /// Say in the room that this member has opened load `load`: what the
    /// leader (and its MCP) sees it follow.
    pub fn opened(&self, load: u64) {
        self.presence
            .set(&here_key(&self.me), here(&self.name, Some(load)));
    }

    /// Why the last attachment to the leader ended or was not made, until
    /// one holds again: the leader refusing this member, a leader gone.
    #[must_use]
    pub fn last_error(&self) -> Option<String> {
        lock(&self.error).clone()
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

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A member's calls to the leader, each presenting the room's pass as its
/// `authorization` bearer: what the leader's door admits it by.
#[derive(Clone)]
pub struct Presents(pub Pass);

impl vox::ClientMiddleware for Presents {
    fn pre<'a>(
        &'a self,
        _context: &'a vox::ClientContext<'a>,
        request: &'a mut vox::ClientRequest<'_, 'a>,
    ) -> vox::BoxMiddlewareFuture<'a> {
        if !self.0.is_empty() {
            request.push_string_metadata(
                architect::permissions_gate::AUTHORIZATION_METADATA_KEY,
                format!("Bearer {}", self.0.expose()),
            );
        }
        Box::pin(async {})
    }
}

/// Where to dial the leader `live` says: its endpoint, at its addresses
/// and through its relays.
///
/// # Errors
///
/// The leader's endpoint is not an endpoint id.
pub fn leader_addr(live: &Live) -> Result<iroh::EndpointAddr, String> {
    let leader: iroh::EndpointId = live
        .endpoint
        .parse()
        .map_err(|e| format!("the leader's endpoint: {e}"))?;
    let mut addr = iroh::EndpointAddr::new(leader);
    for a in &live.addrs {
        match a.parse() {
            Ok(socket) => addr = addr.with_ip_addr(socket),
            Err(e) => {
                tracing::debug!(room.error = %e, "room: an address of the leader's that is none")
            }
        }
    }
    for r in &live.relays {
        match r.parse() {
            Ok(relay) => addr = addr.with_relay_url(relay),
            Err(e) => {
                tracing::debug!(room.error = %e, "room: a relay of the leader's that is none")
            }
        }
    }
    Ok(addr)
}

/// What says where the leader is now: the room's file read again (a
/// leader back after a restart is at new addresses, with a new pass), or
/// `None` when it cannot be read.
pub type Find =
    Arc<dyn Fn() -> architect::platform::BoxFuture<'static, Option<Live>> + Send + Sync>;

/// A member's attachments to the leader, over iroh from `endpoint`: each
/// one finds the leader again (`find`, else `joined`, where it was when
/// the member joined), dials it, and presents the room's pass on every
/// call.
#[must_use]
pub fn attach_to(endpoint: iroh::Endpoint, joined: Live, find: Find) -> Attach {
    Arc::new(move || {
        let (endpoint, joined, find) = (endpoint.clone(), joined.clone(), Arc::clone(&find));
        Box::pin(async move {
            let live = find().await.unwrap_or(joined);
            let addr = leader_addr(&live).map_err(|e| eyre::eyre!(e))?;
            let presents = Presents(live.pass);
            let dial = async || {
                architect::iroh_link::connect(&endpoint, addr.clone())
                    .await
                    .map_err(|e| eyre::eyre!("iroh connect: {e}"))
            };
            let sync = vox_core::initiator_on(dial().await?)
                .establish::<DocSyncClient>()
                .await
                .map_err(|e| eyre::eyre!("the room's doc: {e:?}"))?
                .with_middleware(presents.clone());
            let presence = vox_core::initiator_on(dial().await?)
                .establish::<DocPresenceClient>()
                .await
                .map_err(|e| eyre::eyre!("the room's presence: {e:?}"))?
                .with_middleware(presents);
            Ok((sync, presence))
        })
    })
}

/// A member's endpoint in a browser: n0's relays and lookup, the only way
/// a page reaches a device (it has no sockets of its own). Each page's is
/// new.
///
/// # Errors
///
/// The endpoint could not be bound (no relay reached).
#[cfg(target_arch = "wasm32")]
pub async fn bind_in_browser() -> Result<iroh::Endpoint, String> {
    architect::iroh_link::bind_endpoint_with(
        iroh::SecretKey::generate(),
        architect::iroh_link::PeerLookup::N0,
    )
    .await
    .map_err(|e| format!("iroh: {e}"))
}

/// An attachment that lasted this long worked: the next wait starts short.
const HELD: Duration = Duration::from_secs(10);
const FIRST_WAIT: Duration = Duration::from_secs(1);
const LONGEST_WAIT: Duration = Duration::from_secs(15);
