//! The room over vox: the leader hosts it, a member attaches once and
//! follows every load and the transport on that one attachment.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails loudly"
)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use room::net::{Attach, RoomHost, RoomPeer};
use room::{Loaded, RoomId};
use session_sync::transport::{KEY, SharedTransport};

/// Wait up to two seconds for `ok`.
async fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
    for _ in 0..200 {
        if ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what}: not within two seconds");
}

/// A member's attachments to `host`, in-process, counted.
fn attach(host: &RoomHost, dials: Arc<AtomicUsize>) -> Attach {
    let server = Arc::new(architect::LocalServer::serve(
        host.mount(architect::LayerRouter::new()),
        architect::Scope::new(),
    ));
    Arc::new(move || {
        let (server, dials) = (Arc::clone(&server), Arc::clone(&dials));
        Box::pin(async move {
            dials.fetch_add(1, Ordering::SeqCst);
            Ok((server.establish().await?, server.establish().await?))
        })
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_follows_every_load_on_one_attachment() {
    let id = RoomId::parse("adult-jam-3f9a1c").unwrap();
    let host = RoomHost::new(&id);
    let leader = host.own_presence("leader", "Cody").await.unwrap();
    let root = Path::new("/studio");
    host.doc()
        .load(&Loaded::of_path(
            &root.join("Setlists/Sunday.setlist"),
            Some(root),
        ))
        .unwrap();

    let dials = Arc::new(AtomicUsize::new(0));
    let member = RoomPeer::join(&id, "member-1", "Alex", attach(&host, Arc::clone(&dials)));

    eventually("the member has the first setlist", || {
        member
            .doc()
            .loaded()
            .is_some_and(|(l, _)| l.path == "Setlists/Sunday.setlist")
    })
    .await;
    eventually("the leader sees the member", || {
        leader
            .states()
            .contains_key(&room::state::here_key("member-1"))
    })
    .await;
    assert_eq!(room::net::people(&member.presence()), vec!["Alex", "Cody"]);

    // The leader plays: the member reads the transport.
    let entry = room::lead::entry(
        None,
        &session_sync::transport::LocalTransport {
            playing: true,
            position: 4.0,
            song: Some("1".to_owned()),
        },
        live_doc::clock_ms(),
        "leader",
    )
    .unwrap();
    leader.set(KEY, entry.encode());
    eventually("the member reads the leader's transport", || {
        room::net::transport(&member.presence()).is_some_and(|t: SharedTransport| t.playing)
    })
    .await;

    // Another setlist, mid-rehearsal.
    host.doc()
        .load(&Loaded::of_path(
            &root.join("Setlists/Monday.setlist"),
            Some(root),
        ))
        .unwrap();
    eventually("the member moves to the second setlist", || {
        member
            .doc()
            .loaded()
            .is_some_and(|(l, _)| l.path == "Setlists/Monday.setlist")
    })
    .await;
    assert_eq!(member.attaches(), 1, "the member never attached again");
    assert_eq!(dials.load(Ordering::SeqCst), 1);
    assert_eq!(host.members(), 1);
}

/// An agent working through a member's MCP is in the room as a person is:
/// the leader lists it among the people, and as an agent, until it goes.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_through_a_member_is_in_the_room() {
    let id = RoomId::parse("adult-jam-3f9a1c").unwrap();
    let host = RoomHost::new(&id);
    let leader = host.own_presence("leader", "Cody").await.unwrap();
    let member = RoomPeer::join(
        &id,
        "member-1",
        "Alex",
        attach(&host, Arc::new(AtomicUsize::new(0))),
    );

    room::net::agent_arrived(&member.presence(), "member-1", "Claude");
    eventually("the leader sees the agent", || {
        room::net::agents(&leader) == vec!["Claude"]
    })
    .await;
    assert_eq!(room::net::people(&leader), vec!["Alex", "Claude", "Cody"]);

    room::net::agent_left(&member.presence(), "member-1", "Claude");
    eventually("the agent is gone from the leader's room", || {
        room::net::agents(&leader).is_empty()
    })
    .await;
    assert_eq!(room::net::people(&leader), vec!["Alex", "Cody"]);
}

/// The leader's door (studio#362, studio#711): a room is open to the
/// internet, so its leader admits only a member who presents the room's
/// pass. Over real iroh endpoints on loopback (no relay, no internet), as a
/// member reaches a leader on a device.
mod door {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use architect::iroh_link::{VOX_ALPN, iroh};
    use room::gate::Door;
    use room::net::{Find, RoomHost, RoomPeer, attach_to};
    use room::{Live, Loaded, Pass, RoomId};

    /// Wait up to ten seconds for `ok` (a refused member is dialled again on a
    /// backoff that starts at a second).
    async fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
        for _ in 0..1000 {
            if ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("{what}: not within ten seconds");
    }

    /// An endpoint on loopback with no relay and no lookup: the room's lane
    /// and nothing else.
    async fn endpoint() -> iroh::Endpoint {
        iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
            .relay_mode(iroh::RelayMode::Disabled)
            .alpns(vec![VOX_ALPN.to_vec()])
            .bind()
            .await
            .unwrap()
    }

    /// Where a member dials `endpoint` on this machine.
    fn live_at(endpoint: &iroh::Endpoint, pass: Pass) -> Live {
        let addrs = endpoint
            .bound_sockets()
            .into_iter()
            .map(|s| {
                let ip: std::net::IpAddr = if s.is_ipv4() {
                    std::net::Ipv4Addr::LOCALHOST.into()
                } else {
                    std::net::Ipv6Addr::LOCALHOST.into()
                };
                std::net::SocketAddr::new(ip, s.port()).to_string()
            })
            .collect();
        Live {
            leader: "Cody".to_owned(),
            endpoint: endpoint.id().to_string(),
            addrs,
            relays: Vec::new(),
            pass,
        }
    }

    /// A member of `id` named `name`, dialling `live` from an endpoint of its
    /// own.
    async fn member(id: &RoomId, name: &str, live: Live) -> RoomPeer {
        let find: Find = Arc::new(|| Box::pin(async { None }));
        RoomPeer::join(id, name, name, attach_to(endpoint().await, live, find))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_leader_admits_only_who_presents_the_rooms_pass() {
        let id = RoomId::parse("adult-jam-3f9a1c").unwrap();
        let host = RoomHost::new(&id);
        let _leader = host.own_presence("leader", "Cody").await.unwrap();
        let root = Path::new("/studio");
        host.doc()
            .load(&Loaded::of_path(
                &root.join("Setlists/Sunday.setlist"),
                Some(root),
            ))
            .unwrap();

        let pass = Pass::mint();
        let door = Door::for_room(&id, &pass);
        let serving = endpoint().await;
        {
            let (door, serving, router) = (
                door.clone(),
                serving.clone(),
                host.mount(architect::LayerRouter::new()),
            );
            tokio::spawn(async move { door.serve(&serving, router).await });
        }

        // Someone with the room's endpoint and no pass, and someone with a
        // pass of their own making: neither is let in.
        let stranger = member(&id, "stranger", live_at(&serving, Pass::default())).await;
        let forger = member(&id, "forger", live_at(&serving, Pass::mint())).await;
        eventually("the door refuses both", || {
            door.refused() >= 2 && stranger.last_error().is_some() && forger.last_error().is_some()
        })
        .await;
        for refused in [&stranger, &forger] {
            let why = refused.last_error().unwrap();
            assert!(why.contains("permission denied"), "refused for: {why}");
            assert!(
                refused.doc().loaded().is_none(),
                "a refused member read what the room has loaded"
            );
        }

        // The organization's member, with the pass from the room's file.
        let alex = member(&id, "alex", live_at(&serving, pass.clone())).await;
        eventually("the member reads what the room has loaded", || {
            alex.doc()
                .loaded()
                .is_some_and(|(l, _)| l.path == "Setlists/Sunday.setlist")
        })
        .await;
        assert_eq!(alex.last_error(), None);
        assert_eq!(alex.attaches(), 1);
        assert!(
            !room::net::people(&alex.presence()).contains(&"stranger".to_owned()),
            "a refused member is in the room"
        );
        assert!(stranger.doc().loaded().is_none());
        assert!(forger.doc().loaded().is_none());
    }
}
