//! Collaboration: share the open setlist with others, or join someone's.
//!
//! A session is the whole SET: every song has its own doc (and its own
//! bridge to its engine project), and one presence channel spans them, so
//! people can be on different songs and still see where everyone is —
//! the song tabs show who is on which, and the arrangement draws the
//! people on the song it shows. A lone song is a set of one.
//!
//! **One doc, one bridge, per song.** In an engine (the desktop's `studio
//! engine`, a window's own) a song's one bridge is its project file's
//! (`crate::project_file`): the set's doc of the song is never bridged to
//! the engine. Hosting over iroh serves the song's own doc as it is;
//! joining a set (by ticket, or one Task keeps) keeps the set's replica of
//! the song in step with the song's doc as a CRDT
//! (`session::sync::relay`) — seeded from it when the set has none yet, or,
//! when another peer seeded the set from a copy of its own, the song moved
//! onto the set's doc (`project_file::adopt`). Two bridges on one engine
//! song rewrote each other and doubled its FX chains every round (#316).
//! A page has no project file: there the set's doc is the song's one doc,
//! and its bridge is the one the page built the song with.
//!
//! One task per session owns everything that must not race: a page's
//! bridges, this peer's presence, and the shared transport's follower. It
//! wakes on three things —
//!
//! - a song's engine project changed (its event bus): read it back, write
//!   that song's doc;
//! - a song's doc changed remotely (a Loro import): make that project
//!   match, and have the arrangement read it back if it is showing it;
//! - a ~30 Hz tick: publish what this peer is doing (throttled, only what
//!   changed), fold everyone's presence into the roster, and keep the
//!   engine on the shared transport.
//!
//! Hosting serves the set over iroh; joining dials it. The ticket
//! (`fts-session:<endpoint id>/<set uuid>`) is what you hand someone. A
//! joiner opens the same setlist first — the media is read from its own
//! copy — and each of its songs is then made to match the host's. Or Task
//! keeps the set ([`join_task`]): everyone joins it there, a browser too.
//!
//! The same code runs natively and in a browser: its tasks and timers are
//! `architect::platform`'s (tokio natively, the page's event loop in wasm).
//! Hosting and iroh are the desktop's; a page only joins a set Task keeps.
//!
//! Signed in to Task, the desktop needs no button to share: whatever it
//! has open is hosted in Task by itself (`live_host`), and the
//! members and agents in the set edit it live.
//!
//! **This device's lock.** "Lock / don't sync" ([`set_sync_locked`]) is
//! per device and remembered on it: locked, the session's task holds the
//! others' edits in the songs' docs instead of making them in this engine,
//! and keeps this engine's edits (the chart's too) out of the docs. The
//! set's presence goes on. Unlocking merges, song by song
//! (`Bridge::merge_held`): this engine's edits go into the doc first, as
//! one step, then the engine is made to match the doc, which holds both.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use session::sync::clock::SharedClock;
use session::sync::engine::MediaRoot;
use session::sync::net::{Dial, PresenceSink, SetPeer};
#[cfg(feature = "native")]
use session::sync::net::{SetHost, dial_fixed, session_id, song_id};
use session::sync::presence::{self, PeerState, PlayState, Pointer, Roster, Throttle};
use session::sync::relay::{Relay, Side};
use session::sync::transport::{
    self, Command, LocalTransport, SharedTransport, SyncPosition, TransportMode, TransportSync,
};
use session::sync::{Bridge, Change, Hold, ORIGIN_LOCAL, SessionDoc};
use uuid::Uuid;

/// Where the sessions' FX state is kept while they are shared: one
/// store for the process, so a song shared twice reads its states once.
#[cfg_attr(feature = "native", allow(dead_code))]
fn blobs() -> std::sync::Arc<dyn session::sync::engine::Blobs> {
    static BLOBS: std::sync::LazyLock<std::sync::Arc<session::sync::engine::MemoryBlobs>> =
        std::sync::LazyLock::new(Default::default);
    BLOBS.clone()
}

/// The engine in this process, for the bridge to read and rebuild a
/// song's items through directly; `None` when the song is another
/// process's.
#[cfg_attr(feature = "native", allow(dead_code))]
fn local() -> Option<std::sync::Arc<dyn session::sync::engine::Direct>> {
    #[cfg(feature = "native")]
    {
        crate::open::local_engine().map(|daw| {
            std::sync::Arc::new(daw.clone()) as std::sync::Arc<dyn session::sync::engine::Direct>
        })
    }
    #[cfg(not(feature = "native"))]
    {
        None
    }
}

/// What the collaboration is doing, for the toolbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub hosting: bool,
    /// What to hand someone so they can join.
    pub ticket: String,
    /// How many others are here.
    pub peers: usize,
    pub shared_transport: bool,
    /// The set being shared (`Worship Set`, or a lone song's name).
    pub set: String,
    /// This person, as the others see them: name and colour.
    pub name: String,
    pub color: u32,
    /// When a playground set (the public demo) starts over, on this
    /// machine's clock; `None` for a set that keeps what is done in it.
    pub resets_at: Option<architect::platform::Instant>,
    /// This device is locked ([`sync_locked`]): nothing goes out or comes
    /// in until it is unlocked.
    pub locked: bool,
    /// Songs with something held by the lock, to merge when it lifts.
    pub held: usize,
}

/// One song of the set, as the session keeps it.
#[derive(Clone)]
struct Song {
    /// Its engine project (minted per open — this machine's only).
    project: String,
    /// The name every machine knows it by: its file's stem (`Washed`).
    key: String,
    doc: SessionDoc,
}

struct Live {
    status: Status,
    stop: tokio::sync::watch::Sender<bool>,
    /// This peer's end of the shared transport.
    sync: Arc<Mutex<TransportSync>>,
    /// Charts the others changed, by project, since the editor looked.
    remote_chart: Arc<Mutex<HashMap<String, String>>>,
    songs: Vec<Song>,
    presence: Arc<dyn PresenceSink>,
    me: String,
    /// Shared with anyone (hosting or joined), or only this machine's
    /// record of the set's edits.
    shared: bool,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

/// How a song of the set is kept with the set's doc of it.
enum Link {
    /// A page's song: its one bridge, on the set's doc (a page has no
    /// project file).
    #[cfg_attr(feature = "native", allow(dead_code))]
    Bridged(Bridge),
    /// An engine's song: its one bridge is its file's; the set's doc is
    /// kept in step with the song's doc as a CRDT.
    #[cfg_attr(not(feature = "native"), allow(dead_code))]
    Relayed(Relay),
    /// Nothing to keep: the song's own doc served as it is (hosting over
    /// iroh), or a song the set does not have.
    #[cfg_attr(not(feature = "native"), allow(dead_code))]
    Kept,
}

/// Songs with something held by this device's lock: a page's bridges'
/// (`hold`) and an engine's relays'.
fn held_count(hold: &Hold, links: &[Link]) -> usize {
    hold.held()
        + links
            .iter()
            .filter(|l| matches!(l, Link::Relayed(relay) if relay.held()))
            .count()
}

/// Who this peer is in a session shared with nobody (this machine's own
/// record of its songs' edits) — never locked: there is nothing to hold.
const LOCAL_ME: &str = "local";

/// Join a song's doc that already holds the session: the engine made to
/// match it — or, locked ([`sync_locked`]), left as it is until the lock
/// lifts. A song whose build left its bridge ([`song_built`]) is joined
/// through it ([`Bridge::rejoin`]): the engine is not read again. A
/// page's: an engine's songs keep their file's bridge.
#[cfg_attr(feature = "native", allow(dead_code))]
async fn join_bridge(
    project: daw_control::Project,
    media: MediaRoot,
    doc: SessionDoc,
) -> eyre::Result<Bridge> {
    let built = take_built(project.guid());
    if sync_locked() {
        Bridge::join_held(project, media, doc, blobs(), local()).await
    } else if let Some(built) = built {
        built.rejoin(doc).await
    } else {
        Bridge::join(project, media, doc, blobs(), local()).await
    }
}

thread_local! {
    /// Bridges songs were built by, by project, until the session takes
    /// them ([`song_built`]). The page's: it builds and joins on its one
    /// thread, and its bridges are not `Send` (the facade's handles).
    static BUILT: std::cell::RefCell<HashMap<String, Bridge>> =
        std::cell::RefCell::new(HashMap::new());
}

/// `project` was just built from its file by `bridge`, and is about to
/// join a set Task keeps: the session joins it through that bridge, which
/// knows the engine as built, instead of reading the song back again.
pub fn song_built(project: &str, bridge: Bridge) {
    BUILT.with(|built| built.borrow_mut().insert(project.to_owned(), bridge));
}

/// The bridge `project` was built by, if it left one ([`song_built`]).
#[cfg_attr(feature = "native", allow(dead_code))]
fn take_built(project: &str) -> Option<Bridge> {
    BUILT.with(|built| built.borrow_mut().remove(project))
}

/// Whether an environment switch is on: set, and not to nothing.
#[must_use]
pub fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

/// Why the last share or join failed, for the bar.
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// Why the last share or join failed, if it did.
#[must_use]
pub fn last_error() -> Option<String> {
    LAST_ERROR.lock().ok()?.clone()
}

fn record<T>(result: eyre::Result<T>) -> eyre::Result<T> {
    if let Ok(mut slot) = LAST_ERROR.lock() {
        *slot = result.as_ref().err().map(ToString::to_string);
    }
    result
}

#[cfg(feature = "native")]
/// [`host`] off the calling thread (a click must not wait on iroh).
pub fn host_in_background(name: String) {
    std::thread::spawn(move || {
        let _ = host(name);
    });
}

#[cfg(feature = "native")]
/// [`join`] off the calling thread.
pub fn join_in_background(ticket: String, name: String) {
    std::thread::spawn(move || {
        let _ = join(&ticket, name);
    });
}

/// The session's status, if one is live.
#[must_use]
pub fn status() -> Option<Status> {
    LIVE.lock()
        .ok()?
        .as_ref()
        .filter(|l| l.shared)
        .map(|l| Status {
            locked: sync_locked(),
            held: HELD.load(std::sync::atomic::Ordering::Relaxed),
            ..l.status.clone()
        })
}

// ---- This device's lock ------------------------------------------------
//
// "Lock / don't sync", per device (`session::sync::hold`): locked, the
// session's task holds what the others send in the songs' docs instead of
// making it in this engine, and keeps this engine's edits out of the docs;
// unlocking merges both. Presence goes on either way — the others still
// see who is here and where. The switch is remembered on this device.

/// The switch, as the session's task follows it (it wakes on a change).
static LOCK: std::sync::LazyLock<tokio::sync::watch::Sender<bool>> =
    std::sync::LazyLock::new(|| tokio::sync::watch::channel(remembered_lock()).0);

/// Songs with something held right now (the session's task keeps it).
static HELD: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Chart edits made here while locked, by project: written into their
/// songs' docs as the lock lifts.
static HELD_CHARTS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Whether this device is locked: not sending its edits, not taking the
/// others'.
#[must_use]
pub fn sync_locked() -> bool {
    *LOCK.borrow()
}

/// Lock this device, or unlock it — unlocking merges what was held: this
/// engine's edits go out, then the others' are made here.
pub fn set_sync_locked(locked: bool) {
    if !locked {
        // The chart editor's edits first: the session's task merges the
        // rest, and the doc then holds both.
        let charts = HELD_CHARTS
            .lock()
            .ok()
            .and_then(|mut c| c.take())
            .unwrap_or_default();
        for (project, text) in charts {
            if let Some((song, _, _)) = song_where(|s| s.project == project) {
                write_chart(&song, &text);
            }
        }
    }
    remember_lock(locked);
    LOCK.send_replace(locked);
    tracing::info!(collab.locked = locked, "collab: this device's sync switch");
}

/// Where the switch is remembered: this device's data dir.
#[cfg(feature = "native")]
fn lock_file() -> Option<std::path::PathBuf> {
    Some(dirs::data_dir()?.join("Session").join("live-sync-locked"))
}

#[cfg(feature = "native")]
fn remembered_lock() -> bool {
    lock_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .is_some_and(|v| v.trim() == "1")
}

#[cfg(feature = "native")]
fn remember_lock(locked: bool) {
    let Some(path) = lock_file() else { return };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, if locked { "1" } else { "0" }));
    if let Err(e) = written {
        tracing::warn!(collab.error = %e, "collab: the sync switch was not remembered");
    }
}

