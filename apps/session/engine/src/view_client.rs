//! A window's link to its engine: the one way a window reads and drives it.
//!
//! [`connect`] is handed a vox `Caller` that reaches an engine's router —
//! the daw facade and [`crate::view::EngineView`] side by side — over a
//! Unix socket (`studio engine`, on the desktop) or in memory (an engine in
//! the window's process: iOS, a test). Either way the window then:
//!
//! - **keeps replicas** of what it draws, fed by the engine's streams on a
//!   task of their own: the set ([`set_view`], [`subscribe_set`]), the hit
//!   overlay (`crate::hit_overlay`), the tracks' marks ([`marks`]), the
//!   streaming state (`crate::stream_set::adopt_shown`), the playheads of
//!   every song ([`playhead_of`], from `TransportSync::positions`), what
//!   changed (`crate::song_model`'s read-back flags) and what the engine
//!   asks of it (`crate::window_queue::receive`). A frame reads these and
//!   never waits on the engine;
//! - **calls** the engine for what it reads on demand, off its frame: a
//!   song's waveforms ([`waves`]), a stretch's detail ([`detail`]), its
//!   automation ([`automation`]), what changed ([`item_changes`]), a song's
//!   facts ([`song_facts`]), an action ([`run`]), its edits ([`apply`]).
//!
//! A link that drops (an engine that died) is said in [`state`]; whoever
//! made the link makes another and [`connect`]s again — the replicas are
//! kept meanwhile and refreshed from the new link's intros.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use tokio::sync::watch;

use crate::view::{
    ActionCall, Applied, AutomationRead, ChangesAsk, DetailAsk, EditBatch, EngineViewClient,
    EngineViewStreamClient, ItemChanges, MarksFrame, SetFrame, SongFacts, WaveRead, WavesAsk,
};

/// How long a call may take before the window gives up on it. Giving up
/// on one call is that call's failure, not the link's: only the streams
/// ending ([`follow`]) say the engine went away.
const CALL: Duration = Duration::from_secs(20);

/// How long an action may take: some run long on purpose (a quantize
/// waits up to half a minute for the window to land its edits).
const RUN: Duration = Duration::from_secs(120);

/// The link: the engine's view service over one connection.
#[derive(Clone)]
struct Link {
    view: EngineViewClient,
    streams: EngineViewStreamClient,
    daw: daw_control::Daw,
    /// Bumped by every [`connect`]: a stream task of an older link stops.
    generation: u64,
}

static LINK: RwLock<Option<Link>> = RwLock::new(None);
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How the link stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkState {
    /// Never linked.
    None,
    /// Linked, its streams flowing.
    Up,
    /// It dropped: the engine went away (or stopped answering).
    Down,
}

fn state_cell() -> &'static watch::Sender<LinkState> {
    static STATE: OnceLock<watch::Sender<LinkState>> = OnceLock::new();
    STATE.get_or_init(|| watch::Sender::new(LinkState::None))
}

/// How the link stands now.
#[must_use]
pub fn state() -> LinkState {
    *state_cell().borrow()
}

/// How the link stands, as it changes.
#[must_use]
pub fn watch_state() -> watch::Receiver<LinkState> {
    state_cell().subscribe()
}

/// Whether this window is linked to an engine.
#[must_use]
pub fn is_linked() -> bool {
    LINK.read().is_ok_and(|l| l.is_some())
}

fn link() -> Option<Link> {
    LINK.read().ok().and_then(|l| l.clone())
}

/// The runtime a window's link runs on.
fn runtime() -> Option<&'static tokio::runtime::Runtime> {
    crate::open::engine_runtime().ok()
}

/// Link this window to the engine `caller` reaches (its router serves the
/// daw facade and the view service), and follow it: every replica fed
/// from now on. A link made before is let go.
pub fn connect(caller: vox::Caller) {
    let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
    let link = Link {
        view: EngineViewClient::new(caller.clone()),
        streams: EngineViewStreamClient::new(caller.clone()),
        daw: daw_control::Daw::new(caller),
        generation,
    };
    if let Ok(mut slot) = LINK.write() {
        *slot = Some(link.clone());
    }
    state_cell().send_replace(LinkState::Up);
    let Some(rt) = runtime() else {
        tracing::warn!("view link: no runtime to follow the engine on");
        return;
    };
    rt.spawn(follow(link.clone(), Stream::Set));
    rt.spawn(follow(link.clone(), Stream::Overlays));
    rt.spawn(follow(link.clone(), Stream::Marks));
    rt.spawn(follow(link.clone(), Stream::Streams));
    rt.spawn(follow(link.clone(), Stream::Jobs));
    rt.spawn(follow(link.clone(), Stream::Edits));
    rt.spawn(follow(link.clone(), Stream::Changes));
    rt.spawn(follow(link.clone(), Stream::Queued));
    rt.spawn(follow_playheads(link));
}

