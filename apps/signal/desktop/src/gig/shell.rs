//! The gig on the iPad rig: its pieces lent to the tablet layout
//! ([`guitar::ui::GigSlots`]) so the rig becomes the gig shell.
//!
//!   header     a Session / Signal toggle, then the transport you need with a
//!              guitar on: −5 s, play/pause, +5 s, the section loop, speed
//!              and volume
//!   progress   Session's own song progress bar (its sections, the playhead;
//!              a tap on a section plays from it), the song steps either side
//!   main       Perform's middle: the chosen song's chart, the PDF or the
//!              Keyflow render — or, on Session, the song as Session lays it
//!              out (its sections in a lane, the big clock, speed, volume)
//!   foot       the chart's controls in the rig's foot bar: PDF / Keyflow,
//!              zoom, nudge up / down, fit
//!   follow     the gig's set as one of the rig's setlists (made once; its
//!              songs get no parts from the gig's sections — those are the
//!              progress bar's), and the song up in the rig's Setlist view
//!              is the song whose chart and track are up
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

use dioxus::prelude::*;
use guitar::ui::GigSlots;
use session::ui::components::progress::{ProgressSection, SongProgressBar};
use signal_rig_perform_ui::{PerfState, RigPerformClient};

use super::player::{output_devices, player};
use super::{Gig, Section, Song, clock, gigs_root, image_uri, list_gigs};

// The rig frame's colours (signal-rig-shell's tokens).
const SHEET: &str = "#0e0e11";
const MAIN: &str = "#0f0f12";
const RULE: &str = "#222228";
const INK: &str = "#e4e4e7";
const INK_2: &str = "#a1a1aa";
const INK_3: &str = "#8e8e98";
const FILL: &str = "rgba(255,255,255,0.06)";
const ACCENT: &str = "#38bdf8";
const LOOP: &str = "#f59e0b";
const FONT: &str = "Inter, -apple-system, sans-serif";

/// The speeds the header's speed button steps through, percent.
const SPEEDS: &[u32] = &[100, 90, 80, 75, 70, 60, 50];

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
    header: header_slot,
    progress: progress_slot,
    main: main_slot,
    foot: foot_slot,
    follow: follow_slot,
};

fn header_slot() -> Element {
    rsx! { GigHeader {} }
}
fn progress_slot() -> Element {
    rsx! { GigProgress {} }
}
fn main_slot() -> Element {
    rsx! { GigMain {} }
}
fn foot_slot() -> Element {
    rsx! { GigFoot {} }
}
fn follow_slot() -> Element {
    rsx! { GigFollow {} }
}

