//! The room's door: the permissions gate every call to a room's leader
//! goes through (studio#362, studio#711, studio#713).
//!
//! A room's leader is reachable by anyone on the internet who has its
//! endpoint, so it admits a call only when the call presents a pass: the
//! `authorization` bearer every call carries ([`crate::net::Presents`]).
//! What the transport proves about the caller (an iroh peer's endpoint id)
//! is not enough on its own. Every endpoint is somebody's.
//!
//! - **Members** present the room's [`Pass`], which only the
//!   organization's people can read (the room's file). A member follows
//!   the leader and edits what is loaded.
//! - **Guests** present the guests' pass, which the room's lobby holds for
//!   anyone with the link (`crate::record::Lobby`). A guest follows and
//!   plays along, and edits nothing: the room's doc takes none of a
//!   guest's updates ([`follows_only`], which the leader's doc host asks),
//!   and the live set its members edit together refuses a guest outright.
//!   A guest is known by the endpoint their connection proved, never by
//!   anything they say, so the same device is the same guest every time it
//!   comes ([`Door::take_arrivals`]).
//!
//! The door covers what the leader serves the room with: the room's doc
//! and presence ([`Door::for_room`]), and the live set its members edit
//! together, which also serves the session's clock
//! ([`Door::for_live_set`]). A method nobody wrote a permit for is refused
//! (the gate fails closed), and every refusal is counted
//! ([`Door::refused`]), so `ROOM` can say how many were turned away.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use architect::LayerRouter;
use architect::permissions_gate::{PermissionedRouter, PermissionsGate};
use architect_permissions::{
    Action, AuditEvent, AuditSink, BoxIdentityFuture, Decision, IdentityResolver, MethodPermit,
    PermissionEngine, Principal, Resource, ServicePermits, TracingAudit,
};

use crate::record::{Pass, RoomId};

/// The one resource every permit names: the room.
const RESOURCE: &str = "room";

/// Reading what the room has loaded, and its clock: a guest may.
const FOLLOW: &str = "follow";

/// Being in the room's presence (each held-open call one arrival): a
/// guest may.
const ARRIVE: &str = "arrive";

/// Anything in the live set the room edits together: members only.
const EDIT: &str = "edit";

/// The room's doc (what is loaded).
static DOC: ServicePermits = ServicePermits {
    service: "room-doc",
    methods: &[MethodPermit::new("sync", FOLLOW, RESOURCE)],
};

/// The room's presence (who is here, the transport) and its clock.
static PRESENCE: ServicePermits = ServicePermits {
    service: "room-presence",
    methods: &[
        MethodPermit::new("presence", ARRIVE, RESOURCE),
        MethodPermit::new("now", FOLLOW, RESOURCE),
    ],
};

/// The live set's songs' docs.
static SET_DOC: ServicePermits = ServicePermits {
    service: "room-set-doc",
    methods: &[MethodPermit::new("sync", EDIT, RESOURCE)],
};

/// The live set's presence.
static SET_PRESENCE: ServicePermits = ServicePermits {
    service: "room-set-presence",
    methods: &[
        MethodPermit::new("presence", EDIT, RESOURCE),
        MethodPermit::new("now", EDIT, RESOURCE),
    ],
};

/// The live set's session clock.
static CLOCK: ServicePermits = ServicePermits {
    service: "room-clock",
    methods: &[MethodPermit::new("now", EDIT, RESOURCE)],
};

/// Which of the leader's lanes a door is in front of.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lane {
    Room,
    LiveSet,
}

/// The room's door.
#[derive(Clone)]
pub struct Door {
    inner: Arc<Inner>,
}

struct Inner {
    room: RoomId,
    lane: Lane,
    /// The members' pass, and the guests' (none: no guests here).
    pass: Pass,
    guests: Pass,
    refused: AtomicUsize,
    /// The endpoints of the guests who arrived since they were last
    /// taken, in order (one each time a guest's presence is admitted).
    arrivals: Mutex<Vec<String>>,
}