/// Whether `link` is still the window's.
fn current(link: &Link) -> bool {
    GENERATION.load(std::sync::atomic::Ordering::Acquire) == link.generation
}

/// The window's link dropped: said once (the replicas stay as they were).
fn dropped(link: &Link, why: &str) {
    if !current(link) {
        return;
    }
    if state_cell().send_replace(LinkState::Down) != LinkState::Down {
        tracing::warn!(link.why = why, "view link: the engine went away");
    }
}

#[derive(Clone, Copy, Debug)]
enum Stream {
    Set,
    Overlays,
    Marks,
    Streams,
    Jobs,
    Edits,
    Changes,
    Queued,
}

/// Follow one of the engine's streams into its replica until the link
/// goes.
async fn follow(link: Link, which: Stream) {
    let ended = match which {
        Stream::Set => {
            let (tx, rx) = vox::channel::<SetFrame>();
            drain(link.streams.set(tx), rx, |frame| set_arrived(frame)).await
        }
        Stream::Overlays => {
            let (tx, rx) = vox::channel::<crate::view::OverlayFrame>();
            drain(link.streams.overlays(tx), rx, |frame| {
                crate::hit_overlay::publish(frame.overlay);
                crate::wave_detail::engine_changed();
            })
            .await
        }
        Stream::Marks => {
            let (tx, rx) = vox::channel::<MarksFrame>();
            drain(link.streams.marks(tx), rx, marks_arrived).await
        }
        Stream::Streams => {
            let (tx, rx) = vox::channel::<crate::view::StreamsFrame>();
            drain(link.streams.streams(tx), rx, |frame| {
                #[cfg(feature = "native")]
                crate::stream_set::adopt_shown(frame);
                #[cfg(not(feature = "native"))]
                let _ = frame;
            })
            .await
        }
        Stream::Jobs => {
            let (tx, rx) = vox::channel::<crate::view::JobsFrame>();
            drain(link.streams.jobs(tx), rx, jobs_arrived).await
        }
        Stream::Edits => {
            let (tx, rx) = vox::channel::<crate::view::EditAsked>();
            drain(link.streams.edits(tx), rx, edit_asked).await
        }
        Stream::Changes => {
            let (tx, rx) = vox::channel::<crate::view::Changed>();
            drain(link.streams.changes(tx), rx, changed).await
        }
        Stream::Queued => {
            let (tx, rx) = vox::channel::<crate::view::Queued>();
            let taker = link.clone();
            drain(link.streams.queued(tx), rx, move |queued| {
                if queued.pending > 0 {
                    take_asked(&taker);
                }
            })
            .await
        }
    };
    dropped(&link, &format!("{which:?}: {ended}"));
}

/// Every frame of a held-open subscription to `on`, until it ends; why.
async fn drain<T, R, F>(
    call: impl std::future::Future<Output = Result<(), vox::VoxError<R>>>,
    mut rx: vox::Rx<T>,
    mut on: F,
) -> String
where
    T: facet::Facet<'static> + Send + 'static,
    R: std::fmt::Debug,
    F: FnMut(T),
{
    let mut call = std::pin::pin!(call);
    loop {
        tokio::select! {
            biased;
            res = &mut call => return format!("the stream ended: {res:?}"),
            frame = rx.recv() => match frame {
                Ok(Some(frame)) => {
                    let mut got = None;
                    let _ = frame.map(|f| got = Some(f));
                    if let Some(f) = got {
                        on(f);
                    }
                }
                other => return format!("the channel closed: {:?}", other.err()),
            },
        }
    }
}

// ── The set ──────────────────────────────────────────────────────────

