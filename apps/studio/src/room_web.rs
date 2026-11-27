//! A browser in a room (studio#711): `fasttrackstudio.app/join/<room>`,
//! opened in a page, joins the room a desktop leads, over the internet,
//! and follows its leader as a desktop member does.
//!
//! - **Finding the room.** Its file is in its organization's Rooms folder
//!   (`<Studio>/Rooms/<room>.room`). The page finds it in Drive, through
//!   the node with this person's sign-in, in each of their organizations
//!   (a search for the file, as setlist files are found). A Studio folder
//!   at a link (`?studio=<url>`) stands in for Drive, as `?session=` does
//!   for a session's folder.
//! - **Reaching the leader.** The file says where the leader is: its
//!   endpoint, its relays and the room's pass. The page binds an iroh
//!   endpoint of its own, relay-only (a page has no sockets), dials the
//!   leader through its relay and attaches once (`room::net::attach_to`,
//!   what a desktop member joins by), presenting the pass on every call:
//!   the leader's door refuses anyone without it (`room::gate`).
//! - **Following.** Each load the room's doc gains (a setlist or a lone
//!   session, kept relative to the Studio folder) becomes Now Playing,
//!   each song at its session folder in the same Drive location (or at
//!   the link), so the page's engine opens and plays it
//!   (`crate::engine_web`). Once it is open the member says so in the
//!   room (`RoomPeer::opened`): the leader's `ROOM` shows which load each
//!   member has opened. The transport follows the leader's with
//!   session-sync's `TransportSync`, the song by its number in the set, as
//!   the desktop's engine does (engine-collab's `room`).
//!
//! The page joins when its person presses Join on the link's page
//! (`crate::pages::join`): the press is the gesture a browser needs before
//! it plays any sound. What the membership is doing is written to a
//! hidden element ([`STATE_ID`]), which the browser test reads
//! (`tests/it/web_room.rs`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use room::net::{Find, RoomPeer};
use room::{Kind, Loaded, Room, RoomId};
use session_sync::transport::{Command, LocalTransport, TransportSync};
use studio_engine::web_engine;

use crate::data::{self, Song};
use crate::files::Hit;
use crate::files_live::web;
use crate::room_paths;
use crate::state::{AppState, NowPlaying, Place, Source, use_app};

/// The room the page is to be in: set by the link's page when its person
/// presses Join, `None` once they leave.
pub static JOIN: GlobalSignal<Option<RoomId>> = Signal::global(|| None);

/// What the page's membership is doing: what the link's page shows, and
/// [`STATE_ID`] says.
pub static SHOWN: GlobalSignal<Shown> = Signal::global(Shown::default);

/// The hidden element the membership's state is written to.
pub const STATE_ID: &str = "studio-room";

/// What the room shows a browser as.
const NAME: &str = "Browser";

/// How often the membership looks at the room and the page's engine.
const TICK_MS: u64 = 50;

/// How long after a load that did not open here it is tried again.
const RETRY_MS: f64 = 3000.0;

/// How many times the room's file is looked for, a moment apart (the
/// person's organizations may still be arriving from Drive).
const FIND_TRIES: u32 = 5;
const FIND_WAIT_MS: u64 = 2000;

/// Which membership is the page's: each Join, and each Leave, makes a new
/// one, and one that is no longer current stops and says nothing more.
static CURRENT: AtomicU64 = AtomicU64::new(0);

/// Whether membership `run` is still the page's.
fn current(run: u64) -> bool {
    CURRENT.load(Ordering::SeqCst) == run
}

/// The spaces to look for a room in: the org Studio is in, then each other
/// of this person's.
fn spaces(app: AppState) -> Vec<String> {
    let mut spaces = vec![app.here.peek().clone()];
    for org in app.orgs.peek().iter() {
        if !spaces.contains(org) {
            spaces.push(org.clone());
        }
    }
    spaces
}

/// A membership's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    /// `idle`, `finding`, `joining` (the leader not reached, or refusing:
    /// `why` says), `member` or `failed`.
    pub state: &'static str,
    /// Why it failed, or the last thing that went wrong while a member.
    pub why: String,
    pub room: String,
    /// The room's name and its leader's.
    pub name: String,
    pub leader: String,
    /// What the leader loaded, and which load it is.
    pub loaded: String,
    pub load: Option<u64>,
    /// The load open in the page's engine.
    pub opened: Option<u64>,
    pub attaches: usize,
    pub people: Vec<String>,
    /// The leader's song number, and whether it plays.
    pub song: String,
    pub playing: bool,
    /// The song number the page is on.
    pub here_song: String,
}

