//! What the app remembers while it runs: one context of signals, provided
//! at the root and read wherever it is needed.

use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;

use crate::data::{self, Song, Version};

/// A set, in the org that owns it.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SetRef {
    pub owner: String,
    pub id: String,
}

impl SetRef {
    pub fn new(owner: &str, id: &str) -> Self {
        SetRef {
            owner: owner.into(),
            id: id.into(),
        }
    }
}

/// A song of the set Chart and Lyrics follow.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct LiveSong {
    pub title: String,
    /// The key it is played in, and its own.
    pub key: Option<String>,
    pub own_key: Option<String>,
    pub tempo: Option<u32>,
}

/// A bottom sheet (or the top search sheet) over the page.
#[derive(Clone, PartialEq, Debug)]
pub enum Sheet {
    /// Open anything: the round button.
    Palette,
    /// The player, from the Now Playing bar.
    Player,
    /// A song's key in a set: the set and the entry's id.
    Key(SetRef, String),
    /// A song's arrangement in a set.
    Arr(SetRef, String),
    /// An entry of a set: everything about it (a phone's editor; a
    /// desktop's is the set page's inspector).
    Entry(SetRef, String),
    /// Add songs from the library, or an item, to a set.
    AddSongs(SetRef),
    /// Who plays a set, and what.
    Band(SetRef),
    /// A set's details: its title, date, times, place, who leads it.
    SetInfo(SetRef),
    /// Share a set with other orgs.
    ShareSet(SetRef),
    /// A set as plain text, to copy or print.
    Export(SetRef),
    /// A new set (for a recurring event, when one is given).
    NewSet(Option<String>),
    /// A new session: from scratch, from a folder of stems or from an RPP.
    NewSession,
    /// More for this set: arrangement, mixer, lighting.
    More,
    /// Book a rehearsal.
    Book,
    /// The library's filters.
    Filters,
    /// The org switcher.
    Orgs,
    /// A track's or a record's status ("rec:cr" or a track title).
    Status(String),
    /// A person in the org (their id; empty: a new one).
    Person(String),
    /// How the org plays a song: its owner and id.
    OrgSong(String, String),
}

/// A ⋯ menu from the page's bar.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Menu {
    SetMenu,
    Series,
    Occ,
    Project,
    Mine,
}

/// Studio's engine, as the window last heard of it (native playback goes
/// through `studio engine`; `crate::engine`).
#[derive(Clone, PartialEq, Debug, Default)]
#[cfg_attr(not(feature = "native"), allow(dead_code))]
pub enum EngineStatus {
    /// Not wanted yet (nothing has played), or a build without one.
    #[default]
    Off,
    /// Being found or started.
    Linking,
    /// Opening a song (its title).
    Opening(String),
    /// Linked to the engine of this pid.
    Ready(u32),
    /// Not there, and why: the playhead follows a timer.
    Failed(String),
}

impl EngineStatus {
    /// What the window says of it.
    pub fn line(&self) -> String {
        match self {
            Self::Off => "Engine: not started (it starts when a song plays)".into(),
            Self::Linking => "Engine: starting…".into(),
            Self::Opening(t) => format!("Engine: opening {t}…"),
            Self::Ready(pid) => format!("Engine: playing through studio engine (pid {pid})"),
            Self::Failed(why) => format!("Engine unavailable, the playhead follows a timer: {why}"),
        }
    }
}

/// What is playing, and from where.
#[derive(Clone, PartialEq, Debug)]
pub struct NowPlaying {
    pub list: Vec<Song>,
    pub from: String,
    pub i: usize,
    pub playing: bool,
    /// How far into the song, 0 to 1.
    pub pos: f64,
    /// The song's length in seconds, once its audio says (0 until then).
    pub dur: f64,
    /// Where the song's audio is (the web build plays its reference).
    pub source: Source,
    /// The song's sections, from its session.
    pub timeline: crate::session_file::Timeline,
    /// Where each song's session folder is, when the list came from a
    /// setlist file in Drive (by the list's index; empty otherwise).
    pub places: Vec<Option<Place>>,
}

