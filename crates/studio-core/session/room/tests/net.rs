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
    // The leader seeing the member says nothing of the other way round:
    // the leader's presence reaches the member on its own path.
    eventually("the member sees the leader", || {
        room::net::people(&member.presence()) == ["Alex", "Cody"]
    })
    .await;

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
        endpoint_as(iroh::SecretKey::generate()).await
    }

    /// [`endpoint`], as the endpoint `key` is: a device that keeps its key.
    async fn endpoint_as(key: iroh::SecretKey) -> iroh::Endpoint {
        iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
            .secret_key(key)
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
        let door = Door::for_room(&id, &pass, &Pass::mint());
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

    /// A guest whose device keeps `key`, dialling `live` (the lobby's).
    async fn guest(id: &RoomId, key: &iroh::SecretKey, live: Live) -> RoomPeer {
        let find: Find = Arc::new(|| Box::pin(async { None }));
        let endpoint = endpoint_as(key.clone()).await;
        let me = endpoint.id().to_string();
        RoomPeer::join_as_guest(id, &me, attach_to(endpoint, live, find))
    }

    /// A guest (studio#713): no account, so no members' pass, only the
    /// guests' from the room's lobby. They follow what is loaded, the room's
    /// doc takes none of their edits while a member's go through, the live
    /// set refuses them, and the same device is the same guest when it
    /// comes back.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_guest_follows_edits_nothing_and_is_known_again() {
        let id = RoomId::parse("adult-jam-3f9a1c").unwrap();
        let host = RoomHost::new(&id);
        let leader = host.own_presence("leader", "Cody").await.unwrap();
        let root = Path::new("/studio");
        let load = |name: &str| {
            Loaded::of_path(&root.join(format!("Setlists/{name}.setlist")), Some(root))
        };
        host.doc().load(&load("Sunday")).unwrap();

        let (pass, guests) = (Pass::mint(), Pass::mint());
        let door = Door::for_room(&id, &pass, &guests);
        let serving = endpoint().await;
        {
            let (door, serving, router) = (
                door.clone(),
                serving.clone(),
                host.mount(architect::LayerRouter::new()),
            );
            tokio::spawn(async move { door.serve(&serving, router).await });
        }

        // The guest follows what is loaded, and is in the room as a guest.
        let key = iroh::SecretKey::generate();
        let first = guest(&id, &key, live_at(&serving, guests.clone())).await;
        eventually("the guest reads what the room has loaded", || {
            first
                .doc()
                .loaded()
                .is_some_and(|(l, _)| l.path == "Setlists/Sunday.setlist")
        })
        .await;
        let name = room::guest_name(&key.public().to_string());
        eventually("the leader sees the guest", || {
            room::net::here_now(&leader)
                .iter()
                .any(|p| p.guest && p.name == name)
        })
        .await;
        assert_eq!(door.take_arrivals(), vec![key.public().to_string()]);

        // The guest edits the room's doc: the leader takes none of it.
        first.doc().load(&load("Forged")).unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            host.doc().loaded().map(|(l, _)| l.path),
            Some("Setlists/Sunday.setlist".to_owned()),
            "the leader took a guest's edit"
        );
        // A guest's own client, made to write what is not theirs: the
        // leader's transport, the leader's own entry, and their own entry
        // claiming to be someone else. The leader keeps none of it; their
        // own entry stays theirs, named by their endpoint.
        let me = key.public().to_string();
        let forged = room::lead::entry(
            None,
            &session_sync::transport::LocalTransport {
                playing: true,
                position: 30.0,
                song: Some("9".to_owned()),
            },
            live_doc::clock_ms(),
            "guest",
        )
        .unwrap();
        let said = |name: &str, kind: &str, opened: i64| {
            let mut m = std::collections::HashMap::new();
            m.insert("name".to_owned(), loro::LoroValue::from(name));
            m.insert("kind".to_owned(), loro::LoroValue::from(kind));
            m.insert("opened".to_owned(), loro::LoroValue::I64(opened));
            loro::LoroValue::from(m)
        };
        let presence = first.presence();
        presence.set(session_sync::transport::KEY, forged.encode());
        presence.set(
            &room::state::here_key("leader"),
            said("Mallory", "agent", 0),
        );
        presence.set(
            &room::state::here_key(&room::net::guest_peer(&me)),
            said("Cody", "agent", 7),
        );
        eventually("the leader reads the guest's own entry", || {
            room::net::here_now(&leader)
                .iter()
                .any(|p| p.guest && p.opened == Some(7))
        })
        .await;
        assert!(
            room::net::transport(&leader).is_none(),
            "a guest wrote the room's transport"
        );
        let people = room::net::here_now(&leader);
        assert!(
            people
                .iter()
                .any(|p| p.name == "Cody" && !p.guest && !p.agent),
            "a guest rewrote the leader's entry: {people:?}"
        );
        assert!(
            people
                .iter()
                .any(|p| p.name == name && p.guest && !p.agent && p.opened == Some(7)),
            "a guest's entry said someone else: {people:?}"
        );
        assert!(!people.iter().any(|p| p.name == "Mallory"), "{people:?}");

        // A member's edit, sent after it the same way, goes through: the
        // guest's was dropped, not slow.
        let alex = member(&id, "alex", live_at(&serving, pass.clone())).await;
        eventually("the member attaches", || alex.doc().loaded().is_some()).await;
        alex.doc().load(&load("Monday")).unwrap();
        eventually("the leader takes the member's edit", || {
            host.doc()
                .loaded()
                .is_some_and(|(l, _)| l.path == "Setlists/Monday.setlist")
        })
        .await;
        assert_eq!(
            door.refused(),
            0,
            "the door refused the guest's edit as a call"
        );

        // The live set the members edit together refuses a guest outright.
        let set_door = Door::for_live_set(&id, &pass);
        let set_serving = endpoint().await;
        {
            let (door, serving, router) = (
                set_door.clone(),
                set_serving.clone(),
                host.mount(architect::LayerRouter::new()),
            );
            tokio::spawn(async move { door.serve(&serving, router).await });
        }
        let intruder = guest(&id, &key, live_at(&set_serving, guests.clone())).await;
        eventually("the live set refuses the guest", || {
            set_door.refused() > 0 && intruder.last_error().is_some()
        })
        .await;
        assert!(intruder.doc().loaded().is_none());
        drop(intruder);

        // The guest leaves and comes back from the same device: the same
        // guest, by the endpoint it proved.
        drop(first);
        let again = guest(&id, &key, live_at(&serving, guests.clone())).await;
        eventually("the guest is back", || again.doc().loaded().is_some()).await;
        assert_eq!(door.take_arrivals(), vec![key.public().to_string()]);

        // Another device is another guest; the members' pass is no guest's.
        let other = iroh::SecretKey::generate();
        let _second = guest(&id, &other, live_at(&serving, guests)).await;
        eventually("the second guest arrives", || {
            room::net::here_now(&leader)
                .iter()
                .any(|p| p.guest && p.name == room::guest_name(&other.public().to_string()))
        })
        .await;
        assert_eq!(door.take_arrivals(), vec![other.public().to_string()]);
        assert!(
            room::net::here_now(&leader)
                .iter()
                .any(|p| p.name == "alex" && !p.guest),
            "a member is in the room as a guest"
        );
    }
}