/// A page starts live and remembers nothing.
#[cfg(not(feature = "native"))]
const fn remembered_lock() -> bool {
    false
}

#[cfg(not(feature = "native"))]
const fn remember_lock(_: bool) {}

fn song_where(pick: impl Fn(&Song) -> bool) -> Option<(Song, Arc<dyn PresenceSink>, String)> {
    let live = LIVE.lock().ok()?;
    let l = live.as_ref()?;
    let song = l.songs.iter().find(|s| pick(s))?.clone();
    Some((song, Arc::clone(&l.presence), l.me.clone()))
}

fn current() -> Option<(Song, Arc<dyn PresenceSink>, String)> {
    let project = crate::open::current_song()?;
    song_where(|s| s.project == project)
}

/// The name a song is known by everywhere (`Washed`), from its project
/// here.
#[must_use]
pub fn key_of(project: &str) -> Option<String> {
    song_where(|s| s.project == project).map(|(s, _, _)| s.key)
}

/// A song's full edit history, to save with it — `None` when no session
/// doc is keeping one for `project` (yet).
#[must_use]
pub fn history(project: &str) -> Option<session::sync::loro::LoroDoc> {
    song_where(|s| s.project == project).map(|(s, _, _)| s.doc.loro().clone())
}

/// Every song open in the engine: its project, where its media is, the
/// name every machine knows it by, and where it was opened from.
///
/// The name, not the project guid: the engine mints a guid per open, so
/// two copies of one song never share it. The file's stem is the same on
/// every machine (`Washed`); once songs live in the Task library this is
/// the library's song id.
async fn open_songs()
-> eyre::Result<Vec<(daw_control::Project, MediaRoot, String, std::path::PathBuf)>> {
    let daw = daw::rpc::Daw::try_get().ok_or_else(|| eyre::eyre!("the daw facade is not up"))?;
    let mut out = Vec::new();
    for project in daw.projects().await? {
        let path = std::path::PathBuf::from(project.info().await?.path);
        let folder = path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let key = path.file_stem().map_or_else(
            || project.guid().to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        out.push((project, MediaRoot(folder), key, path));
    }
    Ok(out)
}

#[cfg(feature = "native")]
/// The name the set is known by: the setlist file's stem, or — a lone
/// song — that song's.
fn set_key(songs: &[(daw_control::Project, MediaRoot, String, std::path::PathBuf)]) -> String {
    std::env::var_os("FTS_SESSION_SETLIST")
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .or_else(|| songs.first().map(|s| s.2.clone()))
        .unwrap_or_else(|| "session".into())
}

/// The song's chart: the one beside it (natively), the one the page
/// opened it with ([`remember_chart`]), or what its doc already holds. A
/// page's seed: an engine's song keeps its chart in its file's doc.
#[cfg_attr(feature = "native", allow(dead_code))]
fn chart_for(path: &std::path::Path, doc: &SessionDoc) -> String {
    #[cfg(feature = "native")]
    let beside = crate::prepare::chart_beside(path).and_then(|p| std::fs::read_to_string(p).ok());
    #[cfg(not(feature = "native"))]
    let beside: Option<String> = None;
    beside
        .or_else(|| {
            CHARTS
                .lock()
                .ok()
                .and_then(|c| c.as_ref()?.get(path).cloned())
        })
        .unwrap_or_else(|| doc.read().chart)
}

/// Charts a song was opened with where there is no folder to find one
/// beside it (a page, from a share link): what it seeds its doc with.
static CHARTS: Mutex<Option<HashMap<std::path::PathBuf, String>>> = Mutex::new(None);

/// The chart `path`'s song was opened with (a page's song, from its files).
pub fn remember_chart(path: &std::path::Path, chart: &str) {
    if let Ok(mut charts) = CHARTS.lock() {
        charts
            .get_or_insert_with(HashMap::new)
            .insert(path.to_path_buf(), chart.to_owned());
    }
}

/// Stop whatever is keeping the set, handing back its songs' docs.
fn stop(live: Option<Live>) -> HashMap<String, SessionDoc> {
    let Some(live) = live else {
        return HashMap::new();
    };
    let _ = live.stop.send(true);
    if let Ok(mut opened) = OPENED.lock() {
        *opened = None;
    }
    live.songs.into_iter().map(|s| (s.key, s.doc)).collect()
}

/// A chart the others changed since the editor last looked, for the song
/// on screen.
#[must_use]
pub fn take_remote_chart() -> Option<String> {
    let project = crate::open::current_song()?;
    let live = LIVE.lock().ok()?;
    live.as_ref()?.remote_chart.lock().ok()?.remove(&project)
}

/// The song on screen's doc and the presence, for the chart editor's
/// carets.
#[must_use]
pub fn chart_context() -> Option<(SessionDoc, Arc<dyn PresenceSink>, String)> {
    current().map(|(s, p, me)| (s.doc, p, me))
}

/// The chart was edited here (the song on screen). Locked, it is kept
/// here until the lock lifts.
pub fn local_chart(text: &str) {
    let Some((song, _, _)) = current() else {
        return;
    };
    if sync_locked() && shared() {
        if let Ok(mut held) = HELD_CHARTS.lock() {
            held.get_or_insert_with(HashMap::new)
                .insert(song.project.clone(), text.to_owned());
        }
        return;
    }
    write_chart(&song, text);
}

/// Whether the session is shared with anyone (not only this machine's
/// record of its songs).
fn shared() -> bool {
    LIVE.lock()
        .ok()
        .is_some_and(|l| l.as_ref().is_some_and(|l| l.shared))
}

/// Record `text` as `song`'s chart.
fn write_chart(song: &Song, text: &str) {
    // The bridge owns doc writes; a chart edit is recorded straight into
    // the text, which the next local reconcile leaves alone.
    let mut model = song.doc.read();
    if model.chart != text {
        model.chart = text.to_string();
        if let Err(e) = song.doc.write(&model, ORIGIN_LOCAL) {
            tracing::warn!(collab.chart_error = %e, "collab: the chart edit was not recorded");
        }
    }
}

/// Switch between everyone playing on their own and one transport — for
/// everyone, from where this engine is.
pub fn set_shared_transport(shared: bool) {
    let Ok(mut live) = LIVE.lock() else { return };
    let Some(l) = live.as_mut() else { return };
    let mode = if shared {
        TransportMode::Shared
    } else {
        TransportMode::Independent
    };
    let local = local_transport(&l.songs);
    let Ok(mut sync) = l.sync.lock() else { return };
    let entry = sync.set_mode(mode, &local, crate::presence::now_ms());
    l.status.shared_transport = shared;
    l.presence.set(transport::KEY, entry.encode());
}

/// Someone pressed something on this transport — play, stop, a seek, home,
/// end, another song. Playing together, it becomes everyone's once it has
/// landed.
pub fn transport_pressed() {
    let Ok(live) = LIVE.lock() else { return };
    if let Some(l) = live.as_ref()
        && let Ok(mut sync) = l.sync.lock()
    {
        sync.pressed(crate::presence::now_ms());
    }
}

/// Whether this window is following someone else's transport (playing
/// together, and not the one who pressed last). A follower does nothing
/// to its transport on its own — not even rolling into the next song at
/// the end of one: the leader does, and the follower follows.
#[must_use]
pub fn following() -> bool {
    let Ok(live) = LIVE.lock() else { return false };
    let Some(l) = live.as_ref() else { return false };
    let Ok(sync) = l.sync.lock() else {
        return false;
    };
    sync.mode() == TransportMode::Shared && sync.current().is_some_and(|c| c.by != l.me)
}

/// The song the shared transport wants this window on (its project here),
/// once — for the setlist to open.
static SONG_REQUEST: Mutex<Option<String>> = Mutex::new(None);

/// The song the shared transport asked for, if it asked since last time.
#[must_use]
pub fn take_song_request() -> Option<String> {
    SONG_REQUEST.lock().ok()?.take()
}

/// What this engine is doing: playing, where, on which song (by name).
fn local_transport(songs: &[Song]) -> LocalTransport {
    let (position, playing) = crate::engine::Transport::shared().map_or((0.0, false), |t| t.read());
    let song = crate::open::current_song()
        .and_then(|p| songs.iter().find(|s| s.project == p).map(|s| s.key.clone()));
    LocalTransport {
        playing,
        position,
        song,
    }
}

/// Leave the session (or stop hosting it). The songs keep their docs —
/// everything edited together stays in their history (an engine's song in
/// its file) — shared with nobody.
pub fn leave() {
    if let Ok(mut live) = LIVE.lock()
        && live.as_ref().is_some_and(|l| l.shared)
    {
        drop(stop(live.take()));
    }
    crate::presence::publish(None, 0.0, false);
    crate::presence::publish_everyone(Vec::new());
}

fn next_seq() -> u64 {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let base = u64::try_from(
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis()),
    )
    .unwrap_or(0);
    base.saturating_mul(1000)
        .saturating_add(SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 1000)
}

