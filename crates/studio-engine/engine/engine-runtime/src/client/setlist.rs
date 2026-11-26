//! The setlist: the songs a session holds, and which of them is up.
//!
//! One engine, several projects (`daw_standalone::sync::Standalone` keeps
//! them side by side, as REAPER keeps tabs), and one of them current. This
//! is what the window reads: the songs in order, where the playhead is in
//! the one playing, and the colour each is known by — the setlist tabs in
//! the top bar are that colour, filling as the song plays
//! ([`crate::client::shell::SongTabs`]).
//!
//! In **Remote** and **Cue** (`crate::client::audio_mode`) the songs are the
//! projects the driven system has open — REAPER's project tabs, in tab
//! order, the one it has current current ([`Setlist::attach`]). A song's
//! `project` is then the remote's guid, picking a tab selects that project
//! on the remote (`crate::client::open::switch_song`), and a tab picked in REAPER
//! itself is followed here (`crate::client::collab_bar::use_follow_song`). The set
//! is what REAPER has open, not a `.setlist` file: to play a set remotely,
//! open its songs as tabs in REAPER.
//!
//! A song's SPAN is the SONG region the chart stamps (`prepare`), which is
//! the part that is the song rather than the count-in before it; without
//! one it is everything the project holds. The span is what a progress
//! reads against, and what tells Live mode a song has ended.

use crate::client::song_model::StudioSession;

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

#[cfg(feature = "native")]
impl Setlist {
    /// The engine's set as a window shows it (`frame`, from its link —
    /// `crate::client::view_client`): each song not shown already (`have`) read back
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
        frame: &crate::client::view::SetFrame,
        have: Vec<Song>,
    ) -> eyre::Result<(Self, usize)> {
        let rt = crate::client::open::engine_runtime()?;
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
    /// knows the song by — `crate::client::collab` keys songs by it), else the
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
            .filter(|p| p.is_dir() && !crate::client::open::is_session(p))
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
                if entry.is_dir() && !crate::client::open::is_session(&entry) {
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
