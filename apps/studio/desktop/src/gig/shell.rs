//! The gig on the rig, an iPad's and a desk's alike: Perform becomes the
//! shared performer screen (`app_shell::ui::PerformerScreen`, worship /
//! gig screen spec B1/B2),
//! its panels Signal's ([`guitar::ui::GigSlots`] lends it to the tablet
//! layout, which hands back its own bar and switches):
//!
//!   app bar    the rig's iPad bar: the set, the rig crumb and its ⋯, the
//!              sync pill, the lights, Mute, Panic, the Views ⋯
//!   now        `NowBand` over [`GigPlayback`]: the song up, its clock, the
//!              song after it, and Session's song progress bar (its
//!              sections, the playhead, the loop's wash; a tap seeks)
//!   setlist    the rig's setlist over the gig's set (no sections in the
//!              column; two taps to load while a song plays)
//!   chart      PDF / Keyflow / Lanes (the song as Session lays it out),
//!              the chart's tools in a column at its right
//!   dock       the rig's ten switches beside Session's transport (Play two
//!              columns wide) over the nudge row
//!   follow     the gig's set as one of the rig's setlists (made once; its
//!              songs get no parts from the gig's sections — those are the
//!              progress bar's), and the song up in the rig's setlist is
//!              the song whose chart and track are up
//!
//! Each song keeps its own zoom and offset, one for the PDF and one for the
//! Keyflow chart (`.chart-views.json` in the gigs folder), so a snippet
//! chart opens on exactly the part that matters, offline, across relaunches.
//!
//! The model, the player and the chart images are the gig page's own
//! (`super`, `super::player`) — this is a second face on them, not a second
//! player. One [`GigState`] (context, made by [`use_gig_shell`]) carries the
//! choice of song and mirrors the player ten times a second; every piece
//! reads it.
//!
//! The full Session UI inside Signal (and Signal inside Session) comes with
//! the unified shell (STA-147); until then Session shows the song's session
//! view here, playing the same reference.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use app_shell::ui::performer::{self, PerformDock, PerformerScreen};
use app_shell::ui::playback::{PlaybackModel, PlaybackView, SectionView, SongView};
use app_shell::ui::{
    NowBand, SyncPill, TransportNudgeRow, provide_playback, use_panel_registry, use_playback,
};
use dioxus::prelude::*;
use guitar::ui::{GigScreen, GigSlots};
use session::ui::components::progress::{LoopIndicatorData, ProgressSection, SongProgressBar};
use session::ui::components::transport_controls::TransportControlBar;
use signal_rig_perform_ui::{PerfState, RigPerformClient};

use super::player::{output_devices, player};
use super::{Gig, Section, Song, clock, gigs_root, image_uri, list_gigs};

// The rig frame's colours (signal-rig-shell's tokens).
const MAIN: &str = "#0f0f12";
const INK: &str = "#e4e4e7";
const INK_2: &str = "#a1a1aa";
const INK_3: &str = "#8e8e98";
const FILL: &str = "rgba(255,255,255,0.06)";
const ACCENT: &str = "#38bdf8";
const LOOP: &str = "#f59e0b";
const FONT: &str = "Inter, -apple-system, sans-serif";

/// The progress bar's section colours, in turn.
const SECTION_COLOURS: &[&str] = &[
    "#2563eb", "#7c3aed", "#0d9488", "#c2410c", "#be185d", "#4d7c0f", "#0369a1", "#a16207",
];

/// How far the chart zooms: out to about three grand-staff systems on a
/// page's width, in to a snippet filling it.
const ZOOM_MIN: f64 = 0.4;
const ZOOM_MAX: f64 = 3.0;
const ZOOM_STEP: f64 = 0.1;
/// A nudge of the bottom bar's ▲ / ▼, in pixels.
const NUDGE: f64 = 80.0;

/// A chart's place: its zoom (1 = the page's width) and how far it is moved
/// left and up, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartPlace {
    pub zoom: f64,
    pub x: f64,
    pub y: f64,
}

impl Default for ChartPlace {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            x: 0.0,
            y: 0.0,
        }
    }
}

impl ChartPlace {
    fn clamped(self) -> Self {
        Self {
            zoom: self.zoom.clamp(ZOOM_MIN, ZOOM_MAX),
            x: self.x.clamp(-4000.0, 4000.0),
            y: self.y.clamp(-400.0, 40000.0),
        }
    }
}

/// Every chart's place, by `gig/song number/view`.
type Places = BTreeMap<String, [f64; 3]>;

fn places_path() -> PathBuf {
    gigs_root().join(".chart-views.json")
}