#[cfg(feature = "native")]
/// An iroh identity for one session. Ephemeral: a ticket is minted per
/// session, so nothing needs to recognise this peer later — and two app
/// instances on one machine (the local demo) must not share one.
fn secret_key() -> architect::iroh_link::iroh::SecretKey {
    architect::iroh_link::iroh::SecretKey::generate()
}

#[cfg(feature = "native")]
/// Share the open set. Returns the ticket to hand others.
///
/// # Errors
/// When nothing is open, the engine cannot be read, or iroh cannot bind.
pub fn host(name: String) -> eyre::Result<String> {
    record(host_inner(name))
}

#[cfg(feature = "native")]
fn host_inner(name: String) -> eyre::Result<String> {
    let runtime = crate::open::runtime().ok_or_else(|| eyre::eyre!("the engine is not up"))?;
    let (ticket, live) = runtime.block_on(async move {
        let open = open_songs().await?;
        let set = set_key(&open);
        let id = session_id(&set);
        if let Ok(mut live) = LIVE.lock() {
            drop(stop(live.take()));
        }
        // The songs' own docs, history and all, are what go on the network:
        // served as they are, each song keeping its file's one bridge.
        let host = SetHost::new(id);
        let mut bridges = Vec::new();
        for (project, _, key, path) in open {
            let guid = project.guid().to_owned();
            let own = song_side(guid.clone(), path).await?;
            let doc = SessionDoc::from_loro(own.doc);
            host.add(song_id(&set, &key), &doc);
            bridges.push((
                Song {
                    project: guid,
                    key,
                    doc,
                },
                Link::Kept,
            ));
        }
        let endpoint = architect::iroh_link::bind_endpoint(secret_key())
            .await
            .map_err(|e| eyre::eyre!("iroh: {e}"))?;
        let ticket = format!("fts-session:{}/{id}", endpoint.id());
        let router = host.mount(architect::LayerRouter::new());
        let serving = endpoint.clone();
        tokio::spawn(async move { architect::iroh_link::serve_router(&serving, router).await });

        let me = format!("host-{}", &endpoint.id().to_string()[..8]);
        let presence: Arc<dyn PresenceSink> = Arc::new(host.own_presence().await?);
        let started = start(
            bridges,
            Arc::clone(&presence),
            SharedClock::owned(),
            set,
            me,
            name,
            Some(Box::new((host, endpoint))),
            None,
        );
        Ok::<_, eyre::Report>((ticket.clone(), started.into_live(ticket, true, presence)))
    })?;
    if let Ok(mut slot) = LIVE.lock() {
        *slot = Some(live);
    }
    tracing::info!(collab.role = "host", "collab: sharing the set");
    Ok(ticket)
}

#[cfg(feature = "native")]
/// Join a session from its ticket. The same setlist must be open here.
///
/// # Errors
/// On a malformed ticket, a different set open, or a failed dial.
pub fn join(ticket: &str, name: String) -> eyre::Result<()> {
    record(join_inner(ticket, name))
}

#[cfg(feature = "native")]
fn join_inner(ticket: &str, name: String) -> eyre::Result<()> {
    let runtime = crate::open::runtime().ok_or_else(|| eyre::eyre!("the engine is not up"))?;
    let (endpoint_id, id) = parse_ticket(ticket)?;
    let live = runtime.block_on(async move {
        let open = open_songs().await?;
        let set = set_key(&open);
        if session_id(&set) != id {
            eyre::bail!("open the same setlist first — this ticket is for another one");
        }
        let endpoint = architect::iroh_link::bind_endpoint(secret_key())
            .await
            .map_err(|e| eyre::eyre!("iroh: {e}"))?;
        let dial = async |endpoint: &architect::iroh_link::iroh::Endpoint| {
            architect::iroh_link::connect(endpoint, endpoint_id)
                .await
                .map_err(|e| eyre::eyre!("iroh connect: {e}"))
        };
        let sync = vox_core::initiator_on(dial(&endpoint).await?)
            .establish::<crdt::sync::DocSyncClient>()
            .await
            .map_err(|e| eyre::eyre!("session sync handshake: {e:?}"))?;
        let presence_client = vox_core::initiator_on(dial(&endpoint).await?)
            .establish::<crdt::sync::DocPresenceClient>()
            .await
            .map_err(|e| eyre::eyre!("session presence handshake: {e:?}"))?;
        // The host's clock is the session's: pinged from here on.
        let clock = SharedClock::follow(
            vox_core::initiator_on(dial(&endpoint).await?)
                .establish::<session::sync::clock::SessionClockClient>()
                .await
                .map_err(|e| eyre::eyre!("session clock handshake: {e:?}"))?,
        );

        let mut peer = SetPeer::new(id);
        peer.run_presence(dial_fixed(presence_client));
        let presence: Arc<dyn PresenceSink> = Arc::new(peer.presence().clone());
        // Every song, replicated from the host.
        let replicas: Vec<((SessionDoc, Side), _)> = open
            .into_iter()
            .map(|song| {
                (
                    SetPeer::sync_song_relayed(song_id(&set, &song.2), dial_fixed(sync.clone())),
                    song,
                )
            })
            .collect();
        // The host's songs arrive first; only then may each bridge make
        // its project match. A song the host does not have keeps this
        // machine's own record, unshared.
        for _ in 0..400 {
            if replicas.iter().all(|((doc, _), _)| doc.has_session()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        if let Ok(mut live) = LIVE.lock() {
            drop(stop(live.take()));
        }
        let mut bridges = Vec::new();
        for ((doc, side), song) in replicas {
            let meet = if doc.has_session() {
                Meeting::Join(doc)
            } else {
                Meeting::Own
            };
            let (song, link, _) = link_song(meet, song, Some(side), &mut HashMap::new()).await?;
            bridges.push((song, link));
        }
        crate::song_model::request_resync();
        let me = format!("peer-{}", &endpoint.id().to_string()[..8]);
        let started = start(
            bridges,
            Arc::clone(&presence),
            clock,
            set,
            me,
            name,
            Some(Box::new(endpoint)),
            None,
        );
        let _keep = peer;
        Ok::<_, eyre::Report>(started.into_live(ticket.to_string(), false, presence))
    })?;
    if let Ok(mut slot) = LIVE.lock() {
        *slot = Some(live);
    }
    tracing::info!(collab.role = "peer", "collab: joined a session");
    Ok(())
}

#[cfg(feature = "native")]
fn parse_ticket(ticket: &str) -> eyre::Result<(architect::iroh_link::iroh::EndpointId, Uuid)> {
    let rest = ticket
        .trim()
        .strip_prefix("fts-session:")
        .ok_or_else(|| eyre::eyre!("not a session ticket"))?;
    let (endpoint, id) = rest
        .split_once('/')
        .ok_or_else(|| eyre::eyre!("not a session ticket"))?;
    Ok((
        endpoint
            .parse()
            .map_err(|e| eyre::eyre!("endpoint id: {e}"))?,
        id.parse().map_err(|e| eyre::eyre!("session id: {e}"))?,
    ))
}

/// A set Task keeps (`live-proto`): where to reach Task, and which
/// setlist — a member's org lane with its token, or a live share link's
/// guest lane with none.
#[derive(Clone, Debug)]
pub struct TaskSet {
    /// The vox URL (`wss://…/org/<org>/vox`, or `…/org/<org>/share/<token>/vox`).
    pub url: String,
    /// The member's bearer token; `None` on a share link.
    pub token: Option<String>,
    /// The setlist's id in Task.
    pub setlist: String,
}

/// How the environment names a set Task keeps (`FTS_COLLAB_TASK`): a
/// setlist's id, joined as a member of the library's org (`FTS_TASK_*`), or
/// `share:<live share link>`, joined as a guest of the link's set.
pub const TASK_SET_ENV: &str = "FTS_COLLAB_TASK";

impl TaskSet {
    /// Joined through a live share link (`…/share/<token>/vox`): a guest,
    /// who plays the set but never seeds it ([`may_seed`]).
    #[must_use]
    pub fn is_guest(&self) -> bool {
        self.url.contains("/share/")
    }

    /// The set `FTS_COLLAB_TASK` names, if it names one.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let value = std::env::var(TASK_SET_ENV)
            .ok()
            .filter(|v| !v.trim().is_empty())?;
        Some(Self::parse(value.trim()))
    }

    /// A setlist id, or `share:<link>`.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value.strip_prefix("share:") {
            Some(link) => {
                let link = link.trim_end_matches('/');
                let base = link.split('?').next().unwrap_or(link);
                let url = base
                    .replacen("https://", "wss://", 1)
                    .replacen("http://", "ws://", 1);
                Self {
                    url: format!("{url}/vox"),
                    token: None,
                    setlist: String::new(),
                }
            }
            #[cfg(feature = "native")]
            None => {
                let library = session::library::Library::from_env();
                Self {
                    url: library.org_url(),
                    token: library.token,
                    setlist: value.to_owned(),
                }
            }
            // A page joins by link: a member's lane is the app's.
            #[cfg(not(feature = "native"))]
            None => Self {
                url: String::new(),
                token: None,
                setlist: value.to_owned(),
            },
        }
    }
}

