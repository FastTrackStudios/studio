//! The set the engine has open: its songs in order, the current one, the
//! ones still arriving, and an open under way.
//!
//! **Engine state, not the window's** (`docs/design/set-engine.md`: "the
//! engine owns the set"). It lives here, one per process, changed only
//! through the operations below and published on a watch channel
//! ([`subscribe`]): a window mirrors it ([`SetView`]), the Action List reads
//! it (`SESSION_SONGS`), `studio engine` runs it with no window at all.
//! Before this, the window held the set in a signal and the Action List
//! asked it for picks through a queue the window polled — so with no window
//! nothing could open a set or change songs.
//!
//! Opening runs here too ([`open_with`], [`open_path`]):
//! on a thread of its own, its progress published as [`Opening::Busy`],
//! the set installed when it is ready and the songs behind the first
//! drained into it as they arrive ([`install`]).
//!
//! Songs still carry the window's read-back of each project
//! (`crate::song_model::StudioSession`), made as they open: the window's model
//! cache moves to the window when it reads projects over the facade
//! (`docs/design/engine-boundary.md`, step 5).

use std::sync::{Mutex, OnceLock};

use tokio::sync::watch;

use crate::open_progress::Progress;
use crate::setlist::{Arrival, Setlist};

pub use engine_runtime::client::studio_engine::set::*;

/// The set's songs as titles and projects — what the Action List lists and
/// picks from, without the window's read-back of each song.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lineup {
    /// The songs in order: their titles and engine projects.
    pub songs: Vec<(String, String)>,
    /// The current one, an index into `songs`.
    pub at: usize,
    /// Songs still on their way, by title, after `songs`.
    pub arriving: Vec<String>,
}

impl Lineup {
    /// The lineup of `setlist`.
    #[must_use]
    pub fn of(setlist: &Setlist) -> Self {
        Self {
            songs: setlist
                .songs
                .iter()
                .map(|song| (song.title(), song.project.clone()))
                .collect(),
            at: setlist.at,
            arriving: setlist.pending.clone(),
        }
    }

    /// The current song's title, or nothing.
    fn current_title(&self) -> &str {
        self.songs
            .get(self.at)
            .map_or("", |(title, _)| title.as_str())
    }
}

/// Which song to go to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SongPick {
    /// By its place in the set (0 the first).
    At(usize),
    Next,
    Previous,
}

impl SongPick {
    /// The index this lands on in a set of `len` songs at `at`; `None` past
    /// either end, or when it is the song already current.
    #[must_use]
    pub fn index(&self, at: usize, len: usize) -> Option<usize> {
        let to = match self {
            Self::At(to) => *to,
            Self::Next => at.checked_add(1)?,
            Self::Previous => at.checked_sub(1)?,
        };
        (to < len && to != at).then_some(to)
    }
}

/// Why a song could not be gone to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickRefused {
    NoSet,
    Arriving(String),
    NoSuchSong(String),
    /// The last song open, the rest still arriving.
    LastSoFar {
        current: String,
    },
    Last {
        current: String,
    },
    First {
        current: String,
    },
    Already {
        current: String,
    },
    OutOfRange {
        len: usize,
    },
}

impl std::fmt::Display for PickRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSet => f.write_str("no set is open"),
            Self::Arriving(title) => write!(f, "{title:?} is still arriving"),
            Self::NoSuchSong(title) => write!(f, "no song {title:?} in the set"),
            Self::LastSoFar { current } => {
                write!(
                    f,
                    "{current} is the last song here yet; the rest are arriving"
                )
            }
            Self::Last { current } => write!(f, "{current} is the last song"),
            Self::First { current } => write!(f, "{current} is the first song"),
            Self::Already { current } => write!(f, "already on {current}"),
            Self::OutOfRange { len } => write!(f, "the set has {len} songs"),
        }
    }
}

impl std::error::Error for PickRefused {}