/// Where the room's file and the Studio folder it is in are.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Home {
    /// In Drive: the space and location, and the Studio folder's path.
    Drive {
        space: String,
        location: String,
        studio: String,
    },
    /// A Studio folder at a link (`?studio=<url>`).
    Link { studio: String },
}

impl Home {
    /// The file at `path` in the Studio folder's location, as Drive's
    /// helpers read it.
    fn hit(space: &str, location: &str, path: &str) -> Hit {
        Hit {
            space: space.to_owned(),
            location: location.to_owned(),
            path: path.to_owned(),
            name: room_paths::name_of(path),
            online: true,
            ..Hit::default()
        }
    }

    /// The bytes of the file at `path` (a path in the Studio folder's
    /// location, or a URL).
    async fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let (code, bytes) = match self {
            Self::Drive {
                space, location, ..
            } => {
                let url = web::file_url(&Self::hit(space, location, path));
                web::bytes_in(None, "GET", &url, None).await?
            }
            Self::Link { .. } => {
                crate::engine_web::plain(&crate::engine_web::absolute(path), None).await?
            }
        };
        match code {
            200 | 206 => Ok(bytes),
            code => Err(format!("{} answered {code}", room_paths::name_of(path))),
        }
    }

    fn studio(&self) -> &str {
        match self {
            Self::Drive { studio, .. } | Self::Link { studio } => studio,
        }
    }

    /// The session folder at `folder` (a path in the location, or a URL).
    fn place(&self, folder: String) -> Place {
        let (space, location) = match self {
            Self::Drive {
                space, location, ..
            } => (space.clone(), location.clone()),
            Self::Link { .. } => (String::new(), crate::engine_web::LINK.to_owned()),
        };
        Place {
            space,
            location,
            folder,
            chart: None,
            arrangement: None,
        }
    }
}

/// Room `id`'s file, read from `home`.
async fn read_room(home: &Home, id: &RoomId) -> Result<Room, String> {
    let bytes = home
        .read(&room_paths::room_file(home.studio(), id.as_str()))
        .await?;
    serde_json::from_slice(&bytes).map_err(|e| format!("the room's file: {e}"))
}

/// Room `id`, and where it is: at the link the page's address names, else
/// in Drive, in one of `spaces` (this person's organizations).
async fn find(id: &RoomId, spaces: &[String]) -> Result<(Room, Home), String> {
    if let Some(linked) = crate::engine_web::linked_in_address("studio") {
        let home = Home::Link {
            studio: linked.folder,
        };
        return Ok((read_room(&home, id).await?, home));
    }
    let file = format!("{id}.room");
    for space in spaces {
        let hits = web::search_in(None, space, id.as_str(), &["room".to_owned()], 20).await;
        for hit in hits.into_iter().filter(|h| !h.dir && h.name == file) {
            let Some(studio) = room_paths::studio_of(&hit.path) else {
                continue;
            };
            let home = Home::Drive {
                space: hit.space.clone(),
                location: hit.location.clone(),
                studio,
            };
            match read_room(&home, id).await {
                Ok(room) => return Ok((room, home)),
                Err(e) => {
                    tracing::debug!(room.error = %e, "room: a copy of the room's file did not read")
                }
            }
        }
    }
    Err(format!(
        "no room {id} in your organizations' Drive: sign in with an account in the room's organization"
    ))
}

/// What `loaded` plays, each song at its session folder: a setlist's
/// songs in order, or the lone session.
async fn songs_of(
    home: &Home,
    loaded: &Loaded,
    spaces: &[String],
) -> Result<Vec<(Song, Place)>, String> {
    if room_paths::outside_studio(&loaded.path) {
        return Err(format!(
            "{} is outside the Studio folder on the leader's device",
            loaded.path
        ));
    }
    let path = room_paths::under(home.studio(), &loaded.path);
    match (loaded.kind, home) {
        (
            Kind::Setlist,
            Home::Drive {
                space, location, ..
            },
        ) => Ok(crate::drive_sets::load(space, location, &path, spaces)
            .await?
            .playlist()),
        (Kind::Setlist, Home::Link { .. }) => {
            let text = String::from_utf8_lossy(&home.read(&path).await?).into_owned();
            Ok(crate::setfile::parse(&text)
                .acts
                .iter()
                .flat_map(|act| act.songs.iter())
                .filter_map(|line| room_paths::song_of_line(&path, line))
                .map(|folder| {
                    let song = data::song_named(&room_paths::name_of(&folder));
                    (song, home.place(folder))
                })
                .collect())
        }
        (Kind::Session, _) => {
            let folder = room_paths::song_folder(&path);
            let song = data::song_named(&room_paths::name_of(&folder));
            Ok(vec![(song, home.place(folder))])
        }
    }
}