fn load_places() -> Places {
    std::fs::read_to_string(places_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_places(places: &Places) {
    let path = places_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(places) {
        let _ = std::fs::write(path, text);
    }
}

fn place_key(gig: &str, song: u32, keyflow: bool) -> String {
    format!("{gig}/{song}/{}", if keyflow { "keyflow" } else { "pdf" })
}

/// The gig, the song picked, and the player as the page shows it.
#[derive(Clone, Copy)]
pub struct GigState {
    pub gigs: Signal<Vec<Gig>>,
    pub gig_at: Signal<usize>,
    pub song_at: Signal<Option<usize>>,
    /// The chart shown: the Keyflow render (else the band's PDF, the default).
    pub keyflow: Signal<bool>,
    /// Each chart's zoom and offset.
    pub places: Signal<Places>,
    /// Session (the song's session view) rather than Signal (its chart).
    pub studio: Signal<bool>,
    pub pos: Signal<f64>,
    pub dur: Signal<f64>,
    pub playing: Signal<bool>,
    pub loading: Signal<bool>,
    pub status: Signal<String>,
    pub device: Signal<String>,
    pub speed: Signal<f64>,
    pub volume: Signal<f32>,
    pub looped: Signal<Option<(f64, f64)>>,
    /// A loop's start, set by hand on a song with no sections.
    pub mark_a: Signal<Option<f64>>,
}

impl GigState {
    fn gig(&self) -> Option<Gig> {
        self.gigs.read().get((self.gig_at)()).cloned()
    }

    fn song(&self) -> Option<Song> {
        let at = (self.song_at)()?;
        self.gigs
            .read()
            .get((self.gig_at)())
            .and_then(|g| g.songs.get(at).cloned())
    }

    fn song_count(&self) -> usize {
        self.gigs
            .peek()
            .get(*self.gig_at.peek())
            .map_or(0, |g| g.songs.len())
    }

    /// The key the chart up keeps its place under.
    fn place_key(&self) -> Option<String> {
        let gig = self.gig()?;
        let song = self.song()?;
        Some(place_key(&gig.name, song.number, (self.keyflow)()))
    }

    /// The chart up's place (the default until it is moved).
    fn place(&self) -> ChartPlace {
        self.place_key()
            .and_then(|k| self.places.read().get(&k).copied())
            .map_or_else(ChartPlace::default, |[zoom, x, y]| {
                ChartPlace { zoom, x, y }.clamped()
            })
    }

    /// Move the chart up; `keep` writes it to disk (a drag keeps only its end).
    fn set_place(mut self, p: ChartPlace, keep: bool) {
        let Some(k) = self.place_key() else {
            return;
        };
        let p = p.clamped();
        self.places.write().insert(k, [p.zoom, p.x, p.y]);
        if keep {
            save_places(&self.places.peek());
        }
    }

    /// Pick a song: its chart shows, its track loads (paused, at the top).
    pub fn select(mut self, i: usize) {
        self.song_at.set(Some(i));
        self.looped.set(None);
        self.mark_a.set(None);
        player().set_loop(None);
        let audio = self
            .gigs
            .peek()
            .get(*self.gig_at.peek())
            .and_then(|g| g.songs.get(i))
            .and_then(|s| s.audio.clone());
        match audio {
            Some(path) => player().load(&path),
            None => player().unload(),
        }
    }

    /// The next (`+1`) or previous (`-1`) song in the set.
    fn step(self, by: isize) {
        let n = self.song_count();
        if n == 0 {
            return;
        }
        let next = match *self.song_at.peek() {
            None => 0,
            Some(i) => i.saturating_add_signed(by).min(n - 1),
        };
        if Some(next) != *self.song_at.peek() {
            self.select(next);
        }
    }

    fn set_loop(mut self, range: Option<(f64, f64)>) {
        player().set_loop(range);
        self.looped.set(range);
        if let Some((a, _)) = range {
            player().seek(a);
        }
    }

    /// The header's loop button: the section under the playhead loops (a
    /// tap again lets it go). A song with no sections takes A, then B.
    fn loop_press(mut self) {
        if (self.looped)().is_some() {
            self.set_loop(None);
            self.mark_a.set(None);
            return;
        }
        let p = player().position();
        if let Some(s) = self.song().and_then(|s| {
            s.sections
                .iter()
                .find(|x| p >= x.start && p < x.end)
                .cloned()
        }) {
            self.set_loop(Some((s.start, s.end)));
            return;
        }
        match (self.mark_a)() {
            None => self.mark_a.set(Some(p)),
            Some(a) => {
                let (a, b) = if p > a { (a, p) } else { (p, a) };
                self.mark_a.set(None);
                if b - a > 0.5 {
                    self.set_loop(Some((a, b)));
                }
            }
        }
    }

    fn set_speed(mut self, v: f64) {
        player().set_speed(v);
        self.speed.set(v);
    }

    fn set_volume(mut self, v: f32) {
        player().set_volume(v);
        self.volume.set(v);
    }
}

/// The slots the tablet layout reads.
pub const SLOTS: GigSlots = GigSlots {
    screen: screen_slot,
    follow: follow_slot,
};

fn screen_slot(rig: GigScreen) -> Element {
    rsx! { GigScreenView { rig } }
}
fn follow_slot() -> Element {
    rsx! { GigFollow {} }
}

/// Make the gig shell's state and lend its slots to the rig below — when
/// there is a gig on this device (the app's own, or one put in Files).
pub fn use_gig_shell() {
    use_gig_with(SLOTS);
}

/// The slots under a dock that shares the gig ([`GigScope`]): the scope
/// follows the rig's setlist once, so a rig panel below does not again.
const SCOPED_SLOTS: GigSlots = GigSlots {
    screen: screen_slot,
    follow: no_follow,
};

fn no_follow() -> Element {
    rsx! {}
}

/// The gig's state, with `slots` lent to the rig below when there is a gig.
fn use_gig_with(slots: GigSlots) {
    let st = use_context_provider(|| {
        let gigs = list_gigs();
        let at = super::remembered_index(&gigs);
        GigState {
            gigs: Signal::new(gigs),
            gig_at: Signal::new(at),
            song_at: Signal::new(None),
            keyflow: Signal::new(false),
            places: Signal::new(load_places()),
            studio: Signal::new(false),
            pos: Signal::new(0.0),
            dur: Signal::new(0.0),
            playing: Signal::new(false),
            loading: Signal::new(false),
            status: Signal::new(String::new()),
            device: Signal::new(String::new()),
            speed: Signal::new(1.0),
            volume: Signal::new(1.0),
            looped: Signal::new(None),
            mark_a: Signal::new(None),
        }
    });
    use_hook(move || {
        if !st.gigs.peek().is_empty() {
            provide_context(slots);
        }
        // `FTS_GIG_SONG=<n>`: open on the set's nth song (a simulator's
        // screenshot, a rehearsal relaunch); `FTS_GIG_VIEW=studio|keyflow`
        // on that view; `FTS_GIG_PLAY=1` playing.
        let mut studio = st.studio;
        let mut keyflow = st.keyflow;
        match std::env::var("FTS_GIG_VIEW").as_deref() {
            Ok("studio" | "session") => studio.set(true),
            Ok("keyflow") => keyflow.set(true),
            _ => {}
        }
        if let Some(n) = std::env::var("FTS_GIG_SONG")
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .and_then(|n| n.checked_sub(1))
            .filter(|&i| i < st.song_count())
        {
            st.select(n);
        }
    });
    // Mirror the player, ten times a second.
    use_future(move || async move {
        let GigState {
            mut pos,
            mut dur,
            mut playing,
            mut loading,
            mut status,
            mut device,
            ..
        } = st;
        let mut autoplay = std::env::var("FTS_GIG_PLAY").is_ok_and(|v| v == "1");
        loop {
            // The player (and its output) opens with the first song picked,
            // not at launch: the rig sets the audio session up first.
            if st.song_at.peek().is_none() {
                architect::platform::sleep(Duration::from_millis(200)).await;
                continue;
            }
            let p = player();
            p.poll();
            let (np, nd, npl, nl) = (p.position(), p.duration(), p.is_playing(), p.is_loading());
            if autoplay && !nl && nd > 0.0 {
                autoplay = false;
                p.play();
            }
            if (*pos.peek() - np).abs() > 0.04 {
                pos.set(np);
            }
            if (*dur.peek() - nd).abs() > 0.01 {
                dur.set(nd);
            }
            if *playing.peek() != npl {
                playing.set(npl);
            }
            if *loading.peek() != nl {
                loading.set(nl);
            }
            let s = p.status();
            if *status.peek() != s {
                status.set(s);
            }
            let d = p.device();
            if *device.peek() != d {
                device.set(d);
            }
            architect::platform::sleep(Duration::from_millis(100)).await;
        }
    });
}

// ── The gig as dock panels ──────────────────────────────────────────────────

/// The gig shared by a dock's panels (Studio's Session window): its state,
/// made here so every panel — and a rig panel below — reads the same one.
#[component]
pub(crate) fn GigScope(children: Element) -> Element {
    use_gig_with(SCOPED_SLOTS);
    rsx! { {children} }
}

/// Under [`GigScope`] and the rig's connection: the playback the Now panel
/// drives (the gig page's player, and the rig's setlist when it is the
/// gig's set), and the gig following the rig's setlist — once for the
/// whole dock.
#[component]
pub(crate) fn GigPlay(children: Element) -> Element {
    let st = use_context::<GigState>();
    let perf = try_use_context::<PerfState>();
    provide_playback(GigPlayback { st, perf });
    let follow = perf.is_some() && !st.gigs.peek().is_empty();
    rsx! {
        if follow {
            GigFollow {}
        }
        {children}
    }
}

/// A gig panel's word while the gig's scope is not above it.
fn no_gig() -> Element {
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; align-items: center; justify-content: center; padding: 12px; color: {INK_3}; font-size: 13px; font-family: {FONT}; text-align: center;",
            "Connecting to the rig engine…"
        }
    }
}

