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
