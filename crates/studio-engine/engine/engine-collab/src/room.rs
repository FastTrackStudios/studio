//! A room in the engine: leading one, or following its leader
//! (ADR 0003; the room itself is the `room` crate's).
//!
//! - **Making one.** `ROOM_CREATE` makes an organization's room in its
//!   Rooms folder (`room::Rooms::here`): its link
//!   (`https://fasttrackstudio.app/join/<room>`) never changes.
//! - **Leading.** `ROOM_LEAD` hosts the room over iroh, on an endpoint
//!   whose key this device keeps per room (`<data>/fasttrackstudio/rooms/`),
//!   and says in the room's file where to reach it: its addresses, its
//!   relays (what a browser dials it through) and a new pass
//!   ([`room::Pass`]). `ROOM_LOAD` opens a
//!   setlist or a lone session here and loads it into the room's doc,
//!   and keeps it in the room's file as the room's setlist
//!   ([`room::Room::setlist`]). A task of the room's writes this engine's transport into the room's
//!   presence whenever it starts, stops, changes song or jumps
//!   (`room::lead::entry`).
//! - **The door.** The room is open to the internet, so the leader serves
//!   it behind a permissions gate ([`room::gate::Door`], studio#362): a
//!   call is admitted only when it presents the room's pass, which only
//!   the organization's people can read in its Rooms folder. The live set
//!   the room edits together is behind the same door. `ROOM` says how many
//!   calls it refused (`refused`).
//! - **Guests** (studio#713). Leading also opens the room's lobby
//!   (`room::Rooms::open_lobby`): where the leader is, with a guests' pass
//!   of its own (new each time), for anyone with the link. A guest
//!   presenting it follows and plays along, and edits nothing: the room's
//!   doc takes none of a guest's updates, and the live set refuses them.
//!   Each guest who comes is entered in the room's file by the endpoint
//!   their device proved (`room::Rooms::guest_came`), so `ROOM`'s `guests`
//!   knows a guest who comes back as the same one.
//! - **Following.** `ROOM_JOIN` finds the room by its link, dials its
//!   leader once (presenting the pass), and follows: each load the doc gains is opened here
//!   (through the engine's `open_path`, as `SESSION_OPEN` would), and the
//!   transport is kept on the leader's with session-sync's
//!   `TransportSync`, the song by its number in the set. The leader
//!   loading another setlist is a change to the doc, never a new
//!   connection (`ROOM`'s `attaches` stays 1).
//! - **Practising.** `ROOM_JOIN` of a room nobody leads opens its setlist
//!   here, on this engine's own, to practise (studio#714): no leader to
//!   follow, so the transport is this engine's.
//!
//! - **On the local network.** Both ends bind their endpoint with
//!   n0's lookup and the local network's (`PeerLookup::N0AndLan`), and
//!   the leader writes the addresses it has on this machine's networks
//!   into the room's file: a member on the same network dials the leader
//!   there, or finds it by its id over mDNS, with no internet (no relay,
//!   no DNS). A network that allows no multicast binds n0's alone.
//! - **Editing together.** Once the leader's open of a load has settled,
//!   the leader shares that set as a live set (`collab::host_for_room`,
//!   over iroh, every song's doc) and says in the room's doc where
//!   ([`room::Share`]). A member, once it has opened the same load, joins
//!   it at the leader's addresses (`collab::join_for_room`): from then on
//!   every edit anyone in the room makes (a fader, a section, the chart)
//!   reaches everyone through the songs' docs. Each load is shared afresh,
//!   with the room's connection kept.
//! - **Agents.** An agent working through this engine's MCP (`AGENT_RUN`)
//!   is in the room's presence while it works ([`agent_worked`]), as a
//!   person is. Its undo step is named in the songs' docs
//!   (`Agent: <name>`), so every member's history shows it as the agent's.
//!
//! The media is each device's own: a loaded path inside the Studio folder
//! travels relative to it, so the same setlist opens on every device that
//! has it (as joining a shared set does).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use architect::action::{ActionCaller, ActionError, ActionList, ActionMeta, Effect, RawJson};
use architect::iroh_link::iroh;
use engine_sync::transport as session_transport;
use facet::Facet;
use room::gate::Door;
use room::net::{Find, RoomHost, RoomPeer};
use room::{Guest, Live, Loaded, Opens, Pass, Room, RoomId, Rooms, Share};
use serde_json::{Value, json};
use session_transport::{KEY, LocalTransport, SharedTransport, TransportSync};

/// How often the room's task looks at the engine and the room.
const TICK: Duration = Duration::from_millis(33);

/// How long an agent stays in the room after its last call.
const AGENT_LINGER: Duration = Duration::from_secs(120);

/// How long after a failed share or join of a load it is tried again.
const EDITS_RETRY: Duration = Duration::from_secs(3);

/// The agents working through this engine's MCP: each by name, and when
/// it last called.
static AGENTS: Mutex<Vec<(String, Instant)>> = Mutex::new(Vec::new());