/// The page's transport, as the room follows it: Now Playing's song (by
/// its number in the set), whether it plays, and the playhead as heard.
fn local(app: AppState) -> LocalTransport {
    let np = app.np.peek();
    let (at, _) = web_engine::position();
    LocalTransport {
        playing: np.as_ref().is_some_and(|n| n.playing),
        position: at,
        song: np.as_ref().map(|n| n.i.saturating_add(1).to_string()),
    }
}

/// Do what the follower asks of the page: through Now Playing, which the
/// page's engine plays as it would a person's press.
fn obey(mut app: AppState, command: Command) {
    let mut play = |on: bool| {
        app.np.with_mut(|n| {
            if let Some(n) = n {
                n.playing = on;
            }
        });
    };
    match command {
        Command::SwitchSong(number) => {
            if let Some(i) = number.parse::<usize>().ok().and_then(|n| n.checked_sub(1)) {
                crate::session::go_to(app, i);
            }
        }
        Command::Play { from } => {
            play(true);
            app.seek.set(Some(from));
        }
        Command::Stop { at } => {
            play(false);
            app.seek.set(Some(at));
        }
        Command::Seek { to } => app.seek.set(Some(to)),
    }
}

/// Whether the song on screen is open in the page's engine.
fn ready(app: AppState) -> bool {
    app.np
        .peek()
        .as_ref()
        .is_some_and(|n| n.source == Source::Ready && n.place().is_some())
}

/// The first eight letters of an id.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn failed(run: u64, why: String) {
    if !current(run) {
        return;
    }
    tracing::warn!(room.error = %why, "room: the page did not join");
    SHOWN.with_mut(|s| {
        s.state = "failed";
        s.why = why;
    });
}

/// Room `id` found, looked for again a moment apart while it is not.
async fn find_soon(app: AppState, run: u64, id: &RoomId) -> Result<(Room, Home), String> {
    let mut tries = 0;
    loop {
        let found = find(id, &spaces(app)).await;
        tries += 1;
        if found.is_ok() || tries >= FIND_TRIES || !current(run) {
            return found;
        }
        crate::state::sleep_ms(FIND_WAIT_MS).await;
    }
}

/// Be in room `id` as membership `run`, for as long as it is the page's.
async fn membership(app: AppState, id: RoomId, run: u64) {
    *SHOWN.write() = Shown {
        state: "finding",
        room: id.to_string(),
        ..Shown::default()
    };
    let (room, home) = match find_soon(app, run, &id).await {
        Ok(found) => found,
        Err(e) => return failed(run, e),
    };
    let Some(live) = room.live.clone() else {
        // Practising alone in a room nobody leads is studio#714's.
        return failed(
            run,
            format!("{} is not live: nobody leads it now", room.name),
        );
    };
    if !current(run) {
        return;
    }
    SHOWN.with_mut(|s| {
        s.state = "joining";
        s.name.clone_from(&room.name);
        s.leader.clone_from(&live.leader);
    });
    let endpoint = match room::net::bind_in_browser().await {
        Ok(endpoint) => endpoint,
        Err(e) => return failed(run, e),
    };
    if !current(run) {
        return;
    }
    let me = format!("browser-{}", short(&endpoint.id().to_string()));
    // Where the leader is now, each time the page attaches: a leader back
    // after a restart is at new addresses, with a new pass.
    let find_again: Find = {
        let (home, id) = (home.clone(), id.clone());
        Arc::new(move || {
            let (home, id) = (home.clone(), id.clone());
            Box::pin(async move { read_room(&home, &id).await.ok().and_then(|r| r.live) })
        })
    };
    let peer = RoomPeer::join(
        &room.id,
        &me,
        NAME,
        room::net::attach_to(endpoint, live, find_again),
    );
    tracing::info!(room.id = %room.id, room.role = "member", room.member = "browser", "room: the page joined");
    follow(app, run, &id, &home, &peer, &me).await;
}

/// What a member's page keeps between ticks.
struct Following {
    /// The load put into Now Playing, and the one open in the engine.
    put: Option<u64>,
    opened: Option<u64>,
    /// When a load last failed to open here, and why.
    failed_at: Option<f64>,
    load_error: Option<String>,
    sync: TransportSync,
}

