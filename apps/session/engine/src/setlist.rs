//! The setlist: the songs a session holds, and which of them is up.
//!
//! One engine, several projects (`daw_standalone::sync::Standalone` keeps
//! them side by side, as REAPER keeps tabs), and one of them current. This
//! is what the window reads: the songs in order, where the playhead is in
//! the one playing, and the colour each is known by — the setlist tabs in
//! the top bar are that colour, filling as the song plays
//! ([`crate::shell::SongTabs`]).
//!
//! In **Remote** and **Cue** (`crate::audio_mode`) the songs are the
//! projects the driven system has open — REAPER's project tabs, in tab
//! order, the one it has current current ([`Setlist::attach`]). A song's
//! `project` is then the remote's guid, picking a tab selects that project
//! on the remote (`crate::open::switch_song`), and a tab picked in REAPER
//! itself is followed here (`crate::collab_bar::use_follow_song`). The set
//! is what REAPER has open, not a `.setlist` file: to play a set remotely,
//! open its songs as tabs in REAPER.
//!
//! A song's SPAN is the SONG region the chart stamps (`prepare`), which is
//! the part that is the song rather than the count-in before it; without
//! one it is everything the project holds. The span is what a progress
//! reads against, and what tells Live mode a song has ended.

use crate::song_model::StudioSession;

/// The colours offered when a song's colour is set by hand — distinct at a
/// glance in a row of tabs rather than a smooth ramp: the question a
/// setlist tab answers is "which song is that", and two greens apart by a
/// shade answer it slowly.
pub const COLORS: [&str; 8] = [
    "#3aa0ff", "#f0883e", "#4ac26b", "#d2a8ff", "#f778ba", "#e3b341", "#56d4dd", "#ff7b72",
];

/// A song's colour from its title, when nobody has set one: the same title
/// is always the same colour, from one run to the next and on every
/// machine, so a song is recognised by its colour before its name is read.
///
/// The hue comes from a hash of the title (FNV-1a — stable, unlike std's
/// hasher, which is seeded per process); saturation and lightness are
/// fixed where a tab's white title stays legible on the colour.
#[must_use]
pub fn title_color(title: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in title.trim().to_lowercase().bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    #[expect(clippy::cast_precision_loss, reason = "a hue out of 360")]
    let hue = (hash % 360) as f64;
    hsl_hex(hue, 0.62, 0.52)
}

/// `hsl(h, s, l)` as `#rrggbb` — Blitz reads hex everywhere.
fn hsl_hex(hue: f64, saturation: f64, lightness: f64) -> String {
    let chroma = (1.0 - (2.0f64.mul_add(lightness, -1.0)).abs()) * saturation;
    let h = hue / 60.0;
    let x = chroma * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a channel in 0..=255"
    )]
    let byte = |v: f64| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

/// One song in the setlist.
#[derive(Clone)]
pub struct Song {
    pub name: String,
    /// The project's guid in the engine — what `set_current_project` takes
    /// — or, in Remote and Cue, on the system this window drives.
    pub project: String,
    pub session: StudioSession,
    /// The song's own span in project seconds: the SONG region, or
    /// everything there is.
    pub span: (f64, f64),
    /// The colour it is known by, as CSS: the one set on its SONG region,
    /// else [`title_color`].
    pub color: String,
    /// Where its playhead was when another song was picked — so its tab
    /// still shows how far through it the set got.
    pub left_at: f64,
    /// Its saved `.session`, which a change made here (its colour) is
    /// written back to. `None` for a song with none.
    pub saved: Option<std::path::PathBuf>,
}

impl PartialEq for Song {
    fn eq(&self, other: &Self) -> bool {
        self.project == other.project && self.name == other.name && self.span == other.span
    }
}