impl Door {
    /// The door to room `room`'s doc and presence: members by `pass`,
    /// guests by `guests` (an empty pass: the room has no guests).
    #[must_use]
    pub fn for_room(room: &RoomId, pass: &Pass, guests: &Pass) -> Self {
        Self::new(room, Lane::Room, pass, guests)
    }

    /// The door to the live set room `room`'s members edit together (its
    /// songs' docs, their presence and the session's clock), opened by the
    /// members' `pass` alone: a guest edits nothing.
    #[must_use]
    pub fn for_live_set(room: &RoomId, pass: &Pass) -> Self {
        Self::new(room, Lane::LiveSet, pass, &Pass::default())
    }

    fn new(room: &RoomId, lane: Lane, pass: &Pass, guests: &Pass) -> Self {
        Self {
            inner: Arc::new(Inner {
                room: room.clone(),
                lane,
                pass: pass.clone(),
                guests: guests.clone(),
                refused: AtomicUsize::new(0),
                arrivals: Mutex::new(Vec::new()),
            }),
        }
    }

    /// How many calls the door has refused.
    #[must_use]
    pub fn refused(&self) -> usize {
        self.inner.refused.load(Ordering::SeqCst)
    }

    /// The guests who arrived since this was last asked, by the endpoint
    /// each one's device proved, in order: one each time a guest came into
    /// the room's presence (a guest coming back is in it again).
    #[must_use]
    pub fn take_arrivals(&self) -> Vec<String> {
        std::mem::take(&mut *lock(&self.inner.arrivals))
    }

    /// The gate for one connection whose transport proved its caller is
    /// `proved`: who presents which pass is resolved for that caller.
    fn gate(&self, proved: &Principal) -> PermissionsGate {
        let endpoint = match proved {
            Principal::Host { endpoint } => Some(endpoint.clone()),
            _ => None,
        };
        let gate = PermissionsGate::new(
            Arc::new(Rules {
                door: Arc::clone(&self.inner),
                guest_endpoint: endpoint.clone(),
            }),
            Arc::new(ByPass {
                door: Arc::clone(&self.inner),
                endpoint,
            }),
        )
        .with_audit(Arc::new(Counted {
            door: Arc::clone(&self.inner),
        }));
        match self.inner.lane {
            Lane::Room => gate
                .permit(live_doc::doc_sync_service_descriptor(), &DOC)
                .permit(live_doc::doc_presence_service_descriptor(), &PRESENCE),
            Lane::LiveSet => gate
                .permit(live_doc::doc_sync_service_descriptor(), &SET_DOC)
                .permit(live_doc::doc_presence_service_descriptor(), &SET_PRESENCE)
                .permit(
                    session_sync::clock::session_clock_service_descriptor(),
                    &CLOCK,
                ),
        }
    }

    /// `router` behind the door, for one connection whose transport
    /// proved its caller is `proved` (an iroh peer: `Principal::Host`).
    /// A call that presents no pass is made as `proved`, and refused. A
    /// guest is known by `proved`'s endpoint: with none, the guests' pass
    /// opens nothing.
    #[must_use]
    pub fn guard(&self, router: LayerRouter, proved: Principal) -> PermissionedRouter {
        let gate = Arc::new(self.gate(&proved));
        PermissionsGate::wrap_shared_with_proved_caller(gate, router, proved)
    }

    /// Serve `router` on `endpoint` behind the door, each peer calling as
    /// the endpoint its handshake proved: until the endpoint closes.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn serve(
        &self,
        endpoint: &architect::iroh_link::iroh::Endpoint,
        router: LayerRouter,
    ) {
        let door = self.clone();
        architect::iroh_link::serve_peers(endpoint, move |remote| {
            door.guard(
                router.clone(),
                Principal::Host {
                    endpoint: remote.to_string(),
                },
            )
        })
        .await;
    }
}