/// What `to` asks for in `lineup` — its number (1 the first), its title,
/// `next` or `previous` — as the index to go to.
///
/// # Errors
///
/// Past either end, the song already current, a song still arriving, or
/// no such song: [`PickRefused`] says which.
pub fn resolve(lineup: &Lineup, to: &str) -> Result<usize, PickRefused> {
    let pick = parse_pick(lineup, to)?;
    pick.index(lineup.at, lineup.songs.len())
        .ok_or_else(|| refusal(lineup, &pick))
}

/// `to` as a pick: a word, a number, or a title in the set.
fn parse_pick(lineup: &Lineup, to: &str) -> Result<SongPick, PickRefused> {
    let to = to.trim();
    match to.to_lowercase().as_str() {
        "next" => return Ok(SongPick::Next),
        "previous" | "prev" => return Ok(SongPick::Previous),
        _ => {}
    }
    if let Ok(number) = to.parse::<usize>()
        && number >= 1
    {
        return Ok(SongPick::At(number - 1));
    }
    if let Some(at) = lineup
        .songs
        .iter()
        .position(|(title, _)| title.eq_ignore_ascii_case(to))
    {
        return Ok(SongPick::At(at));
    }
    if lineup.arriving.iter().any(|t| t.eq_ignore_ascii_case(to)) {
        return Err(PickRefused::Arriving(to.to_owned()));
    }
    Err(PickRefused::NoSuchSong(to.to_owned()))
}

/// Why `pick` lands nowhere in `lineup`.
fn refusal(lineup: &Lineup, pick: &SongPick) -> PickRefused {
    let current = lineup.current_title().to_owned();
    let len = lineup.songs.len();
    match pick {
        SongPick::Next if lineup.at + 1 >= len && !lineup.arriving.is_empty() => {
            PickRefused::LastSoFar { current }
        }
        SongPick::Next => PickRefused::Last { current },
        SongPick::Previous => PickRefused::First { current },
        SongPick::At(at) if *at == lineup.at => PickRefused::Already { current },
        SongPick::At(_) => PickRefused::OutOfRange { len },
    }
}

/// One song of the set, as `SESSION_SONGS` lists it.
#[derive(Clone, Debug, PartialEq, Eq, facet::Facet)]
pub struct SongRow {
    /// Its place in the set, 1 the first.
    pub number: u32,
    pub title: String,
    /// The engine project; empty while it is arriving.
    pub project: String,
    pub current: bool,
    /// `open`, or `arriving`.
    pub state: String,
}

/// Every song of `lineup`, open ones first, then the arriving.
#[must_use]
pub fn rows(lineup: &Lineup) -> Vec<SongRow> {
    let number = |i: usize| u32::try_from(i + 1).unwrap_or(u32::MAX);
    let open = lineup
        .songs
        .iter()
        .enumerate()
        .map(|(i, (title, project))| SongRow {
            number: number(i),
            title: title.clone(),
            project: project.clone(),
            current: i == lineup.at,
            state: "open".to_owned(),
        });
    let arriving = lineup
        .arriving
        .iter()
        .enumerate()
        .map(|(i, title)| SongRow {
            number: number(lineup.songs.len() + i),
            title: title.clone(),
            project: String::new(),
            current: false,
            state: "arriving".to_owned(),
        });
    open.chain(arriving).collect()
}

// ── The engine's one set ─────────────────────────────────────────────

/// The process's set, published. One per process: a process runs one
/// engine (`docs/design/one-engine.md`).
fn cell() -> &'static watch::Sender<SetView> {
    static CELL: OnceLock<watch::Sender<SetView>> = OnceLock::new();
    CELL.get_or_init(|| watch::Sender::new(SetView::default()))
}

/// Change the set with `change`; published, with a new revision, when it
/// says it changed something.
pub fn update(change: impl FnOnce(&mut SetView) -> bool) {
    cell().send_if_modified(|view| {
        let changed = change(view);
        if changed {
            view.revision += 1;
        }
        changed
    });
}