/// Make the gig shell's state and lend its slots to the rig below — when
/// there is a gig on this device (the app's own, or one put in Files).
pub fn use_gig_shell() {
    let st = use_context_provider(|| GigState {
        gigs: Signal::new(list_gigs()),
        gig_at: Signal::new(0),
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
    });
    use_hook(move || {
        if !st.gigs.peek().is_empty() {
            provide_context(SLOTS);
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

fn btn(on: bool) -> String {
    let (bg, ink) = if on { (ACCENT, "#04121c") } else { (FILL, INK) };
    format!(
        "height: 38px; min-width: 36px; padding: 0 8px; flex-shrink: 0; display: flex; align-items: center; \
         justify-content: center; gap: 6px; border: none; border-radius: 6px; background: {bg}; color: {ink}; \
         font-size: 14px; font-weight: 650; font-family: {FONT}; cursor: pointer;"
    )
}

fn seg(on: bool, enabled: bool) -> String {
    format!(
        "height: 32px; padding: 0 12px; display: flex; align-items: center; border: none; border-radius: 5px; \
         background: {}; color: {}; font-size: 14px; font-weight: {}; font-family: {FONT}; cursor: pointer;",
        if on { "#2a2a31" } else { "transparent" },
        if !enabled {
            "#4a4a52"
        } else if on {
            INK
        } else {
            INK_3
        },
        if on { 750 } else { 600 },
    )
}

/// An 18-unit icon, filled.
#[component]
fn Icon(d: &'static str, ink: &'static str) -> Element {
    rsx! {
        svg { width: "18", height: "18", view_box: "0 0 18 18", style: "display: block; flex-shrink: 0;",
            path { d: "{d}", fill: "{ink}" }
        }
    }
}

const PLAY: &str = "M5 3v12l10-6Z";
const PAUSE: &str = "M4.5 3h3.2v12H4.5ZM10.3 3h3.2v12h-3.2Z";
const PREV: &str = "M3 3h2v12H3ZM15 3v12L6 9Z";
const NEXT: &str = "M13 3h2v12h-2ZM3 3v12l9-6Z";

// ── Header ──────────────────────────────────────────────────────────────────

/// The Session / Signal toggle and the transport.
#[component]
fn GigHeader() -> Element {
    let st = use_context::<GigState>();
    let mut studio = st.studio;
    let has_song = (st.song_at)().is_some();
    let pct = ((st.speed)() * 100.0).round();
    let vol = (f64::from((st.volume)()) * 100.0).round();
    let loop_label = match ((st.looped)(), (st.mark_a)()) {
        (Some(_), _) => "Loop on".to_string(),
        (None, Some(a)) => format!("A {} → B", clock(a)),
        (None, None) => "Loop".to_string(),
    };
    rsx! {
        div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 5px; padding: 0 8px; border-left: 1px solid {RULE};",
            // Session / Signal.
            div { style: "display: flex; padding: 3px; gap: 2px; border-radius: 7px; background: #08080a; margin-right: 6px;",
                button { style: seg(studio(), true), onclick: move |_| studio.set(true), "Session" }
                button { style: seg(!studio(), true), onclick: move |_| studio.set(false), "Signal" }
            }
            button { style: btn(false), disabled: !has_song,
                onclick: move |_| player().seek((player().position() - 5.0).max(0.0)), "−5" }
            button {
                style: "{btn((st.playing)())} min-width: 56px;",
                "aria-label": if (st.playing)() { "Pause" } else { "Play" },
                disabled: !has_song,
                onclick: move |_| player().toggle(),
                if (st.loading)() {
                    span { style: "font-size: 12px;", "…" }
                } else if (st.playing)() {
                    Icon { d: PAUSE, ink: "#04121c" }
                } else {
                    Icon { d: PLAY, ink: INK }
                }
            }
            button { style: btn(false), disabled: !has_song,
                onclick: move |_| player().seek(player().position() + 5.0), "+5" }
            button {
                style: "{btn((st.looped)().is_some())} {loop_color((st.looped)().is_some() || (st.mark_a)().is_some())}",
                disabled: !has_song,
                onclick: move |_| st.loop_press(),
                "{loop_label}"
            }
            button {
                style: btn(pct < 99.5),
                "aria-label": "Speed (keeps the pitch)",
                onclick: move |_| {
                    let now = (st.speed)() * 100.0;
                    let at = SPEEDS.iter().position(|s| (f64::from(*s) - now).abs() < 0.5).unwrap_or(0);
                    let next = SPEEDS.get(at + 1).or(SPEEDS.first()).copied().unwrap_or(100);
                    st.set_speed(f64::from(next) / 100.0);
                },
                "{pct}%"
            }
            button { style: btn(false), "aria-label": "Quieter",
                onclick: move |_| st.set_volume(((st.volume)() - 0.1).max(0.0)), "−" }
            span { style: "font-size: 12px; color: {INK_2}; min-width: 34px; text-align: center; font-family: {FONT};", "{vol}" }
            button { style: btn(false), "aria-label": "Louder",
                onclick: move |_| st.set_volume(((st.volume)() + 0.1).min(2.0)), "+" }
        }
    }
}

fn loop_color(on: bool) -> String {
    if on {
        format!("box-shadow: inset 0 0 0 1px {LOOP};")
    } else {
        String::new()
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
/// a tap), and the bar's sections.
fn progress_sections(song: &Song, dur: f64) -> (Vec<f64>, Vec<ProgressSection>) {
    let mut parts = spans(&song.sections, dur);
    if parts.is_empty() && dur > 0.0 {
        parts.push(Span {
            name: song.title.clone(),
            start: 0.0,
            end: dur,
            section: Some(0),
        });
    }
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

/// The song's progress: Session's bar, the clock, the song steps.
#[component]
fn GigProgress() -> Element {
    let st = use_context::<GigState>();
    let perf = try_use_context::<PerfState>();
    let song = st.song();
    let d = (st.dur)();
    let p = (st.pos)();
    // A song step: the rig's setlist moves on (the chart follows), or,
    // with the rig on another set, the gig's own list.
    let step = move |by: isize| match rig_on_gig(st, perf) {
        Some((rig, at)) => {
            let n = perf.map_or(0, |s| s.perf.peek().songs.len());
            let next = at.saturating_add_signed(by).min(n.saturating_sub(1));
            if next != at {
                let next = u32::try_from(next).unwrap_or(0);
                spawn(async move {
                    let _ = rig.select_song(next).await;
                });
            }
        }
        None => st.step(by),
    };
    let Some(song) = song else {
        return rsx! {
            div { style: "flex-shrink: 0; height: 40px; display: flex; align-items: center; padding: 0 16px; \
                          border-bottom: 1px solid {RULE}; background: {SHEET}; color: {INK_3}; font-size: 13px; font-family: {FONT};",
                "Pick a song in Setlist — its chart opens below and its track loads here."
            }
        };
    };
    let (starts, bar) = if d > 0.0 {
        progress_sections(&song, d)
    } else {
        (Vec::new(), Vec::new())
    };
    let progress = if d > 0.0 {
        (p / d * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let small = "height: 36px; width: 36px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; \
                 border: none; border-radius: 6px; background: rgba(255,255,255,0.06); cursor: pointer;";
    // Key and tempo first, then the rest of the key's line (a progression,
    // a relative minor) — that is what truncates.
    let (key, key_rest) = signal_rig_perform_ui::colors::split_key(&song.key);
    let facts = [
        (!key.is_empty()).then(|| format!("Key {key}")),
        (!song.tempo.is_empty()).then(|| tempo_text(&song.tempo)),
        (!key_rest.is_empty()).then_some(key_rest),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    rsx! {
        div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 10px; padding: 4px 12px; \
                      border-bottom: 1px solid {RULE}; background: {SHEET}; font-family: {FONT};",
            button { style: small, "aria-label": "Previous song", onclick: move |_| step(-1),
                Icon { d: PREV, ink: INK }
            }
            // The title first: it takes what it needs (up to a third of the
            // bar) and ellipsizes last; the facts under it truncate first.
            div { style: "flex: 1 1 0; min-width: 0; max-width: 34%; display: flex; flex-direction: column; gap: 1px; overflow: hidden;",
                span { style: "min-width: 0; font-size: 15px; font-weight: 750; color: {INK}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                    "{song.number}. {song.title}"
                }
                span { style: "min-width: 0; font-size: 11px; color: #fde68a; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{facts}" }
            }
            span { style: "flex-shrink: 0; font-size: 13px; color: {INK}; font-variant-numeric: tabular-nums; min-width: 40px; text-align: right;", "{clock(p)}" }
            div { style: "flex: 2 1 0; min-width: 0;",
                if bar.is_empty() {
                    div { style: "height: 2.75rem; display: flex; align-items: center; color: {INK_3}; font-size: 12px;",
                        if song.audio.is_none() { "No reference track — the chart only." } else { "Loading the track…" }
                    }
                } else {
                    SongProgressBar {
                        progress,
                        sections: bar,
                        height: "2.75rem".to_string(),
                        on_section_click: move |k: usize| {
                            if let Some(at) = starts.get(k) {
                                player().seek(*at);
                            }
                        },
                    }
                }
            }
            span { style: "flex-shrink: 0; font-size: 13px; color: {INK_2}; font-variant-numeric: tabular-nums; min-width: 40px;", "{clock(d)}" }
            button { style: small, "aria-label": "Next song", onclick: move |_| step(1),
                Icon { d: NEXT, ink: INK }
            }
        }
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
            spawn(async move {
                seed_setlist(r.clone(), gig).await;
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

/// The chart's controls, in the rig's foot bar: PDF / Keyflow, zoom, nudge
/// up and down, and fit (the page's width, at the top).
#[component]
fn GigFoot() -> Element {
    let st = use_context::<GigState>();
    let mut keyflow = st.keyflow;
    let song = st.song();
    let has_keyflow = song
        .as_ref()
        .is_some_and(|s| s.charts.iter().any(|c| is_keyflow(&c.label)));
    let has_pdf = song
        .as_ref()
        .is_some_and(|s| s.charts.iter().any(|c| !is_keyflow(&c.label)));
    let place = st.place();
    let pct = (place.zoom * 100.0).round();
    let small = "height: 34px; min-width: 34px; padding: 0 8px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; \
                 border: none; border-radius: 6px; background: rgba(255,255,255,0.06); color: #e4e4e7; font-size: 15px; font-weight: 650; cursor: pointer;";
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
    rsx! {
        div { style: "display: flex; align-items: center; gap: 6px; font-family: {FONT};",
            div { style: "flex-shrink: 0; display: flex; padding: 3px; gap: 2px; border-radius: 7px; background: #08080a;",
                button { style: seg(!keyflow(), has_pdf), onclick: move |_| keyflow.set(false), "PDF" }
                button { style: seg(keyflow(), has_keyflow), onclick: move |_| keyflow.set(true), "Keyflow" }
            }
            button { style: small, "aria-label": "Zoom out", onclick: move |_| zoom(-ZOOM_STEP), "−" }
            span { style: "font-size: 12px; color: {INK_2}; min-width: 40px; text-align: center; font-variant-numeric: tabular-nums;", "{pct}%" }
            button { style: small, "aria-label": "Zoom in", onclick: move |_| zoom(ZOOM_STEP), "+" }
            button { style: small, "aria-label": "Chart up", onclick: move |_| nudge(NUDGE), "▲" }
            button { style: small, "aria-label": "Chart down", onclick: move |_| nudge(-NUDGE), "▼" }
            button { style: small, "aria-label": "Fit the width",
                onclick: move |_| st.set_place(ChartPlace::default(), true), "Fit" }
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
        div { style: "flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; padding: 16px 18px; \
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
                                            style: "height: 28px; min-width: 28px; border: none; border-radius: 5px; background: {FILL}; color: {INK}; font-size: 13px; cursor: pointer;",
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
                for s in SPEEDS.iter().rev().copied() {
                    button { key: "{s}", style: btn((pct - f64::from(s)).abs() < 0.5),
                        onclick: move |_| st.set_speed(f64::from(s) / 100.0), "{s}%" }
                }
                span { style: "width: 14px;" }
                span { style: "font-size: 13px; color: {INK_3}; margin-right: 4px;", "Volume" }
                button { style: btn(false), onclick: move |_| st.set_volume(((st.volume)() - 0.1).max(0.0)), "−" }
                span { style: "font-size: 14px; min-width: 44px; text-align: center;", "{vol}%" }
                button { style: btn(false), onclick: move |_| st.set_volume(((st.volume)() + 0.1).min(2.0)), "+" }
                span { style: "width: 14px;" }
                button {
                    style: btn(false),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sec(name: &str, start: f64, end: f64) -> Section {
        Section {
            name: name.to_string(),
            start,
            end,
        }
    }

    #[test]
    fn spans_keep_the_gaps_between_sections() {
        let s = spans(&[sec("Intro", 2.0, 10.0), sec("Verse", 10.0, 30.0)], 40.0);
        let got: Vec<_> = s
            .iter()
            .map(|x| (x.name.as_str(), x.start, x.end, x.section))
            .collect();
        assert_eq!(
            got,
            vec![
                ("", 0.0, 2.0, None),
                ("Intro", 2.0, 10.0, Some(0)),
                ("Verse", 10.0, 30.0, Some(1)),
                ("", 30.0, 40.0, None),
            ]
        );
    }

    #[test]
    fn only_parts_named_as_the_songs_sections_are_dropped() {
        let parts: Vec<String> = ["Intro", "Verse", "Solo (mine)", "Chorus", "verse"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let sections = [
            sec("Intro", 0.0, 5.0),
            sec("Verse", 5.0, 20.0),
            sec("Chorus", 20.0, 30.0),
            sec("Verse", 30.0, 40.0),
        ];
        // Exact names only: a hand-made part and a different case stay.
        assert_eq!(
            seeded_parts(&parts, &sections),
            vec!["Intro", "Verse", "Chorus"]
        );
        assert!(seeded_parts(&parts, &[]).is_empty());
    }

    #[test]
    fn no_sections_no_spans() {
        assert!(spans(&[], 100.0).is_empty());
    }

    #[test]
    fn a_section_past_the_end_is_cut_to_it() {
        let s = spans(&[sec("Out", 5.0, 50.0)], 20.0);
        assert_eq!(s.last().map(|x| x.end), Some(20.0));
    }

    #[test]
    fn songs_match_across_punctuation_and_case() {
        assert!(same_song("Don't Stop Believin'", "dont stop believin"));
        assert!(!same_song("Hello", "Hallo"));
    }

    #[test]
    fn a_tempo_reads_its_first_number() {
        assert_eq!(bpm_of("246 in 12/8 (~123 felt)"), 246);
        assert_eq!(bpm_of("120"), 120);
        assert_eq!(bpm_of("free"), 0);
    }

    #[test]
    fn a_chart_place_stays_in_bounds() {
        let p = ChartPlace {
            zoom: 0.1,
            x: 0.0,
            y: -9999.0,
        }
        .clamped();
        assert!((p.zoom - ZOOM_MIN).abs() < f64::EPSILON);
        assert!(p.y >= -400.0);
    }

    #[test]
    fn the_progress_bar_spans_the_song() {
        let song = Song {
            number: 1,
            title: "T".into(),
            artist: String::new(),
            key: String::new(),
            tempo: String::new(),
            guitar: String::new(),
            notes: String::new(),
            audio: None,
            audio_kind: String::new(),
            charts: Vec::new(),
            pdf: None,
            sections: vec![sec("Intro", 0.0, 10.0), sec("Verse", 10.0, 40.0)],
            profile: crate::gig::SongProfile::default(),
        };
        let (starts, bar) = progress_sections(&song, 40.0);
        assert_eq!(starts, vec![0.0, 10.0]);
        assert!((bar[1].end_percent - 100.0).abs() < 1e-9);
        assert_eq!(bar[0].short_name, "INT");
    }

    #[test]
    fn short_names_keep_their_letter() {
        assert_eq!(short_name("Interlude A"), "INT A");
    }
}
