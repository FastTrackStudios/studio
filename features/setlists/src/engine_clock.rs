//! Studio's playback as the playhead: a running `studio engine`'s
//! transport moves this extension's source on `architect.playhead`
//! ([`crate::now`]) to the song and the time Studio plays.
//!
//! **Read only.** It subscribes to the engine's playhead stream
//! (`TransportSync::positions`, one frame per audio buffer) and asks it
//! the names of its projects; it never plays, stops or moves the engine.
//! On going away it drops the subscription, then shuts the connection
//! down and waits for it to close (never a reset on the engine's socket:
//! a person's Studio is on the other end). Lighting's runner
//! (`apps/ignition/src/studio_clock.rs`) follows the engine the same way.
//!
//! **Which song.** Studio plays a set as split songs, each its own project
//! (`<slug>/<Title>.session`), named when it was split: the engine may
//! still hold a song under an older name than the vault's ("7 Nation
//! Army" for "Seven Nation Army", "Know Me" for the setlist's "Know Me
//! (Extended)"). A project is the setlist's song ([`Songs::find`]) whose
//! title is the project's (its name, its file's stem or its folder's),
//! compared without case, punctuation or spelled-out numbers ("7" is
//! "seven"), or whose recording's file is named by the project's folder
//! (the split's slug: `7-nation-army` for `Media/7-nation-army.ogg`).
//! A `Title (Arrangement)` name picks that arrangement; else the
//! setlist's row decides it (`Know Me#Extended`).
//!
//! **Which project.** The one playing; while none plays, the one whose
//! playhead last moved (a locate, a song picked). Between frames the deck
//! carries the time on itself; the follower moves it only when the song,
//! the play state or (by more than [`DRIFT`]) the time differs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crate::host::{SetlistRow, SongRow};
use crate::now::item_id;

/// How far the deck may stray from the engine before it is moved.
pub const DRIFT: f64 = 0.15;

/// How often the deck is set right.
const RECONCILE: Duration = Duration::from_millis(100);

// ---------------------------------------------------------------------------
// Which song a project is
// ---------------------------------------------------------------------------

/// A setlist's songs as the follower matches projects to them.
#[derive(Debug, Clone, Default)]
pub struct Songs {
    /// Each row: its item id, its title's key, its arrangement's key, its
    /// recording's stem's key.
    rows: Vec<(String, String, Option<String>, Option<String>)>,
}

impl Songs {
    /// `set`'s rows, with what `songs` says of each.
    #[must_use]
    pub fn of(set: &SetlistRow, songs: &[SongRow]) -> Self {
        let rows = set
            .songs
            .iter()
            .map(|at| {
                let stem = songs
                    .iter()
                    .find(|s| s.at.library == at.library && s.at.title == at.title)
                    .and_then(|s| s.recording.as_deref())
                    .and_then(|r| Path::new(r).file_stem().and_then(|s| s.to_str()))
                    .map(key);
                (
                    item_id(at),
                    key(&at.title),
                    at.arrangement.as_deref().map(key),
                    stem,
                )
            })
            .collect();
        Self { rows }
    }

    /// Whether it has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The item a project named `name` at `path` is, if any.
    #[must_use]
    pub fn find(&self, name: &str, path: &str) -> Option<String> {
        let path = Path::new(path);
        let candidates = [
            Some(name),
            path.file_stem().and_then(|s| s.to_str()),
            path.parent()
                .and_then(Path::file_name)
                .and_then(|s| s.to_str()),
        ];
        for candidate in candidates.into_iter().flatten() {
            let stem = candidate
                .trim()
                .strip_suffix(".session")
                .unwrap_or(candidate.trim());
            if stem.is_empty() {
                continue;
            }
            let (title, arrangement) = split_arrangement(stem);
            let (title, arrangement) = (key(title), arrangement.map(key));
            let whole = key(stem);
            let same_song = |row: &&(String, String, Option<String>, Option<String>)| {
                row.1 == title || row.1 == whole || row.3.as_deref() == Some(whole.as_str())
            };
            let mut hits = self.rows.iter().filter(same_song).peekable();
            if hits.peek().is_none() {
                continue;
            }
            let hits: Vec<_> = hits.collect();
            // The arrangement the name says, else the setlist's (its first
            // row of the song).
            let pick = arrangement
                .as_ref()
                .and_then(|a| hits.iter().find(|r| r.2.as_ref() == Some(a)))
                .or_else(|| hits.first());
            if let Some(row) = pick {
                return Some(row.0.clone());
            }
        }
        None
    }
}

