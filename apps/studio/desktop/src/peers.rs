//! This engine on the account's mesh (`studio_peers`, docs/design/peers.md).
//!
//! Where the rigs run — `studio-desktop --engine` on a desk, the app
//! itself on an iPad — the engine is one peer among the account's Studio
//! engines, whatever it runs:
//!
//! - **The rig books** are `crdt::docs` documents of a driver kept here
//!   ([`books`]), one id per library on every device of the account (the
//!   account's own, not a server's org), synced with each peer directly.
//!   No Task, no hub.
//! - **The mesh** ([`start`]): an iroh endpoint of its own (n0 and the
//!   LAN), the account pass against the issuer (`auth.fasttrackstudio.app`,
//!   `FTS_AUTH_ISSUER`), the account's other engines let in.
//! - **What it plays** is published to its peers: the guitar rig's set,
//!   song and section and the engine's transport, or what a window in
//!   another process plays ([`studio_peers::PeersControl::publish`]).
//! - **Following** a peer drives the guitar rig: the song (switching set
//!   when another set has it), the section, the rig cue, the tempo and
//!   the transport ([`GuitarFollows`]).
//!
//! [`start`] gives the handle `studio peers …` reaches over the engine's
//! own router.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crdt::docs::{DocSyncEvent, DocsDriver, InMemoryDocsIndex};
use daw_proto::transport::service::Transport as _;
use fts_account::sync::{Files, Org};
use guitar::GuitarRigBackend;
use signal_rigs_proto::perform::service::RigPerform as _;
use studio_peers::mesh::{FollowTarget, Mesh, Node};
use studio_peers::{FollowAction, Here, NowPlaying, RIG_CUES, RigCues};
use uuid::Uuid;

/// The issuer whose address book introduces the account's engines.
const ISSUER: &str = "https://auth.fasttrackstudio.app";

/// How long a fresh book waits for a peer's library before it may take
/// the shipped defaults (alone at the gig, it must still play).
const SEED_GRACE: Duration = Duration::from_secs(10);

/// The namespace an account's rig library ids are made in.
const ACCOUNT_SPACE: Uuid = Uuid::from_u128(0x5d10_7ee2_3c4b_4a51_8f0e_6b2a_0000_0002);

static NODE: OnceLock<Node> = OnceLock::new();
static DRIVER: OnceLock<DocsDriver> = OnceLock::new();

/// The issuer to introduce engines through.
fn issuer() -> String {
    std::env::var("FTS_AUTH_ISSUER")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| ISSUER.to_owned())
}

/// The account's space: the same on every device signed in to it.
fn account_space(account: &fts_account::Account) -> Option<Uuid> {
    let who = account
        .session
        .user_id
        .clone()
        .or_else(|| account.session.email.clone())?;
    Some(Uuid::new_v5(&ACCOUNT_SPACE, who.as_bytes()))
}

/// The rig books as the account's synced docs: a driver kept on this
/// device, each rig library one doc with one id on every device. Signed
/// out, nothing: the books stay this device's. Call from the engine's
/// runtime after `books::enable`, before the first rig makes its home.
pub fn books() {
    let Some(account) = fts_account::signed_in() else {
        tracing::info!(peers.books = "local", "peers: signed out — the rig books stay on this device");
        return;
    };
    let Some(space) = account_space(&account) else {
        tracing::warn!(peers.books = "local", "peers: the kept session names no account");
        return;
    };
    let config = signal_rig_host::store::signal_config_dir();
    let files = Files::new(config.join("peers").join("docs"));
    let driver = DocsDriver::new(InMemoryDocsIndex::new(), files.clone());
    let org = Org {
        slug: "account".into(),
        id: space,
    };
    let handle = tokio::runtime::Handle::current();
    let books = crate::sync::Books::default();
    signal_rig_host::book::set_provider(Arc::new({
        let driver = driver.clone();
        let books = books.clone();
        move |dir: &Path| {
            let (id, book) = crate::sync::rig_book(&handle, &driver, &files, &org, &config, dir)?;
            books.keep(id, &book);
            Some(book)
        }
    }));
    architect::platform::spawn(crate::sync::seed_when_caught_up(
        driver.sync_events(),
        books.clone(),
    ));
    architect::platform::spawn(seed_alone(driver.sync_events(), books));
    let _ = DRIVER.set(driver);
    tracing::info!(peers.books = "account", "peers: the rig books are the account's docs");
}