/// The prefix of a guest's principal: `room:<room>/guest:<endpoint>`.
const GUEST: &str = "/guest:";

/// Who a call presenting room `room`'s members' pass is.
fn member_of(room: &RoomId) -> Principal {
    Principal::Guest {
        link_id: format!("room:{room}"),
        display: None,
    }
}

/// Who a call presenting room `room`'s guests' pass, over a connection
/// that proved `endpoint`, is.
fn guest_of(room: &RoomId, endpoint: &str) -> Principal {
    Principal::Guest {
        link_id: format!("room:{room}{GUEST}{endpoint}"),
        display: None,
    }
}

/// The endpoint of the guest `who` is, when `who` is one of a room's
/// guests.
fn guest_endpoint(who: &Principal) -> Option<&str> {
    match who {
        Principal::Guest { link_id, .. } if link_id.starts_with("room:") => link_id
            .split_once(GUEST)
            .map(|(_, endpoint)| endpoint)
            .filter(|e| !e.is_empty()),
        _ => None,
    }
}

/// Whether the call running now is a room's guest's: what the leader's
/// doc host asks as a joiner attaches, so a guest's updates are dropped
/// (`live_doc::Hosts::follow_only_when`). Outside any gate (the leader's
/// own, in-process), nobody's.
#[must_use]
pub fn follows_only() -> bool {
    architect::permissions_gate::caller().is_some_and(|who| guest_endpoint(&who).is_some())
}

/// A call is a member when it presents the room's pass, a guest when it
/// presents the guests' over a connection that proved its endpoint, and
/// nobody otherwise.
struct ByPass {
    door: Arc<Inner>,
    endpoint: Option<String>,
}

impl IdentityResolver for ByPass {
    fn resolve<'a>(&'a self, bearer: Option<&'a str>) -> BoxIdentityFuture<'a> {
        let door = &self.door;
        let who = match bearer {
            Some(b) if door.pass.matches(b) => member_of(&door.room),
            Some(b) if door.guests.matches(b) => self
                .endpoint
                .as_deref()
                .map_or(Principal::Anonymous, |e| guest_of(&door.room, e)),
            _ => Principal::Anonymous,
        };
        Box::pin(async move { who })
    }
}

/// The room's members may do anything the door has a permit for; its
/// guests may follow and arrive (each arrival noted); nobody else may do
/// anything.
struct Rules {
    door: Arc<Inner>,
    /// The endpoint the connection proved: the only guest it can be.
    guest_endpoint: Option<String>,
}

impl PermissionEngine for Rules {
    fn check(&self, who: &Principal, what: &Resource, action: &Action) -> Decision {
        let door = &self.door;
        if what.as_str() != RESOURCE {
            return Decision::Deny {
                reason: format!("{} is not the room", what.as_str()),
            };
        }
        if *who == member_of(&door.room) {
            return Decision::Allow;
        }
        // This room's guest, on the connection that proved them.
        let guest = guest_endpoint(who).filter(|e| {
            self.guest_endpoint.as_deref() == Some(*e) && *who == guest_of(&door.room, e)
        });
        match (guest, action.as_str()) {
            (Some(endpoint), ARRIVE) => {
                lock(&door.arrivals).push(endpoint.to_owned());
                Decision::Allow
            }
            (Some(_), FOLLOW) => Decision::Allow,
            (Some(_), _) => Decision::Deny {
                reason: "a guest follows the room and edits nothing".to_owned(),
            },
            (None, _) => Decision::Deny {
                reason: format!("{} has not presented the room's pass", who.describe()),
            },
        }
    }
}

/// Each refusal counted, and said as the gate says it (`tracing`).
struct Counted {
    door: Arc<Inner>,
}

impl AuditSink for Counted {
    fn record(&self, event: AuditEvent) {
        if !event.allowed {
            self.door.refused.fetch_add(1, Ordering::SeqCst);
        }
        TracingAudit.record(event);
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