/// `Title (Arrangement)` as its parts.
fn split_arrangement(name: &str) -> (&str, Option<&str>) {
    name.strip_suffix(')')
        .and_then(|n| n.rsplit_once(" ("))
        .map_or((name, None), |(t, a)| (t.trim(), Some(a.trim())))
}

/// A name compared without case, apostrophes or punctuation, a number
/// spelled out: "7 Nation Army", "seven-nation-army" and "Seven Nation
/// Army" are one.
#[must_use]
pub fn key(name: &str) -> String {
    const NUMBERS: [&str; 11] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
    ];
    let words: Vec<String> = name
        .chars()
        .filter(|c| *c != '\'' && *c != '\u{2019}')
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(|w| {
            w.parse::<usize>()
                .ok()
                .and_then(|n| NUMBERS.get(n))
                .map_or_else(|| w.to_owned(), |s| (*s).to_owned())
        })
        .collect();
    words.join("-")
}

// ---------------------------------------------------------------------------
// Which project is up
// ---------------------------------------------------------------------------

/// One frame of a project's playhead, as the engine stamps it.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub project: String,
    pub seconds: f64,
    pub playing: bool,
    pub rate: f64,
}

#[derive(Debug, Clone, Copy)]
struct Heard {
    seconds: f64,
    playing: bool,
    rate: f64,
    at: Instant,
}

/// Where Studio is: the project followed and its playhead.
#[derive(Debug, Default)]
pub struct Follow {
    /// Engine project → the item it is (None: no song of the setlist).
    items: HashMap<String, Option<String>>,
    last: HashMap<String, Heard>,
    current: Option<String>,
}

/// What the engine plays, for the deck.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub item: String,
    pub seconds: f64,
    pub playing: bool,
}

impl Follow {
    /// Says which item a project is.
    pub fn name(&mut self, project: &str, item: Option<String>) {
        self.items.insert(project.to_owned(), item);
    }

    /// Forgets what each project is (the setlist changed).
    pub fn forget_names(&mut self) {
        self.items.clear();
    }

    /// Whether it knows what `project` is.
    #[must_use]
    pub fn knows(&self, project: &str) -> bool {
        self.items.contains_key(project)
    }

    /// One frame: the project followed is the one playing; while nothing
    /// plays, the one whose playhead last moved.
    pub fn observe(&mut self, frame: &Frame, now: Instant) {
        let is_song = self.items.get(&frame.project).is_some_and(Option::is_some);
        let moved = self.last.get(&frame.project).is_some_and(|h| {
            (h.seconds - frame.seconds).abs() > 1e-6 || h.playing != frame.playing
        });
        let current_plays = self
            .current
            .as_ref()
            .and_then(|c| self.last.get(c))
            .is_some_and(|h| h.playing);
        if is_song && (frame.playing || (moved && !current_plays) || self.current.is_none()) {
            self.current = Some(frame.project.clone());
        }
        self.last.insert(
            frame.project.clone(),
            Heard {
                seconds: frame.seconds,
                playing: frame.playing,
                rate: frame.rate,
                at: now,
            },
        );
    }

    /// What the deck should show at `now`: the item, its time (carried on
    /// at its rate, never more than a quarter second past the last frame)
    /// and whether it plays.
    #[must_use]
    pub fn target(&self, now: Instant) -> Option<Target> {
        let project = self.current.as_ref()?;
        let item = self.items.get(project)?.clone()?;
        let heard = self.last.get(project)?;
        let seconds = if heard.playing {
            let since = now.saturating_duration_since(heard.at).as_secs_f64();
            since.clamp(0.0, 0.25).mul_add(heard.rate, heard.seconds)
        } else {
            heard.seconds
        };
        Some(Target {
            item,
            seconds,
            playing: heard.playing,
        })
    }
}

/// What to ask the deck so it shows `want`, given what it shows (`have`,
/// at the same moment): the item, then the time, then play or pause.
#[derive(Debug, Clone, PartialEq)]
pub enum Move {
    Item {
        id: String,
        play: bool,
        seconds: f64,
    },
    Seek(f64),
    Play,
    Pause,
}