/// A song's session folder in Drive, and its chart when one was found.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Place {
    pub space: String,
    pub location: String,
    /// The folder's path in its location.
    pub folder: String,
    pub chart: Option<crate::files::Hit>,
    /// The arrangement the set plays (`@Live` on its line): a branch of the
    /// song's session file. `None`: the song as recorded.
    pub arrangement: Option<String>,
}

/// The playing song's audio, as the player found it.
#[derive(Clone, PartialEq, Debug, Default)]
#[cfg_attr(not(feature = "web"), allow(dead_code))]
pub enum Source {
    /// Nothing looked for (a build without Drive plays nothing).
    #[default]
    None,
    Finding,
    /// Its reference, streamed from the computer that has it.
    Ready,
    /// Not found, and why.
    Missing(String),
}

impl NowPlaying {
    pub fn new(list: Vec<Song>, from: String, i: usize, playing: bool) -> Self {
        Self {
            list,
            from,
            i,
            playing,
            pos: 0.0,
            dur: 0.0,
            source: Source::None,
            timeline: Default::default(),
            places: Vec::new(),
        }
    }

    /// Songs from their session folders (a setlist file's).
    pub fn of_places(list: Vec<(Song, Place)>, from: String, i: usize, playing: bool) -> Self {
        let (songs, places): (Vec<Song>, Vec<Place>) = list.into_iter().unzip();
        Self {
            places: places.into_iter().map(Some).collect(),
            ..Self::new(songs, from, i, playing)
        }
    }

    /// The playing song's session folder, when the list knows it.
    pub fn place(&self) -> Option<&Place> {
        self.places.get(self.i).and_then(Option::as_ref)
    }

    /// Seconds into the song (its length from the built-in list until its
    /// audio has said).
    pub fn secs(&self) -> (f64, f64) {
        let len = if self.dur > 0.0 {
            self.dur
        } else {
            self.song().and_then(|s| s.len).unwrap_or(240) as f64
        };
        (len * self.pos, len)
    }

    /// The next song, from its start (false at the end of the list).
    pub fn next(&mut self) -> bool {
        if self.i + 1 >= self.list.len() {
            return false;
        }
        self.i += 1;
        self.pos = 0.0;
        self.dur = 0.0;
        self.timeline = Default::default();
        true
    }
}

impl NowPlaying {
    pub fn song(&self) -> Option<&Song> {
        self.list.get(self.i)
    }
}

/// What the filters look at in a song, from the built-in list or the library.
pub struct Facts<'a> {
    pub title: &'a str,
    pub artist: Option<&'a str>,
    /// Its key, as one of [`data::FIFTHS`] when it is one.
    pub key: Option<&'a str>,
    pub tempo: Option<u32>,
    pub tags: &'a [&'a str],
    pub energy: u8,
    pub ready: &'a str,
}

/// The library's filters.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Filters {
    pub q: String,
    pub tags: HashSet<&'static str>,
    pub keys: HashSet<&'static str>,
    pub energy: HashSet<u8>,
    pub tempo: HashSet<&'static str>,
    pub ready: HashSet<char>,
    pub rested: bool,
}

impl Filters {
    pub fn matches(&self, x: &data::LibSong) -> bool {
        let s = &x.song;
        self.admits(&Facts {
            title: s.title,
            artist: s.artist,
            key: data::key_of(s),
            tempo: s.tempo,
            tags: &x.tags,
            energy: x.energy,
            ready: s.ready,
        })
    }

    /// Whether a song with these facts passes every filter.
    pub fn admits(&self, f: &Facts) -> bool {
        let hay = format!("{} {}", f.title, f.artist.unwrap_or("")).to_lowercase();
        if !self.q.is_empty() && !hay.contains(&self.q.to_lowercase()) {
            return false;
        }
        if self.tags.iter().any(|t| !f.tags.contains(t)) {
            return false;
        }
        if !self.keys.is_empty() && !self.keys.contains(f.key.unwrap_or("?")) {
            return false;
        }
        if !self.energy.is_empty() && !self.energy.contains(&f.energy) {
            return false;
        }
        if !self.tempo.is_empty() {
            let t = f.tempo.unwrap_or(0);
            let ok = data::TEMPI.iter().any(|(n, lo, hi)| {
                self.tempo.contains(n) && f.tempo.is_some() && t >= *lo && t < *hi
            });
            if !ok {
                return false;
            }
        }
        if self.ready.iter().any(|k| !f.ready.contains(*k)) {
            return false;
        }
        if self.rested && data::weeks_since(f.title) < 4.0 {
            return false;
        }
        true
    }