/// The set as this window shows it: the engine's, each song read back
/// through the facade.
fn set_cell() -> &'static watch::Sender<crate::studio_engine::set::SetView> {
    static CELL: OnceLock<watch::Sender<crate::studio_engine::set::SetView>> = OnceLock::new();
    CELL.get_or_init(|| watch::Sender::new(crate::studio_engine::set::SetView::default()))
}

/// The set this window shows, from now on.
#[must_use]
pub fn subscribe_set() -> watch::Receiver<crate::studio_engine::set::SetView> {
    set_cell().subscribe()
}

/// The set this window shows now.
#[must_use]
pub fn set_view() -> crate::studio_engine::set::SetView {
    set_cell().borrow().clone()
}

/// `f` with the set this window shows, without copying it.
pub fn peek_set<R>(f: impl FnOnce(&crate::studio_engine::set::SetView) -> R) -> R {
    f(&set_cell().borrow())
}

/// Show `setlist` as this window's set: one it drives on another system
/// (a REAPER, Remote or Cue), which has no engine set to follow.
pub fn show_set(setlist: crate::setlist::Setlist) {
    set_cell().send_modify(|view| {
        view.setlist = Some(setlist);
        view.epoch += 1;
        view.opening = crate::studio_engine::set::Opening::Idle;
        view.revision += 1;
    });
}

/// Song `index` as the current one of the set this window shows — a song
/// already current on the system it drives (a tab picked in a REAPER) —
/// the one left keeping where its playhead had got to. The project, or
/// `None` when it is no song or current already.
pub fn show_current(index: usize) -> Option<String> {
    let mut picked = None;
    set_cell().send_if_modified(|view| {
        let Some(setlist) = view.setlist.as_mut() else {
            return false;
        };
        let left_at = setlist
            .songs
            .get(setlist.at)
            .and_then(|s| playhead_of(&s.project))
            .map_or(0.0, |(at, _)| at);
        picked = setlist
            .pick(index, left_at)
            .map(|song| song.project.clone());
        if picked.is_some() {
            view.revision += 1;
        }
        picked.is_some()
    });
    picked
}

/// What the window does with a set the engine sent: its songs read when
/// they are new (on a thread of its own: reading is facade calls), the
/// rest followed in place.
fn set_arrived(frame: SetFrame) {
    let shown: Vec<String> = peek_set(|view| {
        view.setlist
            .as_ref()
            .map(|s| s.songs.iter().map(|song| song.project.clone()).collect())
            .unwrap_or_default()
    });
    let engine: Vec<String> = frame.songs.iter().map(|(_, p)| p.clone()).collect();
    match crate::studio_engine::link::changed_songs(&shown, &engine) {
        crate::studio_engine::link::SongsChanged::Same => follow_in_place(&frame),
        crate::studio_engine::link::SongsChanged::Read => read_set(frame),
    }
}

/// The same songs: the current one, their colours, what is arriving, the
/// opening — followed in the replica.
fn follow_in_place(frame: &SetFrame) {
    set_cell().send_if_modified(|view| {
        let mut changed = false;
        if view.epoch != frame.epoch {
            view.epoch = frame.epoch;
            changed = true;
        }
        let opening = opening_of(&frame.opening);
        if view.opening != opening {
            view.opening = opening;
            changed = true;
        }
        if let Some(setlist) = view.setlist.as_mut() {
            changed |= follow_setlist(setlist, frame);
        }
        if changed {
            view.revision += 1;
        }
        changed
    });
}

/// `setlist` (this window's) made to say what `frame` (the engine's) says
/// of the same songs; whether anything changed.
fn follow_setlist(setlist: &mut crate::setlist::Setlist, frame: &SetFrame) -> bool {
    let mut changed = false;
    if let Some(at) = frame.current.map(|at| at as usize)
        && at != setlist.at
        && at < setlist.songs.len()
    {
        let left_at = playhead_of(&setlist.songs[setlist.at].project).map_or(0.0, |p| p.0);
        setlist.pick(at, left_at);
        changed = true;
    }
    for (song, color) in setlist.songs.iter_mut().zip(&frame.colors) {
        if !color.is_empty() && &song.color != color {
            song.color.clone_from(color);
            changed = true;
        }
    }
    if setlist.pending != frame.arriving {
        setlist.pending.clone_from(&frame.arriving);
        changed = true;
    }
    changed
}