/// The set from now on: every change, as it happens.
#[must_use]
pub fn subscribe() -> watch::Receiver<SetView> {
    cell().subscribe()
}

/// The set as it is now.
#[must_use]
pub fn view() -> SetView {
    cell().borrow().clone()
}

/// `f` with the set as it is now, without copying it — for a reader that
/// looks often (Live mode's roll-on, [`super::live`]). Holds the set's lock
/// while `f` runs: keep it short, and never change the set from it.
pub fn peek<R>(f: impl FnOnce(&SetView) -> R) -> R {
    f(&cell().borrow())
}

/// The set's lineup, when one is open.
#[must_use]
pub fn lineup() -> Option<Lineup> {
    cell().borrow().setlist.as_ref().map(Lineup::of)
}

/// Whether an open is under way.
#[must_use]
pub fn is_opening() -> bool {
    matches!(cell().borrow().opening, Opening::Busy(_))
}

/// `setlist` as the engine's set from now on, the songs still arriving
/// behind its first drained into it as they open.
pub fn install(setlist: Setlist) {
    // Projects the engine keeps but this set does not have (an earlier
    // set's, a window that opened the same song again) stop playing: one
    // engine may hold many projects, but only the set's own may sound.
    let keep: Vec<String> = Lineup::of(&setlist)
        .songs
        .into_iter()
        .map(|(_, project)| project)
        .collect();
    stop_others(keep);
    let mut epoch = 0;
    update(|view| {
        view.epoch += 1;
        epoch = view.epoch;
        view.setlist = Some(setlist);
        view.opening = Opening::Idle;
        true
    });
    tracing::info!(
        set.epoch = epoch,
        set.songs = lineup().map_or(0, |l| l.songs.len()),
        "set: installed in the engine"
    );
    drain_arrivals(epoch);
}