/// The set being played: the rig's setlist over the gig's set.
#[component]
pub(crate) fn NavigatorPanel() -> Element {
    let st = try_use_context::<GigState>();
    let perf = try_use_context::<PerfState>();
    if st.is_none() || perf.is_none() {
        return no_gig();
    }
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: {MAIN};",
            GigSetlist {}
        }
    }
}

/// The song up: its name, clock and the next song, over its progress band.
#[component]
pub(crate) fn NowPanel() -> Element {
    let st = try_use_context::<GigState>();
    let playback = try_use_context::<app_shell::ui::playback::Playback>();
    if st.is_none() || playback.is_none() {
        return no_gig();
    }
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: {MAIN};",
            {now_panel()}
        }
    }
}

/// The song's chart: PDF, Keyflow or Lanes, with its tools.
#[component]
pub(crate) fn GigChartPanel() -> Element {
    if try_use_context::<GigState>().is_none() {
        return no_gig();
    }
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: {MAIN};",
            GigChart {}
        }
    }
}

// ── Progress ────────────────────────────────────────────────────────────────

/// One stretch of the timeline: a section, or the gap between two.
struct Span {
    name: String,
    start: f64,
    end: f64,
    section: Option<usize>,
}

/// The track cut at its sections, the gaps between them kept as unnamed
/// spans so each sits where it plays.
fn spans(sections: &[Section], dur: f64) -> Vec<Span> {
    let mut out = Vec::new();
    let mut at = 0.0;
    for (k, s) in sections.iter().enumerate() {
        let (a, b) = (s.start.max(at), s.end.min(dur));
        if b <= a {
            continue;
        }
        if a - at > 0.25 {
            out.push(Span {
                name: String::new(),
                start: at,
                end: a,
                section: None,
            });
        }
        out.push(Span {
            name: s.name.clone(),
            start: a,
            end: b,
            section: Some(k),
        });
        at = b;
    }
    if dur - at > 0.25 && !out.is_empty() {
        out.push(Span {
            name: String::new(),
            start: at,
            end: dur,
            section: None,
        });
    }
    out
}

/// A section's name, short ("Interlude A" → "INT A"), as Session shows it.
fn short_name(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next().unwrap_or_default();
    let head: String = first.chars().take(3).collect::<String>().to_uppercase();
    let rest: Vec<&str> = words.collect();
    if rest.is_empty() {
        head
    } else {
        format!("{head} {}", rest.join(" "))
    }
}

/// The song's spans as Session's progress bar takes them: each start (for
/// a tap), and the bar's sections. A song without sections (the CAA
/// charts) is one unnamed strip in the song's colour: no sections are
/// made up for it (mounts spec §6).
fn progress_sections(song: &Song, dur: f64) -> (Vec<f64>, Vec<ProgressSection>) {
    if song.sections.is_empty() {
        if dur <= 0.0 {
            return (Vec::new(), Vec::new());
        }
        let strip = ProgressSection {
            start_percent: 0.0,
            end_percent: 100.0,
            color: song_colour(song),
            name: String::new(),
            short_name: String::new(),
            comment: None,
        };
        return (vec![0.0], vec![strip]);
    }
    let parts = spans(&song.sections, dur);
    let starts = parts.iter().map(|s| s.start).collect();
    let bar = parts
        .iter()
        .map(|s| ProgressSection {
            start_percent: s.start / dur * 100.0,
            end_percent: s.end / dur * 100.0,
            color: s.section.map_or_else(
                || "#27272a".to_string(),
                |k| SECTION_COLOURS[k % SECTION_COLOURS.len()].to_string(),
            ),
            name: s.name.clone(),
            short_name: short_name(&s.name),
            comment: None,
        })
        .collect();
    (starts, bar)
}

/// The rig's setlist, when it is the gig's set: its client, and the song up.
fn rig_on_gig(st: GigState, perf: Option<PerfState>) -> Option<(RigPerformClient, usize)> {
    let perf = perf?;
    let p = perf.perf.read();
    let gig = st.gig()?;
    let up = p.setlists.get(p.setlist_index as usize)?;
    (*up == gig.name).then_some(())?;
    let client = signal_rig_perform_ui::perform_client()?;
    Some((client, p.song_index as usize))
}

/// A song's colour (its number badge, and the strip of a song without
/// sections): the rig setlist's own.
fn song_colour(song: &Song) -> String {
    signal_rig_perform_ui::colors::song_colour(&song.title, "")
}

// ── The performer screen (spec B2) ──────────────────────────────────────────

/// How the gig's songs read on the performer screen: each [`Song`] as a
/// [`SongView`], its sections cut where they play (the gaps left out) in
/// the progress bar's colours. A song without sections has none here.
fn song_views(songs: &[Song], dur_of_up: Option<(usize, f64)>) -> Vec<SongView> {
    songs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let dur = dur_of_up
                .filter(|&(up, d)| up == i && d > 0.0)
                .map_or(f64::MAX, |(_, d)| d);
            let sections = spans(&s.sections, dur)
                .into_iter()
                .filter_map(|x| {
                    let k = x.section?;
                    Some(SectionView {
                        name: short_name(&x.name),
                        colour: SECTION_COLOURS[k % SECTION_COLOURS.len()].to_string(),
                        start: x.start,
                        end: x.end,
                    })
                })
                .collect();
            let (key, _) = signal_rig_perform_ui::colors::split_key(&s.key);
            SongView {
                title: s.title.clone(),
                key,
                bpm: bpm_of(&s.tempo),
                colour: song_colour(s),
                sections,
            }
        })
        .collect()
}

/// Where a section step goes (Section back / forward on the transport).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    /// Play from here, in this song.
    Seek(f64),
    /// The next song in the set.
    NextSong,
}

/// Back restarts the section playing (or, inside its first two seconds,
/// goes to the one before; at the first, the song's start); forward goes
/// to the next section's start, and past the last to the next song. A
/// song without sections steps as one: back to its start, forward to the
/// next song.
fn section_step_target(sections: &[SectionView], pos: f64, by: i32) -> Step {
    if sections.is_empty() {
        return if by < 0 {
            Step::Seek(0.0)
        } else {
            Step::NextSong
        };
    }
    // The section the playhead is in (None: before the first).
    let here = sections.iter().rposition(|s| pos >= s.start - 0.01);
    if by < 0 {
        match here {
            Some(h) if pos - sections[h].start > 2.0 => Step::Seek(sections[h].start),
            Some(h) if h > 0 => Step::Seek(sections[h - 1].start),
            _ => Step::Seek(0.0),
        }
    } else {
        let next = here.map_or(0, |h| h + 1);
        sections
            .get(next)
            .map_or(Step::NextSong, |s| Step::Seek(s.start))
    }
}