/// [`join_task`] off the calling thread.
pub fn join_task_in_background(set: TaskSet, name: String) {
    std::thread::spawn(move || {
        let _ = join_task(&set, name);
    });
}

/// Join a set Task keeps: its songs' docs synced through Task, its
/// presence, Task's clock. Each open song meets the set's song of the same
/// library id; the first peer on a song seeds its doc from what it has open
/// (Task starts every song empty), and everyone after takes the doc's
/// state. A song the set does not have stays this machine's own.
///
/// # Errors
///
/// Task could not be reached or refused the set, or an engine call failed.
pub fn join_task(set: &TaskSet, name: String) -> eyre::Result<()> {
    record(join_task_inner(set, name))
}

#[cfg(feature = "native")]
fn join_task_inner(set: &TaskSet, name: String) -> eyre::Result<()> {
    let runtime = crate::open::runtime().ok_or_else(|| eyre::eyre!("the engine is not up"))?;
    let live = runtime.block_on(join_task_live(set.clone(), name))?;
    if let Ok(mut slot) = LIVE.lock() {
        *slot = Some(live);
    }
    Ok(())
}

/// The songs open in this engine, by the name every machine knows each by
/// (`Washed`), in the engine's order.
///
/// # Errors
/// When the engine is not up or cannot be read.
#[cfg(feature = "native")]
pub fn open_song_keys() -> eyre::Result<Vec<String>> {
    let runtime = crate::open::runtime().ok_or_else(|| eyre::eyre!("the engine is not up"))?;
    runtime.block_on(async {
        Ok::<_, eyre::Report>(open_songs().await?.into_iter().map(|s| s.2).collect())
    })
}

/// The name the open set is known by (the setlist's, or a lone song's):
/// what its live set in Task is called.
///
/// # Errors
/// When the engine is not up or cannot be read.
#[cfg(feature = "native")]
pub fn open_set_title() -> eyre::Result<String> {
    let runtime = crate::open::runtime().ok_or_else(|| eyre::eyre!("the engine is not up"))?;
    runtime.block_on(async { Ok::<_, eyre::Report>(set_key(&open_songs().await?)) })
}

/// Run `work` on the engine's runtime, to its end — for what a thread of
/// its own must do against Task (the auto-host's library calls).
///
/// # Errors
/// When the engine is not up, or `work` fails.
#[cfg(feature = "native")]
pub fn on_runtime<T>(work: impl std::future::Future<Output = eyre::Result<T>>) -> eyre::Result<T> {
    let runtime = crate::open::runtime().ok_or_else(|| eyre::eyre!("the engine is not up"))?;
    runtime.block_on(work)
}

/// Whether a session is being joined right now — a page joins in the
/// background, and the session bar says so rather than offering to share.
static JOINING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether a session is being joined (see [`JOINING`]).
#[must_use]
pub fn joining() -> bool {
    JOINING.load(std::sync::atomic::Ordering::Relaxed)
}

/// [`join_task`] in a page: joined on the page's event loop.
#[cfg(not(feature = "native"))]
fn join_task_inner(set: &TaskSet, name: String) -> eyre::Result<()> {
    let set = set.clone();
    JOINING.store(true, std::sync::atomic::Ordering::Relaxed);
    architect::platform::spawn(async move {
        let joined = record(join_task_live(set, name).await);
        JOINING.store(false, std::sync::atomic::Ordering::Relaxed);
        match joined {
            Ok(live) => {
                if let Ok(mut slot) = LIVE.lock() {
                    *slot = Some(live);
                }
            }
            Err(e) => {
                tracing::warn!(collab.error = %e, "collab: could not join the set Task keeps")
            }
        }
    });
    Ok(())
}