/// Agent `name` called through this engine's MCP (`AGENT_RUN`): it is in
/// the room this engine is in, if any, for [`AGENT_LINGER`] after.
pub fn agent_worked(name: &str) {
    let mut agents = lock(&AGENTS);
    let now = Instant::now();
    match agents.iter_mut().find(|(n, _)| n == name) {
        Some((_, at)) => *at = now,
        None => agents.push((name.to_owned(), now)),
    }
}

/// The agents working through this engine now, by name.
fn agents_here() -> Vec<String> {
    let mut agents = lock(&AGENTS);
    agents.retain(|(_, at)| at.elapsed() < AGENT_LINGER);
    agents.iter().map(|(n, _)| n.clone()).collect()
}

/// This engine's agents into the room's presence, as `me`'s: the ones
/// that arrived since `shown`, and the ones gone taken out.
fn show_agents(presence: &live_doc::PresencePeer, me: &str, shown: &mut Vec<String>) {
    let now = agents_here();
    for gone in shown.iter().filter(|n| !now.contains(n)) {
        room::net::agent_left(presence, me, gone);
    }
    for came in now.iter().filter(|n| !shown.contains(n)) {
        room::net::agent_arrived(presence, me, came);
    }
    *shown = now;
}

/// The room this engine is in, if any.
static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);

/// Held across a whole lead, join or leave, so two at once cannot both
/// leave the room before and both take its place after.
static TURN: Mutex<()> = Mutex::new(());

struct Active {
    room: Room,
    name: String,
    role: Role,
    /// What the room's task found, for `ROOM`.
    seen: Arc<Mutex<Seen>>,
    stop: tokio::sync::watch::Sender<bool>,
    endpoint: iroh::Endpoint,
}

enum Role {
    Leader {
        host: RoomHost,
        presence: live_doc::PresencePeer,
        /// The room's door: how many calls it refused.
        door: Door,
    },
    Member {
        peer: Arc<RoomPeer>,
    },
}

/// What the room's task last saw.
#[derive(Default)]
struct Seen {
    /// The load this engine opened (a member's).
    opened: Option<u64>,
    /// The transport in force.
    transport: Option<SharedTransport>,
    /// The last thing that went wrong.
    error: Option<String>,
    /// The load whose set this engine edits together with the room: the
    /// one it shares (a leader) or joined (a member).
    edits: Option<u64>,
    /// A share or join under way, of this load.
    editing: Option<u64>,
    /// When a share or join last failed.
    edits_failed: Option<Instant>,
}

impl Seen {
    /// Whether load `load`'s set is to be shared or joined now: not yet
    /// done, none under way, and not straight after a failure.
    fn edits_due(&self, load: u64) -> bool {
        self.edits != Some(load)
            && self.editing.is_none()
            && self
                .edits_failed
                .is_none_or(|at| at.elapsed() >= EDITS_RETRY)
    }
}

/// The name the set of `room`'s load `load` is shared by: the same at
/// both ends.
fn room_set_name(room: &RoomId, load: u64) -> String {
    format!("room:{room}/{load}")
}

/// The songs of the set this engine has open, by engine project, once its
/// open has settled (done, not failed, every song in).
fn settled_songs() -> Option<Vec<String>> {
    let view = crate::studio_engine::set::view();
    if !matches!(view.opening, crate::studio_engine::set::Opening::Idle) {
        return None;
    }
    let lineup = crate::studio_engine::set::lineup()?;
    if !lineup.arriving.is_empty() || lineup.songs.is_empty() {
        return None;
    }
    Some(
        lineup
            .songs
            .into_iter()
            .map(|(_, project)| project)
            .collect(),
    )
}

/// Share or join load `load`'s set off the room's task, on a thread of
/// its own: `work` blocks on the engine's runtime (`collab`'s host and
/// join do), which a runtime thread must not. What finishes after this
/// engine left every room is left too; one in a room again keeps what
/// that room's own task made.
fn edit_in_background(
    seen: &Arc<Mutex<Seen>>,
    load: u64,
    stopped: &tokio::sync::watch::Receiver<bool>,
    work: impl FnOnce() -> Result<(), String> + Send + 'static,
) {
    lock(seen).editing = Some(load);
    let (noted, stopped) = (Arc::clone(seen), stopped.clone());
    let spawned = std::thread::Builder::new()
        .name("room-edits".into())
        .spawn(move || {
            let done = work();
            if *stopped.borrow() {
                if lock(&ACTIVE).is_none() {
                    crate::collab::leave();
                }
                return;
            }
            let mut seen = lock(&noted);
            seen.editing = None;
            match done {
                Ok(()) => {
                    seen.edits = Some(load);
                    seen.edits_failed = None;
                    seen.error = None;
                }
                Err(e) => {
                    tracing::warn!(room.error = %e, room.load = load, "room: the set is not edited together yet");
                    seen.edits_failed = Some(Instant::now());
                    seen.error = Some(e);
                }
            }
        });
    if let Err(e) = spawned {
        let mut seen = lock(seen);
        seen.editing = None;
        seen.edits_failed = Some(Instant::now());
        seen.error = Some(format!("no thread to share the set on: {e}"));
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `work` run to its end on the engine's runtime, from a thread of its
/// own: an action may be called on one of the runtime's threads.
fn on_runtime<T: Send + 'static>(
    work: impl std::future::Future<Output = Result<T, String>> + Send + 'static,
) -> Result<T, String> {
    let runtime = crate::open::runtime().ok_or("the engine is not up")?;
    std::thread::spawn(move || runtime.block_on(work))
        .join()
        .map_err(|_| "the room's work panicked".to_owned())?
}

/// How long a new leader's endpoint has to read its own addresses (it
/// reads the machine's interfaces as it starts) before the room's file
/// says where to reach it.
const ADDRS_WAIT: Duration = Duration::from_secs(2);

/// The first eight letters of an id: enough to tell peers apart.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn rooms() -> Result<Rooms, String> {
    Rooms::here().ok_or_else(|| "no Rooms folder: set FTS_ROOMS_DIR or HOME".to_owned())
}