/// Signal's playback, as the performer screen drives it: the gig page's
/// own player and [`GigState`], and — when the rig's setlist is the gig's
/// set — the rig's setlist for song changes (the chart and track follow
/// it, so the rig's sound and the song up never disagree).
#[derive(Clone, Copy)]
pub struct GigPlayback {
    st: GigState,
    perf: Option<PerfState>,
}

impl GigPlayback {
    /// The rig, when its setlist is the gig's set: its client and song up.
    fn rig(&self) -> Option<(RigPerformClient, usize)> {
        rig_on_gig(self.st, self.perf)
    }

    fn sections_up(&self) -> Vec<SectionView> {
        let at = *self.st.song_at.peek();
        let dur = *self.st.dur.peek();
        let gigs = self.st.gigs.peek();
        let Some(gig) = gigs.get(*self.st.gig_at.peek()) else {
            return Vec::new();
        };
        at.and_then(|i| song_views(&gig.songs, Some((i, dur))).into_iter().nth(i))
            .map(|s| s.sections)
            .unwrap_or_default()
    }
}

impl PlaybackModel for GigPlayback {
    fn view(&self) -> PlaybackView {
        let st = self.st;
        let songs = st
            .gigs
            .read()
            .get((st.gig_at)())
            .map(|g| g.songs.clone())
            .unwrap_or_default();
        let current = (st.song_at)();
        let duration = (st.dur)();
        let position = (st.pos)();
        let views = song_views(&songs, current.map(|i| (i, duration)));
        let looping = (st.looped)().map(|(a, _)| {
            current
                .and_then(|i| views.get(i))
                .and_then(|s| s.sections.iter().find(|x| (x.start - a).abs() < 0.05))
                .map_or_else(|| "A–B".to_string(), |x| x.name.clone())
        });
        PlaybackView {
            songs: views,
            current,
            position,
            duration,
            playing: (st.playing)(),
            loading: (st.loading)(),
            looping,
            speed: (st.speed)(),
            level: f64::from((st.volume)()).clamp(0.0, 1.0),
            can_control: true,
            following: None,
            peers: 0,
            // Signal counts in time, not bars.
            bar_beat: None,
        }
    }

    fn step_song(&self, by: i32) {
        match self.rig() {
            Some((rig, at)) => {
                let n = self.perf.map_or(0, |s| s.perf.peek().songs.len());
                let next = at
                    .saturating_add_signed(by as isize)
                    .min(n.saturating_sub(1));
                if next != at {
                    let next = u32::try_from(next).unwrap_or(0);
                    spawn(async move {
                        let _ = rig.select_song(next).await;
                    });
                }
            }
            None => self.st.step(by as isize),
        }
    }

    fn load_song(&self, index: usize) {
        match self.rig() {
            Some((rig, _)) => {
                let i = u32::try_from(index).unwrap_or(0);
                spawn(async move {
                    let _ = rig.select_song(i).await;
                });
            }
            None => self.st.select(index),
        }
    }

    fn seek(&self, seconds: f64) {
        player().seek(seconds.max(0.0));
    }

    fn play_pause(&self) {
        if self.st.song_at.peek().is_some() {
            player().toggle();
        }
    }

    fn section_step(&self, by: i32) {
        let pos = player().position();
        match section_step_target(&self.sections_up(), pos, by) {
            Step::Seek(at) => player().seek(at),
            Step::NextSong => self.step_song(1),
        }
    }

    fn set_loop(&self, on: bool) {
        let looping = self.st.looped.peek().is_some();
        if on != looping {
            self.st.loop_press();
        }
    }

    fn set_speed(&self, rate: f64) {
        self.st.set_speed(rate);
    }

    fn set_level(&self, level: f64) {
        self.st.set_volume(level.clamp(0.0, 1.0) as f32);
    }
}

/// The rig's pieces of the screen, for the panels below (the registry's
/// bodies take no props).
#[derive(Clone, Copy)]
struct RigPieces(GigScreen);

/// Whether the iPad's status and home-indicator insets are drawn: on the
/// iPad, and on a desktop window standing in for one
/// (`FTS_GIG_INSETS=ipad`, a 1194 x 834 screenshot).
fn ipad_insets() -> bool {
    cfg!(target_os = "ios") || std::env::var("FTS_GIG_INSETS").is_ok_and(|v| v == "ipad")
}

/// Perform, under a gig: the shared performer screen (app-shell's
/// `PerformerScreen`, B1), its panels Signal's — the now band over
/// [`GigPlayback`] with Session's progress bar, the rig's setlist, the
/// chart, and the dock (the rig's switches beside the transport).
#[component]
fn GigScreenView(
    rig: GigScreen,
    /// Draw the iPad's insets whatever the platform (a test, a screenshot).
    #[props(default)]
    ipad: bool,
) -> Element {
    let st = use_context::<GigState>();
    let perf = try_use_context::<PerfState>();
    provide_playback(GigPlayback { st, perf });
    use_context_provider(|| RigPieces(rig));
    // The one registry function Session's performer view calls too (no fork).
    use_panel_registry(|| {
        performer::performer_registry(performer::PerformerBodies {
            now: now_panel,
            setlist: setlist_panel,
            chart: chart_panel,
            dock: dock_panel,
        })
    });
    let (top, bottom) = if ipad || ipad_insets() {
        (performer::STATUS_INSET, performer::HOME_INSET)
    } else {
        (0, 0)
    };
    // The account: the ⋯ menu's first item, and on the iPad its sheet
    // (on a desktop the window's own sheet, in main.rs, shows it). The
    // scrim starts below the app bar so Mute and Panic stay live.
    let account = try_use_context::<fts_account::ui::AccountState>();
    use_context_provider(|| guitar::ui::BarAccountItem(account_item));
    // On a desktop the window's own sheet (main.rs) is the one.
    let touch = ipad || cfg!(target_os = "ios");
    rsx! {
        PerformerScreen {
            scope: performer::SIGNAL_SCOPE,
            app_bar: (rig.app_bar)(rsx! { SyncPill {} }),
            inset_top: top,
            inset_bottom: bottom,
        }
        if touch && account.is_some() {
            crate::account::AccountOverlay { top: top.saturating_add(48).max(72), width: 834 }
        }
    }
}

/// The ⋯ menu's Account item (STA-361): signed out, signed in, or ended.
fn account_item(close: Callback<()>) -> Element {
    rsx! { AccountItem { close } }
}

#[component]
fn AccountItem(close: Callback<()>) -> Element {
    let Some(state) = try_use_context::<fts_account::ui::AccountState>() else {
        return rsx! {};
    };
    rsx! {
        fts_account::ui::AccountMenuItem { state, locked: false, on_pick: move |()| close.call(()) }
    }
}

fn now_panel() -> Element {
    rsx! { NowBand { progress: rsx! { GigBar {} } } }
}
fn setlist_panel() -> Element {
    rsx! { GigSetlist {} }
}
fn chart_panel() -> Element {
    rsx! { GigChart {} }
}
fn dock_panel() -> Element {
    rsx! { GigDock {} }
}

