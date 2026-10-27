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
//! Opening runs here too ([`open_with`], [`open_library`], [`open_path`]):
//! on a thread of its own, its progress published as [`Opening::Busy`],
//! the set installed when it is ready and the songs behind the first
//! drained into it as they arrive ([`install`]).
//!
//! Songs still carry the window's read-back of each project
//! (`crate::studio::StudioSession`), made as they open: the window's model
//! cache moves to the window when it reads projects over the facade
//! (`docs/design/engine-boundary.md`, step 5).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tokio::sync::watch;

use crate::loading::Progress;
use crate::setlist::{Arrival, Setlist};

/// Where opening a set has got to.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Opening {
    /// Nothing is being opened.
    #[default]
    Idle,
    /// Opening: where it has got to.
    Busy(Progress),
    /// The last open failed, and why, for a person.
    Failed(String),
}

/// The set as the engine has it, as published.
#[derive(Clone, Default)]
pub struct SetView {
    /// Bumped by every change: a reader that saw this revision has seen
    /// everything up to it.
    pub revision: u64,
    /// Which set this is: bumped when one is installed or closed. Songs
    /// arriving for an older set are dropped, and a window remounts its
    /// views when it changes.
    pub epoch: u64,
    /// The set, when one is open.
    pub setlist: Option<Setlist>,
    /// An open under way, or how the last one went.
    pub opening: Opening,
}

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
        self.songs.get(self.at).map_or("", |(title, _)| title.as_str())
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
    LastSoFar { current: String },
    Last { current: String },
    First { current: String },
    Already { current: String },
    OutOfRange { len: usize },
}