/// Where this device keeps its endpoint key for leading `room`: the same
/// endpoint every time it leads it.
fn key_file(room: &RoomId) -> Result<PathBuf, String> {
    let data = dirs::data_local_dir().ok_or("no data folder for the room's key")?;
    Ok(data
        .join("fasttrackstudio")
        .join("rooms")
        .join(format!("{room}.key")))
}

/// This engine's transport, the song by its number in the set.
fn local_transport() -> LocalTransport {
    let (position, playing) =
        crate::engine::Transport::shared().map_or((0.0, false), crate::engine::Transport::read);
    let song = crate::studio_engine::set::lineup().map(|l| l.at.saturating_add(1).to_string());
    LocalTransport {
        playing,
        position,
        song,
    }
}

/// An endpoint for a room: found by its id through n0 and on the local
/// network, so a room plays between devices on one network with no
/// internet. A network with no multicast (no mDNS) binds n0's alone.
async fn bind(key: iroh::SecretKey) -> Result<iroh::Endpoint, String> {
    use architect::iroh_link::{PeerLookup, bind_endpoint_with};
    match bind_endpoint_with(key.clone(), PeerLookup::N0AndLan).await {
        Ok(endpoint) => Ok(endpoint),
        Err(e) => {
            tracing::warn!(room.error = %e, "room: no lookup on the local network; n0's alone");
            bind_endpoint_with(key, PeerLookup::N0)
                .await
                .map_err(|e| format!("iroh: {e}"))
        }
    }
}

/// The addresses a member dials this endpoint at (`collab`'s, which a
/// room's live set is dialed at too).
fn addrs_of(endpoint: &iroh::Endpoint) -> Vec<String> {
    crate::collab::reachable_at(endpoint)
}

/// [`addrs_of`] and the endpoint's relays, once it has read its own
/// addresses on this machine's networks and reached its home relay, or
/// [`ADDRS_WAIT`] has passed: what a member on the local network dials
/// with nothing to look the leader up in, and what a browser dials it
/// through (with no internet, there is no relay to wait for).
async fn settled_addrs(endpoint: &iroh::Endpoint) -> (Vec<String>, Vec<String>) {
    let began = architect::platform::now();
    while (endpoint.addr().ip_addrs().next().is_none()
        || endpoint.addr().relay_urls().next().is_none())
        && began.elapsed() < ADDRS_WAIT
    {
        architect::platform::sleep(Duration::from_millis(20)).await;
    }
    let relays = endpoint
        .addr()
        .relay_urls()
        .map(ToString::to_string)
        .collect();
    (addrs_of(endpoint), relays)
}

/// Leave the room this engine is in: a leader's room stops being live.
fn leave() {
    let Some(active) = lock(&ACTIVE).take() else {
        return;
    };
    let _ = active.stop.send(true);
    crate::collab::leave();
    if let Role::Leader { host, .. } = &active.role {
        host.close();
        if let Ok(rooms) = rooms()
            && let Err(e) = rooms.set_live(&active.room.id, None)
        {
            tracing::warn!(room.error = %e, "room: could not say the room is no longer live");
        }
    }
    let endpoint = active.endpoint;
    if let Some(runtime) = crate::open::runtime() {
        runtime.spawn(async move { endpoint.close().await });
    }
    tracing::info!(room.id = %active.room.id, "room: left");
}

