//! A room in the engine: leading one, or following its leader
//! (ADR 0003; the room itself is the `room` crate's).
//!
//! - **Making one.** `ROOM_CREATE` makes an organization's room in its
//!   Rooms folder (`room::Rooms::here`): its link
//!   (`https://fasttrackstudio.app/join/<room>`) never changes.
//! - **Leading.** `ROOM_LEAD` hosts the room over iroh, on an endpoint
//!   whose key this device keeps per room (`<data>/fasttrackstudio/rooms/`),
//!   and says in the room's file where to reach it. `ROOM_LOAD` opens a
//!   setlist or a lone session here and loads it into the room's doc. A
//!   task of the room's writes this engine's transport into the room's
//!   presence whenever it starts, stops, changes song or jumps
//!   (`room::lead::entry`).
//! - **Following.** `ROOM_JOIN` finds the room by its link, dials its
//!   leader once, and follows: each load the doc gains is opened here
//!   (through the engine's `open_path`, as `SESSION_OPEN` would), and the
//!   transport is kept on the leader's with session-sync's
//!   `TransportSync`, the song by its number in the set. The leader
//!   loading another setlist is a change to the doc, never a new
//!   connection (`ROOM`'s `attaches` stays 1).
//!
//! - **On the local network.** Both ends bind their endpoint with
//!   n0's lookup and the local network's (`PeerLookup::N0AndLan`), and
//!   the leader writes the addresses it has on this machine's networks
//!   into the room's file: a member on the same network dials the leader
//!   there, or finds it by its id over mDNS, with no internet (no relay,
//!   no DNS). A network that allows no multicast binds n0's alone.
//!
//! The media is each device's own: a loaded path inside the Studio folder
//! travels relative to it, so the same setlist opens on every device that
//! has it (as joining a shared set does). Editing the open session
//! together rides on this later.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use architect::action::{ActionCaller, ActionError, ActionList, ActionMeta, Effect, RawJson};
use architect::iroh_link::iroh;
use engine_sync::transport as session_transport;
use facet::Facet;
use room::net::{Attach, RoomHost, RoomPeer};
use room::{Live, Loaded, Room, RoomId, Rooms};
use serde_json::{Value, json};
use session_transport::{KEY, LocalTransport, SharedTransport, TransportSync};

/// How often the room's task looks at the engine and the room.
const TICK: Duration = Duration::from_millis(33);

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

/// The addresses a member dials this endpoint at: what it knows of
/// itself, and its sockets on this machine.
fn addrs_of(endpoint: &iroh::Endpoint) -> Vec<String> {
    let mut addrs: Vec<String> = endpoint
        .addr()
        .ip_addrs()
        .map(ToString::to_string)
        .collect();
    for socket in endpoint.bound_sockets() {
        let socket = if socket.ip().is_unspecified() {
            let ip: std::net::IpAddr = if socket.is_ipv4() {
                std::net::Ipv4Addr::LOCALHOST.into()
            } else {
                std::net::Ipv6Addr::LOCALHOST.into()
            };
            std::net::SocketAddr::new(ip, socket.port())
        } else {
            socket
        };
        let socket = socket.to_string();
        if !addrs.contains(&socket) {
            addrs.push(socket);
        }
    }
    addrs
}

/// [`addrs_of`] once the endpoint has read its own addresses on this
/// machine's networks, or [`ADDRS_WAIT`] has passed: what a member on
/// the local network dials with nothing to look the leader up in.
async fn settled_addrs(endpoint: &iroh::Endpoint) -> Vec<String> {
    let began = architect::platform::now();
    while endpoint.addr().ip_addrs().next().is_none() && began.elapsed() < ADDRS_WAIT {
        architect::platform::sleep(Duration::from_millis(20)).await;
    }
    addrs_of(endpoint)
}

/// Leave the room this engine is in: a leader's room stops being live.
fn leave() {
    let Some(active) = lock(&ACTIVE).take() else {
        return;
    };
    let _ = active.stop.send(true);
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
    let (host, presence, endpoint, addrs) = on_runtime({
        let (me, name) = (me.clone(), name.clone());
        async move {
            let endpoint = bind(key).await?;
            let host = RoomHost::new(&id);
            let router = host.mount(architect::LayerRouter::new());
            let serving = endpoint.clone();
            architect::platform::spawn(async move {
                architect::iroh_link::serve_router(&serving, router).await;
            });
            let presence = host
                .own_presence(&me, &name)
                .await
                .map_err(|e| e.to_string())?;
            let addrs = settled_addrs(&endpoint).await;
            Ok((host, presence, endpoint, addrs))
        }
    })?;
    tracing::debug!(room.addrs = ?addrs, "room: the leader's addresses");
    let live = Live {
        leader: name.clone(),
        endpoint: endpoint.id().to_string(),
        addrs,
    };
    let room = rooms
        .set_live(&room.id, Some(live))
        .map_err(|e| e.to_string())?;
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let seen = Arc::new(Mutex::new(Seen::default()));
    let runtime = crate::open::runtime().ok_or("the engine is not up")?;
    runtime.spawn(publish_transport(
        presence.clone(),
        me,
        Arc::clone(&seen),
        stopped,
    ));
    let link = room.link();
    tracing::info!(room.id = %room.id, room.role = "leader", "room: leading");
    *lock(&ACTIVE) = Some(Active {
        room,
        name,
        role: Role::Leader { host, presence },
        seen,
        stop,
        endpoint,
    });
    Ok(json!({ "link": link, "role": "leader" }))
}