impl std::fmt::Display for PickRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSet => f.write_str("no set is open"),
            Self::Arriving(title) => write!(f, "{title:?} is still arriving"),
            Self::NoSuchSong(title) => write!(f, "no song {title:?} in the set"),
            Self::LastSoFar { current } => {
                write!(f, "{current} is the last song here yet; the rest are arriving")
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
fn update(change: impl FnOnce(&mut SetView) -> bool) {
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

/// The songs arriving behind the first ([`crate::stream_set::take_arrivals`])
/// into the set of `epoch`, on the engine's runtime, until they stop or
/// another set replaces it.
fn drain_arrivals(epoch: u64) {
    let Some(mut arrivals) = crate::stream_set::take_arrivals() else {
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
        .and_then(|(_, project)| crate::stream_set::transport_of(project))
        .map_or(0.0, |(at, _)| at)
}

/// Give song `index` a colour by hand (`None`: the one its title makes) —
/// on its SONG region and in its `.session` (`Setlist::recolor`).
///
/// The region and the file are written outside the set's lock: in Remote
/// that is a call to the other system, which may take seconds.
pub fn recolor(index: usize, color: Option<String>) {
    let shown = view();
    let Some(mut setlist) = shown.setlist else {
        return;
    };
    setlist.recolor(index, color);
    let Some(recoloured) = setlist.songs.get(index).cloned() else {
        return;
    };
    update(|view| {
        let song = view
            .setlist
            .as_mut()
            .filter(|_| view.epoch == shown.epoch)
            .and_then(|s| s.songs.get_mut(index))
            .filter(|song| song.project == recoloured.project);
        let Some(song) = song else {
            return false;
        };
        song.color = recoloured.color;
        true
    });
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
    let spawned = std::thread::Builder::new()
        .name("set-open".into())
        .spawn(move || finish_open(work(&said)));
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
    let why = crate::task_set::brief(&format!("{e:#}"));
    update(|view| {
        view.opening = Opening::Failed(why);
        true
    });
}

/// A setlist of an org's Task library to open: its title, the org whose
/// library holds it, and one of its songs to open alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryOpen {
    /// The org whose library holds it; `FTS_TASK_ORG`, else the org last
    /// picked here, when `None`.
    pub org: Option<String>,
    /// Its title, as the library lists it.
    pub setlist: String,
    /// One of its songs (slug or title) to open alone; the whole set when
    /// `None`.
    pub song: Option<String>,
}

/// Stream `request`'s setlist in from the library, as the person signed in
/// on this machine (`fts_account`, `FTS_ACCOUNT_FILE`): what a click on it
/// in a window does, and `SESSION_OPEN_SETLIST`.
///
/// # Errors
///
/// Another open is under way.
pub fn open_library(request: LibraryOpen) -> Result<(), Busy> {
    open_with(
        Progress::Joining { retry: None },
        Box::new(move |progress| open_library_now(&request, progress)),
    )
}

fn open_library_now(request: &LibraryOpen, progress: &dyn Fn(Progress)) -> eyre::Result<Setlist> {
    use crate::task_account::AccountLibrary as _;
    let account = crate::task_account::signed_in()
        .ok_or_else(|| eyre::eyre!("not signed in to Task on this machine"))?;
    let org = request
        .org
        .clone()
        .or_else(|| std::env::var("FTS_TASK_ORG").ok().filter(|o| !o.is_empty()))
        .or_else(crate::task_account::remembered_org)
        .ok_or_else(|| eyre::eyre!("which org's library? (`org`, or FTS_TASK_ORG)"))?;
    let library = account.library(&org);
    let lists = crate::stream_set::setlists(&library)?;
    let titles: Vec<&str> = lists.iter().map(|l| l.title.as_str()).collect();
    let at = position_named(&titles, &request.setlist)
        .ok_or_else(|| eyre::eyre!("no setlist {:?} in {org}'s library", request.setlist))?;
    let mut setlist = lists
        .into_iter()
        .nth(at)
        .ok_or_else(|| eyre::eyre!("the library's lists changed while reading them"))?;
    if let Some(song) = &request.song {
        setlist
            .songs
            .retain(|s| s.slug.eq_ignore_ascii_case(song) || s.title.eq_ignore_ascii_case(song));
        if setlist.songs.is_empty() {
            eyre::bail!("no song {song:?} in {:?}", request.setlist);
        }
    }
    tracing::info!(
        open.org = %org,
        open.songs = setlist.songs.len(),
        "set: opening a library setlist"
    );
    crate::stream_set::open(
        crate::stream_set::Remote::Library {
            library,
            setlist,
            name: account.name(),
        },
        progress,
    )
}

/// Where `want` is among `titles`, ignoring case and the space around it.
fn position_named(titles: &[&str], want: &str) -> Option<usize> {
    let want = want.trim();
    titles.iter().position(|t| t.trim().eq_ignore_ascii_case(want))
}

/// Open what is at `path` — a song (`.RPP`, `.session`, a song's folder) or
/// a setlist (`.setlist`, a folder of songs) — as the set: `SESSION_OPEN`.
///
/// # Errors
///
/// Another open is under way.
pub fn open_path(path: PathBuf) -> Result<(), Busy> {
    let title = path.file_stem().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    open_with(
        Progress::Opening { title },
        Box::new(move |_| {
            let songs = songs_at(&path)
                .ok_or_else(|| eyre::eyre!("{} is not a song or a setlist", path.display()))?;
            Setlist::open(&songs)
        }),
    )
}

/// The songs at `target`: itself for a song, the songs a setlist names, the
/// song in a song's folder; `None` when it is none of those.
#[must_use]
pub fn songs_at(target: &Path) -> Option<Vec<PathBuf>> {
    use crate::setlist::{read_setlist, song_in};
    if target.is_dir() && !crate::open::is_session(target) {
        if let Some(song) = song_in(target) {
            return Some(vec![song]);
        }
    } else if !target
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("setlist"))
    {
        return Some(vec![target.to_path_buf()]);
    }
    read_setlist(target)
        .inspect_err(|e| tracing::error!(error = %e, "the setlist could not be read"))
        .ok()
}

#[cfg(test)]
mod tests;