/// Lead `link`'s room as `name`. A room another device leads is refused
/// unless `take`: a room has one leader.
fn lead(link: &str, name: String, take: bool) -> Result<Value, String> {
    let _turn = lock(&TURN);
    let rooms = rooms()?;
    let room = rooms.find(link).map_err(|e| e.to_string())?;
    let key = architect::iroh_link::load_or_create_secret_key(&key_file(&room.id)?)
        .map_err(|e| format!("the room's key: {e}"))?;
    if let Some(live) = &room.live
        && live.endpoint != key.public().to_string()
        && !take
    {
        return Err(format!(
            "{} leads {} now: ROOM_LEAD with take to lead it instead",
            live.leader, room.name
        ));
    }
    leave();
    let id = room.id.clone();
    let me = format!("leader-{}", short(&key.public().to_string()));
    // A new pass each time this device leads: whoever has left the
    // organization since holds one that opens nothing.
    let pass = Pass::mint();
    // The guests' own, as new: it lets them follow, and nothing else.
    let guests = Pass::mint();
    let door = Door::for_room(&id, &pass, &guests);
    let (host, presence, endpoint, (addrs, relays)) = on_runtime({
        let (me, name, door) = (me.clone(), name.clone(), door.clone());
        async move {
            let endpoint = bind(key).await?;
            let host = RoomHost::new(&id);
            let router = host.mount(architect::LayerRouter::new());
            let serving = endpoint.clone();
            architect::platform::spawn(async move { door.serve(&serving, router).await });
            let presence = host
                .own_presence(&me, &name)
                .await
                .map_err(|e| e.to_string())?;
            let addrs = settled_addrs(&endpoint).await;
            Ok((host, presence, endpoint, addrs))
        }
    })?;
    tracing::debug!(room.addrs = ?addrs, room.relays = ?relays, "room: the leader's addresses");
    let live = Live {
        leader: name.clone(),
        endpoint: endpoint.id().to_string(),
        addrs,
        relays,
        pass: pass.clone(),
    };
    let room = rooms
        .set_live(&room.id, Some(live))
        .map_err(|e| e.to_string())?;
    // A room whose lobby did not open is still the organization's to play
    // in: only its guests are kept out.
    if let Err(e) = rooms.open_lobby(&room, &guests) {
        tracing::warn!(room.error = %e, "room: the lobby did not open; no guests");
    }
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let seen = Arc::new(Mutex::new(Seen::default()));
    let runtime = crate::open::runtime().ok_or("the engine is not up")?;
    runtime.spawn(lead_task(
        Leading {
            host: host.clone(),
            presence: presence.clone(),
            room: room.id.clone(),
            pass,
            door: door.clone(),
            rooms: rooms.clone(),
            me,
            name: name.clone(),
        },
        Arc::clone(&seen),
        stopped,
    ));
    let link = room.link();
    tracing::info!(room.id = %room.id, room.role = "leader", "room: leading");
    *lock(&ACTIVE) = Some(Active {
        room,
        name,
        role: Role::Leader {
            host,
            presence,
            door,
        },
        seen,
        stop,
        endpoint,
    });
    Ok(json!({ "link": link, "role": "leader" }))
}

/// What the leader's task leads.
struct Leading {
    host: RoomHost,
    presence: live_doc::PresencePeer,
    room: RoomId,
    /// The room's pass: the live set it edits together opens with it too.
    pass: Pass,
    /// Its door, whose guests' arrivals go into the room's file.
    door: Door,
    rooms: Rooms,
    me: String,
    name: String,
}

/// Now, in milliseconds since the epoch: when a guest came.
fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The guests `door` let in since last asked, each entered in the room's
/// file: a new one added, one who came before known again.
fn enter_guests(door: &Door, rooms: &Rooms, room: &RoomId) {
    for endpoint in door.take_arrivals() {
        match rooms.guest_came(room, &endpoint, epoch_ms()) {
            Ok(guest) => tracing::info!(
                room.guest = %guest.name,
                room.visits = guest.visits,
                "room: a guest came"
            ),
            Err(e) => tracing::warn!(room.error = %e, "room: a guest came and was not entered"),
        }
    }
}

/// The leader's task: this engine's transport into the room's presence
/// whenever it says something new, each load shared for editing once it
/// has opened, and this engine's agents in the room.
async fn lead_task(
    leading: Leading,
    seen: Arc<Mutex<Seen>>,
    mut stopped: tokio::sync::watch::Receiver<bool>,
) {
    let Leading {
        host,
        presence,
        room,
        pass,
        door,
        rooms,
        me,
        name,
    } = leading;
    let mut prev: Option<SharedTransport> = None;
    let mut agents = Vec::new();
    loop {
        if let Some(entry) =
            room::lead::entry(prev.as_ref(), &local_transport(), live_doc::clock_ms(), &me)
        {
            presence.set(KEY, entry.encode());
            lock(&seen).transport = Some(entry.clone());
            prev = Some(entry);
        }
        if let Some((_, load)) = host.doc().loaded()
            && lock(&seen).edits_due(load)
            && let Some(projects) = settled_songs()
        {
            let set = crate::collab::RoomSet {
                set: room_set_name(&room, load),
                projects,
                door: Some((room.clone(), pass.clone())),
            };
            let (host, name) = (host.clone(), name.clone());
            edit_in_background(&seen, load, &stopped, move || {
                let (ticket, addrs) =
                    crate::collab::host_for_room(name, set).map_err(|e| e.to_string())?;
                // Another load came while this one was shared: that one is
                // shared next, never this one under its number.
                if host.doc().loaded().map(|(_, n)| n) != Some(load) {
                    return Err(format!("load {load} was replaced while it was shared"));
                }
                host.doc()
                    .share(&Share {
                        load,
                        ticket,
                        addrs,
                    })
                    .map_err(|e| e.to_string())?;
                tracing::info!(room.load = load, "room: the set is shared for editing");
                Ok(())
            });
        }
        show_agents(&presence, &me, &mut agents);
        enter_guests(&door, &rooms, &room);
        tokio::select! {
            () = architect::platform::sleep(TICK) => {}
            _ = stopped.wait_for(|s| *s) => return,
        }
    }
}