/// Alone (no peer within [`SEED_GRACE`]), every fresh book may seed: the
/// rig must play on a device that meets nobody.
async fn seed_alone(
    mut events: tokio::sync::broadcast::Receiver<DocSyncEvent>,
    books: crate::sync::Books,
) {
    let met = tokio::time::timeout(SEED_GRACE, async {
        loop {
            match events.recv().await {
                Ok(DocSyncEvent::Attached { .. }) => return,
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => return,
            }
        }
    })
    .await;
    if met.is_err() {
        books.allow_all();
        tracing::info!(peers.books = "alone", "peers: no peer yet — fresh books may seed");
    }
}

/// The guitar rig's library directory: its book.
fn guitar_dir() -> PathBuf {
    signal_rig_host::store::signal_config_dir().join("rig")
}

/// Start this engine on the mesh, following onto `guitar`. Signed out, the
/// node is up but says why the mesh is not. Call from the engine's runtime.
pub async fn start(guitar: &GuitarRigBackend) -> &'static Node {
    let target: Arc<dyn FollowTarget> = Arc::new(GuitarFollows {
        guitar: guitar.clone(),
    });
    let node = NODE.get_or_init(|| Node::new(target, "starting"));
    node.set_rigs(vec!["guitar".into()]);
    let Some(account) = fts_account::signed_in() else {
        node.set_down("signed out: sign in to the FastTrackStudio account");
        return node;
    };
    let Some(user) = account.session.user_id.clone().or_else(|| account.session.email.clone())
    else {
        node.set_down("the kept session names no account");
        return node;
    };
    let config = signal_rig_host::store::signal_config_dir();
    let key = match architect::iroh_link::load_or_create_secret_key(&config.join("peers.key")) {
        Ok(key) => key,
        Err(e) => {
            tracing::warn!(error = %e, "peers: no lasting key — one for this run");
            architect::iroh_link::iroh::SecretKey::generate()
        }
    };
    let name = auth_client::this_device::name();
    let endpoint = match studio_peers::mesh::bind(key, &format!("{name} · Studio")).await {
        Ok(ep) => ep,
        Err(why) => {
            node.set_down(&why);
            return node;
        }
    };
    let about = studio_peers::About {
        name: name.clone(),
        platform: auth_client::this_device::platform().to_owned(),
        version: option_env!("FTS_RELEASE_VERSION")
            .unwrap_or(env!("CARGO_PKG_VERSION"))
            .to_owned(),
        ..studio_peers::About::default()
    };
    let mesh = match Mesh::start(endpoint, about, DRIVER.get().cloned()).await {
        Ok(mesh) => mesh,
        Err(why) => {
            node.set_down(&why);
            return node;
        }
    };
    let install = auth_client::this_device::install_id().unwrap_or_else(|| {
        // No machine id here: the endpoint's key is this install's.
        Uuid::new_v5(&ACCOUNT_SPACE, mesh.id().as_bytes())
    });
    let me = studio_peers::mesh::this_engine(install, &user, &name, auth_client::this_device::platform());
    mesh.keep_on_account(
        &issuer(),
        || fts_account::signed_in().map(|a| a.session.token),
        me,
        {
            let node = node.clone();
            move |outcome| match outcome {
                Ok(report) => {
                    node.set_account("ok");
                    tracing::info!(
                        peers.admitted = report.admitted.len(),
                        peers.org = ?report.org.as_ref().map(|o| o.slug.clone()),
                        "peers: account pass"
                    );
                }
                Err(e) => {
                    node.set_account(&e.to_string());
                    tracing::warn!(error = %e, "peers: the account pass failed; again soon");
                }
            }
        },
    );
    mesh.sync_books(Duration::from_secs(5));
    node.set_mesh(mesh.clone());
    // An engine nobody drives by hand (an iPad on a stand) follows the one
    // peer playing tracks; `studio peers follow`/`unfollow` override it.
    node.auto_follow(Duration::from_secs(3));
    tracing::info!(peers.id = %mesh.id(), "peers: on the account's mesh");
    architect::platform::spawn(publish_loop(guitar.clone(), node.clone()));
    node
}