impl Song {
    /// A song from what was opened.
    #[must_use]
    pub fn of(name: String, project: String, session: StudioSession) -> Self {
        let sections = &session.project.sections;
        let song = sections.iter().find(|s| s.lane == 0);
        let span = song.map(|s| (s.start, s.end)).unwrap_or_else(|| {
            let start = sections
                .iter()
                .map(|s| s.start)
                .fold(f64::INFINITY, f64::min);
            let end = sections.iter().map(|s| s.end).fold(0.0, f64::max);
            if end > start {
                (start, end)
            } else {
                (0.0, 0.0)
            }
        });
        let color = song
            .and_then(|s| s.color.clone())
            .unwrap_or_else(|| title_color(&name));
        Self {
            name,
            project,
            session,
            span,
            color,
            left_at: 0.0,
            saved: None,
        }
    }

    /// What the song is shown as: its SONG region's name ("Unbreakable -
    /// Crescendum", the one prepare gives it) when that names this song,
    /// else its [`Self::name`] — the file's, or the set's title for it,
    /// which stays what the song is KNOWN by (collaboration and the modes
    /// it was worked in are kept under it). A region naming something else
    /// (a set's name stamped into every song of it) is not the song's.
    #[must_use]
    pub fn title(&self) -> String {
        self.session
            .project
            .sections
            .iter()
            .find(|s| s.lane == 0)
            .map(|s| s.name.trim().to_string())
            .filter(|t| names_song(t, &self.name))
            .unwrap_or_else(|| self.name.clone())
    }

    /// How far through the song `at` seconds is, 0 before it and 1 at its
    /// end.
    #[must_use]
    pub fn progress(&self, at: f64) -> f64 {
        let (from, to) = self.span;
        if to <= from {
            return 0.0;
        }
        ((at - from) / (to - from)).clamp(0.0, 1.0)
    }

    /// Whether the playhead has run past the song's end — what Live mode
    /// reads to start the next one.
    #[must_use]
    pub fn ended(&self, at: f64) -> bool {
        self.span.1 > self.span.0 && at >= self.span.1
    }
}