fn guest_json(guest: &Guest) -> Value {
    json!({ "id": guest.id, "name": guest.name, "visits": guest.visits, "first": guest.first, "last": guest.last })
}

/// Load what is at `path` into the room this engine leads.
fn load(path: &str) -> Result<Value, String> {
    let path = std::path::absolute(Path::new(path)).map_err(|e| e.to_string())?;
    if !path.exists() {
        return Err(format!("{}: no such file", path.display()));
    }
    let (host, id) = match lock(&ACTIVE).as_ref() {
        Some(Active {
            role: Role::Leader { host, .. },
            room,
            ..
        }) => (host.clone(), room.id.clone()),
        _ => return Err("this engine leads no room: ROOM_LEAD first".to_owned()),
    };
    let open = crate::assembly::get()
        .ok_or("no engine installed")?
        .open_path;
    open(path.clone())?;
    // The last load's set is edited together no more: this one is shared
    // once it has opened.
    crate::collab::leave();
    let rooms = rooms()?;
    let loaded = Loaded::of_path(&path, rooms.studio_root());
    host.doc().load(&loaded).map_err(|e| e.to_string())?;
    // The room's setlist from now on: what its link opens to practise
    // once nobody leads it. Only one inside the Studio folder, which means
    // the same file on every device.
    if Path::new(&loaded.path).is_relative() {
        match rooms.set_setlist(&id, loaded.clone()) {
            Ok(room) => {
                if let Some(active) = lock(&ACTIVE).as_mut()
                    && active.room.id == room.id
                {
                    active.room = room;
                }
            }
            Err(e) => {
                tracing::warn!(room.error = %e, "room: could not keep what was loaded as the room's setlist");
            }
        }
    }
    tracing::info!(room.loaded = %loaded.title, room.kind = loaded.kind.as_str(), "room: loaded");
    Ok(json!({ "loaded": loaded_json(&loaded) }))
}

/// Join `link`'s room as `name`, and follow its leader.
fn join(link: &str, name: String) -> Result<Value, String> {
    let _turn = lock(&TURN);
    let rooms = rooms()?;
    let room = rooms.find(link).map_err(|e| e.to_string())?;
    let live = match room.opens() {
        Opens::Leader(live) => live.clone(),
        Opens::Practice(setlist) => return practise(&rooms, &room, setlist),
        Opens::Nothing => {
            return Err(format!(
                "{} is not live, and has no setlist yet: nobody has led it",
                room.name
            ));
        }
    };
    room::net::leader_addr(&live)?;
    leave();
    let endpoint = on_runtime(bind(iroh::SecretKey::generate()))?;
    let me = format!("member-{}", short(&endpoint.id().to_string()));
    let (finding, id) = (rooms.clone(), room.id.clone());
    // Where the leader is now, each time: a leader back after a restart is
    // at new addresses, with a new pass (its endpoint is the same).
    let find: Find = Arc::new(move || {
        let found = finding.find(id.as_str()).ok().and_then(|r| r.live);
        Box::pin(async move { found })
    });
    let attach = room::net::attach_to(endpoint.clone(), live, find);
    let runtime = crate::open::runtime().ok_or("the engine is not up")?;
    let peer = {
        let _entered = runtime.enter();
        Arc::new(RoomPeer::join(&room.id, &me, &name, attach))
    };
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let seen = Arc::new(Mutex::new(Seen::default()));
    runtime.spawn(follow(
        Following {
            peer: Arc::clone(&peer),
            room: room.id.clone(),
            rooms: rooms.clone(),
            me,
            name: name.clone(),
            studio_root: rooms.studio_root().map(Path::to_path_buf),
        },
        Arc::clone(&seen),
        stopped,
    ));
    let link = room.link();
    tracing::info!(room.id = %room.id, room.role = "member", "room: joined");
    *lock(&ACTIVE) = Some(Active {
        room,
        name,
        role: Role::Member { peer },
        seen,
        stop,
        endpoint,
    });
    Ok(json!({ "link": link, "role": "member" }))
}