/// The moves that bring the deck (`have`: item, seconds, playing) to `want`.
#[must_use]
pub fn moves(have: Option<(&str, f64, bool)>, want: &Target) -> Vec<Move> {
    match have {
        Some((item, _, _)) if item != want.item => vec![Move::Item {
            id: want.item.clone(),
            play: want.playing,
            seconds: want.seconds,
        }],
        None => vec![Move::Item {
            id: want.item.clone(),
            play: want.playing,
            seconds: want.seconds,
        }],
        Some((_, seconds, playing)) => {
            let mut out = Vec::new();
            if (seconds - want.seconds).abs() > DRIFT {
                out.push(Move::Seek(want.seconds));
            }
            match (playing, want.playing) {
                (false, true) => out.push(Move::Play),
                (true, false) => out.push(Move::Pause),
                _ => {}
            }
            out
        }
    }
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

/// The newest running engine's socket: `$XDG_RUNTIME_DIR/fts/engine-<pid>.sock`
/// whose process is alive.
#[must_use]
pub fn find_engine() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)?;
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(runtime.join("fts"))
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let pid = name.strip_prefix("engine-")?.strip_suffix(".sock")?;
            Path::new("/proc").join(pid).is_dir().then_some(())?;
            Some((entry.metadata().ok()?.modified().ok()?, entry.path()))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().next().map(|(_, p)| p)
}

/// The follower running, once per process: what lets the engine go.
static RUNNING: OnceLock<Stopper> = OnceLock::new();

/// Lets the engine go cleanly.
#[derive(Clone)]
pub struct Stopper {
    stop: Arc<tokio::sync::Notify>,
    thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl Stopper {
    /// Disconnects and waits (at most two seconds) for it to be done.
    pub fn stop(&self) {
        self.stop.notify_one();
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(thread) = thread {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !thread.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

/// Starts following Studio's engine (the newest running, or the socket
/// `STUDIO_ENGINE_SOCKET` names), once per process, from a thread of its
/// own; it waits for an engine and connects again whenever one goes
/// away. `FTS_SETLISTS_ENGINE_CLOCK=off` leaves it off.
pub fn start() {
    if std::env::var("FTS_SETLISTS_ENGINE_CLOCK").is_ok_and(|v| v == "off") {
        return;
    }
    RUNNING.get_or_init(|| {
        let stop = Arc::new(tokio::sync::Notify::new());
        let socket = std::env::var_os("STUDIO_ENGINE_SOCKET").map(PathBuf::from);
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("setlists-engine-clock".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    tracing::warn!(
                        clock.outcome = "no_runtime",
                        "setlists: the engine clock could not start"
                    );
                    return;
                };
                runtime.block_on(follow_forever(socket, thread_stop));
            })
            .ok();
        Stopper {
            stop,
            thread: Arc::new(Mutex::new(thread)),
        }
    });
}

/// Lets the engine go, if it follows one (the app quitting).
pub fn stop() {
    if let Some(s) = RUNNING.get() {
        s.stop();
    }
}

async fn follow_forever(socket: Option<PathBuf>, stop: Arc<tokio::sync::Notify>) {
    let mut said_waiting = false;
    loop {
        let Some(path) = socket.clone().or_else(find_engine) else {
            if !said_waiting {
                tracing::debug!(
                    "setlists: no Studio engine running; the playhead keeps its own clock"
                );
                said_waiting = true;
            }
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(2)) => continue,
                () = stop.notified() => return,
            }
        };
        said_waiting = false;
        let outcome = follow(&path, &stop).await;
        crate::now::set_engine_drives(false);
        match outcome {
            Ok(true) => {
                tracing::info!(
                    clock.outcome = "let_go",
                    "setlists: let the Studio engine go"
                );
                return;
            }
            Ok(false) => tracing::info!(
                clock.outcome = "ended",
                "setlists: the engine's stream ended"
            ),
            Err(error) => {
                tracing::warn!(clock.outcome = "lost", %error, "setlists: lost the Studio engine")
            }
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(2)) => {}
            () = stop.notified() => return,
        }
    }
}

/// The DAW service lane's caller (what `studio::cli::connect` opens).
#[derive(Clone)]
struct DawLane {
    caller: vox::Caller,
}

impl vox::FromVoxLane for DawLane {
    const SERVICE_NAME: &'static str = "daw-cli";

    fn from_vox_lane(caller: vox::Caller, _connection: Option<vox::ConnectionHandle>) -> Self {
        Self { caller }
    }
}