/// Whether `region` ("Washed - Elevation Rhythm") names the song known
/// as `name` (`Washed`, `washed.current`, `Washed - Elevation Rhythm`):
/// their titles — before the ` - ` an artist follows — are the same
/// letters and digits, case, spacing and punctuation aside.
fn names_song(region: &str, name: &str) -> bool {
    let title = |text: &str| -> String {
        let text = [".current", ".organized", ".raw"]
            .iter()
            .find_map(|suffix| text.trim().strip_suffix(suffix))
            .unwrap_or(text);
        let text = text.split('[').next().unwrap_or(text);
        text.split(" - ")
            .next()
            .unwrap_or(text)
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let region = title(region);
    !region.is_empty() && region == title(name)
}

/// The songs, in order, and which one is up.
#[derive(Clone, Default, PartialEq)]
pub struct Setlist {
    pub songs: Vec<Song>,
    /// Which song is current — an index into `songs`, past the end when
    /// the list is empty.
    pub at: usize,
    /// Songs of the set still on their way, by title, in set order — a page
    /// shows its first song while the rest open behind it; each moves into
    /// `songs` as it arrives. Their tabs show them coming.
    pub pending: Vec<String>,
    /// The set's order, by title, when its songs arrive out of it (they
    /// open at once, and finish when they finish): each takes its place.
    /// Empty: arrivals go at the end.
    pub order: Vec<String>,
}

impl Setlist {
    #[must_use]
    pub fn of(songs: Vec<Song>) -> Self {
        Self {
            songs,
            at: 0,
            pending: Vec::new(),
            order: Vec::new(),
        }
    }

    #[must_use]
    pub fn current(&self) -> Option<&Song> {
        self.songs.get(self.at)
    }

    /// The song after the current one, if the set goes on.
    #[must_use]
    pub fn next(&self) -> Option<usize> {
        (self.at + 1 < self.songs.len()).then_some(self.at + 1)
    }

    /// Make `to` the current song, remembering where the one it replaces
    /// had got to (`at`, its playhead). Returns the song now current, or
    /// `None` when `to` is out of range or already current.
    pub fn pick(&mut self, to: usize, at: f64) -> Option<&Song> {
        if to == self.at || to >= self.songs.len() {
            return None;
        }
        if let Some(leaving) = self.songs.get_mut(self.at) {
            leaving.left_at = at;
        }
        self.at = to;
        self.songs.get(to)
    }

    /// How far through `song` the set is: the playhead in the current one,
    /// where it was left in the others.
    #[must_use]
    pub fn progress_of(&self, song: usize, playhead: f64) -> f64 {
        self.songs.get(song).map_or(0.0, |s| {
            s.progress(if song == self.at { playhead } else { s.left_at })
        })
    }

    /// Move `song` to `to`, carrying the current selection with it so the
    /// song playing stays the song playing.
    pub fn reorder(&mut self, song: usize, to: usize) {
        if song >= self.songs.len() || to >= self.songs.len() || song == to {
            return;
        }
        let current = self.songs.get(self.at).cloned();
        let moved = self.songs.remove(song);
        self.songs.insert(to, moved);
        if let Some(current) = current
            && let Some(found) = self.songs.iter().position(|s| *s == current)
        {
            self.at = found;
        }
    }

    /// Drop a song. The selection follows the song that was playing, or
    /// stays in range.
    pub fn remove(&mut self, song: usize) {
        if song >= self.songs.len() {
            return;
        }
        let current = self.songs.get(self.at).cloned();
        self.songs.remove(song);
        self.at = current
            .filter(|c| self.songs.iter().any(|s| s == c))
            .and_then(|c| self.songs.iter().position(|s| *s == c))
            .unwrap_or_else(|| self.at.min(self.songs.len().saturating_sub(1)));
    }
}

#[cfg(feature = "native")]
impl Song {
    /// The song opened from `path` into the engine as `project`: named by
    /// `title`, or its file's; saved into its `.session` (a project file,
    /// `crate::project_file`), when it has one.
    fn opened(
        path: &std::path::Path,
        title: Option<String>,
        project: String,
        session: StudioSession,
    ) -> Self {
        let name = title.unwrap_or_else(|| {
            path.file_stem()
                .map_or_else(|| path.to_string_lossy(), |s| s.to_string_lossy())
                .into_owned()
        });
        let mut song = Self::of(name, project, session);
        song.saved = Some(if crate::open::is_session(path) {
            path.to_path_buf()
        } else {
            path.with_extension("session")
        })
        .filter(|saved| saved.exists());
        song
    }
}

/// A song of the set arriving after the set opened — the songs behind the
/// first open one by one while it is on screen.
pub enum Arrival {
    Song(Song),
    /// A song that could not be opened: its tab goes.
    Failed(String),
}

impl Setlist {
    /// `arrival` into the set: its song in its place, out of the pending.
    pub fn arrive(&mut self, arrival: Arrival) {
        match arrival {
            Arrival::Song(song) => {
                if let Some(at) = self.pending.iter().position(|t| *t == song.name) {
                    self.pending.remove(at);
                }
                let here: Vec<&str> = self.songs.iter().map(|s| s.name.as_str()).collect();
                let to = slot(&self.order, &here, &song.name);
                // The song on screen stays the song on screen.
                if to <= self.at && self.at < self.songs.len() {
                    self.at += 1;
                }
                self.songs.insert(to, song);
            }
            Arrival::Failed(title) => self.pending.retain(|t| *t != title),
        }
    }
}

/// Songs opening behind the one on screen, right now: the pictures wait
/// for none to be ([`pictures_later`]).
#[cfg(feature = "native")]
static OPENING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(feature = "native")]
impl Opening {
    /// One more song opening, until the guard drops.
    fn counted() -> OpeningGuard {
        OPENING.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        OpeningGuard
    }
}

/// A song opening ([`OPENING`]), counted while it lives.
#[cfg(feature = "native")]
struct OpeningGuard;