/// Open `room`'s setlist here, alone, to practise: nobody leads it, so
/// there is no one to follow, and this engine's transport is its own. The
/// room this engine was in, it leaves.
fn practise(rooms: &Rooms, room: &Room, setlist: &Loaded) -> Result<Value, String> {
    let path = setlist.resolve(rooms.studio_root());
    if !path.exists() {
        return Err(format!(
            "{}'s setlist, {}, is not on this device: {}",
            room.name,
            setlist.title,
            path.display()
        ));
    }
    let open = crate::assembly::get()
        .ok_or("no engine installed")?
        .open_path;
    leave();
    open(path)?;
    tracing::info!(
        room.id = %room.id,
        room.role = "practice",
        room.loaded = %setlist.title,
        "room: nobody leads it; practising its setlist"
    );
    Ok(json!({
        "link": room.link(),
        "role": "practice",
        "loaded": loaded_json(setlist),
    }))
}

/// What a member's task follows.
struct Following {
    peer: Arc<RoomPeer>,
    room: RoomId,
    /// Where the room's pass is read again for the live set: the leader's
    /// latest.
    rooms: Rooms,
    me: String,
    name: String,
    studio_root: Option<PathBuf>,
}

/// A member's task: each load opened here, joined for editing once the
/// leader shares it, the transport kept on the leader's, and this
/// engine's agents in the room.
async fn follow(
    following: Following,
    seen: Arc<Mutex<Seen>>,
    mut stopped: tokio::sync::watch::Receiver<bool>,
) {
    let Following {
        peer,
        room,
        rooms,
        me,
        name,
        studio_root,
    } = following;
    let mut sync = TransportSync::new(me.clone());
    let mut agents = Vec::new();
    loop {
        if let Some((loaded, load)) = peer.doc().loaded()
            && lock(&seen).opened != Some(load)
        {
            // The last load's set is edited together no more.
            if lock(&seen).edits.is_some() {
                crate::collab::leave();
                lock(&seen).edits = None;
            }
            let open = crate::assembly::get().map(|a| a.open_path);
            match open.map(|open| open(loaded.resolve(studio_root.as_deref()))) {
                Some(Ok(())) => {
                    tracing::info!(room.loaded = %loaded.title, room.load = load, "room: opening what the leader loaded");
                    peer.opened(load);
                    {
                        let mut seen = lock(&seen);
                        seen.opened = Some(load);
                        seen.error = None;
                    }
                    // What is in force carries over: the follower starts
                    // again on the new set.
                    sync = TransportSync::new(me.clone());
                }
                // Another open under way: tried again next tick.
                Some(Err(e)) => lock(&seen).error = Some(e),
                None => lock(&seen).error = Some("no engine installed".to_owned()),
            }
        }
        // An open that failed (what the leader loaded is not on this
        // device, say) is said, not followed into silence.
        if let crate::studio_engine::set::Opening::Failed(why) =
            crate::studio_engine::set::view().opening
        {
            let mut seen = lock(&seen);
            if seen.opened.is_some() && seen.error.as_deref() != Some(why.as_str()) {
                tracing::warn!(room.error = %why, "room: what the leader loaded did not open here");
                seen.error = Some(why);
            }
        }
        // Once this engine has opened the load the leader shares, it joins
        // it: everyone's edits to it reach this engine, and its own reach
        // everyone.
        let opened = lock(&seen).opened;
        if let Some(share) = peer.doc().shared()
            && opened == Some(share.load)
            && lock(&seen).edits_due(share.load)
            && let Some(projects) = settled_songs()
        {
            let pass = rooms
                .find(room.as_str())
                .ok()
                .and_then(|r| r.live)
                .map(|live| live.pass)
                .unwrap_or_default();
            let set = crate::collab::RoomSet {
                set: room_set_name(&room, share.load),
                projects,
                door: Some((room.clone(), pass)),
            };
            let name = name.clone();
            edit_in_background(&seen, share.load, &stopped, move || {
                crate::collab::join_for_room(&share.ticket, &share.addrs, name, set)
                    .map_err(|e| e.to_string())?;
                tracing::info!(
                    room.load = share.load,
                    "room: editing the set with the room"
                );
                Ok(())
            });
        }
        let presence = peer.presence();
        show_agents(&presence, &me, &mut agents);
        if let Some(entry) = room::net::transport(&presence) {
            sync.remote(entry);
        }
        lock(&seen).transport = sync.current().cloned();
        let ready = !crate::studio_engine::set::is_opening()
            && crate::studio_engine::set::lineup().is_some();
        if ready && let Some(now) = presence.clock().now_ms() {
            for command in sync.tick(&local_transport(), now).commands {
                obey(command);
            }
        }
        tokio::select! {
            () = architect::platform::sleep(TICK) => {}
            _ = stopped.wait_for(|s| *s) => return,
        }
    }
}