/// The band's progress bar: Session's `SongProgressBar` over the song's
/// sections (a tap plays from one), the loop's wash passed; a song
/// without sections is one strip, no names, no wash.
#[component]
fn GigBar() -> Element {
    let st = use_context::<GigState>();
    let song = st.song();
    let d = (st.dur)();
    let p = (st.pos)();
    let Some(song) = song.filter(|_| d > 0.0) else {
        let words = match st.song() {
            None => "No song",
            Some(s) if s.audio.is_none() => "No reference track: the chart only",
            Some(_) => "Loading the track…",
        };
        return rsx! {
            div { role: "progressbar", "aria-label": "{words}",
                style: "display: flex; align-items: center; flex: 1; height: 44px; padding: 0 12px; border-radius: 6px; background: #141418; color: {INK_3}; font-size: 14px; font-family: {FONT};",
                "{words}"
            }
        };
    };
    let (starts, bar) = progress_sections(&song, d);
    let progress = (p / d * 100.0).clamp(0.0, 100.0);
    let loop_indicator = loop_indicator((st.looped)(), d, song.sections.is_empty());
    rsx! {
        div { style: "flex: 1; min-width: 0; height: 44px;",
            SongProgressBar {
                progress,
                sections: bar,
                height: "44px".to_string(),
                labels: !song.sections.is_empty(),
                loop_indicator,
                on_section_click: move |k: usize| {
                    if let Some(at) = starts.get(k) {
                        player().seek(*at);
                    }
                },
            }
        }
    }
}

/// The loop as the progress bar draws it (its wash), in percent of the
/// song — none for a song without sections (one strip, no wash).
fn loop_indicator(
    looped: Option<(f64, f64)>,
    dur: f64,
    unsectioned: bool,
) -> Option<LoopIndicatorData> {
    let (a, b) = looped?;
    if unsectioned || dur <= 0.0 {
        return None;
    }
    Some(LoopIndicatorData::new(
        true,
        (a / dur * 100.0).clamp(0.0, 100.0),
        (b / dur * 100.0).clamp(0.0, 100.0),
    ))
}

/// The setlist column: the rig's setlist over the gig's set, folded (no
/// sections in the narrow column), two taps to load while a song plays.
#[component]
fn GigSetlist() -> Element {
    let st = use_context::<GigState>();
    let Some(perf) = try_use_context::<PerfState>() else {
        return rsx! {
            div { style: "flex: 1; display: flex; align-items: center; justify-content: center; color: {INK_3}; font-size: 14px; font-family: {FONT};",
                "No rig setlist"
            }
        };
    };
    rsx! {
        signal_rig_perform_ui::setlist::TabletSetlist {
            state: perf,
            sections: false,
            arm_to_load: (st.playing)(),
        }
    }
}

/// What the chart region shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ChartMode {
    Pdf,
    Keyflow,
    /// The song as Session lays it out (its sections in a lane).
    Lanes,
}

/// The chart region: PDF / Keyflow / Lanes along the top (48 tall), the
/// chart under them, and the chart's tools in a column at the right
/// (52 x 48, 8 apart): zoom in, the zoom, zoom out, up, down, Fit.
#[component]
fn GigChart() -> Element {
    let st = use_context::<GigState>();
    let mut keyflow = st.keyflow;
    let mut studio = st.studio;
    let mode = if studio() {
        ChartMode::Lanes
    } else if keyflow() {
        ChartMode::Keyflow
    } else {
        ChartMode::Pdf
    };
    let song = st.song();
    let has_keyflow = song
        .as_ref()
        .is_some_and(|s| s.charts.iter().any(|c| is_keyflow(&c.label)));
    let has_pdf = song
        .as_ref()
        .is_some_and(|s| s.charts.iter().any(|c| !is_keyflow(&c.label)));
    let place = st.place();
    let pct = (place.zoom * 100.0).round();
    let zoom = move |by: f64| {
        let p = st.place();
        st.set_place(
            ChartPlace {
                zoom: ((p.zoom + by) * 10.0).round() / 10.0,
                ..p
            },
            true,
        );
    };
    let nudge = move |by: f64| {
        let p = st.place();
        st.set_place(ChartPlace { y: p.y + by, ..p }, true);
    };
    let segs = [
        (ChartMode::Pdf, "PDF", has_pdf),
        (ChartMode::Keyflow, "Keyflow", has_keyflow),
        (ChartMode::Lanes, "Lanes", true),
    ];
    rsx! {
        div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0; min-height: 0; background: {MAIN}; font-family: {FONT};",
            div { role: "tablist", "aria-label": "Chart view",
                style: "display: flex; gap: 8px; flex-shrink: 0; height: 48px; padding: 0 12px; margin-top: 8px; box-sizing: border-box;",
                for (m, label, has) in segs {
                    button {
                        key: "{label}",
                        role: "tab",
                        "aria-selected": "{mode == m}",
                        "aria-label": "{label}",
                        style: "{tool(mode == m, has)} padding: 0 16px;",
                        onclick: move |_| match m {
                            ChartMode::Pdf => { studio.set(false); keyflow.set(false); }
                            ChartMode::Keyflow => { studio.set(false); keyflow.set(true); }
                            ChartMode::Lanes => studio.set(true),
                        },
                        "{label}"
                    }
                }
            }
            div { style: "display: flex; flex: 1; min-height: 0; min-width: 0;",
                div { style: "display: flex; flex-direction: column; flex: 1; min-width: 0; min-height: 0;",
                    if mode == ChartMode::Lanes { StudioSong {} } else { ChartView {} }
                }
                if mode != ChartMode::Lanes {
                    div { "aria-label": "Chart tools",
                        style: "display: flex; flex-direction: column; gap: 8px; flex-shrink: 0; width: 52px; margin: 12px 12px 12px 0;",
                        button { "aria-label": "Zoom in", style: "{tool(false, true)} width: 52px;", onclick: move |_| zoom(ZOOM_STEP), "+" }
                        span { "aria-label": "Zoom {pct}%", style: "display: flex; align-items: center; justify-content: center; height: 48px; font-size: 14px; color: {INK_2}; font-variant-numeric: tabular-nums;", "{pct}%" }
                        button { "aria-label": "Zoom out", style: "{tool(false, true)} width: 52px;", onclick: move |_| zoom(-ZOOM_STEP), "−" }
                        button { "aria-label": "Chart up", style: "{tool(false, true)} width: 52px;", onclick: move |_| nudge(NUDGE), "▲" }
                        button { "aria-label": "Chart down", style: "{tool(false, true)} width: 52px;", onclick: move |_| nudge(-NUDGE), "▼" }
                        button { "aria-label": "Fit the width", style: "{tool(false, true)} width: 52px; font-size: 15px;",
                            onclick: move |_| st.set_place(ChartPlace::default(), true), "Fit" }
                    }
                }
            }
        }
    }
}

/// A chart or lane tool, 48 tall: filled, a hairline round it; `on` is
/// the segment picked; a segment with nothing behind it is dimmed.
fn tool(on: bool, enabled: bool) -> String {
    let (bg, edge, ink) = if on {
        ("rgba(255,255,255,0.1)", "#d4d4d8", INK)
    } else {
        ("#141418", "#2b2b31", INK)
    };
    let opacity = if enabled { 1.0 } else { 0.45 };
    format!(
        "display: flex; align-items: center; justify-content: center; flex-shrink: 0; height: 48px; \
         box-sizing: border-box; border-radius: 8px; border: 1px solid {edge}; background: {bg}; color: {ink}; \
         font-family: {FONT}; font-size: 18px; font-weight: 650; opacity: {opacity}; cursor: pointer;"
    )
}