    /// What is on, as chips that take themselves off: (which, label).
    pub fn chips(&self) -> Vec<(&'static str, String)> {
        let mut c = Vec::new();
        if !self.keys.is_empty() {
            let mut k: Vec<_> = self.keys.iter().copied().collect();
            k.sort();
            c.push(("keys", format!("Key {}", k.join(", "))));
        }
        if !self.energy.is_empty() {
            let mut e: Vec<_> = self.energy.iter().map(u8::to_string).collect();
            e.sort();
            c.push(("energy", format!("Energy {}", e.join(", "))));
        }
        if !self.tempo.is_empty() {
            let t: Vec<_> = data::TEMPI
                .iter()
                .filter(|x| self.tempo.contains(x.0))
                .map(|x| x.0)
                .collect();
            c.push(("tempo", t.join(", ")));
        }
        if !self.ready.is_empty() {
            let r: Vec<_> = data::READY
                .iter()
                .filter(|x| self.ready.contains(&x.0))
                .map(|x| x.1.to_lowercase())
                .collect();
            c.push(("ready", format!("Has {}", r.join(", "))));
        }
        if self.rested {
            c.push(("rested", "Not played in 4 weeks".into()));
        }
        c
    }

    pub fn clear(&mut self, which: &str) {
        match which {
            "keys" => self.keys.clear(),
            "energy" => self.energy.clear(),
            "tempo" => self.tempo.clear(),
            "ready" => self.ready.clear(),
            "rested" => self.rested = false,
            _ => {
                self.keys.clear();
                self.energy.clear();
                self.tempo.clear();
                self.ready.clear();
                self.rested = false;
            }
        }
    }
}

/// The rehearsal being booked.
#[derive(Clone, PartialEq, Debug)]
pub struct Booking {
    pub set: SetRef,
    /// The set's title and date, to say what is booked.
    pub title: String,
    pub date: Option<data::Day>,
    /// Days from today.
    pub day: usize,
    pub time: &'static str,
    pub room: &'static str,
}

/// Reviewing a track: which version, where the playhead is.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Review {
    pub title: String,
    pub v: Option<usize>,
    pub pos: f64,
    pub playing: bool,
    pub draft: String,
}

/// A set's chart and lyrics: the song, section and bar playing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Live {
    pub song: usize,
    pub sec: usize,
    pub bar: usize,
    pub playing: bool,
    pub click: bool,
    pub looping: bool,
}

/// The org a device without Drive starts in.
pub const FIRST_ORG: &str = "crossroads-church";