/// The setlist the engine's songs are matched against: the one on the
/// playhead now (its queue), else the first setlist with a song of the
/// engine's (found by name as projects arrive).
#[derive(Default)]
struct Library {
    setlists: Vec<SetlistRow>,
    songs: Vec<SongRow>,
    /// The setlist matched, and its songs.
    chosen: Option<(usize, Songs)>,
    read_at: Option<Instant>,
}

impl Library {
    async fn refresh(&mut self) {
        let fresh = self
            .read_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(30));
        if fresh {
            return;
        }
        self.read_at = Some(Instant::now());
        if let (Ok(setlists), Ok(songs)) =
            (crate::host::setlists().await, crate::host::songs().await)
        {
            let chosen = self
                .chosen
                .as_ref()
                .and_then(|(i, _)| self.setlists.get(*i))
                .map(|s| s.path.clone());
            self.setlists = setlists;
            self.songs = songs;
            self.chosen = chosen
                .and_then(|p| self.setlists.iter().position(|s| s.path == p))
                .map(|i| (i, Songs::of(&self.setlists[i], &self.songs)));
        }
    }

    /// The item a project is: in the setlist chosen, else in the first
    /// setlist that has it (which is then chosen), preferring the
    /// setlist on the playhead.
    fn find(&mut self, playing: &[String], name: &str, path: &str) -> Option<String> {
        if let Some((_, songs)) = &self.chosen
            && let Some(hit) = songs.find(name, path)
        {
            return Some(hit);
        }
        let order = (0..self.setlists.len()).map(|i| {
            let songs = Songs::of(&self.setlists[i], &self.songs);
            let on_deck = songs.rows.iter().any(|r| playing.contains(&r.0));
            (i, songs, on_deck)
        });
        let mut found: Vec<(usize, Songs, bool)> = order
            .filter(|(_, songs, _)| songs.find(name, path).is_some())
            .collect();
        found.sort_by_key(|(_, _, on_deck)| !on_deck);
        let (i, songs, _) = found.into_iter().next()?;
        let hit = songs.find(name, path);
        self.chosen = Some((i, songs));
        hit
    }

    fn chosen(&self) -> Option<&SetlistRow> {
        self.chosen
            .as_ref()
            .and_then(|(i, _)| self.setlists.get(*i))
    }
}

/// Follows one engine until its stream ends (`Ok(false)`) or `stop` is
/// notified (`Ok(true)`, after a clean disconnect).
async fn follow(socket: &Path, stop: &tokio::sync::Notify) -> anyhow::Result<bool> {
    let stream = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::UnixStream::connect(socket),
    )
    .await
    .map_err(|_| anyhow::anyhow!("timed out connecting"))??;
    let connection = vox::initiator_on(vox_stream::StreamLink::unix(stream))
        .establish_connection()
        .await
        .map_err(|e| anyhow::anyhow!("handshake: {e:?}"))?;
    let lane = connection
        .open_lane::<DawLane>()
        .await
        .map_err(|e| anyhow::anyhow!("opening the daw lane: {e:?}"))?;
    let daw = daw_control::Daw::new(lane.caller);
    tracing::info!(
        clock.outcome = "following",
        "setlists: following the Studio engine's transport"
    );
    let mut library = Library::default();
    library.refresh().await;
    let mut state = Follow::default();
    let mut names: HashMap<String, (String, String)> = HashMap::new();
    let mut positions = daw.transport_sync().positions();
    let mut tick = tokio::time::interval(RECONCILE);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut loaded_for: Option<String> = None;
    loop {
        tokio::select! {
            next = positions.recv() => {
                let Some(frame) = next.map_err(|e| anyhow::anyhow!("the position stream: {e:?}"))? else {
                    break;
                };
                let frame = frame.get();
                let project = frame.project_guid.clone();
                if !state.knows(&project) {
                    let (name, path) = project_name(&daw, &project).await;
                    let deck = crate::now::state().await.map(|n| n.queue).unwrap_or_default();
                    let item = library.find(&deck, &name, &path);
                    tracing::debug!(
                        clock.project_matched = item.is_some(),
                        "setlists: an engine project named"
                    );
                    names.insert(project.clone(), (name, path));
                    state.name(&project, item);
                }
                state.observe(
                    &Frame {
                        project,
                        seconds: frame.playhead_seconds,
                        playing: frame.is_playing,
                        rate: if frame.playrate > 0.0 { frame.playrate } else { 1.0 },
                    },
                    Instant::now(),
                );
            }
            _ = tick.tick() => {
                let before = library.chosen().map(|s| s.path.clone());
                library.refresh().await;
                if library.chosen().map(|s| s.path.clone()) != before {
                    // Names again, against the setlist as it is now.
                    state.forget_names();
                    for (project, (name, path)) in &names {
                        let item = library.find(&[], name, path);
                        state.name(project, item);
                    }
                }
                let Some(want) = state.target(Instant::now()) else { continue };
                crate::now::set_engine_drives(true);
                // The setlist on the deck: the engine's song's.
                let chosen = library.chosen().cloned();
                if let Some(set) = &chosen
                    && loaded_for.as_deref() != Some(set.path.as_str())
                {
                    if let Err(error) = crate::now::load(set, Some(&want.item)).await {
                        tracing::warn!(clock.outcome = "queue_refused", %error, "setlists: the playhead refused the setlist");
                    }
                    loaded_for = Some(set.path.clone());
                }
                let deck = crate::now::state().await;
                let have = deck.as_ref().and_then(|n| {
                    n.item.as_deref().map(|i| (i, n.at(crate::now::now_ms()), n.playing))
                });
                for step in moves(have, &want) {
                    if let Err(error) = apply(&step).await {
                        tracing::debug!(%error, "setlists: a playhead move refused");
                        break;
                    }
                }
            }
            () = stop.notified() => {
                // The subscription first (dropping the stream cancels it
                // on the engine), then the connection, waited for.
                drop(positions);
                let _ = connection.shutdown();
                let _ = tokio::time::timeout(Duration::from_secs(1), connection.closed()).await;
                return Ok(true);
            }
        }
    }
    let _ = connection.shutdown();
    Ok(false)
}