/// Stops the transport of every project the engine has that is not in
/// `keep` (engine project guids), off the caller's thread. A project that
/// was playing goes quiet; its state stays, as the engine keeps it.
fn stop_others(keep: Vec<String>) {
    let spawned = std::thread::Builder::new()
        .name("set-stop-others".into())
        .spawn(move || {
            let stopped = crate::open::facade_blocking(
                "stop the projects not in the set",
                |daw| async move {
                    let mut stopped = 0_u32;
                    for project in daw.projects().await? {
                        let info = project.info().await?;
                        if keep.iter().any(|k| *k == info.guid) {
                            continue;
                        }
                        let transport = project.transport();
                        if transport.is_playing().await.unwrap_or(true) {
                            transport.stop().await?;
                            stopped += 1;
                        }
                    }
                    Ok(stopped)
                },
            );
            match stopped {
                Ok(0) => {}
                Ok(n) => tracing::info!(
                    set.stopped = n,
                    "set: projects outside the set stopped playing"
                ),
                Err(error) => {
                    tracing::warn!(error = %error, "set: the projects outside it were not stopped")
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(error = %error, "set: no thread to stop the projects outside it");
    }
}

/// No set from now on (its projects stay in the engine, as they did when
/// a window went back to its start screen).
pub fn close() {
    update(|view| {
        if view.setlist.is_none() {
            return false;
        }
        view.epoch += 1;
        view.setlist = None;
        true
    });
}

/// The songs arriving behind the first ([`take_arrivals`])
/// into the set of `epoch`, on the engine's runtime, until they stop or
/// another set replaces it.
fn drain_arrivals(epoch: u64) {
    let Some(mut arrivals) = take_arrivals() else {
        return;
    };
    let Some(runtime) = crate::open::runtime() else {
        tracing::warn!("set: no engine runtime to take the arriving songs on");
        return;
    };
    runtime.spawn(async move {
        while let Some(arrival) = arrivals.recv().await {
            if !arrive(epoch, arrival) {
                return;
            }
        }
    });
}

/// `arrival` into the set of `epoch`; `false` when that set is gone.
fn arrive(epoch: u64, arrival: Arrival) -> bool {
    let mut current = false;
    update(|view| {
        current = view.epoch == epoch;
        let Some(setlist) = view.setlist.as_mut().filter(|_| current) else {
            return false;
        };
        setlist.arrive(arrival);
        true
    });
    current
}

/// Go to song `to` of the set: the engine's current song from now on, the
/// one it replaces left where its playhead was (`left_at`). The project
/// gone to, or `None` when `to` is out of range or current already.
pub fn pick(to: usize, left_at: f64) -> Option<String> {
    let project = mark_current(to, left_at)?;
    crate::open::switch_song(&project);
    Some(project)
}

/// [`pick`], for a song already current on the system the engine drives (a
/// tab picked in REAPER, a collaborator's pick): the set follows, nothing
/// is switched.
pub fn mark_current(to: usize, left_at: f64) -> Option<String> {
    let mut picked = None;
    update(|view| {
        picked = view
            .setlist
            .as_mut()
            .and_then(|setlist| setlist.pick(to, left_at))
            .map(|song| song.project.clone());
        picked.is_some()
    });
    picked
}

/// Go to the song `to` names (see [`resolve`]); its title and number, said.
///
/// # Errors
///
/// No set is open, or [`resolve`] refused.
pub fn pick_by(to: &str) -> Result<String, PickRefused> {
    let lineup = lineup().ok_or(PickRefused::NoSet)?;
    let index = resolve(&lineup, to)?;
    let title = lineup
        .songs
        .get(index)
        .map(|(title, _)| title.clone())
        .unwrap_or_default();
    let left_at = playhead_now(&lineup);
    pick(index, left_at);
    Ok(format!("going to {title} (song {})", index + 1))
}

/// Where the current song's playhead is, as the engine has it.
fn playhead_now(lineup: &Lineup) -> f64 {
    lineup
        .songs
        .get(lineup.at)
        .and_then(|(_, project)| transport_of(project))
        .map_or(0.0, |(at, _)| at)
}

/// Move song `song` to `to`, the current song staying current.
pub fn reorder(song: usize, to: usize) {
    update(|view| {
        let Some(setlist) = view.setlist.as_mut() else {
            return false;
        };
        setlist.reorder(song, to);
        true
    });
}

/// Take song `song` out of the set.
pub fn remove(song: usize) {
    update(|view| {
        let Some(setlist) = view.setlist.as_mut() else {
            return false;
        };
        setlist.remove(song);
        true
    });
}

// ── Opening ──────────────────────────────────────────────────────────

/// A way of opening a set, run on a thread of its own: it says each step
/// to its `progress`.
pub type OpenWork = Box<dyn FnOnce(&dyn Fn(Progress)) -> eyre::Result<Setlist> + Send>;

/// Another open was under way: where it has got to.
#[derive(Clone, Debug, PartialEq)]
pub struct Busy(pub Progress);

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a set is already opening ({:?})", self.0)
    }
}

impl std::error::Error for Busy {}

/// Run `work` on a thread of its own, its progress published from `first`
/// on, and install the set it opens; a failure is published as
/// [`Opening::Failed`].
///
/// # Errors
///
/// Another open is under way: one at a time.
pub fn open_with(first: Progress, work: OpenWork) -> Result<(), Busy> {
    let mut busy = None;
    update(|view| {
        if let Opening::Busy(progress) = &view.opening {
            busy = Some(progress.clone());
            return false;
        }
        view.opening = Opening::Busy(first);
        true
    });
    if let Some(progress) = busy {
        return Err(Busy(progress));
    }
    // The set's opening, one unit of work on its own thread, under whoever
    // started it (`engine.start` when the engine reopens its set): each
    // song's `project.load` lands under it.
    let parent = tracing::Span::current();
    let spawned = std::thread::Builder::new()
        .name("set-open".into())
        .spawn(move || {
            let set = tracing::info_span!(
                parent: &parent,
                "set.open",
                set.songs = tracing::field::Empty,
                set.opened = tracing::field::Empty,
                set.join_ms = tracing::field::Empty,
                set.first_fetch_ms = tracing::field::Empty,
                set.ok = tracing::field::Empty,
            );
            let _set = set.enter();
            let outcome = work(&said);
            set.record("set.ok", outcome.is_ok());
            finish_open(outcome);
        });
    if let Err(e) = spawned {
        failed(&eyre::eyre!("no thread to open the set on: {e}"));
    }
    Ok(())
}

/// A step of the open under way, published.
fn said(progress: Progress) {
    update(|view| {
        if !matches!(view.opening, Opening::Busy(_)) {
            return false;
        }
        view.opening = Opening::Busy(progress);
        true
    });
}

/// What an open came to: the set installed, or why it is not.
fn finish_open(outcome: eyre::Result<Setlist>) {
    match outcome {
        Ok(setlist) => install(setlist),
        Err(e) => failed(&e),
    }
}

fn failed(e: &eyre::Report) {
    tracing::warn!(error = %e, "set: the set did not open");
    let why = brief(&format!("{e:#}"));
    update(|view| {
        view.opening = Opening::Failed(why);
        true
    });
}

/// Where `want` is among `titles`, ignoring case and the space around it.
pub fn position_named(titles: &[&str], want: &str) -> Option<usize> {
    let want = want.trim();
    titles
        .iter()
        .position(|t| t.trim().eq_ignore_ascii_case(want))
}

// ── The songs arriving, the transport, an error said briefly ─────────────

/// The set's songs after the first, as each opens — once, for the window
/// that shows them.
static ARRIVALS: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Arrival>>> = Mutex::new(None);

/// The songs arriving behind the first (see [`open`]), for the window to
/// add to its set; `None` while another holds them, or when nothing is
/// arriving. Given back when the holder drops them: a window mounted again
/// (the welcome screen handing the set over) picks up where the last one
/// stopped — taken for good, the songs after it never arrived.
pub fn take_arrivals() -> Option<Arrivals> {
    ARRIVALS.lock().ok()?.take().map(|rx| Arrivals(Some(rx)))
}

/// The arrivals, held — given back on drop ([`take_arrivals`]).
pub struct Arrivals(Option<tokio::sync::mpsc::UnboundedReceiver<Arrival>>);

impl Arrivals {
    /// The next song to arrive; `None` once every one has.
    pub async fn recv(&mut self) -> Option<Arrival> {
        self.0.as_mut()?.recv().await
    }
}

impl Drop for Arrivals {
    fn drop(&mut self) {
        if let (Some(rx), Ok(mut slot)) = (self.0.take(), ARRIVALS.lock())
            && slot.is_none()
        {
            *slot = Some(rx);
        }
    }
}

/// Hold `arrivals`, the songs arriving behind the first of a set just
/// opened (`stream_set::open`), for [`take_arrivals`].
pub fn hold_arrivals(arrivals: tokio::sync::mpsc::UnboundedReceiver<Arrival>) {
    if let Ok(mut slot) = ARRIVALS.lock() {
        *slot = Some(arrivals);
    }
}

/// The transport's playhead and whether it moves, for `project`.
pub fn transport_of(project: &str) -> Option<(f64, bool)> {
    let daw = crate::open::local_engine()?;
    let backend = daw.sync_backend(project)?;
    let snap = daw_transport_sync::TransportBackend::snapshot(&backend)?;
    Some((snap.playhead_seconds, snap.is_playing))
}

/// A dial's error in a few words for a person: the HTTP status when the
/// server answered one (a dial error carries the whole response — headers,
/// and the body as bytes), else its first line, cut short.
#[must_use]
pub fn brief(error: &str) -> String {
    if let Some(at) = error.find("status: ") {
        let code: String = error[at + 8..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if !code.is_empty() {
            return format!("the server answered {code}");
        }
    }
    let line = error.lines().next().unwrap_or_default();
    match line.char_indices().nth(120) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_owned(),
    }
}

#[cfg(test)]
mod tests;