/// The app's state, every piece its own signal.
#[derive(Clone, Copy)]
pub struct AppState {
    pub sheet: Signal<Option<Sheet>>,
    pub menu: Signal<Option<Menu>>,
    pub toast: Signal<Option<(u32, String)>>,
    pub np: Signal<Option<NowPlaying>>,
    /// A jump the player asks for, in seconds (the audio takes it).
    pub seek: Signal<Option<f64>>,
    pub booking: Signal<Option<Booking>>,
    pub filters: Signal<Filters>,
    /// What the pages show: one org's (`Some`, the org Studio is in) or
    /// every org's at once (`None`, All orgs).
    pub org: Signal<Option<String>>,
    /// The org Studio is in: where its documents are (Drive's org on the
    /// web; picked here on a device without Drive).
    pub here: Signal<String>,
    /// The orgs this person is in (Drive's `/orgs`, or the known ones).
    pub orgs: Signal<Vec<String>>,
    /// Who is signed in (their account at the issuer), when anyone is.
    pub who: Signal<String>,
    pub every_q: Signal<String>,
    pub review: Signal<Review>,
    pub versions: Signal<HashMap<&'static str, Vec<Version>>>,
    pub stages: Signal<HashMap<String, usize>>,
    pub live: Signal<Live>,
    /// The set whose chart and lyrics `live` follows, and its songs.
    pub live_set: Signal<SetRef>,
    pub live_list: Signal<Vec<LiveSong>>,
    /// The entry the set page's inspector shows (a desktop's).
    pub picked: Signal<Option<String>>,
    /// Drive: the sets shared live, presence, the calendar (web only).
    pub sync: crate::live_sync::Sync,
    /// The setlist files in the org's Drive (`.setlist`), once found:
    /// the Setlists page and the sidebar's count read them.
    pub drive_sets: Signal<Option<Vec<crate::files::Hit>>>,
    /// The engine, natively; and whether its transport drives Now
    /// Playing (else the timer does).
    pub engine: Signal<EngineStatus>,
    pub engine_drives: Signal<bool>,
    /// The setlist file a page shows: its name and songs, for an action
    /// to pick one of them.
    pub shown_set: Signal<Option<(String, Vec<(Song, Place)>)>>,
    /// Where "back to sets" goes from the session: the page it was opened from.
    pub session_from: Signal<Option<crate::Route>>,
    /// A session asked for before its set was read (an action naming a
    /// setlist file): the song to start at and whether it plays.
    pub pending_session: Signal<Option<(Option<String>, bool)>>,
}

impl AppState {
    pub fn new() -> Self {
        let cya = data::occ("cya_2026-11-19")
            .map(|o| o.songs.clone())
            .unwrap_or_default();
        Self {
            sheet: Signal::new(None),
            menu: Signal::new(None),
            toast: Signal::new(None),
            // A browser plays real songs: an empty list until a set is
            // played (an empty one, not none: the shell goes blank
            // without its accessory at the start).
            np: Signal::new(Some(if cfg!(feature = "drive") {
                NowPlaying::new(Vec::new(), String::new(), 0, false)
            } else {
                NowPlaying::new(cya, "CYA · Thu Nov 19".into(), 0, false)
            })),
            seek: Signal::new(None),
            booking: Signal::new(None),
            filters: Signal::new(Filters::default()),
            org: Signal::new(Some(FIRST_ORG.to_string())),
            here: Signal::new(FIRST_ORG.to_string()),
            // With Drive, the node's (`live_sync::Attach`): a fresh
            // account sees its own orgs, never the made-up ones.
            orgs: Signal::new(if cfg!(feature = "drive") {
                Vec::new()
            } else {
                crate::orgs::KNOWN
                    .iter()
                    .map(|k| k.slug.to_string())
                    .collect()
            }),
            who: Signal::new(String::new()),
            every_q: Signal::new(String::new()),
            review: Signal::new(Review::default()),
            versions: Signal::new(data::all_versions()),
            stages: Signal::new(HashMap::new()),
            live: Signal::new(Live {
                song: 0,
                sec: 1,
                bar: 3,
                playing: false,
                click: false,
                looping: false,
            }),
            live_set: Signal::new(SetRef::default()),
            live_list: Signal::new(Vec::new()),
            picked: Signal::new(None),
            sync: crate::live_sync::Sync::new(),
            drive_sets: Signal::new(None),
            engine: Signal::new(EngineStatus::Off),
            engine_drives: Signal::new(false),
            shown_set: Signal::new(None),
            session_from: Signal::new(None),
            pending_session: Signal::new(None),
        }
    }

    /// Say what a tap did, for a moment.
    pub fn say(mut self, msg: impl Into<String>) {
        let n = self.toast.peek().as_ref().map_or(0, |t| t.0) + 1;
        self.toast.set(Some((n, msg.into())));
    }

    pub fn open(mut self, sheet: Sheet) {
        self.menu.set(None);
        self.sheet.set(Some(sheet));
    }

    /// Follow a set's chart and lyrics, from its first song when it is a new one.
    pub fn enter_set(mut self, set: &SetRef) {
        if *self.live_set.peek() != *set {
            self.live_set.set(set.clone());
            self.picked.set(None);
            self.live.with_mut(|l| {
                l.song = 0;
                l.sec = 1;
                l.bar = 3;
            });
        }
    }

