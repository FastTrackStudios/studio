//! The leader's door (studio#362, studio#711): a room is open to the
//! internet, so its leader admits only a member who presents the room's
//! pass. Over real iroh endpoints on loopback (no relay, no internet), as a
//! member reaches a leader on a device.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails loudly"
)]

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