fn opening_of(opening: &crate::view::SetOpening) -> crate::studio_engine::set::Opening {
    match opening {
        crate::view::SetOpening::Idle => crate::studio_engine::set::Opening::Idle,
        crate::view::SetOpening::Busy(p) => crate::studio_engine::set::Opening::Busy(p.clone()),
        crate::view::SetOpening::Failed(why) => {
            crate::studio_engine::set::Opening::Failed(why.clone())
        }
    }
}

/// Another set (or other songs in it): its new songs read through the
/// facade, the ones shown kept, then shown — newest frame wins.
fn read_set(frame: SetFrame) {
    static LATEST: Mutex<Option<SetFrame>> = Mutex::new(None);
    static READING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if let Ok(mut latest) = LATEST.lock() {
        *latest = Some(frame);
    }
    if READING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("view-set-read".into())
        .spawn(|| {
            loop {
                let Some(frame) = LATEST.lock().ok().and_then(|mut l| l.take()) else {
                    READING.store(false, std::sync::atomic::Ordering::Release);
                    // A frame that came while the flag was going down.
                    if LATEST.lock().is_ok_and(|l| l.is_some())
                        && !READING.swap(true, std::sync::atomic::Ordering::AcqRel)
                    {
                        continue;
                    }
                    return;
                };
                install_read(&frame);
            }
        });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "view link: no thread to read the set on");
        READING.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Read `frame`'s songs (those not shown already) and show the set.
fn install_read(frame: &SetFrame) {
    let have: Vec<crate::setlist::Song> = peek_set(|view| {
        view.setlist
            .as_ref()
            .map(|s| s.songs.clone())
            .unwrap_or_default()
    });
    let opening = opening_of(&frame.opening);
    if frame.songs.is_empty() {
        set_cell().send_modify(|view| {
            view.setlist = None;
            view.epoch = frame.epoch;
            view.opening = opening;
            view.revision += 1;
        });
        return;
    }
    let span = tracing::info_span!(
        "view.set_read",
        set.songs = frame.songs.len(),
        set.read = tracing::field::Empty,
        set.outcome = tracing::field::Empty
    );
    let _in = span.enter();
    let read = crate::setlist::Setlist::from_link(frame, have);
    match read {
        Ok((mut setlist, read)) => {
            span.record("set.read", read);
            span.record("set.outcome", "shown");
            follow_setlist(&mut setlist, frame);
            set_cell().send_modify(|view| {
                view.setlist = Some(setlist);
                view.epoch = frame.epoch;
                view.opening = opening;
                view.revision += 1;
            });
        }
        Err(e) => {
            span.record("set.outcome", "unread");
            tracing::warn!(error = %e, "view link: the engine's set could not be read");
        }
    }
}