/// How long a Song-forward tap stays armed while playing (as a setlist
/// row's Load does).
const ARM_FOR: Duration = Duration::from_secs(6);

/// The dock: the rig's ten switches (left, 592) beside the transport
/// (562): Session's `TransportControlBar` (Play two columns wide, green
/// while playing) over the nudge row.
#[component]
fn GigDock() -> Element {
    let rig = use_context::<RigPieces>().0;
    rsx! {
        PerformDock {
            rig: (rig.rig)(),
            transport: rsx! { GigTransport {} },
        }
    }
}

#[component]
fn GigTransport() -> Element {
    let st = use_context::<GigState>();
    let playback = use_playback();
    let playing = (st.playing)();
    let has_song = (st.song_at)().is_some();
    let unsectioned = st.song().is_some_and(|s| s.sections.is_empty());
    // No song up: the transport reads as off (40%), its taps do nothing.
    let shade = if has_song { 1.0 } else { 0.4 };
    // A song without sections: forward while playing is armed (the first
    // tap asks, the second loads the next song), so one tap never changes
    // the rig's sound mid-song.
    let mut armed = use_signal(|| false);
    let (p1, p2, p3, p4) = (
        playback.clone(),
        playback.clone(),
        playback.clone(),
        playback,
    );
    let on_forward = move |()| {
        if unsectioned && playing && !armed() {
            armed.set(true);
            spawn(async move {
                architect::platform::sleep(ARM_FOR).await;
                armed.set(false);
            });
            return;
        }
        armed.set(false);
        p3.section_step(1);
    };
    rsx! {
        div { style: "display: flex; height: 60px; min-width: 0; opacity: {shade};",
            TransportControlBar {
                is_playing: playing,
                is_looping: (st.looped)().is_some(),
                is_recording: false,
                is_armed: false,
                show_recording: false,
                wide_play: true,
                label_px: 14,
                skip_song: unsectioned,
                forward_armed: armed(),
                on_play_pause: move |()| p1.play_pause(),
                on_loop_toggle: move |()| {
                    let on = st.looped.peek().is_some();
                    p2.set_loop(!on);
                },
                on_record_toggle: move |()| {},
                on_arm_toggle: move |()| {},
                on_back: move |()| p4.section_step(-1),
                on_forward,
            }
        }
        TransportNudgeRow {}
    }
}
// ── Follow: the gig's set is one of the rig's setlists ─────────────────────

/// Whether the set is being written into the rig (its song changes then are
/// the writing's, not a pick).
static SEEDING: AtomicBool = AtomicBool::new(false);

/// A song name, for matching the gig's to the rig's.
fn same_song(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    norm(a) == norm(b)
}