#[cfg(feature = "native")]
impl Drop for OpeningGuard {
    fn drop(&mut self) {
        OPENING.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

/// The songs whose pictures are still to read, oldest first.
#[cfg(feature = "native")]
static PICTURES: std::sync::Mutex<std::collections::VecDeque<(StudioSession, String)>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());

/// Forget the pictures still to read: a new set is opening, and the songs
/// it replaces are not worth reading.
#[cfg(feature = "native")]
pub fn forget_pictures() {
    if let Ok(mut queue) = PICTURES.lock() {
        queue.clear();
    }
}

/// Read `session`'s pictures (notes, waveforms) later: one song at a time,
/// on a thread of their own, and only while no song is opening — they go
/// through the engine the opens need, and read beside them they made every
/// song after the first wait seconds.
#[cfg(feature = "native")]
fn pictures_later(session: StudioSession, project: String) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static RUNNING: AtomicBool = AtomicBool::new(false);
    if let Ok(mut queue) = PICTURES.lock() {
        queue.push_back((session, project));
    }
    if RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("set-pictures".into())
        .spawn(|| {
            let Ok(runtime) = crate::open::engine_runtime() else {
                RUNNING.store(false, Ordering::Release);
                return;
            };
            loop {
                while OPENING.load(Ordering::Acquire) > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                let next = PICTURES.lock().ok().and_then(|mut q| q.pop_front());
                let Some((session, project)) = next else {
                    RUNNING.store(false, Ordering::Release);
                    // One queued as this let go: take it up again.
                    if PICTURES.lock().is_ok_and(|q| !q.is_empty())
                        && !RUNNING.swap(true, Ordering::AcqRel)
                    {
                        continue;
                    }
                    return;
                };
                runtime.block_on(session.fill_previews(&project));
            }
        });
    if spawned.is_err() {
        RUNNING.store(false, Ordering::Release);
        tracing::warn!("set: no thread to read the songs' pictures on");
    }
}

/// Where a song named `name` goes among the songs `here` (in set order),
/// by the set's `order`: after every song that comes before it, and any
/// the order does not name. At the end when the order does not name it.
fn slot(order: &[String], here: &[&str], name: &str) -> usize {
    let place = |name: &str| order.iter().position(|t| t == name);
    place(name).map_or(here.len(), |mine| {
        here.iter()
            .filter(|s| place(s).is_none_or(|p| p < mine))
            .count()
    })
}

/// A song for a set to open: its project — and, when it is streamed in
/// rather than on this disk, the stream its media plays from (the project
/// is then in the stream's mirror) and the title the set gives it.
#[cfg(feature = "native")]
pub struct Opening {
    pub path: std::path::PathBuf,
    pub title: Option<String>,
    pub streamed: Option<crate::song_stream::StreamedSong>,
}

#[cfg(feature = "native")]
impl From<std::path::PathBuf> for Opening {
    fn from(path: std::path::PathBuf) -> Self {
        Self {
            path,
            title: None,
            streamed: None,
        }
    }
}

#[cfg(feature = "native")]
impl Setlist {
    /// Open every song into the one engine — each prepared once and saved
    /// as `.session` if it was not already (see `StudioSession::open`) —
    /// and make the first current, with the audio.
    ///
    /// A song that fails to open is left out, with the reason logged: one
    /// broken song folder should not take the rest of the set with it.
    ///
    /// # Errors
    ///
    /// None of the songs opened.
    pub fn open(paths: &[std::path::PathBuf]) -> eyre::Result<Self> {
        Self::open_with(paths.iter().cloned().map(Opening::from).collect(), true)
    }

