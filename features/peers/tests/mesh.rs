//! Two Studio engines of one account, end to end: a real issuer
//! (in-memory SQLite, in-process vox) introduces them, and they talk over
//! real iroh endpoints on loopback — after the style of Architect's
//! `features/devices/tests/peer_to_peer.rs`.
//!
//! - Each finds and admits the other through the account pass alone; an
//!   engine off the account is refused.
//! - Each says what it runs; either may follow the other's song and
//!   transport — the desk's tracks driving the iPad's rig, and the other
//!   way round after the roles swap.
//! - A rig book edited on either side arrives on the other, with the
//!   issuer stopped and no hub.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use architect::LayerRouter;
use architect::iroh_link::VOX_ALPN;
use architect::{LocalServer, Scope};
use architect_devices::account::ThisDevice;
use auth::transport::vox::AuthVoxService;
use auth::{AuthServiceClient, DeviceServiceClient, SignUpEmailPassword};
use crdt::docs::{CreateDoc, DocsDriver, InMemoryDocsIndex, Lineage, SharedStorage};
use crdt::in_memory::InMemoryPersistence;
use studio_peers::mesh::{FollowTarget, Mesh};
use studio_peers::{About, FollowAction, Here, NowPlaying, Roles};
use uuid::Uuid;

const RECITAL: [&str; 10] = [
    "DTLA Cinematic Intro",
    "Drive To LA",
    "For You",
    "Heart Isn't Ready",
    "Hey!",
    "She's American",
    "WORST PART OF YOU",
    "Fire",
    "Not Too Much Longer (Live Arrangement)",
    "Know Me (Extended)",
];

// ── The issuer ─────────────────────────────────────────────────────────

struct Issuer {
    server: LocalServer,
    devices: DeviceServiceClient,
}

async fn issuer() -> Issuer {
    use auth::backend_db::{AuthSeaOrmStorage, Migrator};
    use sea_orm_migration::MigratorTrait;

    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    let auth = auth::ArchitectAuth::builder()
        .secret("a-secret-at-least-32-bytes-long!!")
        .storage(AuthSeaOrmStorage::new(db))
        .build()
        .unwrap();
    let router = LayerRouter::new()
        .merge(auth::auth_service_layer(AuthVoxService::new(auth.clone())))
        .merge(auth::device_service_layer(AuthVoxService::new(auth)));
    let server = LocalServer::serve(router, Scope::new());
    let devices = server.establish().await.unwrap();
    Issuer { server, devices }
}

async fn sign_up(issuer: &Issuer, email: &str) -> String {
    let session: AuthServiceClient = issuer.server.establish().await.unwrap();
    session
        .sign_up_email_password(SignUpEmailPassword {
            email: email.into(),
            password: "correct horse battery staple".into(),
            name: Some("Band Member".into()),
            username: None,
            image: None,
            metadata_json: None,
            ip_address: None,
            user_agent: Some("studio-peers-test".into()),
        })
        .await
        .unwrap()
        .token
}

// ── An engine ──────────────────────────────────────────────────────────

async fn loopback() -> iroh::Endpoint {
    iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![VOX_ALPN.to_vec()])
        .bind_addr("127.0.0.1:0")
        .unwrap()
        .bind()
        .await
        .unwrap()
}

fn addr(ep: &iroh::Endpoint) -> iroh::EndpointAddr {
    iroh::EndpointAddr::from_parts(
        ep.id(),
        ep.bound_sockets().into_iter().map(iroh::TransportAddr::Ip),
    )
}

struct Engine {
    mesh: Mesh,
    driver: DocsDriver,
    me: ThisDevice,
}

async fn engine(name: &str, platform: &str, roles: Roles) -> Engine {
    let driver = DocsDriver::new(
        InMemoryDocsIndex::new(),
        SharedStorage(Arc::new(InMemoryPersistence::new())),
    );
    let mesh = Mesh::start(
        loopback().await,
        About {
            name: name.into(),
            platform: platform.into(),
            roles,
            ..About::default()
        },
        Some(driver.clone()),
    )
    .await
    .unwrap();
    let me = studio_peers::mesh::this_engine(Uuid::new_v4(), "acct", name, platform);
    Engine { mesh, driver, me }
}

/// Each knows where the other is (the network's lookup, on a real LAN).
fn introduce(a: &Engine, b: &Engine) {
    a.mesh.learn_addr(addr(b.mesh.endpoint()));
    b.mesh.learn_addr(addr(a.mesh.endpoint()));
}