/// The leader's task: this engine's transport into the room's presence
/// whenever it says something new.
async fn publish_transport(
    presence: live_doc::PresencePeer,
    me: String,
    seen: Arc<Mutex<Seen>>,
    mut stopped: tokio::sync::watch::Receiver<bool>,
) {
    let mut prev: Option<SharedTransport> = None;
    loop {
        if let Some(entry) =
            room::lead::entry(prev.as_ref(), &local_transport(), live_doc::clock_ms(), &me)
        {
            presence.set(KEY, entry.encode());
            lock(&seen).transport = Some(entry.clone());
            prev = Some(entry);
        }
        tokio::select! {
            () = architect::platform::sleep(TICK) => {}
            _ = stopped.wait_for(|s| *s) => return,
        }
    }
}

/// Load what is at `path` into the room this engine leads.
fn load(path: &str) -> Result<Value, String> {
    let path = std::path::absolute(Path::new(path)).map_err(|e| e.to_string())?;
    if !path.exists() {
        return Err(format!("{}: no such file", path.display()));
    }
    let host = match lock(&ACTIVE).as_ref() {
        Some(Active {
            role: Role::Leader { host, .. },
            ..
        }) => host.clone(),
        _ => return Err("this engine leads no room: ROOM_LEAD first".to_owned()),
    };
    let open = crate::assembly::get()
        .ok_or("no engine installed")?
        .open_path;
    open(path.clone())?;
    let rooms = rooms()?;
    let loaded = Loaded::of_path(&path, rooms.studio_root());
    host.doc().load(&loaded).map_err(|e| e.to_string())?;
    tracing::info!(room.loaded = %loaded.title, room.kind = loaded.kind.as_str(), "room: loaded");
    Ok(json!({ "loaded": loaded_json(&loaded) }))
}

/// Where to dial the leader `live` says: its endpoint at its addresses.
fn leader_addr(live: &Live) -> Result<iroh::EndpointAddr, String> {
    let leader: iroh::EndpointId = live
        .endpoint
        .parse()
        .map_err(|e| format!("the leader's endpoint: {e}"))?;
    let mut addr = iroh::EndpointAddr::new(leader);
    for a in &live.addrs {
        match a.parse() {
            Ok(socket) => addr = addr.with_ip_addr(socket),
            Err(e) => {
                tracing::debug!(room.error = %e, "room: an address of the leader's that is none");
            }
        }
    }
    Ok(addr)
}