    /// [`Self::open`], for songs some of which are streamed in rather than
    /// on this disk (`crate::stream_set`): each opens the same way, its
    /// media deferred and then attached to its streams.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_streamed(songs: Vec<Opening>) -> eyre::Result<Self> {
        Self::open_with(songs, true)
    }

    /// Open one more song into the engine the set is in, behind the one on
    /// screen — it does not become current, and nothing on screen moves: a
    /// streamed set's songs after the first, arriving (see [`Arrival`]).
    ///
    /// # Errors
    ///
    /// The song did not open, or could not be read back.
    pub fn open_behind(opening: Opening) -> eyre::Result<Song> {
        let _opening = Opening::counted();
        let Opening {
            path,
            title,
            streamed,
        } = opening;
        let engine = crate::open::local_engine()
            .ok_or_else(|| eyre::eyre!("this window has no engine of its own to open into"))?;
        let media = if streamed.is_some() {
            crate::open::Media::Deferred
        } else {
            crate::open::Media::All
        };
        // Its stages on its `project.load` (`stream_set`'s).
        let load = tracing::Span::current();
        let mut stages = crate::stages::Stages::start();
        let prepare = crate::prepare::Prepare::for_song(&path);
        let (opened, plan) = crate::open::open_song_into_with(engine, &path, &prepare, media)?;
        load.record("open.engine_ms", stages.mark("open.engine_ms"));
        let guid = opened.project_guid.clone();
        // A voice of the set's engine now: it can sound the moment it is
        // picked, or while another is on screen.
        crate::open::sound(&guid);
        if let Some(streamed) = streamed {
            crate::stream_set::stream(streamed, &guid);
        } else if let Some(folder) = path.parent() {
            // A song downloaded by its reference plays it from disk.
            crate::reference_play::hear_local(&guid, folder);
        }
        // A project file (`Song.session`) opens through its doc: it has no
        // text, and its track kinds are read from the engine instead.
        let rpp_text = if crate::project_file::is_project_file(&plan.open) {
            String::new()
        } else {
            crate::open::project_text(&plan.open)?.text
        };
        let chart_text = prepare
            .chart
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok());
        load.record("open.media_ms", stages.mark("open.media_ms"));
        let mut session = crate::open::engine_runtime()?.block_on(StudioSession::read_project(
            &guid,
            &opened.name,
            &rpp_text,
            chart_text.as_deref(),
            false,
        ))?;
        load.record("open.read_ms", stages.mark("open.read_ms"));
        load.record("song.tracks", session.project.0.tracks.len());
        session.chart_file = prepare.chart.clone();
        crate::collab::song_opened(&guid);
        // Its waveforms behind it: the song is in the set now, its
        // pictures drawn as they are read.
        pictures_later(session.clone(), guid.clone());
        Ok(Song::opened(&path, title, guid, session))
    }

    /// [`Self::open`], with no audio device — for a test, or anything that
    /// looks at a set without playing it.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_silent(paths: &[std::path::PathBuf]) -> eyre::Result<Self> {
        Self::open_with(paths.iter().cloned().map(Opening::from).collect(), false)
    }

    fn open_with(openings: Vec<Opening>, audio: bool) -> eyre::Result<Self> {
        let count = openings.len();
        // On the set's `set.open` (`studio_engine::set::open_with`), when
        // this is one; each song its own `project.load` under it.
        let set = tracing::Span::current();
        set.record("set.songs", count);
        let mut songs = Vec::new();
        for (index, opening) in openings.into_iter().enumerate() {
            if let Some(song) = Self::load(opening, index, songs.is_empty().then_some(audio)) {
                songs.push(song);
            }
        }
        set.record("set.opened", songs.len());
        let first = songs
            .first()
            .ok_or_else(|| eyre::eyre!("none of the {count} songs opened"))?;
        crate::open::switch_song(&first.project);
        Ok(Self::of(songs))
    }

    /// One song of a set, loaded: opened in the engine (`song.open`) and
    /// its media attached — streamed, or played from disk — so it can play.
    /// `first` is `Some(audio)` for the song that stands the engine up.
    /// `None` when it did not open (the set goes on without it).
    fn load(opening: Opening, index: usize, first: Option<bool>) -> Option<Song> {
        let Opening {
            path,
            title,
            streamed,
        } = opening;
        let load = tracing::info_span!(
            "project.load",
            song.index = index,
            song.name = title.as_deref().unwrap_or_default(),
            song.media = if streamed.is_some() {
                "streamed"
            } else {
                "disk"
            },
            song.tracks = tracing::field::Empty,
            song.items = tracing::field::Empty,
        );
        let _load = load.enter();
        let prepare = crate::prepare::Prepare::for_song(&path);
        let media = if streamed.is_some() {
            crate::open::Media::Deferred
        } else {
            crate::open::Media::All
        };
        match StudioSession::open_first_as(&path, &prepare, first, media) {
            Ok((session, guid)) => {
                load.record("song.tracks", session.project.0.tracks.len());
                load.record(
                    "song.items",
                    session.project.0.items.values().flatten().count(),
                );
                if let Some(streamed) = streamed {
                    crate::stream_set::stream(streamed, &guid);
                } else if let Some(folder) = path.parent() {
                    // A song downloaded by its reference plays it from disk.
                    crate::reference_play::hear_local(&guid, folder);
                }
                Some(Song::opened(&path, title, guid, session))
            }
            Err(e) => {
                tracing::error!(
                    setlist.song = %path.display(),
                    error = %e,
                    "a song did not open; the set goes on without it"
                );
                None
            }
        }
    }

    /// The engine's set as a window shows it (`frame`, from its link —
    /// `crate::view_client`): each song not shown already (`have`) read back
    /// through the facade by its project, without selecting it — the engine
    /// may be playing one of them — in the engine's order, its current one
    /// current; its track kinds and chart asked of the engine. With how
    /// many songs were read.
    ///
    /// # Errors
    ///
    /// No song could be read.
    #[cfg(feature = "native")]
    pub fn from_link(
        frame: &crate::view::SetFrame,
        have: Vec<Song>,
    ) -> eyre::Result<(Self, usize)> {
        let rt = crate::open::engine_runtime()?;
        let mut songs = Vec::new();
        let mut read = 0;
        for (title, project) in &frame.songs {
            if let Some(song) = have.iter().find(|s| &s.project == project) {
                songs.push(song.clone());
                continue;
            }
            match rt.block_on(StudioSession::read_linked(project, title)) {
                Ok(session) => {
                    read += 1;
                    songs.push(Song::of(title.clone(), project.clone(), session));
                }
                Err(e) => {
                    tracing::warn!(song.project = %project, error = %e, "a song of the engine's set could not be read; the set goes on without it");
                }
            }
        }
        if songs.is_empty() {
            eyre::bail!(
                "none of the engine's {} songs could be read",
                frame.songs.len()
            );
        }
        let at = frame
            .current
            .and_then(|i| frame.songs.get(i as usize))
            .and_then(|(_, p)| songs.iter().position(|s| &s.project == p))
            .unwrap_or(0);
        let mut setlist = Self::of(songs);
        setlist.at = at;
        setlist.pending.clone_from(&frame.arriving);
        Ok((setlist, read))
    }

    /// The set a Remote or Cue window drives: attach to `target`, and make
    /// a song of every project it has open, in its order, its current one
    /// current.
    ///
    /// Each project is read the way an opened song is
    /// (`StudioSession::read_current`) — which reads the CURRENT project, so
    /// the remote's projects are selected one at a time while they are read
    /// and the one that was current is selected again at the end. A project
    /// saved on this machine gets its track kinds and chart from beside it;
    /// one that is not (another machine's — any reached over the network —
    /// or never saved) opens without.
    ///
    /// # Errors
    ///
    /// The attach failed, the remote's projects could not be listed, or
    /// none could be read.
    pub fn attach(target: &crate::open::RemoteTarget) -> eyre::Result<Self> {
        let attached = crate::open::attach(target)?;
        // A project's path means this machine's disk only when the system
        // driven is on it: a local REAPER over its socket. A Session engine
        // or a REAPER's bridge is reached over the network, so its paths
        // are another machine's — read here they are at best missing and at
        // worst a folder the OS guards (a phone simulator on a Mac stalls on
        // a privacy prompt for its owner's Downloads).
        let local = matches!(
            target,
            crate::open::RemoteTarget::Reaper { .. } | crate::open::RemoteTarget::Engine { .. }
        );
        let listed = crate::open::facade_blocking("list projects", |daw| async move {
            let mut out = Vec::new();
            for project in daw.projects().await? {
                let info = project.info().await?;
                out.push(RemoteProject {
                    guid: info.guid,
                    name: info.name,
                    path: Some(std::path::PathBuf::from(info.path))
                        .filter(|p| local && !p.as_os_str().is_empty()),
                });
            }
            Ok(out)
        })?;
        let (projects, _) = remote_songs(listed, &attached.project_guid);
        let mut selected = attached.project_guid.clone();
        let mut songs = Vec::new();
        for project in projects {
            if project.guid != selected {
                let guid = project.guid.clone();
                if let Err(e) = crate::open::facade_blocking("select project", |daw| async move {
                    daw.select_project(guid).await.map(|_| ())
                }) {
                    tracing::error!(error = %e, "a remote project could not be selected to read; the set goes on without it");
                    continue;
                }
                selected.clone_from(&project.guid);
            }
            let chart = project
                .path
                .as_deref()
                .and_then(crate::prepare::chart_beside);
            match StudioSession::read_current(project.path.as_deref(), chart) {
                Ok(session) => songs.push(Song::of(project.song_name(), project.guid, session)),
                Err(e) => {
                    tracing::error!(error = %e, "a remote project could not be read; the set goes on without it")
                }
            }
        }
        if selected != attached.project_guid {
            let guid = attached.project_guid.clone();
            if let Err(e) = crate::open::facade_blocking("select project", |daw| async move {
                daw.select_project(guid).await.map(|_| ())
            }) {
                tracing::warn!(error = %e, "the remote's current project could not be selected again");
            }
        }
        if songs.is_empty() {
            eyre::bail!("none of the remote's projects could be read");
        }
        let at = songs
            .iter()
            .position(|s| s.project == attached.project_guid)
            .unwrap_or(0);
        crate::open::switch_song(&songs[at].project);
        Ok(Self {
            songs,
            at,
            pending: Vec::new(),
            order: Vec::new(),
        })
    }

    /// Give `index` a colour by hand — or, with `None`, give it back the
    /// colour its title makes. Set on its SONG region, where REAPER shows
    /// it too, and saved into its `.session`, so it is the song's colour
    /// from now on and not this window's.
    pub fn recolor(&mut self, index: usize, color: Option<String>) {
        let Some(song) = self.songs.get_mut(index) else {
            return;
        };
        let rgb = color.as_deref().and_then(hex_rgb);
        song.color = color
            .filter(|_| rgb.is_some())
            .unwrap_or_else(|| title_color(&song.name));
        crate::open::set_song_color(&song.project, rgb.unwrap_or(0), song.saved.as_deref());
    }
}