async fn apply(step: &Move) -> Result<(), String> {
    use serde_json::json;
    match step {
        Move::Item { id, play, seconds } => {
            crate::host::run("PLAYHEAD_ITEM", json!({ "id": id, "play": play })).await?;
            crate::host::run("PLAYHEAD_SEEK", json!({ "t": seconds.max(0.0) })).await?;
        }
        Move::Seek(t) => {
            crate::host::run("PLAYHEAD_SEEK", json!({ "t": t.max(0.0) })).await?;
        }
        Move::Play => {
            crate::host::run("PLAYHEAD_PLAY", json!({})).await?;
        }
        Move::Pause => {
            crate::host::run("PLAYHEAD_PAUSE", json!({})).await?;
        }
    }
    Ok(())
}

async fn project_name(daw: &daw_control::Daw, project: &str) -> (String, String) {
    match daw.project(project).await {
        Ok(p) => match p.info().await {
            Ok(info) => (info.name, info.path),
            Err(_) => (String::new(), String::new()),
        },
        Err(_) => (String::new(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SongRef;

    fn row(title: &str, stem: &str) -> SongRow {
        SongRow {
            at: SongRef::new("JT Senior Recital", title, None),
            path: String::new(),
            key: None,
            key_end: None,
            key_outside: false,
            tempo: None,
            meter: None,
            artist: None,
            length: Some(100.0),
            categories: Vec::new(),
            arrangements: Vec::new(),
            recording: Some(format!(
                "Libraries/JT Senior Recital/{title}/Media/{stem}.ogg"
            )),
        }
    }

    fn recital() -> Songs {
        let lib = "JT Senior Recital";
        let set = SetlistRow {
            path: "Setlists/JT's Senior Recital.md".into(),
            title: "JT's Senior Recital".into(),
            date: None,
            songs: vec![
                SongRef::new(lib, "Seven Nation Army", None),
                SongRef::new(lib, "Not Too Much Longer", Some("Live")),
                SongRef::new(lib, "Know Me", Some("Extended")),
                SongRef::new(lib, "Heart Isn't Ready", None),
                SongRef::new(lib, "Fire", None),
            ],
            sets: Vec::new(),
            share: None,
        };
        let songs = vec![
            row("Seven Nation Army", "7-nation-army"),
            row("Not Too Much Longer", "not-too-much-longer"),
            row("Know Me", "know-me"),
            row("Heart Isn't Ready", "heart-isnt-ready"),
            row("Fire", "fire"),
        ];
        Songs::of(&set, &songs)
    }

    const LIB: &str = "Libraries/JT Senior Recital";

    #[test]
    fn the_engines_old_names_are_the_vaults_songs() {
        let s = recital();
        let songs = "/x/recital/songs";
        assert_eq!(
            s.find(
                "7 Nation Army",
                &format!("{songs}/7-nation-army/7 Nation Army.session")
            ),
            Some(format!("{LIB}/Seven Nation Army/Seven Nation Army.md"))
        );
        // The setlist plays the arrangement: the row decides.
        assert_eq!(
            s.find("Know Me", &format!("{songs}/know-me/Know Me.session")),
            Some(format!("{LIB}/Know Me/Know Me.md#Extended"))
        );
        assert_eq!(
            s.find(
                "Not Too Much Longer",
                &format!("{songs}/not-too-much-longer/Not Too Much Longer.session")
            ),
            Some(format!(
                "{LIB}/Not Too Much Longer/Not Too Much Longer.md#Live"
            ))
        );
        // `Title (Arrangement)`, apostrophes, the `.session` name.
        assert_eq!(
            s.find("Know Me (Extended).session", ""),
            Some(format!("{LIB}/Know Me/Know Me.md#Extended"))
        );
        assert_eq!(
            s.find("Heart Isnt Ready", ""),
            Some(format!("{LIB}/Heart Isn't Ready/Heart Isn't Ready.md"))
        );
        // By the folder (the split's slug) alone.
        assert_eq!(
            s.find("", &format!("{songs}/7-nation-army/Untitled.session")),
            Some(format!("{LIB}/Seven Nation Army/Seven Nation Army.md"))
        );
        assert_eq!(
            s.find("Berenstein", "/x/berenstein/Berenstein.session"),
            None
        );
    }

    #[test]
    fn keys_ignore_case_punctuation_and_spelled_numbers() {
        assert_eq!(key("7 Nation Army"), key("Seven Nation Army"));
        assert_eq!(key("seven-nation-army"), key("Seven Nation Army"));
        assert_eq!(key("She’s American"), "shes-american");
        assert_ne!(key("Fire"), key("Fire Away"));
    }

    fn frame(project: &str, seconds: f64, playing: bool) -> Frame {
        Frame {
            project: project.into(),
            seconds,
            playing,
            rate: 1.0,
        }
    }

    #[test]
    fn the_playing_song_is_followed_never_one_that_is_no_song() {
        let mut f = Follow::default();
        f.name("a", Some("A".into()));
        f.name("b", Some("B".into()));
        f.name("x", None);
        let t = Instant::now();
        f.observe(&frame("a", 1.0, false), t);
        f.observe(&frame("b", 0.0, false), t);
        assert_eq!(
            f.target(t).map(|w| w.item),
            Some("A".into()),
            "the first heard stays up"
        );
        f.observe(&frame("x", 3.0, true), t);
        assert_eq!(f.target(t).map(|w| w.item), Some("A".into()));
        f.observe(&frame("b", 10.0, true), t);
        let w = f.target(t + Duration::from_millis(100)).unwrap_or(Target {
            item: String::new(),
            seconds: 0.0,
            playing: false,
        });
        assert_eq!(w.item, "B");
        assert!((w.seconds - 10.1).abs() < 1e-9);
        // Another song located while B plays does not take it.
        f.observe(&frame("a", 50.0, false), t);
        assert_eq!(f.target(t).map(|w| w.item), Some("B".into()));
    }

    #[test]
    fn the_deck_is_moved_only_where_it_differs() {
        let want = Target {
            item: "B".into(),
            seconds: 10.0,
            playing: true,
        };
        assert_eq!(
            moves(Some(("A", 3.0, false)), &want),
            [Move::Item {
                id: "B".into(),
                play: true,
                seconds: 10.0
            }]
        );
        assert!(
            moves(Some(("B", 10.05, true)), &want).is_empty(),
            "within the drift"
        );
        assert_eq!(moves(Some(("B", 12.0, true)), &want), [Move::Seek(10.0)]);
        assert_eq!(moves(Some(("B", 10.0, false)), &want), [Move::Play]);
        let stopped = Target {
            playing: false,
            ..want
        };
        assert_eq!(moves(Some(("B", 10.0, true)), &stopped), [Move::Pause]);
    }
}