    /// Go to an org: on the web every app on this address follows (Drive
    /// reloads into it); without Drive, here.
    pub fn switch_org(mut self, org: &str) {
        if *self.here.peek() == org {
            self.org.set(Some(org.to_string()));
            return;
        }
        if !self.sync.switch_org(org) {
            self.here.set(org.to_string());
            self.org.set(Some(org.to_string()));
        }
    }

    pub fn close(mut self) {
        self.sheet.set(None);
    }

    /// Play a set's songs in order, from `first` (or its first song): the
    /// library's songs of those titles.
    pub fn play_titles(self, titles: &[String], from: String, first: Option<&str>) {
        let list: Vec<Song> = titles
            .iter()
            .filter_map(|t| {
                data::LIB
                    .iter()
                    .find(|l| l.song.title == t.as_str())
                    .map(|l| l.song)
            })
            .collect();
        if let Some(start) = first.or(list.first().map(|s| s.title)).map(str::to_string) {
            self.play(list, from, &start);
        }
    }

    /// Show song `i` of a setlist file's songs, playing or not: what is
    /// queued keeps playing when it is the same song of the same list.
    pub fn select_places(mut self, list: Vec<(Song, Place)>, from: String, i: usize, play: bool) {
        let same = self
            .np
            .peek()
            .as_ref()
            .is_some_and(|n| n.from == from && n.i == i && n.list.len() == list.len());
        if same {
            if play {
                self.np.with_mut(|n| {
                    if let Some(n) = n {
                        n.playing = true;
                    }
                });
            }
            return;
        }
        self.np
            .set(Some(NowPlaying::of_places(list, from, i, play)));
    }

    /// Start playing a list at a song.
    pub fn play(mut self, list: Vec<Song>, from: String, title: &str) {
        let i = list.iter().position(|x| x.title == title).unwrap_or(0);
        self.np.set(Some(NowPlaying::new(list, from, i, true)));
    }

    /// Move a playing chart on by a bar: through the section (again, when
    /// looping), then the next section and the next song.
    pub fn step_bar(self) {
        let mut live = self.live;
        let n = self.live_list.peek().len();
        live.with_mut(|l| {
            l.bar += 1;
            if l.bar < data::BARS[l.sec] {
                return;
            }
            l.bar = 0;
            if l.looping {
                return;
            }
            if l.sec + 1 < data::SECS.len() {
                l.sec += 1;
            } else if l.song + 1 < n {
                l.song += 1;
                l.sec = 0;
            } else {
                l.playing = false;
            }
        });
    }

    /// How long a bar of the chart's song lasts (four beats).
    pub fn bar_ms(&self) -> u64 {
        let tempo = self
            .live_list
            .peek()
            .get(self.live.peek().song)
            .and_then(|s| s.tempo)
            .unwrap_or(120)
            .max(40);
        240_000 / tempo as u64
    }

    /// A track's stage, after any change.
    pub fn stage_of(&self, t: &data::Track) -> usize {
        self.stages.read().get(t.title).copied().unwrap_or(t.stage)
    }

    /// A record's status, after any change.
    pub fn status_of(&self, a: &data::Album) -> usize {
        self.stages
            .read()
            .get(&format!("rec:{}", a.id))
            .copied()
            .unwrap_or(a.status)
    }

    /// Open comments on a track: its versions' and its chart's and lyrics'.
    pub fn open_on(&self, title: &str) -> usize {
        let audio = self.versions.read().get(title).map_or(0, |vs| {
            vs.iter()
                .map(|v| v.comments.iter().filter(|c| c.open).count())
                .sum()
        });
        audio + data::other_open(title)
    }

    pub fn latest(&self, title: &str) -> Option<Version> {
        self.versions
            .read()
            .get(title)
            .and_then(|v| v.last().cloned())
    }
}

pub fn use_app() -> AppState {
    use_context::<AppState>()
}

/// Wait a while, on any platform.
pub async fn sleep_ms(ms: u64) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}