/// A project the driven system has open, as a Remote setlist sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteProject {
    pub guid: String,
    /// The remote's name for it (REAPER's: the file name, or empty).
    pub name: String,
    /// Where it is saved, when it is.
    pub path: Option<std::path::PathBuf>,
}

impl RemoteProject {
    /// The song's name: the file's stem (`Washed`, the name every machine
    /// knows the song by — `crate::collab` keys songs by it), else the
    /// remote's own name without an extension, else `Untitled`.
    #[must_use]
    pub fn song_name(&self) -> String {
        let stem = |p: &std::path::Path| p.file_stem().map(|s| s.to_string_lossy().into_owned());
        self.path
            .as_deref()
            .and_then(stem)
            .or_else(|| stem(std::path::Path::new(self.name.trim())))
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Untitled".to_owned())
    }
}

/// The remote's projects as a set: all of them, in the remote's order
/// (REAPER's tabs), and the index of `current` — `0` when it is not among
/// them. Every tab is a song, saved or not: an empty tab on stage is
/// still what REAPER will play if it is picked.
#[must_use]
pub fn remote_songs(projects: Vec<RemoteProject>, current: &str) -> (Vec<RemoteProject>, usize) {
    let at = projects.iter().position(|p| p.guid == current).unwrap_or(0);
    (projects, at)
}