/// The tempo's first number ("246 in 12/8 (~123 felt)" → 246), 0 if none.
fn bpm_of(tempo: &str) -> u32 {
    tempo
        .split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Write the gig's set into the rig's library, once: its songs (no parts:
/// see below), a setlist named for the gig, and that set up.
async fn seed_setlist(r: RigPerformClient, gig: Gig) {
    let Ok(lib) = r.library().await else {
        return;
    };
    if lib.setlists.iter().any(|s| s.name == gig.name) {
        return;
    }
    SEEDING.store(true, Ordering::SeqCst);
    for s in &gig.songs {
        if !lib.songs.iter().any(|x| same_song(&x.name, &s.title)) {
            let _ = r
                .add_song(s.title.clone(), s.key.clone(), bpm_of(&s.tempo))
                .await;
        }
    }
    let _ = r.add_setlist(gig.name.clone()).await;
    let Ok(lib) = r.library().await else {
        SEEDING.store(false, Ordering::SeqCst);
        return;
    };
    let Some(set) = lib.setlists.iter().position(|s| s.name == gig.name) else {
        SEEDING.store(false, Ordering::SeqCst);
        return;
    };
    let set = u32::try_from(set).unwrap_or(0);
    for s in &gig.songs {
        let name = lib
            .songs
            .iter()
            .find(|x| same_song(&x.name, &s.title))
            .map_or_else(|| s.title.clone(), |x| x.name.clone());
        let _ = r.add_setlist_entry(set, name).await;
    }
    let _ = r.select_setlist(set).await;
    // No parts: a gig's sections are its playback (the progress bar), not
    // the rig's. A song gets parts when the player adds them by hand, and
    // plays the All-Around profile until then (STA-297).
    SEEDING.store(false, Ordering::SeqCst);
    let _ = r.select_song(0).await;
}

/// Load the gig's set in the rig: written in first if it is not there yet
/// ([`seed_setlist`]), then up, on its first song — what Studio's home asks
/// for when the gig is picked or on now.
async fn load_set(r: RigPerformClient, gig: Gig) {
    seed_setlist(r.clone(), gig.clone()).await;
    let Ok(lib) = r.library().await else {
        return;
    };
    let Some(set) = lib.setlists.iter().position(|s| s.name == gig.name) else {
        tracing::warn!(gig.name = %gig.name, "gig: the set was not in the rig to load");
        return;
    };
    let _ = r.select_setlist(u32::try_from(set).unwrap_or(0)).await;
    let _ = r.select_song(0).await;
}

/// Kept once the one-time clean-up below has run on this device.
fn parts_cleaned_path() -> PathBuf {
    gigs_root().join(".gig-parts-cleaned")
}

/// Once per device: take back the parts an earlier `seed_setlist` made
/// from each gig song's sections (it added every section as a rig part).
/// A part goes only when its name is exactly one of that song's sections
/// in gig.json — so a part added by hand with a section's name goes too.
/// Only the gigs that were seeded (the rig has a setlist of the gig's
/// name). The flag is written after a pass that reached the rig, so a rig
/// that was not up yet is tried again next launch.
async fn drop_seeded_parts(r: RigPerformClient, gigs: Vec<Gig>) {
    let flag = parts_cleaned_path();
    if flag.exists() {
        return;
    }
    let Ok(lib) = r.library().await else {
        return;
    };
    let mut dropped = 0usize;
    for gig in gigs
        .iter()
        .filter(|g| lib.setlists.iter().any(|s| s.name == g.name))
    {
        for s in &gig.songs {
            let Some(entry) = lib.songs.iter().find(|x| same_song(&x.name, &s.title)) else {
                continue;
            };
            for part in seeded_parts(&entry.parts, &s.sections) {
                if r.edit_song_part(entry.name.clone(), "remove".into(), part, String::new())
                    .await
                    .is_ok()
                {
                    dropped += 1;
                }
            }
        }
    }
    tracing::info!(dropped, "gig: dropped the parts seeded from gig sections");
    if let Some(dir) = flag.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&flag, format!("{dropped}\n"));
}

/// A song's parts that are named exactly as one of its gig sections, in
/// the song's order (each once per occurrence, so a doubled one goes too).
fn seeded_parts(parts: &[String], sections: &[Section]) -> Vec<String> {
    parts
        .iter()
        .filter(|p| sections.iter().any(|sec| sec.name == **p))
        .cloned()
        .collect()
}

/// Draws nothing: makes the gig's setlist once, then keeps the chart and
/// the track on the song up in the rig's setlist.
#[component]
fn GigFollow() -> Element {
    let st = use_context::<GigState>();
    let perf = try_use_context::<PerfState>();
    use_hook(move || {
        if let (Some(r), Some(gig)) = (signal_rig_perform_ui::perform_client(), st.gig()) {
            let gigs = st.gigs.peek().clone();
            let load = super::take_load_on_entry();
            spawn(async move {
                if load {
                    load_set(r.clone(), gig).await;
                } else {
                    seed_setlist(r.clone(), gig).await;
                }
                drop_seeded_parts(r, gigs).await;
            });
        }
    });
    use_effect(move || {
        let Some(perf) = perf else {
            return;
        };
        let p = perf.perf.read();
        let name = p.songs.get(p.song_index as usize).map(|s| s.name.clone());
        drop(p);
        if SEEDING.load(Ordering::SeqCst) {
            return;
        }
        let Some(name) = name else {
            return;
        };
        let at = st
            .gigs
            .peek()
            .get(*st.gig_at.peek())
            .and_then(|g| g.songs.iter().position(|s| same_song(&s.title, &name)));
        if let Some(i) = at
            && *st.song_at.peek() != Some(i)
        {
            st.select(i);
        }
    });
    rsx! {}
}

// ── Main: the chart, or the song's session view ────────────────────────────

#[component]
fn GigMain() -> Element {
    let st = use_context::<GigState>();
    if (st.studio)() {
        rsx! { StudioSong {} }
    } else {
        rsx! { ChartView {} }
    }
}

/// How far apart the first two fingers are, in pixels (0 with fewer).
fn spread(fingers: &BTreeMap<i32, (f64, f64)>) -> f64 {
    let mut at = fingers.values();
    match (at.next(), at.next()) {
        (Some(a), Some(b)) => (a.0 - b.0).hypot(a.1 - b.1),
        _ => 0.0,
    }
}

fn is_keyflow(label: &str) -> bool {
    label.to_lowercase().contains("keyflow")
}

/// A tempo as the chart gives it: a bare number reads "N bpm", anything
/// fuller ("246 in 12/8 (~123 felt)") stays as written.
fn tempo_text(tempo: &str) -> String {
    if tempo.trim().parse::<f64>().is_ok() {
        format!("{} bpm", tempo.trim())
    } else {
        tempo.to_string()
    }
}

fn fact(text: &str, bg: &str, ink: &str) -> Element {
    rsx! {
        span { style: "flex-shrink: 0; font-size: 13px; padding: 3px 8px; border-radius: 5px; background: {bg}; color: {ink};", "{text}" }
    }
}

/// The song's chart, the band's PDF (every page) or the Keyflow render, at
/// its own zoom and offset: a drag moves it, the foot bar zooms and nudges.
#[component]
fn ChartView() -> Element {
    let st = use_context::<GigState>();
    let keyflow = st.keyflow;
    // A drag: where it started, and the place then.
    let mut drag = use_signal(|| None::<(f64, f64, ChartPlace)>);
    // The fingers on the chart, by pointer id, and a pinch: how far apart
    // two were when it started, and the place then. A pinch writes the
    // same per-song, per-view zoom as the foot bar's − / +.
    let mut fingers = use_signal(BTreeMap::<i32, (f64, f64)>::new);
    let mut pinch = use_signal(|| None::<(f64, ChartPlace)>);
    let Some(song) = st.song() else {
        let name = st.gig().map(|g| g.name).unwrap_or_default();
        return rsx! {
            div { style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; \
                          background: {MAIN}; color: {INK_2}; font-family: {FONT};",
                span { style: "font-size: 20px; font-weight: 750; color: {INK};", "{name}" }
                span { style: "font-size: 14px;", "Pick a song in Setlist (top left) — its chart opens here." }
            }
        };
    };
    let pages: Vec<_> = song
        .charts
        .iter()
        .filter(|c| is_keyflow(&c.label) == keyflow())
        .filter_map(|c| image_uri(&c.path).map(|uri| (c.label.clone(), uri)))
        .collect();
    // A finger (or the mouse) up: a pinch or a drag ends there, and its
    // place is kept.
    let mut lift = move |id: i32| {
        fingers.write().remove(&id);
        let ended = (pinch.peek().is_some() && fingers.peek().len() < 2) || drag.peek().is_some();
        if pinch.peek().is_some() && fingers.peek().len() < 2 {
            pinch.set(None);
        }
        if fingers.peek().is_empty() || ended {
            drag.set(None);
        }
        if ended {
            st.set_place(st.place(), true);
        }
    };
    let place = st.place();
    let width = place.zoom * 100.0;
    let img_style = format!(
        "width: {width:.1}%; height: auto; display: block; margin: 0 auto 10px; background: #fff; border-radius: 4px;"
    );
    rsx! {
        div {
            style: "flex: 1; min-height: 0; position: relative; overflow: hidden; background: {MAIN}; color: {INK}; font-family: {FONT}; touch-action: none;",
            onpointerdown: move |e: PointerEvent| {
                let c = e.client_coordinates();
                fingers.write().insert(e.pointer_id(), (c.x, c.y));
                let f = fingers.peek();
                if f.len() >= 2 {
                    // A second finger: the drag becomes a pinch.
                    let d = spread(&f);
                    drop(f);
                    drag.set(None);
                    if d > 10.0 {
                        pinch.set(Some((d, st.place())));
                    }
                } else {
                    drop(f);
                    drag.set(Some((c.x, c.y, st.place())));
                }
            },
            onpointermove: move |e: PointerEvent| {
                let c = e.client_coordinates();
                if let Some(at) = fingers.write().get_mut(&e.pointer_id()) {
                    *at = (c.x, c.y);
                }
                if let Some((d0, from)) = *pinch.peek() {
                    let d = spread(&fingers.peek());
                    if d > 0.0 {
                        let zoom = (from.zoom * d / d0 * 100.0).round() / 100.0;
                        st.set_place(ChartPlace { zoom, ..from }, false);
                    }
                    return;
                }
                if let Some((x0, y0, from)) = *drag.peek() {
                    st.set_place(
                        ChartPlace { x: from.x - (c.x - x0), y: from.y - (c.y - y0), ..from },
                        false,
                    );
                }
            },
            onpointerup: move |e: PointerEvent| lift(e.pointer_id()),
            onpointercancel: move |e: PointerEvent| lift(e.pointer_id()),
            onpointerleave: move |e: PointerEvent| lift(e.pointer_id()),
            div { style: "position: relative; left: {-place.x:.0}px; top: {-place.y:.0}px; padding: 8px 10px;",
                if pages.is_empty() {
                    div { style: "padding: 24px; color: {INK_3}; font-size: 14px;",
                        if keyflow() { "No Keyflow chart for this song." } else { "No PDF chart for this song." }
                    }
                }
                for (k, (label, uri)) in pages.into_iter().enumerate() {
                    img { key: "{k}-{label}", src: "{uri}", style: "{img_style}", draggable: "false" }
                }
                div { style: "display: flex; flex-wrap: wrap; gap: 8px; padding: 4px 2px 12px; font-size: 13px; color: {INK_2};",
                    {fact(&format!("Profile: {}", song.profile.label()), "#172554", "#bfdbfe")}
                    if !song.guitar.is_empty() { {fact(&format!("Guitar: {}", song.guitar), "#052e16", "#bbf7d0")} }
                    if !song.notes.is_empty() { span { "{song.notes}" } }
                }
            }
        }
    }
}

/// The song as Session lays it out: its sections in a lane (a tap plays
/// from one, its ↻ loops it), the clock, speed and volume, where it plays.
#[component]
fn StudioSong() -> Element {
    let st = use_context::<GigState>();
    let Some(song) = st.song() else {
        return rsx! {
            div { style: "flex: 1; display: flex; align-items: center; justify-content: center; background: {MAIN}; color: {INK_2}; font-size: 14px; font-family: {FONT};",
                "Session: pick a song in Setlist to hear its reference."
            }
        };
    };
    let d = (st.dur)();
    let p = (st.pos)();
    let looped = (st.looped)();
    let mut parts = spans(&song.sections, d);
    if parts.is_empty() && d > 0.0 {
        parts.push(Span {
            name: "Full song".to_string(),
            start: 0.0,
            end: d,
            section: None,
        });
    }
    let pct = ((st.speed)() * 100.0).round();
    let vol = (f64::from((st.volume)()) * 100.0).round();
    let device = (st.device)();
    let status = (st.status)();
    rsx! {
        div { style: "flex: 1; min-height: 0; overflow-y: scroll; display: flex; flex-direction: column; gap: 14px; padding: 16px 18px; \
                      background: {MAIN}; color: {INK}; font-family: {FONT};",
            div { style: "display: flex; align-items: baseline; gap: 12px;",
                span { style: "font-size: 12px; font-weight: 700; letter-spacing: 0.14em; color: {ACCENT};", "SESSION" }
                span { style: "font-size: 22px; font-weight: 800;", "{song.title}" }
                span { style: "font-size: 14px; color: {INK_2};", "{song.artist}" }
                span { style: "flex: 1;" }
                span { style: "font-size: 30px; font-weight: 750; font-variant-numeric: tabular-nums;", "{clock(p)}" }
                span { style: "font-size: 16px; color: {INK_3}; font-variant-numeric: tabular-nums;", "/ {clock(d)}" }
            }
            if song.audio.is_none() {
                div { style: "font-size: 14px; color: {LOOP};", "No reference track for this song — the chart only." }
            }
            // The arrangement lane.
            div { style: "display: flex; gap: 3px; height: 96px;",
                for (k, s) in parts.iter().enumerate() {
                    {
                        let grow = (s.end - s.start).max(0.01);
                        let (a, b) = (s.start, s.end);
                        let here = p >= a && p < b;
                        let looping = looped.is_some_and(|(x, y)| (x - a).abs() < 0.01 && (y - b).abs() < 0.01);
                        let named = !s.name.is_empty();
                        let bg = if here { "#0c4a6e" } else if named { "#1c1c21" } else { "#121215" };
                        let edge = if looping { format!("box-shadow: inset 0 0 0 2px {LOOP};") } else { String::new() };
                        rsx! {
                            div { key: "{k}",
                                style: "flex: {grow} 1 0px; min-width: 0; display: flex; flex-direction: column; justify-content: space-between; \
                                        padding: 6px; border-radius: 6px; background: {bg}; overflow: hidden; box-sizing: border-box; {edge}",
                                onclick: move |_| player().seek(a),
                                span { style: "font-size: 13px; font-weight: 700; white-space: nowrap; overflow: hidden;", "{s.name}" }
                                div { style: "display: flex; align-items: center; justify-content: space-between; gap: 4px;",
                                    span { style: "font-size: 11px; color: {INK_3}; white-space: nowrap;", "{clock(a)}" }
                                    if named {
                                        button {
                                            style: "height: 48px; min-width: 48px; border: none; border-radius: 8px; background: {FILL}; color: {INK}; font-size: 16px; cursor: pointer;",
                                            "aria-label": "Loop {s.name}",
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                st.set_loop(if looping { None } else { Some((a, b)) });
                                            },
                                            "↻"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Speed (pitch kept) and volume.
            div { style: "display: flex; flex-wrap: wrap; align-items: center; gap: 6px;",
                span { style: "font-size: 13px; color: {INK_3}; margin-right: 4px;", "Speed" }
                for rate in app_shell::ui::playback::SPEEDS {
                    button { key: "{rate}", style: "{tool((pct - rate * 100.0).abs() < 0.5, true)} padding: 0 12px;",
                        "aria-label": "Speed {app_shell::ui::playback::speed_label(rate)}",
                        onclick: move |_| st.set_speed(rate), "{app_shell::ui::playback::speed_label(rate)}" }
                }
                span { style: "width: 14px;" }
                span { style: "font-size: 13px; color: {INK_3}; margin-right: 4px;", "Volume" }
                button { style: "{tool(false, true)} width: 48px;", "aria-label": "Quieter", onclick: move |_| st.set_volume(((st.volume)() - 0.1).max(0.0)), "−" }
                span { style: "font-size: 14px; min-width: 44px; text-align: center;", "{vol}%" }
                button { style: "{tool(false, true)} width: 48px;", "aria-label": "Louder", onclick: move |_| st.set_volume(((st.volume)() + 0.1).min(1.0)), "+" }
                span { style: "width: 14px;" }
                button {
                    style: "{tool(false, true)} padding: 0 12px; font-size: 15px;",
                    "aria-label": "Output: {device}",
                    onclick: move |_| {
                        let all = output_devices();
                        if all.is_empty() { return; }
                        let now = player().device();
                        let next = all.iter().position(|n| *n == now).map_or(0, |i| (i + 1) % all.len());
                        if let Some(name) = all.get(next) { player().open_device(Some(name.clone())); }
                    },
                    "Out: {device}"
                }
            }
            if !status.is_empty() {
                div { style: "font-size: 13px; color: #f87171;", "{status}" }
            }
            div { style: "display: flex; flex-wrap: wrap; gap: 8px;",
                if !song.key.is_empty() { {fact(&format!("Key {}", song.key), "#1c1917", "#fde68a")} }
                if !song.tempo.is_empty() { {fact(&tempo_text(&song.tempo), "#1c1917", "#fde68a")} }
                {fact(&format!("Profile: {}", song.profile.label()), "#172554", "#bfdbfe")}
                if !song.guitar.is_empty() { {fact(&format!("Guitar: {}", song.guitar), "#052e16", "#bbf7d0")} }
                if !song.audio_kind.is_empty() { {fact(&song.audio_kind, "#18181b", INK_3)} }
            }
            div { style: "font-size: 12px; color: {INK_3};",
                "The full Session app opens here with the unified shell; for now this plays the song's reference."
            }
        }
    }
}