/// Do what the follower asks of this engine.
fn obey(command: session_transport::Command) {
    use crate::engine::{Move, transport_following};
    use session_transport::Command;
    match command {
        Command::SwitchSong(number) => {
            if let Err(e) = crate::studio_engine::set::pick_by(&number) {
                tracing::debug!(room.error = ?e, "room: the leader's song is not here yet");
            }
        }
        Command::Play { from } => transport_following(Move::PlayFrom, from),
        Command::Stop { at } => {
            transport_following(Move::Stop, at);
            transport_following(Move::Seek, at);
        }
        Command::Seek { to } => transport_following(Move::Seek, to),
    }
}

fn loaded_json(loaded: &Loaded) -> Value {
    serde_json::to_value(loaded).unwrap_or_default()
}

fn room_json(room: &Room) -> Value {
    json!({
        "id": room.id.as_str(),
        "name": room.name,
        "org": room.org,
        "link": room.link(),
        "live": room.live.as_ref().map(|l| json!({ "leader": l.leader })),
        "setlist": room.setlist.as_ref().map(loaded_json),
    })
}

/// What `ROOM` says.
fn status() -> Value {
    let guard = lock(&ACTIVE);
    let Some(active) = guard.as_ref() else {
        return json!({ "room": null });
    };
    let seen = lock(&active.seen);
    let (transport, error, opened) = (seen.transport.clone(), seen.error.clone(), seen.opened);
    let (edits, editing) = (seen.edits, seen.editing);
    drop(seen);
    let (role, doc, presence) = match &active.role {
        Role::Leader { host, presence, .. } => ("leader", host.doc().clone(), presence.clone()),
        Role::Member { peer } => ("member", peer.doc().clone(), peer.presence()),
    };
    // A member's attachment refused or lost says so, when nothing else
    // has gone wrong since.
    let error = error.or_else(|| match &active.role {
        Role::Member { peer } => peer.last_error(),
        Role::Leader { .. } => None,
    });
    let loaded = doc.loaded();
    let mut out = json!({
        "room": room_json(&active.room),
        "role": role,
        "name": active.name,
        "loaded": loaded.as_ref().map(|(l, _)| loaded_json(l)),
        "load": loaded.as_ref().map(|(_, n)| n),
        "people": room::net::people(&presence),
        "agents": room::net::agents(&presence),
        "here": room::net::here_now(&presence)
            .into_iter()
            .map(|p| json!({ "name": p.name, "agent": p.agent, "guest": p.guest, "opened": p.opened }))
            .collect::<Vec<_>>(),
        "edits": edits,
        "editing": editing.is_some(),
        "transport": transport.as_ref().map(|t| json!({
            "playing": t.playing,
            "position": t.position,
            "song": t.song,
        })),
        "error": error,
    });
    let more = match &active.role {
        Role::Leader { host, door, .. } => vec![
            ("members", json!(host.members())),
            ("refused", json!(door.refused())),
            (
                "guests",
                json!(
                    rooms()
                        .and_then(|r| r.find(active.room.id.as_str()).map_err(|e| e.to_string()))
                        .map(|r| r.guests.iter().map(guest_json).collect::<Vec<_>>())
                        .unwrap_or_default()
                ),
            ),
        ],
        Role::Member { peer } => vec![
            ("attaches", json!(peer.attaches())),
            ("opened", json!(opened)),
        ],
    };
    drop(guard);
    if let Some(out) = out.as_object_mut() {
        for (k, v) in more {
            out.insert(k.to_owned(), v);
        }
    }
    out
}

// ── the actions ─────────────────────────────────────────────────────────

static CREATE: ActionMeta = crate::actions::meta(
    "ROOM_CREATE",
    "room_create",
    "Make a room",
    "Make a room named `name` for organization `org` (its slug): an organization's lasting place to play together, kept in its Rooms folder. Answers its `id` and its `link` (`https://fasttrackstudio.app/join/<id>`), which never changes. ROOM_LEAD makes it live.",
    "Room",
    "",
);

static LIST: ActionMeta = crate::actions::meta(
    "ROOMS",
    "rooms",
    "The rooms",
    "Every room in the Rooms folder: `id`, `name`, `org`, `link`, `live` (who leads it now; null when it is not live) and `setlist` (what its leader last loaded: kind, path, title; what ROOM_JOIN opens to practise while nobody leads it). `org` narrows it to one organization's. Read-only.",
    "Room",
    "",
);

static LEAD: ActionMeta = crate::actions::meta(
    "ROOM_LEAD",
    "room_lead",
    "Lead a room",
    "Lead room `room` (its link or id) as `name` (left out: Leader): this engine hosts it behind its door, and its file says where its leader is and the pass the door opens to (new each time), so the organization's people can ROOM_JOIN by the link, from a desktop, a phone or a browser. It also opens the room's lobby to guests (no account: they follow and edit nothing), with a guests' pass of its own. A room has one leader: one another device leads is refused unless `take` (true: lead it instead, e.g. after that device went away without leaving). Then ROOM_LOAD what everyone plays; this engine's transport is the room's.",
    "Room",
    "",
);