/// `#rrggbb` as `0xRRGGBB`.
#[cfg(feature = "native")]
fn hex_rgb(css: &str) -> Option<u32> {
    let hex = css.strip_prefix('#')?;
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// The songs a setlist names, in order.
///
/// A **folder** is a set of song folders (`Set/Song/Song.RPP`, …), taken
/// in name order; a **file** lists one song per line — a `.session`, a
/// `.RPP` or a song's folder — relative to the file, with blank lines and
/// `#` comments skipped. A song folder's `.session` is preferred to its
/// `.RPP`, which is what opening an `.RPP` does anyway.
///
/// # Errors
///
/// The folder or file could not be read, or it names no songs.
#[cfg(feature = "native")]
pub fn read_setlist(path: &std::path::Path) -> eyre::Result<Vec<std::path::PathBuf>> {
    let songs: Vec<std::path::PathBuf> = if path.is_dir() {
        let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(path)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| p.is_dir() && !crate::open::is_session(p))
            .collect();
        dirs.sort();
        dirs.into_iter().filter_map(|dir| song_in(&dir)).collect()
    } else {
        let base = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        std::fs::read_to_string(path)?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| {
                let entry = base.join(line);
                if entry.is_dir() && !crate::open::is_session(&entry) {
                    song_in(&entry)
                } else {
                    Some(entry)
                }
            })
            .collect()
    };
    if songs.is_empty() {
        eyre::bail!("{} names no songs", path.display());
    }
    Ok(songs)
}