/// What the window asks of the engine's set — each an action on the
/// engine's list, run off the caller's thread; the window shows what the
/// engine then publishes.
pub mod ask {
    /// Run `id` with `args` on the engine behind the window; a refusal said.
    fn off_the_window(id: &'static str, args: serde_json::Value) {
        let spawned = std::thread::Builder::new()
            .name("view-ask".into())
            .spawn(move || {
                if let Err(e) = super::run(id, args.to_string().as_bytes()) {
                    tracing::warn!(action.id = id, error = %e, "view link: the engine refused");
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(action.id = id, error = %e, "view link: no thread to ask on");
        }
    }

    /// Go to song `index` (0 the first): shown at once (the one left
    /// keeps where its playhead had got to), and asked of the engine. The
    /// project gone to, or `None` when it is no song or current already.
    pub fn pick(index: usize) -> Option<String> {
        let picked = super::show_current(index)?;
        off_the_window(
            "SESSION_SONG",
            serde_json::json!({ "to": (index + 1).to_string() }),
        );
        Some(picked)
    }

    /// Give song `index` a colour (`None`: the one its title makes).
    pub fn recolor(index: usize, color: Option<String>) {
        off_the_window(
            "SESSION_SONG_SET",
            serde_json::json!({ "song": index + 1, "color": color.unwrap_or_default() }),
        );
    }

    /// Move song `song` to `to` (0 the first).
    pub fn reorder(song: usize, to: usize) {
        off_the_window(
            "SESSION_SONG_SET",
            serde_json::json!({ "song": song + 1, "to": to + 1 }),
        );
    }

    /// Take song `song` out of the set.
    pub fn remove(song: usize) {
        off_the_window(
            "SESSION_SONG_SET",
            serde_json::json!({ "song": song + 1, "remove": true }),
        );
    }

    /// Close the engine's set (the transport stopped first).
    pub fn close() {
        off_the_window("SESSION_CLOSE", serde_json::json!({}));
    }

    /// A new, untitled song as the engine's set. Blocks on the engine's
    /// answer, as [`open_path`] does: call it off the UI thread.
    ///
    /// # Errors
    ///
    /// The engine refused (another open is under way, or no engine).
    pub fn new_song() -> Result<String, String> {
        super::run("SESSION_NEW", b"{}")
    }

    /// Join a live set by its link, as `name`. Blocks on the engine's
    /// answer: call it off the UI thread.
    ///
    /// # Errors
    ///
    /// The engine refused (another open is under way, or no engine).
    pub fn join(link: &str, name: &str) -> Result<String, String> {
        let args = serde_json::json!({ "link": link, "name": name });
        super::run("SESSION_JOIN", args.to_string().as_bytes())
    }

    /// Put the set in Live, or take it out.
    pub fn live(on: bool) {
        off_the_window("SESSION_LIVE", serde_json::json!({ "on": on }));
    }

    /// Open `path` (a song, a setlist, a `.session`) as the engine's set.
    ///
    /// # Errors
    ///
    /// The engine refused (another open is under way).
    pub fn open_path(path: &std::path::Path) -> Result<String, String> {
        let args = serde_json::json!({ "path": path.display().to_string() });
        super::run("SESSION_OPEN", args.to_string().as_bytes())
    }

    /// Open a library setlist as the engine's set.
    ///
    /// # Errors
    ///
    /// The engine refused (another open is under way).
    pub fn open_library(
        request: &crate::studio_engine::set::LibraryOpen,
    ) -> Result<String, String> {
        let args = serde_json::json!({
            "setlist": request.setlist,
            "org": request.org,
            "song": request.song,
        });
        super::run("SESSION_OPEN_SETLIST", args.to_string().as_bytes())
    }
}

// ── The marks ────────────────────────────────────────────────────────

static MARKS: RwLock<Option<Arc<MarksFrame>>> = RwLock::new(None);

fn marks_arrived(frame: MarksFrame) {
    crate::poi::adopt(frame.pois.clone(), frame.poi_current.clone());
    if let Ok(mut slot) = MARKS.write() {
        *slot = Some(Arc::new(frame));
    }
}

/// The tracks' own marks of the song on screen, as the engine last said.
#[must_use]
pub fn marks() -> Option<Arc<MarksFrame>> {
    MARKS.read().ok().and_then(|m| m.clone())
}

// ── The engine's jobs ────────────────────────────────────────────────

static JOBS: RwLock<Option<Arc<crate::view::JobsFrame>>> = RwLock::new(None);

fn jobs_arrived(frame: crate::view::JobsFrame) {
    if let Ok(mut slot) = JOBS.write() {
        *slot = Some(Arc::new(frame));
    }
}

/// What the engine is doing in the background, as it last said: none
/// with no engine linked.
#[must_use]
pub fn jobs() -> Vec<crate::view::JobRow> {
    JOBS.read()
        .ok()
        .and_then(|j| j.as_ref().map(|f| f.jobs.clone()))
        .unwrap_or_default()
}

// ── The edit cursor ──────────────────────────────────────────────────

/// An action put a song's edit cursor and time selection somewhere: the
/// window's frames take it up as they would one asked here.
fn edit_asked(asked: crate::view::EditAsked) {
    crate::edit_state::take_up(
        &asked.song,
        crate::edit_state::Edit {
            at: asked.at,
            selection: asked
                .selection
                .map(|s| crate::edit_state::Span::between(s.from, s.to)),
        },
    );
}

/// Tell the engine where the window shows the current song's edit cursor,
/// time selection and selection (every frame; only a change is sent, the
/// latest winning) — what its actions act on.
pub fn show_edit(edit: crate::edit_state::Edit, scope: Option<editing::scope::Scope>) {
    static SHOWN: std::sync::OnceLock<watch::Sender<Option<crate::view::EditShown>>> =
        std::sync::OnceLock::new();
    let Some(song) = crate::open::current_song() else {
        return;
    };
    let shown = crate::view::EditShown {
        song,
        at: edit.at,
        selection: edit
            .selection
            .map(|s| editing::scope::Span::between(s.start, s.end)),
        scope,
    };
    let tx = SHOWN.get_or_init(|| {
        let (tx, rx) = watch::channel(None);
        if let Some(rt) = runtime() {
            rt.spawn(send_edits(rx));
        }
        tx
    });
    tx.send_if_modified(|was| {
        if was.as_ref() == Some(&shown) {
            return false;
        }
        *was = Some(shown);
        true
    });
}

/// Send what [`show_edit`] was told, the latest each time, while the
/// window runs.
async fn send_edits(mut rx: watch::Receiver<Option<crate::view::EditShown>>) {
    while rx.changed().await.is_ok() {
        let Some(shown) = rx.borrow_and_update().clone() else {
            continue;
        };
        let Some(link) = link() else { continue };
        if tokio::time::timeout(CALL, link.view.show_edit(shown))
            .await
            .is_err()
        {
            tracing::warn!(
                view.call = "show_edit",
                "view link: the engine did not answer in time"
            );
        }
    }
}

// ── What changed ─────────────────────────────────────────────────────

fn changed(change: crate::view::Changed) {
    use crate::view::Change;
    match change.what {
        Change::All | Change::Automation => crate::song_model::request_resync_here(),
        Change::Undone => {
            crate::actions::undone_there();
            crate::song_model::request_resync_here();
        }
        Change::Items => {
            crate::song_model::request_resync_items_here();
            crate::wave_detail::engine_changed();
        }
        Change::Waves => {
            crate::midi::mark_stale_here(change.items);
            crate::song_model::request_resync_items_here();
        }
        Change::Marks => {}
    }
}

// ── What the engine asks ─────────────────────────────────────────────

fn take_asked(link: &Link) {
    let view = link.view.clone();
    let Some(rt) = runtime() else { return };
    rt.spawn(async move {
        match tokio::time::timeout(CALL, view.take_window()).await {
            Ok(Ok(asked)) => crate::window_queue::receive(asked, |id, args| {
                if let Err(e) = crate::actions::list().run_json(id, args.as_bytes()) {
                    tracing::warn!(action.id = id, error = %e, "view link: the engine's ask refused");
                }
            }),
            Ok(Err(e)) => tracing::warn!(error = ?e, "view link: what the engine asked was not taken"),
            Err(_) => tracing::warn!("view link: taking what the engine asked timed out"),
        }
    });
}

// ── The playheads ────────────────────────────────────────────────────

/// Each song's last stamped position, and when it came.
static PLAYHEADS: Mutex<Option<HashMap<String, (daw_proto::StampedPosition, web_time::Instant)>>> =
    Mutex::new(None);

async fn follow_playheads(link: Link) {
    let mut positions = link.daw.transport_sync().positions();
    while let Ok(Some(frame)) = positions.recv().await {
        if !current(&link) {
            return;
        }
        let at = frame.get();
        if let Ok(mut map) = PLAYHEADS.lock() {
            map.get_or_insert_with(HashMap::new).insert(
                at.project_guid.clone(),
                (at.clone(), web_time::Instant::now()),
            );
        }
    }
}

/// Where `song`'s playhead is now, seconds, and whether it plays — from its
/// last stamped position, carried on to now at its rate while it plays.
/// `None` before the engine has said.
#[must_use]
pub fn playhead_of(song: &str) -> Option<(f64, bool)> {
    let map = PLAYHEADS.lock().ok()?;
    let (stamp, when) = map.as_ref()?.get(song)?;
    Some(extrapolate(stamp, when.elapsed().as_secs_f64()))
}

/// A stamped position `since` seconds after it was heard: on, at its rate,
/// while it plays — never further than a quarter second (a stalled stream
/// does not run the playhead away).
#[must_use]
pub fn extrapolate(stamp: &daw_proto::StampedPosition, since: f64) -> (f64, bool) {
    if !stamp.is_playing {
        return (stamp.playhead_seconds, false);
    }
    let ahead = since.clamp(0.0, 0.25) * stamp.playrate;
    (stamp.playhead_seconds + ahead, true)
}

// ── Calls ────────────────────────────────────────────────────────────

/// Run `call` on the link, from sync code, bounded by `within`; `None`
/// unlinked or when the engine did not answer in time.
fn block<T: Send, F>(
    what: &str,
    within: Duration,
    call: impl FnOnce(EngineViewClient) -> F,
) -> Option<T>
where
    F: std::future::Future<Output = T> + Send,
{
    let link = link()?;
    let rt = runtime()?;
    let asked = call(link.view.clone());
    let bounded = async move { tokio::time::timeout(within, asked).await.ok() };
    // Blocking on the runtime from inside one panics: a thread of its own.
    let answered = if tokio::runtime::Handle::try_current().is_err() {
        rt.block_on(bounded)
    } else {
        std::thread::scope(|scope| scope.spawn(|| rt.block_on(bounded)).join().ok().flatten())
    };
    if answered.is_none() {
        tracing::warn!(
            view.call = what,
            "view link: the engine did not answer in time"
        );
    }
    answered
}

/// Run Action List action `id` with `args` (JSON) on the engine.
///
/// # Errors
///
/// Not linked, the engine did not answer, or the action refused.
pub fn run(id: &str, args: &[u8]) -> Result<String, String> {
    let call = ActionCall {
        id: id.to_owned(),
        args: String::from_utf8_lossy(args).into_owned(),
    };
    if !is_linked() {
        return Err(format!("{id}: no engine to run it on"));
    }
    block("run", RUN, |view| async move { view.run(call).await })
        .ok_or_else(|| format!("{id}: the engine did not answer in time"))?
        .map_err(|e| match e {
            vox::VoxError::User(e) => e.to_string(),
            other => format!("{id}: {other:?}"),
        })
}

/// The window's edits, carried out by the engine in order.
pub async fn apply(song: String, edits: Vec<crate::engine::Edit>) -> Option<Applied> {
    let link = link()?;
    match tokio::time::timeout(CALL, link.view.apply(EditBatch { song, edits })).await {
        Ok(Ok(applied)) => Some(applied),
        Ok(Err(e)) => {
            tracing::warn!(error = ?e, "view link: the engine refused the window's edits");
            None
        }
        Err(_) => {
            tracing::warn!("view link: the engine did not apply the window's edits in time");
            None
        }
    }
}

/// `items` of `song`'s waveforms, as the engine draws them.
#[must_use]
pub fn waves(song: &str, items: Vec<String>) -> Option<Vec<WaveRead>> {
    let ask = WavesAsk {
        song: song.to_owned(),
        items,
    };
    block(
        "waves",
        CALL,
        |view| async move { view.waves(ask).await.ok() },
    )
    .flatten()
}

/// `items` of `song`'s waveforms, as the engine draws them, awaited.
pub async fn waves_of(song: &str, items: Vec<String>) -> Option<Vec<WaveRead>> {
    let link = link()?;
    let ask = WavesAsk {
        song: song.to_owned(),
        items,
    };
    match tokio::time::timeout(CALL, link.view.waves(ask)).await {
        Ok(read) => read.ok(),
        Err(_) => {
            tracing::warn!(
                view.call = "waves",
                "view link: the engine did not answer in time"
            );
            None
        }
    }
}

/// One stretch's detail, from the engine.
#[must_use]
pub fn detail(ask: DetailAsk) -> Option<crate::wave_detail::Detail> {
    block(
        "detail",
        CALL,
        |view| async move { view.detail(ask).await.ok() },
    )
    .flatten()
    .flatten()
}

/// `song`'s automation, from the engine.
#[must_use]
pub fn automation(song: &str) -> Option<AutomationRead> {
    let song = song.to_owned();
    block("automation", CALL, |view| async move {
        view.automation(song).await.ok()
    })
    .flatten()
}

/// `song`'s automation, awaited.
pub async fn automation_of(song: &str) -> Option<AutomationRead> {
    let link = link()?;
    tokio::time::timeout(CALL, link.view.automation(song.to_owned()))
        .await
        .ok()?
        .ok()
}

/// The items of `song` changed since `since`.
#[must_use]
pub fn item_changes(song: &str, since: u64) -> Option<ItemChanges> {
    let ask = ChangesAsk {
        song: song.to_owned(),
        since,
    };
    block("item_changes", CALL, |view| async move {
        view.item_changes(ask).await.ok()
    })
    .flatten()
}

/// Where the engine's journal stands.
#[must_use]
pub fn journal_now() -> Option<u64> {
    block("journal_now", CALL, |view| async move {
        view.journal_now().await.ok()
    })
    .flatten()
}

/// What the window needs of `song` beside the facade, awaited.
pub async fn song_facts(song: &str) -> Option<SongFacts> {
    let link = link()?;
    tokio::time::timeout(CALL, link.view.song_facts(song.to_owned()))
        .await
        .ok()?
        .ok()
}

/// Every song's playhead, asked of the engine.
#[must_use]
pub fn playheads() -> Option<Vec<crate::view::Playhead>> {
    block("playheads", CALL, |view| async move {
        view.playheads().await.ok()
    })
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(at: f64, playing: bool) -> daw_proto::StampedPosition {
        daw_proto::StampedPosition {
            project_guid: "song".to_owned(),
            host_micros: 0.0,
            playhead_seconds: at,
            playrate: 1.0,
            is_playing: playing,
            sequence: 0,
            sample_rate: 48_000.0,
        }
    }

    #[test]
    fn a_playing_position_runs_on_at_its_rate_but_not_far() {
        assert_eq!(extrapolate(&stamp(10.0, true), 0.1), (10.1, true));
        assert_eq!(extrapolate(&stamp(10.0, true), 5.0), (10.25, true));
        assert_eq!(extrapolate(&stamp(10.0, false), 5.0), (10.0, false));
    }
}

// ── A take's samples, over the link ──────────────────────────────────

/// The engine's audio accessors, reached over the window's link: what
/// reads a take's samples for the window's own analysis (the expression
/// editor's audio takes) as it would read them from an engine in its
/// process. Each call waits on the engine — use it off the window's
/// thread.
#[derive(Clone, Copy, Debug, Default)]
pub struct LinkedAudio;

impl LinkedAudio {
    fn client() -> Option<daw_proto::AudioAccessorsClient> {
        let daw = daw::rpc::Daw::try_get()?;
        Some(daw_proto::AudioAccessorsClient::new(daw.caller().clone()))
    }

    fn ask<T: Send, F>(
        what: &str,
        call: impl FnOnce(daw_proto::AudioAccessorsClient) -> F,
    ) -> Option<T>
    where
        F: std::future::Future<Output = Option<T>> + Send,
    {
        let client = Self::client()?;
        let rt = runtime()?;
        let asked = call(client);
        let bounded = async move { tokio::time::timeout(CALL, asked).await.ok().flatten() };
        let answered = if tokio::runtime::Handle::try_current().is_err() {
            rt.block_on(bounded)
        } else {
            std::thread::scope(|scope| scope.spawn(|| rt.block_on(bounded)).join().ok().flatten())
        };
        if answered.is_none() {
            tracing::debug!(audio.call = what, "linked audio: no answer");
        }
        answered
    }
}

impl daw_proto::AudioAccessors for LinkedAudio {
    fn create_track_accessor(
        &self,
        project: daw_proto::ProjectContext,
        track: daw_proto::TrackRef,
    ) -> Option<String> {
        Self::ask("create_track_accessor", |c| async move {
            c.create_track_accessor(project, track).await.ok().flatten()
        })
    }

    fn create_take_accessor(
        &self,
        project: daw_proto::ProjectContext,
        item: daw_proto::ItemRef,
        take: daw_proto::TakeRef,
    ) -> Option<String> {
        Self::ask("create_take_accessor", |c| async move {
            c.create_take_accessor(project, item, take)
                .await
                .ok()
                .flatten()
        })
    }

    fn has_state_changed(&self, accessor_id: &str) -> bool {
        let id = accessor_id.to_owned();
        Self::ask("has_state_changed", |c| async move {
            c.has_state_changed(id).await.ok()
        })
        .unwrap_or(false)
    }

    fn get_samples(&self, request: daw_proto::GetSamplesRequest) -> daw_proto::AudioSampleData {
        Self::ask("get_samples", |c| async move {
            c.get_samples(request).await.ok()
        })
        .unwrap_or_default()
    }

    fn destroy_accessor(&self, accessor_id: &str) {
        let id = accessor_id.to_owned();
        let _ = Self::ask("destroy_accessor", |c| async move {
            c.destroy_accessor(id).await.ok()
        });
    }
}