async fn eventually(what: &str, mut ok: impl AsyncFnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while !ok().await {
        assert!(std::time::Instant::now() < deadline, "never: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ── A rig that follows ─────────────────────────────────────────────────

/// A rig as the follow drives it: its set, the song and part up, its
/// transport; and every step it was told.
#[derive(Default)]
struct Rig {
    here: Mutex<Here>,
    told: Mutex<Vec<FollowAction>>,
}

impl Rig {
    fn with_recital() -> Arc<Self> {
        let rig = Self::default();
        {
            let mut h = rig.here.lock().unwrap();
            h.songs = RECITAL.map(String::from).to_vec();
            h.parts = vec!["Intro".into(), "Verse".into(), "Chorus".into()];
            h.bpm = 120.0;
        }
        Arc::new(rig)
    }

    fn song(&self) -> String {
        let h = self.here.lock().unwrap();
        h.songs[h.song_index].clone()
    }
}

impl FollowTarget for Rig {
    fn here(&self) -> Here {
        self.here.lock().unwrap().clone()
    }

    fn apply(&self, actions: &[FollowAction]) {
        let mut h = self.here.lock().unwrap();
        for a in actions {
            match a {
                FollowAction::Song { index, .. } => h.song_index = *index,
                FollowAction::Part { index, .. } => h.part_index = *index,
                FollowAction::Tempo(bpm) => h.bpm = *bpm,
                FollowAction::Play => h.playing = true,
                FollowAction::Stop => h.playing = false,
                FollowAction::Seek(t) => h.position = *t,
                FollowAction::Missing { .. } | FollowAction::Cue { .. } => {}
            }
        }
        self.told.lock().unwrap().extend_from_slice(actions);
    }
}

fn playing(song: &str, index: u32, section: &str, bpm: f64) -> NowPlaying {
    NowPlaying {
        setlist: "Recital".into(),
        song: song.into(),
        song_index: index,
        section: section.into(),
        playing: true,
        position: 1.0,
        bpm,
        ..NowPlaying::default()
    }
}

// ── The tests ──────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn two_engines_of_one_account_admit_each_other_and_follow_either_way() {
    let issuer = issuer().await;
    let token = sign_up(&issuer, "band@example.com").await;
    let desk = engine("battleship", "linux", Roles {
        tracks: true,
        rigs: vec![],
    })
    .await;
    let ipad = engine("Cody's iPad", "ios", Roles {
        tracks: false,
        rigs: vec!["guitar".into()],
    })
    .await;
    introduce(&desk, &ipad);

    // Nobody typed an id: the address book introduces them. The first
    // pass of the first engine finds nobody; the second engine's finds it,
    // and the first's next pass finds the second.
    let first = desk.mesh.account_pass(&issuer.devices, &token, &desk.me).await.unwrap();
    assert!(first.admitted.is_empty());
    let second = ipad.mesh.account_pass(&issuer.devices, &token, &ipad.me).await.unwrap();
    assert_eq!(second.admitted, vec![desk.mesh.id()]);
    desk.mesh.account_pass(&issuer.devices, &token, &desk.me).await.unwrap();
    assert!(desk.mesh.admission().admits(&ipad.mesh.id()));

    // Each says what it runs.
    let seen = ipad.mesh.peers().await;
    assert_eq!(seen.len(), 1);
    let about = seen[0].about.clone().expect("the desk answers");
    assert_eq!(about.roles.describe(), "tracks");
    assert_eq!(about.name, "battleship");

    // The iPad's rig follows the desk's tracks through the recital.
    let rig = Rig::with_recital();
    ipad.mesh.follow(&desk.mesh.id(), rig.clone());
    desk.mesh.publish(playing("Heart Isn't Ready", 3, "Verse", 76.0));
    eventually("the rig is on Heart Isn't Ready, playing", async || {
        let h = rig.here();
        rig.song() == "Heart Isn't Ready" && h.playing && h.part_index == 1 && (h.bpm - 76.0).abs() < 1e-9
    })
    .await;
    // The desk moves on (the hub replays nothing: only live changes).
    desk.mesh.publish(playing("WORST PART OF YOU", 6, "Chorus", 140.0));
    eventually("the rig is on WORST PART OF YOU", async || {
        rig.song() == "WORST PART OF YOU" && rig.here().part_index == 2
    })
    .await;
    desk.mesh.publish(NowPlaying {
        playing: false,
        ..playing("WORST PART OF YOU", 6, "Chorus", 140.0)
    });
    eventually("the rig stops with the desk", async || !rig.here().playing).await;
    assert_eq!(ipad.mesh.about().following, desk.mesh.id());

    // Roles change: the iPad plays the set now, the desk runs a rig and
    // follows it. Nothing about either is special.
    ipad.mesh.unfollow();
    ipad.mesh.set_roles(Roles {
        tracks: true,
        rigs: vec!["guitar".into()],
    });
    desk.mesh.set_roles(Roles {
        tracks: false,
        rigs: vec!["guitar".into()],
    });
    let desk_rig = Rig::with_recital();
    desk.mesh.follow(&ipad.mesh.id(), desk_rig.clone());
    ipad.mesh.publish(playing("Know Me", 9, "", 100.0));
    eventually("the desk's rig is on Know Me (Extended)", async || {
        desk_rig.song() == "Know Me (Extended)" && desk_rig.here().playing
    })
    .await;
    let seen = desk.mesh.peers().await;
    assert_eq!(
        seen[0].about.as_ref().map(|a| a.roles.describe()).as_deref(),
        Some("tracks + guitar")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_off_the_account_is_refused() {
    let issuer = issuer().await;
    let ours = sign_up(&issuer, "band@example.com").await;
    let theirs = sign_up(&issuer, "someone-else@example.com").await;
    let desk = engine("battleship", "linux", Roles::default()).await;
    let stranger = engine("stranger", "linux", Roles::default()).await;
    introduce(&desk, &stranger);
    desk.mesh.account_pass(&issuer.devices, &ours, &desk.me).await.unwrap();
    stranger.mesh.account_pass(&issuer.devices, &theirs, &stranger.me).await.unwrap();
    assert!(!desk.mesh.admission().admits(&stranger.mesh.id()));

    // It may dial, but it is refused every call.
    let client = stranger.mesh.peer(&desk.mesh.id()).await.unwrap();
    let refused = client.about().await;
    assert!(refused.is_err(), "a stranger is told nothing: {refused:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rig_book_edited_on_either_engine_arrives_on_the_other_with_the_issuer_gone() {
    let issuer = issuer().await;
    let token = sign_up(&issuer, "band@example.com").await;
    let desk = engine("battleship", "linux", Roles::default()).await;
    let ipad = engine("Cody's iPad", "ios", Roles::default()).await;
    introduce(&desk, &ipad);
    desk.mesh.account_pass(&issuer.devices, &token, &desk.me).await.unwrap();
    ipad.mesh.account_pass(&issuer.devices, &token, &ipad.me).await.unwrap();
    desk.mesh.account_pass(&issuer.devices, &token, &desk.me).await.unwrap();
    // The issuer only introduced them.
    drop(issuer);

    // The guitar rig's book: one id on every device, made on each.
    let book = Uuid::from_u128(0x5e7_1157);
    let make = || CreateDoc {
        doc_id: book,
        kind: "rig".into(),
        name: "Guitar rig".into(),
        parent: None,
        lineage: Lineage::Merge,
    };
    let on_desk = desk.driver.create(make()).await.unwrap();
    let on_ipad = ipad.driver.create(make()).await.unwrap();
    desk.mesh.sync_books(Duration::from_millis(200));
    ipad.mesh.sync_books(Duration::from_millis(200));

    // The setlist made on the iPad.
    let recital = RECITAL.join("\n");
    on_ipad
        .loro()
        .get_map("library")
        .insert("setlists.styx", recital.as_str())
        .unwrap();
    on_ipad.loro().commit();
    let read = |doc: &crdt::CrdtDoc, key: &str| {
        doc.loro()
            .get_map("library")
            .get(key)
            .and_then(|v| v.into_value().ok())
            .and_then(|v| v.into_string().ok())
            .map(|s| s.to_string())
    };
    eventually("the desk has the iPad's setlist", async || {
        read(&on_desk, "setlists.styx").as_deref() == Some(recital.as_str())
    })
    .await;

    // And a preset made on the desk reaches the iPad.
    on_desk
        .loro()
        .get_map("library")
        .insert("presets/Edge Of Breakup.styx", "drive 3.5")
        .unwrap();
    on_desk.loro().commit();
    eventually("the iPad has the desk's preset", async || {
        read(&on_ipad, "presets/Edge Of Breakup.styx").as_deref() == Some("drive 3.5")
    })
    .await;
}

/// A rig with no CLI (an iPad) follows the one peer playing tracks by
/// itself, and not once told to stop.
#[tokio::test(flavor = "multi_thread")]
async fn an_ipad_follows_the_peer_that_plays_tracks_by_itself() {
    use studio_peers::mesh::Node;
    use studio_peers::PeersControl as _;
    let issuer = issuer().await;
    let token = sign_up(&issuer, "band@example.com").await;
    let desk = engine("battleship", "linux", Roles::default()).await;
    let ipad = engine("Cody's iPad", "ios", Roles::default()).await;
    introduce(&desk, &ipad);
    desk.mesh.account_pass(&issuer.devices, &token, &desk.me).await.unwrap();
    ipad.mesh.account_pass(&issuer.devices, &token, &ipad.me).await.unwrap();
    desk.mesh.account_pass(&issuer.devices, &token, &desk.me).await.unwrap();

    let desk_node = Node::new(Rig::with_recital(), "");
    desk_node.set_mesh(desk.mesh.clone());
    let rig = Rig::with_recital();
    let ipad_node = Node::new(rig.clone(), "");
    ipad_node.set_mesh(ipad.mesh.clone());
    ipad_node.set_rigs(vec!["guitar".into()]);

    // Nobody plays tracks yet: nothing to follow.
    assert_eq!(ipad_node.auto_follow_once().await, None);
    // The desk's Session window publishes through its engine.
    desk_node.publish(playing("Fire", 7, "", 128.0)).await;
    assert_eq!(ipad_node.auto_follow_once().await, Some(desk.mesh.id()));
    eventually("the iPad's rig is on Fire", async || rig.song() == "Fire").await;
    // Told to stop, it stays stopped.
    ipad_node.unfollow().await;
    assert_eq!(ipad_node.auto_follow_once().await, None);
}