/// The song in a song folder: its `.session`, else its one `.RPP`. `None`
/// for a folder that is not a song's (a setlist's folder of songs).
#[cfg(feature = "native")]
pub fn song_in(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    let with = |ext: &str| {
        entries
            .iter()
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
            .cloned()
    };
    with("session").or_else(|| with("rpp"))
}

/// A set this page may play but not change: what a guest of a live set
/// gets (the web app, joined by a live share link without signing in —
/// the public demo's playground among them). Provided as context by the
/// page that opened it; the set's editors read it and offer no reorder or
/// removal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadOnlySet(pub bool);

impl ReadOnlySet {
    /// Whether the set on screen is only viewed here.
    #[must_use]
    pub fn here() -> bool {
        dioxus::prelude::try_consume_context::<Self>().is_some_and(|r| r.0)
    }
}

#[cfg(test)]
mod tests {
    use super::names_song;

    #[test]
    fn songs_that_finish_out_of_order_take_their_places_in_the_set() {
        use super::slot;
        let order = ["A", "B", "C", "D"].map(str::to_owned);
        assert_eq!(slot(&order, &["A"], "D"), 1);
        assert_eq!(slot(&order, &["A", "D"], "B"), 1, "before D, after A");
        assert_eq!(slot(&order, &["B"], "A"), 0, "before the song on screen");
        assert_eq!(
            slot(&order, &["A", "B"], "Z"),
            2,
            "not in the order: at the end"
        );
        assert_eq!(slot(&[], &["A"], "B"), 1, "no order: at the end");
    }

    #[test]
    fn a_song_region_names_its_own_song() {
        assert!(names_song("Washed - Elevation Rhythm", "Washed"));
        assert!(names_song(
            "Unbreakable - Crescendum",
            "unbreakable.current"
        ));
        assert!(names_song("Always On Time - Pat Barrett", "Always_on_Time"));
        assert!(names_song(
            "Belief - John Mayer",
            "Belief - John Mayer [Battle SP26]"
        ));
        assert!(names_song("God, I'm Just Grateful", "God Im Just Grateful"));
    }

    #[test]
    fn a_set_name_stamped_into_a_song_is_not_its_title() {
        assert!(!names_song("Sr. Recital Showcase - Session", "Washed"));
        assert!(!names_song(
            "Sr. Recital Showcase - Session",
            "Holy Forever - Bethel"
        ));
        assert!(!names_song("", "Washed"));
    }
}