/// Joining a set Task keeps — the part both a desktop and a page run.
async fn join_task_live(set: TaskSet, name: String) -> eyre::Result<Live> {
    use live_proto::LiveSessionsClient;
    let open = open_songs().await?;
    let token = set.token.as_deref();
    let lane: LiveSessionsClient = task_dial::establish_at(&set.url, token)
        .await
        .map_err(|e| eyre::eyre!("dialling {}: {e}", set.url))?;
    let joined = lane
        .join(set.setlist.clone())
        .await
        .map_err(|e| eyre::eyre!("joining the set: {e:?}"))?;
    // A playground's end, from Task's clock onto this machine's.
    let resets_at = match joined.resets_at {
        Some(at) => lane.now().await.ok().map(|now| {
            let left = std::time::Duration::from_secs_f64(((at - now) / 1e6).max(0.0));
            architect::platform::Instant::now() + left
        }),
        None => None,
    };
    // Each replica and the presence dial Task again whenever their
    // connection goes (Task restarting, a network blip) and catch up.
    let sync: Dial<crdt::sync::DocSyncClient> = task_dial_for(&set);
    let presence_dial: Dial<crdt::sync::DocPresenceClient> = task_dial_for(&set);
    // Task's clock is the session's.
    let clock = task_clock(&set, lane);
    let presence_id: Uuid = joined
        .presence_id
        .parse()
        .map_err(|e| eyre::eyre!("presence id: {e}"))?;
    let mut peer = SetPeer::new(presence_id);
    peer.run_presence(presence_dial);
    let presence: Arc<dyn PresenceSink> = Arc::new(peer.presence().clone());

    // Each open song, replicated from the set's song of its id.
    let ids: Arc<HashMap<String, Uuid>> = Arc::new(
        joined
            .songs
            .iter()
            .filter_map(|s| Some((s.slug.clone(), s.doc_id.parse().ok()?)))
            .collect(),
    );
    let replicas: Vec<_> = open
        .into_iter()
        .map(|song| (replicate(&ids, &sync, &song.2), song))
        .collect();
    settle(
        replicas
            .iter()
            .filter_map(|(replica, _)| replica.as_ref().map(|(doc, _)| doc)),
    )
    .await;
    let mut docs = LIVE
        .lock()
        .ok()
        .map(|mut l| stop(l.take()))
        .unwrap_or_default();
    let seeds = may_seed(&set, joined.read_only);
    let mut bridges = Vec::new();
    let mut waiting = Vec::new();
    let mut seeded = 0usize;
    for (replica, song) in replicas {
        let (doc, side) = replica.unzip();
        match meeting(doc, seeds) {
            Meeting::Wait(doc) => waiting.push((doc, side, song)),
            meet => {
                let (song, link, was_seeded) =
                    sliced(link_song(meet, song, side, &mut docs)).await?;
                seeded += usize::from(was_seeded);
                bridges.push((song, link));
            }
        }
    }
    crate::song_model::request_resync();
    let me = format!("peer-{}", &Uuid::new_v4().simple().to_string()[..8]);
    tracing::info!(
        collab.setlist = %joined.setlist,
        collab.epoch = joined.epoch,
        collab.songs = bridges.len(),
        collab.seeded = seeded,
        collab.waiting = waiting.len(),
        collab.guest = set.is_guest(),
        "collab: joined a set Task keeps"
    );
    // Songs opened after this (a page opens its first song, then the
    // rest): the same replica and bridge, handed to the running session.
    let (adds_tx, adds_rx) = tokio::sync::mpsc::unbounded_channel();
    let (opened_tx, mut opened_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let mut bridged: std::collections::HashSet<String> = bridges
        .iter()
        .map(|(song, _)| song.project.clone())
        .chain(waiting.iter().map(|(_, _, song)| song.0.guid().to_owned()))
        .collect();
    for (doc, side, song) in waiting {
        bridge_when_seeded(doc, side, song, adds_tx.clone());
    }
    architect::platform::spawn(async move {
        while let Some(project) = opened_rx.recv().await {
            if !bridged.insert(project.clone()) {
                continue;
            }
            let Ok(open) = open_songs().await else {
                continue;
            };
            let Some(song) = open.into_iter().find(|s| s.0.guid() == project) else {
                continue;
            };
            let (doc, side) = replicate(&ids, &sync, &song.2).unzip();
            settle(doc.iter()).await;
            let meet = match meeting(doc, seeds) {
                Meeting::Wait(doc) => {
                    bridge_when_seeded(doc, side, song, adds_tx.clone());
                    continue;
                }
                meet => meet,
            };
            match sliced(link_song(meet, song, side, &mut HashMap::new())).await {
                Ok((song, link, _)) => {
                    if adds_tx.send((song, link)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(collab.error = %e, "collab: a song opened later could not join the session")
                }
            }
        }
    });
    if let Ok(mut slot) = OPENED.lock() {
        // Songs that opened while this was joining — after `open_songs`
        // looked, or before: the adder skips the ones already bridged.
        for project in EARLY
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default()
        {
            let _ = opened_tx.send(project);
        }
        *slot = Some(opened_tx);
    }
    let started = start(
        bridges,
        Arc::clone(&presence),
        clock,
        joined.title.clone(),
        me,
        name,
        None,
        Some(adds_rx),
    );
    let _keep = peer;
    let mut live = started.into_live(set.url.clone(), false, presence);
    live.status.resets_at = resets_at;
    Ok::<_, eyre::Report>(live)
}

/// Where a song opened after the session began is announced (see
/// [`song_opened`]); set while in a set Task keeps.
static OPENED: Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>> = Mutex::new(None);

/// Songs that opened before a session could take them (it was still
/// joining): handed to it once it has.
static EARLY: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// `project` has just opened in this engine: in a set Task keeps, it joins
/// the session like the songs that were open when it began — or, while
/// the session is still joining, as soon as it has.
pub fn song_opened(project: &str) {
    let Ok(slot) = OPENED.lock() else { return };
    match slot.as_ref() {
        Some(opened) => {
            let _ = opened.send(project.to_owned());
        }
        None => {
            if let Ok(mut early) = EARLY.lock() {
                early.push(project.to_owned());
            }
        }
    }
}

/// A song as `open_songs` finds it.
type OpenSong = (daw_control::Project, MediaRoot, String, std::path::PathBuf);

// The same trait: the desktop reaches vox through vox-core, a page
// through the vox facade.
#[cfg(not(feature = "native"))]
use vox::FromVoxLane;
#[cfg(feature = "native")]
use vox_core::FromVoxLane;

/// A dial for `C` on the Task server `set` is on, for a replica to
/// reconnect with.
fn task_dial_for<C>(set: &TaskSet) -> Dial<C>
where
    C: FromVoxLane + 'static,
{
    let (url, token) = (set.url.clone(), set.token.clone());
    Arc::new(move || {
        let (url, token) = (url.clone(), token.clone());
        Box::pin(async move {
            task_dial::establish_at::<C>(&url, token.as_deref())
                .await
                .map_err(|e| eyre::eyre!("dialling {url}: {e}"))
        })
    })
}

/// Task's clock, followed — through `lane` while it answers, and a lane
/// dialled again when it stops (Task restarting).
fn task_clock(set: &TaskSet, lane: live_proto::LiveSessionsClient) -> SharedClock {
    let lane = Arc::new(Mutex::new(Some(lane)));
    let dial: Dial<live_proto::LiveSessionsClient> = task_dial_for(set);
    SharedClock::follow_with(move || {
        let (lane, dial) = (Arc::clone(&lane), Arc::clone(&dial));
        async move {
            let current = lane.lock().ok().and_then(|l| l.clone());
            if let Some(client) = current {
                if let Ok(now) = client.now().await {
                    return Some(now);
                }
            }
            // Gone: dial again for the next ping.
            let fresh = dial().await.ok();
            if let Ok(mut slot) = lane.lock() {
                *slot = fresh;
            }
            None
        }
    })
}

/// A replica of the set's doc for the song named `key`, syncing, and it as
/// a relay's side — `None` for a song the set does not have.
fn replicate(
    ids: &HashMap<String, Uuid>,
    sync: &Dial<crdt::sync::DocSyncClient>,
    key: &str,
) -> Option<(SessionDoc, Side)> {
    ids.get(&session::sync::slug::slugify(key))
        .map(|id| SetPeer::sync_song_relayed(*id, Arc::clone(sync)))
}

/// Wait for what Task has of these replicas to arrive — at once, when it
/// has any; a replica still empty after that is this peer's to seed.
async fn settle<'a>(docs: impl Iterator<Item = &'a SessionDoc> + Clone) {
    for _ in 0..80 {
        if docs.clone().all(SessionDoc::has_session) {
            break;
        }
        architect::platform::sleep(Duration::from_millis(25)).await;
    }
}

/// Whether this peer may seed a song's doc the set has not got yet: a
/// member who can write may; a guest (a live share link) or an observer
/// (`read_only`) never — a guest's song waits for the host's seed.
#[must_use]
pub fn may_seed(set: &TaskSet, read_only: bool) -> bool {
    !set.is_guest() && !read_only
}

/// How one open song meets the set's replica of it.
enum Meeting {
    /// The set holds it: take the doc's state.
    Join(SessionDoc),
    /// The set has none yet and this peer seeds it from its song.
    Seed(SessionDoc),
    /// The set has none yet and this peer may not seed it: joined once
    /// the doc holds the session ([`bridge_when_seeded`]).
    Wait(SessionDoc),
    /// The set does not have this song: it stays this machine's own.
    Own,
}

/// How a song with replica `doc` meets the set, for a peer that may or
/// may not seed ([`may_seed`]).
fn meeting(doc: Option<SessionDoc>, may_seed: bool) -> Meeting {
    match doc {
        Some(doc) if doc.has_session() => Meeting::Join(doc),
        Some(doc) if may_seed => Meeting::Seed(doc),
        Some(doc) => Meeting::Wait(doc),
        None => Meeting::Own,
    }
}

/// How often a song waiting for the host's seed looks again.
const SEED_POLL: Duration = Duration::from_millis(250);

/// Join `song` to its replica once the doc holds the session (another
/// peer seeded it), and hand it to the running session through `adds`.
/// Until then the song is this machine's alone and nothing is written.
fn bridge_when_seeded(
    doc: SessionDoc,
    side: Option<Side>,
    song: OpenSong,
    adds: tokio::sync::mpsc::UnboundedSender<(Song, Link)>,
) {
    architect::platform::spawn(async move {
        while !doc.has_session() {
            if adds.is_closed() {
                return;
            }
            architect::platform::sleep(SEED_POLL).await;
        }
        match sliced(link_song(
            Meeting::Join(doc),
            song,
            side,
            &mut HashMap::new(),
        ))
        .await
        {
            Ok((song, link, _)) => {
                let _ = adds.send((song, link));
            }
            Err(e) => {
                tracing::warn!(collab.error = %e, "collab: a song seeded by the host could not join the session");
            }
        }
    });
}

/// Keep one song with the set's replica of it (`side`, the replica as a
/// relay's side) as `meet` says — an engine's through its project file's
/// one doc ([`relay_song`]), a page's through its one bridge
/// ([`bridge_song`]). Says whether it seeded.
#[cfg_attr(not(feature = "native"), allow(clippy::unused_async))]
async fn link_song(
    meet: Meeting,
    song: OpenSong,
    side: Option<Side>,
    docs: &mut HashMap<String, SessionDoc>,
) -> eyre::Result<(Song, Link, bool)> {
    #[cfg(feature = "native")]
    {
        let _ = docs;
        relay_song(meet, song, side).await
    }
    #[cfg(not(feature = "native"))]
    {
        let _ = side;
        let (song, bridge, seeded) = bridge_song(meet, song, docs).await?;
        Ok((song, Link::Bridged(bridge), seeded))
    }
}

/// The song `guid`'s one doc, as a relay's side: its project file's,
/// begun on its file first when it has none (`project_file::live_side`).
#[cfg(feature = "native")]
async fn song_side(guid: String, path: std::path::PathBuf) -> eyre::Result<Side> {
    tokio::task::spawn_blocking(move || {
        let daw = crate::open::local_engine()
            .ok_or_else(|| eyre::eyre!("the engine is not in this process"))?;
        crate::project_file::live_side(daw, &guid, &path).map_err(|e| eyre::eyre!(e))
    })
    .await
    .map_err(|e| eyre::eyre!("the song's doc did not open: {e}"))?
}

/// An engine's song kept with the set's replica of it (`live`) as `meet`
/// says, through its file's one doc and bridge — never a second bridge:
///
/// - **Seed**: the set has no doc of it yet; it is seeded from the song's
///   own, and kept in step with it.
/// - **Join**: the set's doc is of the song's own lineage (seeded from
///   it, or from a copy of its file): kept in step with it. Of another
///   lineage (another peer's copy seeded it): the song is moved onto it
///   (`project_file::adopt`), then kept in step.
/// - **Own**: the set does not have it; nothing is kept.
#[cfg(feature = "native")]
async fn relay_song(
    meet: Meeting,
    (project, _, key, path): OpenSong,
    live: Option<Side>,
) -> eyre::Result<(Song, Link, bool)> {
    use session::sync::relay;
    let guid = project.guid().to_owned();
    let (doc, link, seeded) = match (meet, live) {
        (Meeting::Seed(doc), Some(live)) => {
            let own = song_side(guid.clone(), path).await?;
            (live.take)(relay::seed(&own.doc)?).await?;
            (
                doc,
                Link::Relayed(Relay::start(own, live, LOCK.subscribe())),
                true,
            )
        }
        (Meeting::Join(doc), Some(live)) => {
            let own = song_side(guid.clone(), path).await?;
            let own = if relay::same_lineage(&own.doc, &live.doc) {
                Some(own)
            } else {
                let (moving, from) = (guid.clone(), live.doc.clone());
                let moved =
                    tokio::task::spawn_blocking(move || crate::project_file::adopt(&moving, &from))
                        .await
                        .map_err(|e| eyre::eyre!("{key}: the song did not move: {e}"))?;
                // Refused (a song synced with its org keeps its org's
                // doc): this song stays out of the set; the rest join.
                moved
                    .inspect_err(|why| {
                        tracing::warn!(collab.song = %key, collab.why = %why, "collab: the song stays out of the set");
                    })
                    .ok()
            };
            match own {
                Some(own) => (
                    doc,
                    Link::Relayed(Relay::start(own, live, LOCK.subscribe())),
                    false,
                ),
                None => (doc, Link::Kept, false),
            }
        }
        (Meeting::Wait(_), _) => eyre::bail!("{key}: the set's doc is not seeded yet"),
        (Meeting::Own | Meeting::Seed(_) | Meeting::Join(_), _) => {
            (SessionDoc::new(), Link::Kept, false)
        }
    };
    tracing::info!(
        collab.song = %key,
        collab.seeded = seeded,
        collab.kept = matches!(link, Link::Kept),
        "collab: the song is kept with the set through its own doc"
    );
    Ok((
        Song {
            project: guid,
            key,
            doc,
        },
        link,
        seeded,
    ))
}

/// Bridge one song to its replica as `meet` says: joining the set's
/// session of it, seeding it from this song, or (no replica) keeping this
/// machine's own record from `docs`. Says whether it seeded. A page's: it
/// has no project file, so the set's doc is its song's one doc.
#[cfg(not(feature = "native"))]
async fn bridge_song(
    meet: Meeting,
    (project, media, key, path): OpenSong,
    docs: &mut HashMap<String, SessionDoc>,
) -> eyre::Result<(Song, Bridge, bool)> {
    let guid = project.guid().to_owned();
    let (bridge, seeded) = match meet {
        Meeting::Join(doc) => (join_bridge(project, media, doc).await?, false),
        Meeting::Seed(doc) => {
            let chart = chart_for(&path, &doc);
            (
                Bridge::host(project, media, doc, chart, blobs(), local()).await?,
                true,
            )
        }
        Meeting::Wait(_) => eyre::bail!("{key}: the set's doc is not seeded yet"),
        Meeting::Own => {
            // Not in the set: nothing joins through its build's bridge.
            drop(take_built(&guid));
            let own = docs.remove(&key).unwrap_or_default();
            let chart = chart_for(&path, &own);
            (
                Bridge::host(project, media, own, chart, blobs(), local()).await?,
                false,
            )
        }
    };
    let song = Song {
        project: guid,
        key,
        doc: bridge.doc().clone(),
    };
    Ok((song, bridge, seeded))
}

/// `work`, on a page cut into slices (`web_engine::in_slices`): a song's
/// bridge reads and rebuilds the whole song through the facade, and in
/// one piece froze the page for most of a second per song.
async fn sliced<T>(work: impl std::future::Future<Output = T>) -> T {
    #[cfg(all(feature = "web", not(feature = "native")))]
    let done = crate::web_engine::in_slices(work).await;
    #[cfg(not(all(feature = "web", not(feature = "native"))))]
    let done = work.await;
    done
}

/// Songs handed to a running session ([`start`]), each with its link.
type Adds = tokio::sync::mpsc::UnboundedReceiver<(Song, Link)>;

/// The next song handed to the session, or never when none can come.
async fn next_add(adds: &mut Option<Adds>) -> Option<(Song, Link)> {
    match adds.as_mut() {
        Some(adds) => adds.recv().await,
        None => std::future::pending().await,
    }
}

/// Tell the session's task, by `index`, whenever this song's engine
/// changes — from its own slice of the event bus.
fn follow_engine(
    index: usize,
    project: String,
    local_tx: tokio::sync::mpsc::UnboundedSender<usize>,
) {
    architect::platform::spawn(async move {
        let Some(daw) = daw::rpc::Daw::try_get() else {
            tracing::warn!("collab: no daw facade; this song's edits will not be shared");
            return;
        };
        let filter = daw_proto::event_bus::BusFilter {
            tracks: true,
            items: true,
            takes: true,
            markers: true,
            regions: true,
            tempo_map: true,
            project_guid: Some(project),
            ..daw_proto::event_bus::BusFilter::default()
        };
        let mut stream = match daw.events().subscribe(filter).await {
            Ok(stream) => stream,
            Err(e) => {
                tracing::warn!(collab.error = %e, "collab: this song's edits cannot be followed; none will be shared");
                return;
            }
        };
        while let Ok(Some(_)) = stream.recv().await {
            if local_tx.send(index).is_err() {
                break;
            }
        }
    });
}

/// Tell the session's task, by `index`, whenever the others' edits arrive
/// in this song's doc.
fn follow_doc(
    index: usize,
    song: &Song,
    remote_tx: tokio::sync::mpsc::UnboundedSender<usize>,
) -> session::sync::loro::Subscription {
    song.doc.loro().subscribe_root(Arc::new(move |event| {
        if event.triggered_by == session::sync::loro::EventTriggerKind::Import {
            let _ = remote_tx.send(index);
        }
    }))
}

/// A chart the others changed, among `changes` to `song`: for the chart
/// editor to take when it next looks.
fn note_remote_chart(
    changes: &[Change],
    song: &Song,
    remote_chart: &Mutex<HashMap<String, String>>,
) {
    for change in changes {
        if let Change::ChartChanged(text) = change
            && let Ok(mut slot) = remote_chart.lock()
        {
            slot.insert(song.project.clone(), text.clone());
        }
    }
}

/// The half-built `Live` [`start`] returns; finished by the caller.
struct Started {
    stop: tokio::sync::watch::Sender<bool>,
    sync: Arc<Mutex<TransportSync>>,
    remote_chart: Arc<Mutex<HashMap<String, String>>>,
    songs: Vec<Song>,
    me: String,
    name: String,
    set: String,
}

impl Started {
    fn into_live(self, ticket: String, hosting: bool, presence: Arc<dyn PresenceSink>) -> Live {
        Live {
            status: Status {
                hosting,
                ticket,
                peers: 0,
                shared_transport: false,
                set: self.set,
                color: presence::color_for(&self.me),
                name: self.name,
                resets_at: None,
                locked: false,
                held: 0,
            },
            stop: self.stop,
            sync: self.sync,
            remote_chart: self.remote_chart,
            songs: self.songs,
            presence,
            me: self.me,
            shared: true,
        }
    }
}

/// Whatever a session's task keeps alive with it (a host, an iroh endpoint).
#[cfg(not(target_arch = "wasm32"))]
type Keep = Option<Box<dyn std::any::Any + Send>>;
/// Whatever a session's task keeps alive with it.
#[cfg(target_arch = "wasm32")]
type Keep = Option<Box<dyn std::any::Any>>;

/// Spawn the session's task. `keep` lives as long as it.
#[allow(clippy::too_many_arguments)]
fn start(
    bridges: Vec<(Song, Link)>,
    presence: Arc<dyn PresenceSink>,
    clock: SharedClock,
    set: String,
    me: String,
    name: String,
    keep: Keep,
    adds: Option<Adds>,
) -> Started {
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let sync = Arc::new(Mutex::new(TransportSync::new(me.clone())));
    let remote_chart = Arc::new(Mutex::new(HashMap::new()));
    let songs: Vec<Song> = bridges.iter().map(|(s, _)| s.clone()).collect();
    let started = Started {
        stop,
        sync: Arc::clone(&sync),
        remote_chart: Arc::clone(&remote_chart),
        songs: songs.clone(),
        me: me.clone(),
        name: name.clone(),
        set,
    };
    let mut links: Vec<Link> = bridges.into_iter().map(|(_, l)| l).collect();

    // A page's songs (bridged): each one's engine changes, from its own
    // slice of the event bus, and its remote changes — any import into its
    // doc. An engine's songs are kept by their files' bridges.
    let (local_tx, mut local_rx) = tokio::sync::mpsc::unbounded_channel::<usize>();
    let (remote_tx, mut remote_rx) = tokio::sync::mpsc::unbounded_channel::<usize>();
    let mut subscriptions = Vec::new();
    for (index, (song, link)) in songs.iter().zip(&links).enumerate() {
        if matches!(link, Link::Bridged(_)) {
            follow_engine(index, song.project.clone(), local_tx.clone());
            subscriptions.push(follow_doc(index, song, remote_tx.clone()));
        }
    }
    let mut adds = adds;
    let mut songs = songs;

    architect::platform::spawn(async move {
        let _keep = keep;
        // ~30 Hz, from a deadline: a burst of edits does not starve it.
        let mut next_tick = architect::platform::now();
        let mut out = Outbox::new(me.clone(), name);
        let mut roster = Roster::default();
        let mut seen: HashMap<String, session::sync::loro::LoroValue> = HashMap::default();
        // Sample-accurate following (daw-transport-sync): the leader's
        // stamped playhead, this engine locked to it.
        let mut lock = Lock::default();
        // This device's lock (`session::sync::hold`): only a shared
        // session holds anything. A locked device starts with every song
        // held: its joins left its engine as it was.
        let holds = me != LOCAL_ME;
        let mut lock_rx = LOCK.subscribe();
        let mut hold = Hold::new(holds && *lock_rx.borrow_and_update());
        if hold.locked() {
            for (index, link) in links.iter().enumerate() {
                if matches!(link, Link::Bridged(_)) {
                    hold.remote(index);
                }
            }
        }
        HELD.store(
            held_count(&hold, &links),
            std::sync::atomic::Ordering::Relaxed,
        );
        let key_of = |songs: &[Song], project: Option<String>| {
            project.and_then(|p| songs.iter().find(|s| s.project == p).map(|s| s.key.clone()))
        };
        loop {
            tokio::select! {
                _ = stopped.changed() => break,
                // Locked or unlocked here: unlocking merges what was held,
                // song by song — this engine's edits out, the others' in.
                Ok(()) = lock_rx.changed(), if holds => {
                    let locked = *lock_rx.borrow_and_update();
                    let held = hold.set_locked(locked);
                    let showing = crate::open::current_song();
                    for i in &held.songs {
                        let (Some(Link::Bridged(bridge)), Some(song)) = (links.get_mut(*i), songs.get(*i)) else { continue };
                        match bridge.merge_held("Edits while locked").await {
                            Ok(changes) => {
                                note_remote_chart(&changes, song, &remote_chart);
                                if !changes.is_empty() && showing.as_deref() == Some(song.project.as_str()) {
                                    crate::song_model::request_resync();
                                }
                            }
                            Err(e) => tracing::warn!(collab.error = %e, "collab: what the lock held did not merge"),
                        }
                    }
                    if !held.is_empty() {
                        tracing::info!(
                            collab.songs = held.songs.len(),
                            collab.local = held.local,
                            collab.remote = held.remote,
                            "collab: unlocked; what was held is merged"
                        );
                    }
                    HELD.store(held_count(&hold, &links), std::sync::atomic::Ordering::Relaxed);
                }
                // A song opened after the session began (a page's songs
                // arrive one by one): followed the same way as the rest.
                Some((song, link)) = next_add(&mut adds) => {
                    let index = songs.len();
                    let bridged = matches!(link, Link::Bridged(_));
                    if bridged {
                        follow_engine(index, song.project.clone(), local_tx.clone());
                        subscriptions.push(follow_doc(index, &song, remote_tx.clone()));
                    }
                    if let Ok(mut live) = LIVE.lock()
                        && let Some(l) = live.as_mut()
                    {
                        l.songs.push(song.clone());
                    }
                    tracing::info!(collab.song = %song.key, collab.songs = index + 1, "collab: a song joined the session");
                    if hold.locked() && bridged {
                        // Joined while locked: its engine waits too.
                        hold.remote(index);
                    }
                    songs.push(song);
                    links.push(link);
                    HELD.store(held_count(&hold, &links), std::sync::atomic::Ordering::Relaxed);
                    crate::song_model::request_resync();
                }
                Some(first) = local_rx.recv() => {
                    // Let a burst (a drag, a rebuild) settle into one read
                    // per song it touched.
                    architect::platform::sleep(Duration::from_millis(40)).await;
                    let mut touched = BTreeSet::from([first]);
                    while let Ok(i) = local_rx.try_recv() {
                        touched.insert(i);
                    }
                    for i in touched {
                        // Locked: kept in this engine, not recorded yet.
                        if !hold.local(i) {
                            continue;
                        }
                        if let Some(Link::Bridged(bridge)) = links.get_mut(i)
                            && let Err(e) = bridge.local_changed().await
                        {
                            tracing::warn!(collab.error = %e, "collab: a local edit was not shared");
                        }
                    }
                    HELD.store(held_count(&hold, &links), std::sync::atomic::Ordering::Relaxed);
                }
                Some(first) = remote_rx.recv() => {
                    let mut touched = BTreeSet::from([first]);
                    while let Ok(i) = remote_rx.try_recv() {
                        touched.insert(i);
                    }
                    let showing = crate::open::current_song();
                    for i in touched {
                        // Locked: held in the doc, not made here yet.
                        if !hold.remote(i) {
                            continue;
                        }
                        let (Some(Link::Bridged(bridge)), Some(song)) = (links.get_mut(i), songs.get(i)) else { continue };
                        match bridge.remote_changed().await {
                            Ok(changes) if !changes.is_empty() => {
                                note_remote_chart(&changes, song, &remote_chart);
                                if showing.as_deref() == Some(song.project.as_str()) {
                                    crate::song_model::request_resync();
                                }
                            }
                            Ok(_) => {}
                            Err(e) => tracing::warn!(collab.error = %e, "collab: a remote edit did not apply"),
                        }
                    }
                    HELD.store(held_count(&hold, &links), std::sync::atomic::Ordering::Relaxed);
                }
                () = architect::platform::sleep(next_tick.saturating_duration_since(architect::platform::now())) => {
                    next_tick = architect::platform::now() + TICK;
                    // An engine's relays hold and let go on their own.
                    HELD.store(held_count(&hold, &links), std::sync::atomic::Ordering::Relaxed);
                    let now = crate::presence::now_ms();
                    let here = key_of(&songs, crate::open::current_song());
                    out.publish(presence.as_ref(), now, here.clone());
                    // Everyone else, into the roster.
                    let states = presence.states();
                    for (key, value) in &states {
                        if seen.get(key) != Some(value) {
                            roster.apply(&me, key, Some(value), now);
                        }
                    }
                    for key in seen.keys().filter(|k| !states.contains_key(*k)) {
                        roster.apply(&me, key, None, now);
                    }
                    seen = states;
                    // The tabs show everyone; the arrangement, the people
                    // on the song it shows.
                    // (By this machine's project for the song: the tabs
                    // know their songs by it.)
                    crate::presence::publish_everyone(
                        roster
                            .peers
                            .values()
                            .filter_map(|p| p.state.as_ref())
                            .filter_map(|s| {
                                let song = songs.iter().find(|x| s.song.as_ref() == Some(&x.key))?;
                                Some(crate::presence::Person {
                                    project: song.project.clone(),
                                    song: song.key.clone(),
                                    name: s.name.clone(),
                                    color: s.color,
                                })
                            })
                            .collect(),
                    );
                    let mut on_this_song = roster.clone();
                    on_this_song.peers.retain(|_, p| {
                        p.state.as_ref().is_some_and(|s| s.song.is_none() || s.song == here)
                    });
                    // The shared transport: whoever wrote it last, unless
                    // a press here is newer — then keep this engine on it.
                    let local = local_transport(&songs);
                    let (current, tick) = {
                        let Ok(mut sync) = sync.lock() else { continue };
                        if let Some(entry) = seen.get(transport::KEY).and_then(SharedTransport::decode) {
                            sync.remote(entry);
                        }
                        let tick = sync.tick(&local, now);
                        (sync.mode(), tick)
                    };
                    if let Some(entry) = tick.publish {
                        presence.set(transport::KEY, entry.encode());
                    }
                    let leader = sync.lock().ok().and_then(|s| s.current().map(|c| c.by.clone()));
                    let locked = lock.step(current, leader.as_deref(), &me, here.as_ref(), &seen, &clock, presence.as_ref());
                    for command in tick.commands {
                        // Locked to the leader's stamped playhead, the
                        // drift follower starts, stops and moves this
                        // engine (to the sample); the coarse commands are
                        // only for a song switch, or when it cannot lock.
                        if locked && !matches!(command, Command::SwitchSong(_)) {
                            continue;
                        }
                        obey(command, &songs);
                    }
                    crate::presence::publish(Some(on_this_song), 0.0, current == TransportMode::Independent);
                    if let Ok(mut live) = LIVE.lock()
                        && let Some(l) = live.as_mut()
                    {
                        l.status.peers = roster.peers.len();
                        l.status.shared_transport = current == TransportMode::Shared;
                    }
                }
            }
        }
        presence.delete(&presence::key(&me, presence::STATE));
        presence.delete(&presence::key(&me, presence::POINTER));
        presence.delete(&presence::key(&me, presence::PLAY));
    });
    started
}

/// This engine's lock to the leader of the shared transport.
///
/// Playing together, whoever pressed last leads: its engine plays on its
/// own, and every tick it stamps its playhead in the session's shared
/// clock ([`SyncPosition`] under its `sync` key). Everyone else locks to
/// that with daw-transport-sync's drift follower — a scheduled locate to
/// start or catch a jump, then a rate nudge of a fraction of a percent,
/// resampled, so the engines hold within a few samples and never click.
#[derive(Default)]
struct Lock {
    follower: daw_transport_sync::Follower,
    /// What this peer was doing last tick.
    role: Role,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Role {
    #[default]
    Apart,
    Leading,
    Following,
}

impl Lock {
    /// One tick. Returns whether this engine is locked to a leader (the
    /// coarse transport commands then stand down).
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        mode: TransportMode,
        leader: Option<&str>,
        me: &str,
        here: Option<&String>,
        seen: &HashMap<String, session::sync::loro::LoroValue>,
        clock: &SharedClock,
        presence: &dyn PresenceSink,
    ) -> bool {
        let backend = crate::open::current_song().and_then(|p| sync_backend(&p));
        let role = match (mode, leader) {
            (TransportMode::Shared, Some(l)) if l == me => Role::Leading,
            (TransportMode::Shared, Some(_)) => Role::Following,
            _ => Role::Apart,
        };
        if role != self.role {
            // A new role starts from rate 1 and nothing learned (the
            // mismatch is against a leader that may have changed).
            self.follower.reset();
            if let Some(b) = &backend {
                b.set_rate(1.0);
            }
            if self.role == Role::Leading {
                presence.delete(&transport::sync_key(me));
            }
            self.role = role;
        }
        let (Some(backend), Some(offset)) = (backend, clock.offset_micros()) else {
            return false;
        };
        match role {
            Role::Apart => false,
            Role::Leading => {
                if let Some(snapshot) = backend.snapshot() {
                    let stamped = SyncPosition {
                        song: here.cloned(),
                        position: snapshot.position().shifted(offset),
                    };
                    presence.set(&transport::sync_key(me), stamped.encode());
                }
                false
            }
            Role::Following => {
                let Some(lead) = leader
                    .and_then(|l| seen.get(&transport::sync_key(l)))
                    .and_then(SyncPosition::decode)
                else {
                    return false;
                };
                // Another song: the switch comes first (the coarse path).
                if lead.song.as_ref() != here {
                    return false;
                }
                let now = daw_transport_sync::clock::now_micros_f64();
                let correction = self
                    .follower
                    .tick(backend.as_ref(), &lead.position, offset, now);
                if !matches!(correction, daw_transport_sync::Correction::Hold) {
                    tracing::debug!(
                        sync.correction = ?correction,
                        sync.drift_us = self.follower.controller().last_drift().map(|d| d * 1e6),
                        sync.offset_us = offset,
                        "collab: following the leader"
                    );
                }
                true
            }
        }
    }
}

/// A song's sync backend (daw-transport-sync), by project — made once:
/// making one stands up the project's transport engine.
fn sync_backend(
    project: &str,
) -> Option<Arc<dyn daw_transport_sync::TransportBackend + Send + Sync>> {
    type Backends = HashMap<String, Arc<dyn daw_transport_sync::TransportBackend + Send + Sync>>;
    static BACKENDS: Mutex<Option<Backends>> = Mutex::new(None);
    let mut cache = BACKENDS.lock().ok()?;
    let backends = cache.get_or_insert_with(HashMap::new);
    if let Some(backend) = backends.get(project) {
        return Some(Arc::clone(backend));
    }
    // Remote: None — the per-buffer backend is the local engine's; locking a
    // follower to a remote transport over the facade is its own issue.
    let backend: Arc<dyn daw_transport_sync::TransportBackend + Send + Sync> = Arc::new(
        crate::open::with_local_engine(crate::open::LocalOnly::TransportSync, |daw| {
            daw.sync_backend(project)
        })
        .flatten()?,
    );
    backends.insert(project.to_owned(), Arc::clone(&backend));
    Some(backend)
}

/// Do what the shared transport asks of this engine — quietly: it is not
/// a press here, so it is not published back.
fn obey(command: Command, songs: &[Song]) {
    use crate::engine::{Move, transport_following};
    match command {
        Command::SwitchSong(key) => {
            if let Some(song) = songs.iter().find(|s| s.key == key)
                && let Ok(mut slot) = SONG_REQUEST.lock()
            {
                *slot = Some(song.project.clone());
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

/// How often the session's task publishes and folds presence.
const TICK: Duration = Duration::from_millis(33);

/// How far the playhead may stray from where the others think it is
/// before they are told again, seconds.
const JUMP: f64 = 0.25;

/// What this peer has told the others, so only changes go out.
struct Outbox {
    me: String,
    name: String,
    color: u32,
    state: Option<PeerState>,
    pointer: Throttle<Option<Pointer>>,
    pointer_sent: Option<Option<Pointer>>,
    play: Option<PlayState>,
    puppet_beat: Option<u64>,
}

impl Outbox {
    fn new(me: String, name: String) -> Self {
        let color = presence::color_for(&me);
        Self {
            me,
            name,
            color,
            state: None,
            pointer: Throttle::new(33.0),
            pointer_sent: None,
            play: None,
            puppet_beat: None,
        }
    }

    fn publish(&mut self, sink: &dyn PresenceSink, now: f64, song: Option<String>) {
        if env_set("FTS_COLLAB_PUPPET") {
            self.puppet(sink, now, song);
            return;
        }
        let local = crate::presence::local();
        let edit = crate::edit_state::current();
        let state = PeerState {
            name: self.name.clone(),
            color: self.color,
            song,
            view: "arrangement".into(),
            edit_cursor: edit.map(|e| e.at),
            time_selection: edit.and_then(|e| e.selection).map(|s| (s.start, s.end)),
            selected_tracks: local.selected_tracks,
            selected_items: local.selected_items,
            #[cfg(feature = "native")]
            chart_caret: crate::presence::local_caret(),
            #[cfg(not(feature = "native"))]
            chart_caret: None,
        };
        if self.state.as_ref() != Some(&state) {
            sink.set(&presence::key(&self.me, presence::STATE), state.encode());
            self.state = Some(state);
        }

        let pointer = self
            .pointer
            .offer(local.pointer, now)
            .or_else(|| self.pointer.flush(now));
        if let Some(pointer) = pointer
            && self.pointer_sent.as_ref() != Some(&pointer)
        {
            let key = presence::key(&self.me, presence::POINTER);
            match &pointer {
                Some(pointer) => sink.set(&key, pointer.encode(now)),
                None => sink.delete(&key),
            }
            self.pointer_sent = Some(pointer);
        }

        // The play state goes out when it starts, stops or jumps — not as
        // it moves: the others extrapolate from the last one.
        if let Some(transport) = crate::engine::Transport::shared() {
            let (at, playing) = transport.read();
            let changed = self.play.is_none_or(|last| {
                last.playing != playing || (last.position_at(now) - at).abs() > JUMP
            });
            if changed {
                let play = PlayState {
                    playing,
                    position: at,
                    at_ms: now,
                    rate: 1.0,
                };
                sink.set(&presence::key(&self.me, presence::PLAY), play.encode());
                self.play = Some(play);
            }
        }
    }

    /// A peer that moves on its own: the pointer sweeping across the first
    /// minute of the song over the first few tracks, an edit cursor, a
    /// time selection and an item selected, playing. For the two-window
    /// test (`FTS_COLLAB_PUPPET=1` on one of them) and a demo's "someone
    /// else is here" — never set in normal use.
    #[allow(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )] // a small, positive time
    fn puppet(&mut self, sink: &dyn PresenceSink, now: f64, song: Option<String>) {
        let t = (now / 1000.0) % 8.0;
        let at = 4.0 + t * 7.0;
        let tracks: Vec<String> = crate::presence::local_rows();
        let track = tracks.get((t as usize / 2) % tracks.len().max(1)).cloned();
        if self.state.is_none() {
            let state = PeerState {
                name: self.name.clone(),
                color: self.color,
                song,
                view: "arrangement".into(),
                edit_cursor: Some(24.0),
                time_selection: Some((32.0, 48.0)),
                selected_tracks: tracks.iter().take(1).cloned().collect(),
                selected_items: crate::presence::local_item_guids()
                    .into_iter()
                    .take(2)
                    .collect(),
                chart_caret: None,
            };
            sink.set(&presence::key(&self.me, presence::STATE), state.encode());
            self.state = Some(state);
            let play = PlayState {
                playing: true,
                position: 10.0,
                at_ms: now,
                rate: 1.0,
            };
            sink.set(&presence::key(&self.me, presence::PLAY), play.encode());
        }
        // And an edit, through this engine like any other: every few
        // seconds the drums are muted or unmuted, which the others see
        // arrive through the doc.
        let beat = (now / 3000.0) as u64;
        if self.puppet_beat != Some(beat) {
            // Every fourth beat, everyone plays together from 20 s; the
            // beat after, apart again.
            if self.puppet_beat.is_some() && env_set("FTS_COLLAB_PUPPET_TRANSPORT") {
                let together = beat % 4 == 0;
                let press = SharedTransport {
                    mode: if together {
                        TransportMode::Shared
                    } else {
                        TransportMode::Independent
                    },
                    playing: together,
                    position: 20.0,
                    at_ms: now,
                    song: None,
                    seq: next_seq(),
                    by: self.me.clone(),
                };
                if together || beat % 4 == 1 {
                    sink.set(transport::KEY, press.encode());
                }
            }
            self.puppet_beat = Some(beat);
            // A caret in the chart, a line further down each time, as a
            // stable position the others resolve against their own copy.
            if let (Some(state), Some((doc, _, _))) = (self.state.as_mut(), chart_context()) {
                let chart = doc.read().chart;
                let starts: Vec<usize> = std::iter::once(0)
                    .chain(chart.match_indices('\n').map(|(i, _)| i + 1))
                    .filter(|i| *i < chart.len())
                    .collect();
                if let Some(at) = starts.get((beat as usize) % starts.len().max(1)) {
                    let at = chart[..*at].chars().count();
                    let end = at + 4;
                    state.chart_caret = doc.chart_cursor(at).zip(doc.chart_cursor(end));
                    sink.set(&presence::key(&self.me, presence::STATE), state.encode());
                }
            }
            if let Some(guid) = tracks.iter().find(|g| {
                crate::presence::local_track_name(g).is_some_and(|n| n.starts_with("Drums"))
            }) {
                let guid = guid.clone();
                architect::platform::spawn(async move {
                    let Some(daw) = daw::rpc::Daw::try_get() else {
                        return;
                    };
                    let Ok(project) = daw.current_project().await else {
                        return;
                    };
                    if let Ok(Some(track)) = project.tracks().by_guid(&guid).await {
                        let _ = track.toggle_mute().await;
                    }
                });
            }
        }
        // Three seconds on the lanes, three over the chart's page, three
        // over the lyrics: every kind of place a pointer can be.
        let place = match (now / 3000.0) as u64 % 3 {
            0 => Pointer::Timeline { at, track, y: 0.5 },
            1 => Pointer::Region {
                region: "chart".into(),
                x: 0.2 + 0.6 * (t % 1.0),
                y: 0.3 + 0.05 * t,
            },
            _ => Pointer::Region {
                region: "lyrics".into(),
                x: 0.15 + 0.7 * (t % 1.0),
                y: 0.5,
            },
        };
        if let Some(Some(pointer)) = self.pointer.offer(Some(place), now) {
            sink.set(
                &presence::key(&self.me, presence::POINTER),
                pointer.encode(now),
            );
        }
    }
}

/// Where an env-started host leaves its ticket (`FTS_COLLAB_TICKET`, else
/// the temp dir's `fts-session-ticket`).
#[cfg(feature = "native")]
fn ticket_path() -> std::path::PathBuf {
    std::env::var_os("FTS_COLLAB_TICKET").map_or_else(
        || std::env::temp_dir().join("fts-session-ticket"),
        std::path::PathBuf::from,
    )
}

/// The engine's live sessions, started by itself — in the engine's process
/// (`studio engine`, or a window that is its own engine), never in a
/// window driving an engine elsewhere: a live session keeps each song with
/// its doc through the engine's own files, and a window's would be a second
/// bridge on the engine's songs (#316).
///
/// Once the song is up: the session the environment names, for the
/// two-instance demo and tests — `FTS_COLLAB_TASK` (a set Task keeps),
/// `FTS_COLLAB_HOST=1` (shares the set and writes the ticket to
/// `FTS_COLLAB_TICKET`), `FTS_COLLAB_JOIN` (a ticket, or a file the host
/// writes one to) — then hosting what is open in Task by itself, signed in
/// ([`crate::live_host`]; off when the environment started a session).
/// `name` is who this person is in a set when the account does not say.
#[cfg(feature = "native")]
pub fn start_in_engine(name: String) {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("collab-start".into())
            .spawn(move || {
                std::thread::sleep(Duration::from_millis(1500));
                from_env(&name);
                crate::live_host::start(name);
            });
        if let Err(e) = spawned {
            tracing::warn!(collab.error = %e, "collab: no thread to start the live sessions on");
        }
    });
}

/// The session the environment names, joined or hosted (see
/// [`start_in_engine`]).
#[cfg(feature = "native")]
fn from_env(name: &str) {
    if let Some(set) = TaskSet::from_env() {
        match join_task(&set, name.to_owned()) {
            Ok(()) => {
                tracing::info!(collab.setlist = %set.setlist, "collab: in the set Task keeps")
            }
            Err(e) => {
                tracing::warn!(collab.error = %e, "collab: could not join the set Task keeps");
            }
        }
    } else if env_set("FTS_COLLAB_HOST") {
        match host(name.to_owned()) {
            Ok(ticket) => {
                let path = ticket_path();
                if let Err(e) = std::fs::write(&path, &ticket) {
                    tracing::warn!(collab.ticket_error = %e, "collab: the ticket was not written");
                }
                tracing::info!(collab.ticket_file = %path.display(), "collab: hosting");
            }
            Err(e) => tracing::warn!(collab.error = %e, "collab: could not host"),
        }
    } else if let Some(join) = std::env::var_os("FTS_COLLAB_JOIN").filter(|v| !v.is_empty()) {
        let join = join.to_string_lossy().into_owned();
        let ticket = if join.starts_with("fts-session:") {
            Some(join)
        } else {
            // A file the host writes: wait for it.
            (0..120).find_map(|_| {
                let t = std::fs::read_to_string(&join)
                    .ok()
                    .filter(|t| t.starts_with("fts-session:"));
                if t.is_none() {
                    std::thread::sleep(Duration::from_millis(500));
                }
                t
            })
        };
        match ticket.map(|t| self::join(t.trim(), name.to_owned())) {
            Some(Ok(())) => tracing::info!("collab: joined"),
            Some(Err(e)) => tracing::warn!(collab.error = %e, "collab: could not join"),
            None => tracing::warn!("collab: no ticket to join"),
        }
    }
}

#[cfg(test)]
mod tests;