// ── What this engine plays ─────────────────────────────────────────────

/// The engine's transport, if it has one.
struct Transport {
    playing: bool,
    position: f64,
    bpm: f64,
}

fn transport() -> Option<Transport> {
    let engine = signal_rig_host::engine::shared()?;
    let daw = engine.daw();
    let project = daw_proto::ProjectContext::Project(engine.project_guid().to_string());
    Some(Transport {
        playing: daw.is_playing(project.clone()),
        position: daw.get_position(project.clone()),
        bpm: daw.get_tempo(project),
    })
}

/// What the guitar rig plays now, as its peers hear it.
fn rig_now(guitar: &GuitarRigBackend) -> NowPlaying {
    let state = guitar.state();
    let t = transport();
    let index = state.song_index as usize;
    NowPlaying {
        setlist: state
            .setlists
            .get(state.setlist_index as usize)
            .cloned()
            .unwrap_or_default(),
        song: state.songs.get(index).map(|s| s.name.clone()).unwrap_or_default(),
        song_index: state.song_index,
        section: state
            .parts
            .get(state.part_index as usize)
            .map(|p| p.name.clone())
            .unwrap_or_default(),
        playing: t.as_ref().is_some_and(|t| t.playing),
        position: t.as_ref().map_or(0.0, |t| t.position),
        bpm: t
            .as_ref()
            .map(|t| t.bpm)
            .filter(|b| *b > 0.0)
            .unwrap_or(f64::from(state.tempo_bpm)),
        ..NowPlaying::default()
    }
}

/// Publish what the rig plays: at once on a change, each second while it
/// plays. Not while a window publishes this engine's tracks.
async fn publish_loop(guitar: GuitarRigBackend, node: Node) {
    let mut last = NowPlaying::default();
    let mut ticks = 0u32;
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let Some(mesh) = node.mesh() else { continue };
        if node.window_plays() {
            continue;
        }
        let g = guitar.clone();
        let Ok(now) = tokio::task::spawn_blocking(move || rig_now(&g)).await else {
            continue;
        };
        ticks += 1;
        let moved = NowPlaying {
            position: last.position,
            at_ms: last.at_ms,
            revision: last.revision,
            ..now.clone()
        } != last;
        if moved || (now.playing && ticks >= 5) {
            ticks = 0;
            last = now.clone();
            mesh.publish(now);
        }
    }
}

// ── Following onto the guitar rig ─────────────────────────────────────

/// The guitar rig as a follow drives it.
struct GuitarFollows {
    guitar: GuitarRigBackend,
}

impl GuitarFollows {
    fn cues(song: &str) -> Vec<(f64, String)> {
        let Some(book) = signal_rig_host::book::for_dir(&guitar_dir()) else {
            return Vec::new();
        };
        match book.read::<RigCues>(RIG_CUES) {
            signal_rig_host::book::BookRead::Ok(cues) => cues.for_song(song),
            _ => Vec::new(),
        }
    }

