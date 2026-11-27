//! The room's door: the permissions gate every call to a room's leader
//! goes through (studio#362, studio#711).
//!
//! A room's leader is reachable by anyone on the internet who has its
//! endpoint, so it admits a call only when the call presents the room's
//! [`Pass`]: the `authorization` bearer a member sends on every call
//! ([`crate::net::Presents`]). What the transport proves about the caller
//! (an iroh peer's endpoint id) is not enough on its own. Every endpoint
//! is somebody's, and the pass is what says it is one of the
//! organization's people.
//!
//! The door covers what the leader serves the room with: the room's doc
//! and presence ([`Door::for_room`]), and the live set its members edit
//! together, which also serves the session's clock
//! ([`Door::for_live_set`]). A method nobody wrote a permit for is refused
//! (the gate fails closed), and every refusal is counted
//! ([`Door::refused`]), so `ROOM` can say how many were turned away.
//!
//! Guests (studio#713) come in at this door too, later, with a pass of
//! their own that admits following and not editing.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use architect::LayerRouter;
use architect::permissions_gate::{PermissionedRouter, PermissionsGate};
use architect_permissions::{
    Action, AuditEvent, AuditSink, BoxIdentityFuture, Decision, IdentityResolver, MethodPermit,
    PermissionEngine, Principal, Resource, ServicePermits, TracingAudit,
};

use crate::record::{Pass, RoomId};

/// The one resource every permit names: the room.
const RESOURCE: &str = "room";

/// The one thing a permit allows: being in the room (following, and
/// editing what is loaded).
const JOIN: &str = "join";

/// The room's doc (what is loaded).
static DOC: ServicePermits = ServicePermits {
    service: "room-doc",
    methods: &[MethodPermit::new("sync", JOIN, RESOURCE)],
};

/// The room's presence (who is here, the transport) and its clock.
static PRESENCE: ServicePermits = ServicePermits {
    service: "room-presence",
    methods: &[
        MethodPermit::new("presence", JOIN, RESOURCE),
        MethodPermit::new("now", JOIN, RESOURCE),
    ],
};

/// The live set's session clock.
static CLOCK: ServicePermits = ServicePermits {
    service: "room-clock",
    methods: &[MethodPermit::new("now", JOIN, RESOURCE)],
};

/// The room's door.
#[derive(Clone)]
pub struct Door {
    gate: Arc<PermissionsGate>,
    refused: Arc<AtomicUsize>,
}

impl Door {
    /// The door to room `room`'s doc and presence, opened by `pass`.
    #[must_use]
    pub fn for_room(room: &RoomId, pass: &Pass) -> Self {
        Self::new(room, pass, |gate| gate)
    }

    /// The door to the live set room `room`'s members edit together
    /// (its songs' docs, their presence and the session's clock), opened
    /// by the same `pass`.
    #[must_use]
    pub fn for_live_set(room: &RoomId, pass: &Pass) -> Self {
        Self::new(room, pass, |gate| {
            gate.permit(
                session_sync::clock::session_clock_service_descriptor(),
                &CLOCK,
            )
        })
    }

    fn new(
        room: &RoomId,
        pass: &Pass,
        more: impl FnOnce(PermissionsGate) -> PermissionsGate,
    ) -> Self {
        let member = member_of(room);
        let refused = Arc::new(AtomicUsize::new(0));
        let gate = PermissionsGate::new(
            Arc::new(Members {
                member: member.clone(),
            }),
            Arc::new(ByPass {
                pass: pass.clone(),
                member,
            }),
        )
        .with_audit(Arc::new(Counted {
            refused: Arc::clone(&refused),
        }))
        .permit(live_doc::doc_sync_service_descriptor(), &DOC)
        .permit(live_doc::doc_presence_service_descriptor(), &PRESENCE);
        Self {
            gate: Arc::new(more(gate)),
            refused,
        }
    }

    /// How many calls the door has refused.
    #[must_use]
    pub fn refused(&self) -> usize {
        self.refused.load(Ordering::SeqCst)
    }

    /// `router` behind the door, for one connection whose transport
    /// proved its caller is `proved` (an iroh peer: `Principal::Host`).
    /// A call that presents no pass is made as `proved`, and refused.
    #[must_use]
    pub fn guard(&self, router: LayerRouter, proved: Principal) -> PermissionedRouter {
        PermissionsGate::wrap_shared_with_proved_caller(Arc::clone(&self.gate), router, proved)
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

/// Who a call presenting room `room`'s pass is.
fn member_of(room: &RoomId) -> Principal {
    Principal::Guest {
        link_id: format!("room:{room}"),
        display: None,
    }
}

/// A call is its room's member when it presents the room's pass; anyone
/// else is nobody.
struct ByPass {
    pass: Pass,
    member: Principal,
}

impl IdentityResolver for ByPass {
    fn resolve<'a>(&'a self, bearer: Option<&'a str>) -> BoxIdentityFuture<'a> {
        let who = if bearer.is_some_and(|b| self.pass.matches(b)) {
            self.member.clone()
        } else {
            Principal::Anonymous
        };
        Box::pin(async move { who })
    }
}

/// The room's members may be in it; nobody else may do anything.
struct Members {
    member: Principal,
}

impl PermissionEngine for Members {
    fn check(&self, who: &Principal, what: &Resource, action: &Action) -> Decision {
        if *who == self.member && what.as_str() == RESOURCE && action.as_str() == JOIN {
            Decision::Allow
        } else {
            Decision::Deny {
                reason: format!("{} has not presented the room's pass", who.describe()),
            }
        }
    }
}

/// Each refusal counted, and said as the gate says it (`tracing`).
struct Counted {
    refused: Arc<AtomicUsize>,
}

impl AuditSink for Counted {
    fn record(&self, event: AuditEvent) {
        if !event.allowed {
            self.refused.fetch_add(1, Ordering::SeqCst);
        }
        TracingAudit.record(event);
    }
}