/// Join `link`'s room as `name`, and follow its leader.
fn join(link: &str, name: String) -> Result<Value, String> {
    let _turn = lock(&TURN);
    let rooms = rooms()?;
    let room = rooms.find(link).map_err(|e| e.to_string())?;
    let live = room
        .live
        .clone()
        .ok_or_else(|| format!("{} is not live: nobody leads it now", room.name))?;
    let joined_at = leader_addr(&live)?;
    leave();
    let endpoint = on_runtime(bind(iroh::SecretKey::generate()))?;
    let me = format!("member-{}", short(&endpoint.id().to_string()));
    let dialing = endpoint.clone();
    let (finding, id) = (rooms.clone(), room.id.clone());
    let attach: Attach = Arc::new(move || {
        // Where the leader is now, each time: a leader back after a restart
        // is at new addresses (its endpoint is the same).
        let addr = finding
            .find(id.as_str())
            .ok()
            .and_then(|r| r.live)
            .and_then(|live| leader_addr(&live).ok())
            .unwrap_or_else(|| joined_at.clone());
        let endpoint = dialing.clone();
        Box::pin(async move {
            let dial = async || {
                architect::iroh_link::connect(&endpoint, addr.clone())
                    .await
                    .map_err(|e| eyre::eyre!("iroh connect: {e}"))
            };
            let sync = vox_core::initiator_on(dial().await?)
                .establish::<live_doc::DocSyncClient>()
                .await
                .map_err(|e| eyre::eyre!("the room's doc: {e:?}"))?;
            let presence = vox_core::initiator_on(dial().await?)
                .establish::<live_doc::DocPresenceClient>()
                .await
                .map_err(|e| eyre::eyre!("the room's presence: {e:?}"))?;
            Ok((sync, presence))
        })
    });
    let runtime = crate::open::runtime().ok_or("the engine is not up")?;
    let peer = {
        let _entered = runtime.enter();
        Arc::new(RoomPeer::join(&room.id, &me, &name, attach))
    };
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let seen = Arc::new(Mutex::new(Seen::default()));
    runtime.spawn(follow(
        Arc::clone(&peer),
        me,
        rooms.studio_root().map(Path::to_path_buf),
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

/// A member's task: each load opened here, and the transport kept on the
/// leader's.
async fn follow(
    peer: Arc<RoomPeer>,
    me: String,
    studio_root: Option<PathBuf>,
    seen: Arc<Mutex<Seen>>,
    mut stopped: tokio::sync::watch::Receiver<bool>,
) {
    let mut sync = TransportSync::new(me.clone());
    loop {
        if let Some((loaded, load)) = peer.doc().loaded()
            && lock(&seen).opened != Some(load)
        {
            let open = crate::assembly::get().map(|a| a.open_path);
            match open.map(|open| open(loaded.resolve(studio_root.as_deref()))) {
                Some(Ok(())) => {
                    tracing::info!(room.loaded = %loaded.title, room.load = load, "room: opening what the leader loaded");
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
        let presence = peer.presence();
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
    json!({ "kind": loaded.kind.as_str(), "path": loaded.path, "title": loaded.title })
}

fn room_json(room: &Room) -> Value {
    json!({
        "id": room.id.as_str(),
        "name": room.name,
        "org": room.org,
        "link": room.link(),
        "live": room.live.as_ref().map(|l| json!({ "leader": l.leader })),
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
    drop(seen);
    let (role, doc, presence) = match &active.role {
        Role::Leader { host, presence } => ("leader", host.doc().clone(), presence.clone()),
        Role::Member { peer } => ("member", peer.doc().clone(), peer.presence()),
    };
    let loaded = doc.loaded();
    let mut out = json!({
        "room": room_json(&active.room),
        "role": role,
        "name": active.name,
        "loaded": loaded.as_ref().map(|(l, _)| loaded_json(l)),
        "load": loaded.as_ref().map(|(_, n)| n),
        "people": room::net::people(&presence),
        "transport": transport.as_ref().map(|t| json!({
            "playing": t.playing,
            "position": t.position,
            "song": t.song,
        })),
        "error": error,
    });
    let more = match &active.role {
        Role::Leader { host, .. } => vec![("members", json!(host.members()))],
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
    "Every room in the Rooms folder: `id`, `name`, `org`, `link`, and `live` (who leads it now; null when it is not live). `org` narrows it to one organization's. Read-only.",
    "Room",
    "",
);

static LEAD: ActionMeta = crate::actions::meta(
    "ROOM_LEAD",
    "room_lead",
    "Lead a room",
    "Lead room `room` (its link or id) as `name` (left out: Leader): this engine hosts it, and its file says where its leader is, so anyone with the link can ROOM_JOIN. A room has one leader: one another device leads is refused unless `take` (true: lead it instead, e.g. after that device went away without leaving). Then ROOM_LOAD what everyone plays; this engine's transport is the room's.",
    "Room",
    "",
);

static LOAD: ActionMeta = crate::actions::meta(
    "ROOM_LOAD",
    "room_load",
    "Load into the room",
    "Open `path` (a `.setlist`, or a lone session: `.RPP`, `.session`, a song's folder) here and load it into the room this engine leads: everyone in it opens it too, without reconnecting. Returns at once — SESSION_OPENING says how the open goes.",
    "Room",
    "",
);

static JOIN: ActionMeta = crate::actions::meta(
    "ROOM_JOIN",
    "room_join",
    "Join a room",
    "Join room `link` (`https://fasttrackstudio.app/join/<id>`, or the id) as `name` (left out: Member), and follow its leader: what they load opens here, and the transport plays, stops, jumps and changes song with theirs. Joined once; the leader loading something else needs no rejoin. ROOM says how it goes.",
    "Room",
    "",
);

static STATUS: ActionMeta = crate::actions::meta(
    "ROOM",
    "room",
    "The room",
    "The room this engine is in: `room` (id, name, org, link; null when in none), `role` (leader or member), `name`, `loaded` (kind, path, title) and `load` (which load it is), `people` (everyone here, by name), `transport` (the leader's: playing, position, song number), `error`. A leader's `members` (attached now); a member's `attaches` (how many times it connected to the leader: 1 unless a connection was lost) and `opened` (the load it opened). Read-only.",
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