    /// Put `title` up from whichever set has it: the set up first, then
    /// the others in order (an archived song is there, out of the order).
    fn song_anywhere(&self, title: &str) -> bool {
        let library = self.guitar.library();
        for (si, set) in library.setlists.iter().enumerate() {
            let names: Vec<String> = set.songs.iter().map(|s| s.name.clone()).collect();
            if let Some(i) = studio_peers::follow::find_song(&names, title) {
                let _ = self.guitar.select_setlist(si as u32);
                let _ = self.guitar.select_song(i as u32);
                tracing::info!(peers.set = %set.name, peers.song = %title, "peers: the song is in another set");
                return true;
            }
        }
        false
    }

    /// Play what cue `name` names: a part of the song up, else a patch,
    /// else a preset.
    fn cue(&self, name: &str) {
        let key = studio_peers::follow::title_key(name);
        let state = self.guitar.state();
        if let Some(i) = state
            .parts
            .iter()
            .position(|p| studio_peers::follow::title_key(&p.name) == key)
        {
            let _ = self.guitar.select_part(i as u32);
            return;
        }
        if let Some(i) = self
            .guitar
            .patches()
            .iter()
            .position(|p| studio_peers::follow::title_key(&p.name) == key)
        {
            let _ = self.guitar.select_patch(i as u32);
            return;
        }
        let presets = self.guitar.compositions();
        if let Some(p) = presets
            .presets
            .iter()
            .find(|p| studio_peers::follow::title_key(&p.name) == key)
        {
            self.guitar.choose_preset(p.name.clone(), String::new());
            return;
        }
        tracing::warn!(peers.cue = %name, "peers: the rig has nothing this cue names");
    }
}

impl FollowTarget for GuitarFollows {
    fn here(&self) -> Here {
        let state = self.guitar.state();
        let t = transport();
        let song = state
            .songs
            .get(state.song_index as usize)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        Here {
            songs: state.songs.iter().map(|s| s.name.clone()).collect(),
            song_index: state.song_index as usize,
            parts: state.parts.iter().map(|p| p.name.clone()).collect(),
            part_index: state.part_index as usize,
            playing: t.as_ref().is_some_and(|t| t.playing),
            position: t.as_ref().map_or(0.0, |t| t.position),
            bpm: t.as_ref().map_or(f64::from(state.tempo_bpm), |t| t.bpm),
            cues: Self::cues(&song),
        }
    }

    fn set_cues(&self, cues: &RigCues) -> Result<(), String> {
        let book = signal_rig_host::book::for_dir(&guitar_dir())
            .ok_or_else(|| "the guitar rig keeps no book here".to_owned())?;
        book.write(RIG_CUES, cues)?;
        tracing::info!(peers.cue_songs = cues.songs.len(), "peers: the rig's cues are kept");
        Ok(())
    }

    fn apply(&self, actions: &[FollowAction]) {
        let engine = signal_rig_host::engine::shared();
        let project = engine
            .as_ref()
            .map(|e| daw_proto::ProjectContext::Project(e.project_guid().to_string()));
        let daw = || engine.as_ref().zip(project.clone());
        for action in actions {
            match action {
                FollowAction::Song { index, title } => {
                    let _ = self.guitar.select_song(*index as u32);
                    tracing::info!(peers.song = %title, "peers: following to a song");
                }
                FollowAction::Missing { title } => {
                    if !self.song_anywhere(title) {
                        tracing::warn!(peers.song = %title, "peers: no set of this rig has the song");
                    }
                }
                FollowAction::Part { index, .. } => {
                    let _ = self.guitar.select_part(*index as u32);
                }
                FollowAction::Cue { name } => self.cue(name),
                FollowAction::Tempo(bpm) => signal_rig_host::tempo::set(*bpm as f32),
                FollowAction::Play => {
                    if let Some((e, p)) = daw() {
                        let _ = e.daw().play(p);
                    }
                }
                FollowAction::Stop => {
                    if let Some((e, p)) = daw() {
                        let _ = e.daw().stop(p);
                    }
                }
                FollowAction::Seek(t) => {
                    if let Some((e, p)) = daw() {
                        let _ = e.daw().set_position(p, *t);
                    }
                }
            }
        }
    }
}