/// The member's loop: each load into Now Playing, the transport kept on
/// the leader's, until the page leaves the room.
async fn follow(app: AppState, run: u64, id: &RoomId, home: &Home, peer: &RoomPeer, me: &str) {
    let mut f = Following {
        put: None,
        opened: None,
        failed_at: None,
        load_error: None,
        sync: TransportSync::new(me),
    };
    while current(run) {
        if let Some((loaded, load)) = peer.doc().loaded()
            && f.put != Some(load)
            && f.failed_at
                .is_none_or(|at| js_sys::Date::now() - at >= RETRY_MS)
        {
            match songs_of(home, &loaded, &spaces(app)).await {
                Ok(list) if !list.is_empty() => {
                    tracing::info!(room.loaded = %loaded.title, room.load = load, "room: opening what the leader loaded");
                    let mut next = NowPlaying::of_places(list, loaded.title.clone(), 0, false);
                    // A load that starts on the song already on screen: the
                    // page's engine has it open, and opens nothing again (it
                    // opens when the song on screen changes), so it stays
                    // ready.
                    if let Some(now) = app.np.peek().as_ref()
                        && now.place().is_some()
                        && now.place() == next.place()
                    {
                        next.source = now.source.clone();
                        next.dur = now.dur;
                    }
                    let mut np = app.np;
                    np.set(Some(next));
                    f.put = Some(load);
                    f.failed_at = None;
                    f.load_error = None;
                    // What is in force carries over: the follower starts
                    // again on the new set.
                    f.sync = TransportSync::new(me);
                }
                Ok(_) => {
                    f.failed_at = Some(js_sys::Date::now());
                    f.load_error = Some(format!("{} has no songs here", loaded.title));
                }
                Err(e) => {
                    f.failed_at = Some(js_sys::Date::now());
                    f.load_error = Some(format!("{} did not open here: {e}", loaded.title));
                }
            }
        }
        if f.put.is_some() && f.opened != f.put && ready(app) {
            f.opened = f.put;
            if let Some(load) = f.opened {
                peer.opened(load);
            }
        }
        let presence = peer.presence();
        if let Some(entry) = room::net::transport(&presence) {
            f.sync.remote(entry);
        }
        if ready(app)
            && let Some(now) = presence.clock().now_ms()
        {
            for command in f.sync.tick(&local(app), now).commands {
                obey(app, command);
            }
        }
        if !current(run) {
            break;
        }
        let loaded = peer.doc().loaded();
        let attached = peer.last_error();
        // Never reached, or refused by the leader's door: not in the room.
        let state = if loaded.is_none() && attached.is_some() {
            "joining"
        } else {
            "member"
        };
        let why = f.load_error.clone().or(attached).unwrap_or_default();
        let in_force = f.sync.current().cloned();
        let shown = Shown {
            state,
            why,
            room: id.to_string(),
            name: SHOWN.peek().name.clone(),
            leader: SHOWN.peek().leader.clone(),
            loaded: loaded
                .as_ref()
                .map(|(l, _)| l.title.clone())
                .unwrap_or_default(),
            load: loaded.map(|(_, n)| n),
            opened: f.opened,
            attaches: peer.attaches(),
            people: room::net::people(&presence),
            song: in_force
                .as_ref()
                .and_then(|t| t.song.clone())
                .unwrap_or_default(),
            playing: in_force.is_some_and(|t| t.playing),
            here_song: local(app).song.unwrap_or_default(),
        };
        if *SHOWN.peek() != shown {
            *SHOWN.write() = shown;
        }
        crate::state::sleep_ms(TICK_MS).await;
    }
    if current(run) {
        *SHOWN.write() = Shown::default();
    }
    tracing::info!(room.id = %id, "room: the page left");
}

/// The page's membership: mounted once, in the shell, beside the page's
/// engine, so it lasts while its person moves between pages.
#[component]
pub fn RoomMember() -> Element {
    let app = use_app();
    let wanted = JOIN();
    use_effect(use_reactive!(|wanted| {
        // A Join or a Leave: whatever membership ran is no longer the
        // page's.
        let run = CURRENT.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        match wanted {
            Some(id) => {
                spawn(membership(app, id, run));
            }
            None => *SHOWN.write() = Shown::default(),
        }
    }));
    let s = SHOWN();
    let people = s.people.join(", ");
    let load = s.load.map(|n| n.to_string()).unwrap_or_default();
    let opened = s.opened.map(|n| n.to_string()).unwrap_or_default();
    rsx! {
        div {
            id: STATE_ID,
            style: "display:none",
            "data-state": if s.state.is_empty() { "idle" } else { s.state },
            "data-why": "{s.why}",
            "data-room": "{s.room}",
            "data-leader": "{s.leader}",
            "data-loaded": "{s.loaded}",
            "data-load": "{load}",
            "data-opened": "{opened}",
            "data-attaches": "{s.attaches}",
            "data-people": "{people}",
            "data-song": "{s.song}",
            "data-playing": "{s.playing}",
            "data-here-song": "{s.here_song}",
        }
    }
}