static LOAD: ActionMeta = crate::actions::meta(
    "ROOM_LOAD",
    "room_load",
    "Load into the room",
    "Open `path` (a `.setlist`, or a lone session: `.RPP`, `.session`, a song's folder) here and load it into the room this engine leads: everyone in it opens it too, without reconnecting. It becomes the room's setlist, which its link opens to practise while nobody leads it. Returns at once — SESSION_OPENING says how the open goes.",
    "Room",
    "",
);

static JOIN: ActionMeta = crate::actions::meta(
    "ROOM_JOIN",
    "room_join",
    "Join a room",
    "Join room `link` (`https://fasttrackstudio.app/join/<id>`, or the id) as `name` (left out: Member), and follow its leader: what they load opens here, and the transport plays, stops, jumps and changes song with theirs. Joined once; the leader loading something else needs no rejoin. ROOM says how it goes. A room nobody leads opens its setlist here instead (`role`: practice, `loaded`), to practise alone; a room nobody has loaded anything into is refused.",
    "Room",
    "",
);

static STATUS: ActionMeta = crate::actions::meta(
    "ROOM",
    "room",
    "The room",
    "The room this engine is in: `room` (id, name, org, link; null when in none), `role` (leader or member), `name`, `loaded` (kind, path, title) and `load` (which load it is), `people` (everyone here, by name, agents working through anyone's MCP included), `agents` (the agents among them), `here` (each one: `name`, `agent`, `guest` (no account in the organization: follows, edits nothing), and `opened`, the load a member has opened, so a leader sees each member follow, a browser's or a phone's too), `edits` (the load whose set this engine edits together with the room: everyone's edits to it reach everyone; null until it is shared and joined) and `editing` (a share or join under way), `transport` (the leader's: playing, position, song number), `error`. A leader's `members` (attached now), `refused` (calls its door turned away: any without the room's pass, which only the organization's people can read, or a guest's at anything but following) and `guests` (every guest who has come: `id`, the endpoint their device proved; `name`; `visits`; `first` and `last`, ms since the epoch: a guest who comes back is the same one, one more visit); a member's `attaches` (how many times it connected to the leader: 1 unless a connection was lost) and `opened` (the load it opened). Read-only.",
    "Room",
    "",
);

static LEAVE: ActionMeta = crate::actions::meta(
    "ROOM_LEAVE",
    "room_leave",
    "Leave the room",
    "Leave the room this engine is in. A leader's room stops being live; its link stays the same.",
    "Room",
    "",
);

#[derive(Facet)]
struct CreateArg {
    name: String,
    org: String,
}

#[derive(Facet)]
struct ListArg {
    org: Option<String>,
}

#[derive(Facet)]
struct LeadArg {
    room: String,
    name: Option<String>,
    take: Option<bool>,
}

#[derive(Facet)]
struct LoadArg {
    path: String,
}

#[derive(Facet)]
struct JoinArg {
    link: String,
    name: Option<String>,
}

fn named(name: Option<String>, otherwise: &str) -> String {
    name.map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| otherwise.to_owned())
}

fn raw(value: &Value) -> RawJson {
    RawJson::from(value.to_string())
}

/// The room's actions, into `list`.
///
/// # Errors
///
/// The first registration the list refuses.
pub fn register(list: &ActionList) -> Result<(), ActionError> {
    list.register_with(
        &CREATE,
        Effect::Mutating,
        |_: &ActionCaller, a: CreateArg| {
            let room = rooms()?
                .create(&a.name, &a.org)
                .map_err(|e| e.to_string())?;
            tracing::info!(room.id = %room.id, room.org = %room.org, "room: made");
            Ok(raw(&room_json(&room)))
        },
    )?;
    list.register_with(&LIST, Effect::ReadOnly, |_: &ActionCaller, a: ListArg| {
        let all = rooms()?.list().map_err(|e| e.to_string())?;
        let listed: Vec<Value> = all
            .iter()
            .filter(|r| a.org.as_ref().is_none_or(|o| &r.org == o))
            .map(room_json)
            .collect();
        Ok(raw(&Value::Array(listed)))
    })?;
    list.register_with(&LEAD, Effect::Mutating, |_: &ActionCaller, a: LeadArg| {
        Ok(raw(&lead(
            &a.room,
            named(a.name, "Leader"),
            a.take.unwrap_or(false),
        )?))
    })?;
    list.register_with(&LOAD, Effect::Mutating, |_: &ActionCaller, a: LoadArg| {
        Ok(raw(&load(&a.path)?))
    })?;
    list.register_with(&JOIN, Effect::Mutating, |_: &ActionCaller, a: JoinArg| {
        Ok(raw(&join(&a.link, named(a.name, "Member"))?))
    })?;
    list.register_with(&STATUS, Effect::ReadOnly, |_: &ActionCaller, (): ()| {
        Ok(raw(&status()))
    })?;
    list.register(&LEAVE, Effect::Mutating, |(): ()| {
        let _turn = lock(&TURN);
        leave();
        Ok("left the room".to_owned())
    })?;
    Ok(())
}
