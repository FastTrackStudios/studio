//! What the app remembers while it runs: one context of signals, provided
//! at the root and read wherever it is needed.

use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;

use crate::data::{self, Rehearsal, Song, Version};

/// A bottom sheet (or the top search sheet) over the page.
#[derive(Clone, PartialEq, Debug)]
pub enum Sheet {
    /// Open anything: the round button.
    Palette,
    /// The player, from the Now Playing bar.
    Player,
    /// A song's key in a set: the set's id and the song's place.
    Key(String, usize),
    /// A song's arrangement in a set.
    Arr(String, usize),
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
}

/// A ⋯ menu from the page's bar.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Menu {
    NewSet,
    SetMenu,
    Series,
    Occ,
    Project,
    Mine,
}

/// What is playing, and from where.
#[derive(Clone, PartialEq, Debug)]
pub struct NowPlaying {
    pub list: Vec<Song>,
    pub from: String,
    pub i: usize,
    pub playing: bool,
    pub pos: f64,
}

impl NowPlaying {
    pub fn song(&self) -> Option<&Song> {
        self.list.get(self.i)
    }
}

/// What a set changes on a song: its key and arrangement.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Override {
    pub key: Option<&'static str>,
    pub arr: Option<&'static str>,
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
        let hay = format!("{} {}", s.title, s.artist.unwrap_or("")).to_lowercase();
        if !self.q.is_empty() && !hay.contains(&self.q.to_lowercase()) {
            return false;
        }
        if self.tags.iter().any(|t| !x.tags.contains(t)) {
            return false;
        }
        if !self.keys.is_empty() && !self.keys.contains(data::key_of(s).unwrap_or("?")) {
            return false;
        }
        if !self.energy.is_empty() && !self.energy.contains(&x.energy) {
            return false;
        }
        if !self.tempo.is_empty() {
            let t = s.tempo.unwrap_or(0);
            let ok = data::TEMPI
                .iter()
                .any(|(n, lo, hi)| self.tempo.contains(n) && s.tempo.is_some() && t >= *lo && t < *hi);
            if !ok {
                return false;
            }
        }
        if self.ready.iter().any(|k| !s.ready.contains(*k)) {
            return false;
        }
        if self.rested && data::weeks_since(s.title) < 4.0 {
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
    pub set: String,
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

/// The app's state, every piece its own signal.
#[derive(Clone, Copy)]
pub struct AppState {
    pub sheet: Signal<Option<Sheet>>,
    pub menu: Signal<Option<Menu>>,
    pub toast: Signal<Option<(u32, String)>>,
    pub side: Signal<bool>,
    pub np: Signal<Option<NowPlaying>>,
    pub ov: Signal<HashMap<(String, usize), Override>>,
    pub reh: Signal<Vec<Rehearsal>>,
    pub booking: Signal<Option<Booking>>,
    pub filters: Signal<Filters>,
    pub folder: Signal<Option<&'static str>>,
    pub org: Signal<Option<&'static str>>,
    pub every_q: Signal<String>,
    pub review: Signal<Review>,
    pub versions: Signal<HashMap<&'static str, Vec<Version>>>,
    pub stages: Signal<HashMap<String, usize>>,
    pub live: Signal<Live>,
    /// The set whose chart and lyrics `live` follows.
    pub live_set: Signal<String>,
    /// Drive: the sets shared live, presence, the calendar (web only).
    pub sync: crate::live_sync::Sync,
}

impl AppState {
    pub fn new() -> Self {
        let cya = data::occ("cya_2026-11-19").map(|o| o.songs.clone()).unwrap_or_default();
        Self {
            sheet: Signal::new(None),
            menu: Signal::new(None),
            toast: Signal::new(None),
            side: Signal::new(false),
            np: Signal::new(Some(NowPlaying {
                list: cya,
                from: "CYA · Thu Nov 19".into(),
                i: 0,
                playing: false,
                pos: 0.35,
            })),
            ov: Signal::new(HashMap::new()),
            reh: Signal::new(data::first_rehearsals()),
            booking: Signal::new(None),
            filters: Signal::new(Filters::default()),
            folder: Signal::new(None),
            org: Signal::new(None),
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
            live_set: Signal::new(String::new()),
            sync: crate::live_sync::Sync::new(),
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
    pub fn enter_set(mut self, id: &str) {
        if *self.live_set.peek() != id {
            self.live_set.set(id.to_string());
            self.live.with_mut(|l| {
                l.song = 0;
                l.sec = 1;
                l.bar = 3;
            });
        }
    }

    pub fn close(mut self) {
        self.sheet.set(None);
    }

    /// Start playing a list at a song.
    pub fn play(mut self, list: Vec<Song>, from: String, title: &str) {
        let i = list.iter().position(|x| x.title == title).unwrap_or(0);
        self.np.set(Some(NowPlaying { list, from, i, playing: true, pos: 0.08 }));
    }

    /// Change a song's key in a set, here and for everyone in it.
    pub fn set_key(self, set: &str, i: usize, k: &'static str) {
        self.put(set, i, |o| o.key = Some(k));
    }

    /// Change a song's arrangement in a set, here and for everyone in it.
    pub fn set_arr(self, set: &str, i: usize, a: &'static str) {
        self.put(set, i, |o| o.arr = Some(a));
    }

    fn put(self, set: &str, i: usize, f: impl FnOnce(&mut Override)) {
        let mut ov = self.ov;
        let o = ov.with_mut(|m| {
            let o = m.entry((set.to_string(), i)).or_default();
            f(o);
            *o
        });
        self.sync.put_override(set, i, o);
    }

    /// Move a playing chart on by a bar: through the section (again, when
    /// looping), then the next section and the next song.
    pub fn step_bar(self) {
        let mut live = self.live;
        let n = data::set_songs(&self.live_set.peek()).len();
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
        let songs = data::set_songs(&self.live_set.peek());
        let tempo = songs.get(self.live.peek().song).and_then(|s| s.tempo).unwrap_or(120).max(40);
        240_000 / tempo as u64
    }

    /// The key a set plays a song in.
    pub fn key_in(&self, set: &str, i: usize, x: &Song) -> Option<&'static str> {
        self.ov
            .read()
            .get(&(set.to_string(), i))
            .and_then(|o| o.key)
            .or_else(|| data::key_default(set, i))
            .or(x.key)
    }

    /// The arrangement a set plays a song in.
    pub fn arr_in(&self, o: &data::Occ, i: usize) -> &'static str {
        self.ov
            .read()
            .get(&(o.id.clone(), i))
            .and_then(|x| x.arr)
            .unwrap_or_else(|| data::arr_default(o, i))
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
        self.versions.read().get(title).and_then(|v| v.last().cloned())
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
