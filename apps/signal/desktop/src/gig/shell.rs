//! The gig on the iPad rig: its pieces lent to the tablet layout
//! ([`guitar::ui::GigSlots`]) so the rig becomes the gig shell.
//!
//!   header     a Studio / Signal toggle, then the transport you need with a
//!              guitar on: previous song, −5 s, play/pause, +5 s, next song,
//!              the section loop, speed and volume
//!   progress   the song's timeline under the top bar: its sections named
//!              over a row of seek targets, the playhead filling it
//!   main       Perform's middle: the chosen song's chart, the PDF or the
//!              Keyflow render — or, on Studio, the song as Session lays it
//!              out (its sections in a lane, the big clock, speed, volume)
//!   setlist    the sidebar on Setlist: the gig's set; a tap shows the
//!              song's chart and loads its track
//!
//! The model, the player and the chart images are the gig page's own
//! (`super`, `super::player`) — this is a second face on them, not a second
//! player. One [`GigState`] (context, made by [`use_gig_shell`]) carries the
//! choice of song and mirrors the player ten times a second; every piece
//! reads it.
//!
//! The full Session UI inside Signal (and Signal inside Session) comes with
//! the unified shell (STA-147); until then Studio shows the song's session
//! view here, playing the same reference.

use std::time::Duration;

use dioxus::prelude::*;
use guitar::ui::GigSlots;

use super::player::{output_devices, player};
use super::{Gig, Section, Song, clock, image_uri, list_gigs};

// The rig frame's colours (signal-rig-shell's tokens).
const SHEET: &str = "#0e0e11";
const MAIN: &str = "#0f0f12";
const RULE: &str = "#222228";
const INK: &str = "#e4e4e7";
const INK_2: &str = "#a1a1aa";
const INK_3: &str = "#8e8e98";
const FILL: &str = "rgba(255,255,255,0.06)";
const LIVE: &str = "#22c55e";
const ACCENT: &str = "#38bdf8";
const LOOP: &str = "#f59e0b";
const FONT: &str = "Inter, -apple-system, sans-serif";

/// Seek targets across the progress bar (no hit-testing needed, which Blitz
/// and a WebView do differently).
const CELLS: usize = 96;

/// The speeds the header's speed button steps through, percent.
const SPEEDS: &[u32] = &[100, 90, 80, 75, 70, 60, 50];

/// The gig, the song picked, and the player as the page shows it.
#[derive(Clone, Copy)]
pub struct GigState {
    pub gigs: Signal<Vec<Gig>>,
    pub gig_at: Signal<usize>,
    pub song_at: Signal<Option<usize>>,
    /// The chart shown: the Keyflow render (else the band's PDF).
    pub keyflow: Signal<bool>,
    /// The chart fitted to the page's height (else to its width).
    pub fit_page: Signal<bool>,
    /// Studio (the song's session view) rather than Signal (its chart).
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
        if let Some(s) = self
            .song()
            .and_then(|s| s.sections.iter().find(|x| p >= x.start && p < x.end).cloned())
        {
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
    setlist: setlist_slot,
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
fn setlist_slot() -> Element {
    rsx! { GigSetlist {} }
}

/// Make the gig shell's state and lend its slots to the rig below — when
/// there is a gig on this device (the app's own, or one put in Files).
pub fn use_gig_shell() {
    let st = use_context_provider(|| GigState {
        gigs: Signal::new(list_gigs()),
        gig_at: Signal::new(0),
        song_at: Signal::new(None),
        keyflow: Signal::new(false),
        fit_page: Signal::new(false),
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

fn usize_f64(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

fn btn(on: bool) -> String {
    let (bg, ink) = if on { (ACCENT, "#04121c") } else { (FILL, INK) };
    format!(
        "height: 38px; min-width: 36px; padding: 0 8px; flex-shrink: 0; display: flex; align-items: center; \
         justify-content: center; gap: 6px; border: none; border-radius: 6px; background: {bg}; color: {ink}; \
         font-size: 14px; font-weight: 650; font-family: {FONT}; cursor: pointer;"
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

/// The Studio / Signal toggle and the transport.
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
    let seg = |on: bool| {
        format!(
            "height: 32px; padding: 0 10px; display: flex; align-items: center; border: none; border-radius: 5px; \
             background: {}; color: {}; font-size: 14px; font-weight: {}; font-family: {FONT}; cursor: pointer;",
            if on { "#2a2a31" } else { "transparent" },
            if on { INK } else { INK_3 },
            if on { 750 } else { 600 },
        )
    };
    rsx! {
        div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 5px; padding: 0 8px; border-left: 1px solid {RULE};",
            // Studio / Signal.
            div { style: "display: flex; padding: 3px; gap: 2px; border-radius: 7px; background: #08080a; margin-right: 6px;",
                button { style: seg(studio()), onclick: move |_| studio.set(true), "Studio" }
                button { style: seg(!studio()), onclick: move |_| studio.set(false), "Signal" }
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
            out.push(Span { name: String::new(), start: at, end: a, section: None });
        }
        out.push(Span { name: s.name.clone(), start: a, end: b, section: Some(k) });
        at = b;
    }
    if dur - at > 0.25 && !out.is_empty() {
        out.push(Span { name: String::new(), start: at, end: dur, section: None });
    }
    out
}

/// The song's timeline: sections over seek targets, the playhead filling it.
#[component]
fn GigProgress() -> Element {
    let st = use_context::<GigState>();
    let song = st.song();
    let d = (st.dur)();
    let p = (st.pos)();
    let looped = (st.looped)();
    let Some(song) = song else {
        return rsx! {
            div { style: "flex-shrink: 0; height: 40px; display: flex; align-items: center; padding: 0 16px; \
                          border-bottom: 1px solid {RULE}; background: {SHEET}; color: {INK_3}; font-size: 13px; font-family: {FONT};",
                "Pick a song in Setlist — its chart opens below and its track loads here."
            }
        };
    };
    let parts = spans(&song.sections, d);
    let current = song.sections.iter().position(|s| p >= s.start && p < s.end);
    let small = "height: 36px; width: 36px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; \
                 border: none; border-radius: 6px; background: rgba(255,255,255,0.06); cursor: pointer;";
    rsx! {
        div { style: "flex-shrink: 0; display: flex; align-items: center; gap: 10px; padding: 4px 12px; \
                      border-bottom: 1px solid {RULE}; background: {SHEET}; font-family: {FONT};",
            button { style: small, "aria-label": "Previous song", onclick: move |_| st.step(-1),
                Icon { d: PREV, ink: INK }
            }
            span { style: "font-size: 13px; color: {INK}; font-variant-numeric: tabular-nums; min-width: 40px; text-align: right;", "{clock(p)}" }
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                if !parts.is_empty() && d > 0.0 {
                    div { style: "height: 18px; display: flex; gap: 1px;",
                        for (k, s) in parts.iter().enumerate() {
                            {
                                let grow = (s.end - s.start).max(0.01);
                                let on = s.section.is_some() && s.section == current;
                                let start = s.start;
                                let bg = if on { "#0c4a6e" } else if s.section.is_some() { "#1c1c21" } else { "transparent" };
                                let ink = if on { "#e0f2fe" } else { INK_2 };
                                rsx! {
                                    div {
                                        key: "{k}",
                                        style: "flex: {grow} 1 0px; min-width: 0; overflow: hidden; white-space: nowrap; border-radius: 3px; \
                                                background: {bg}; color: {ink}; font-size: 11px; font-weight: 650; padding: 1px 4px; box-sizing: border-box;",
                                        onclick: move |_| player().seek(start),
                                        "{s.name}"
                                    }
                                }
                            }
                        }
                    }
                }
                div { style: "height: 22px; display: flex; border-radius: 5px; overflow: hidden; background: #18181b;",
                    for i in 0..CELLS {
                        {
                            let from = (usize_f64(i) / usize_f64(CELLS) * d).clamp(0.0, d.max(0.0));
                            let mid = ((usize_f64(i) + 0.5) / usize_f64(CELLS)) * d;
                            let played = d > 0.0 && mid <= p;
                            let odd = song.sections.iter().position(|s| mid >= s.start && mid < s.end).is_some_and(|k| k % 2 == 1);
                            let in_loop = looped.is_some_and(|(a, b)| mid >= a && mid < b);
                            let bg = match (played, odd) {
                                (true, false) => "#0284c7",
                                (true, true) => ACCENT,
                                (false, false) => "#27272a",
                                (false, true) => "#34343b",
                            };
                            let edge = if in_loop { "border-top: 3px solid #f59e0b; box-sizing: border-box;" } else { "" };
                            rsx! {
                                div { key: "{i}", style: "flex: 1; height: 100%; background: {bg}; {edge}", onclick: move |_| player().seek(from) }
                            }
                        }
                    }
                }
            }
            span { style: "font-size: 13px; color: {INK_2}; font-variant-numeric: tabular-nums; min-width: 40px;", "{clock(d)}" }
            button { style: small, "aria-label": "Next song", onclick: move |_| st.step(1),
                Icon { d: NEXT, ink: INK }
            }
        }
    }
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

fn is_keyflow(label: &str) -> bool {
    label.to_lowercase().contains("keyflow")
}

fn fact(text: &str, bg: &str, ink: &str) -> Element {
    rsx! {
        span { style: "flex-shrink: 0; font-size: 13px; padding: 3px 8px; border-radius: 5px; background: {bg}; color: {ink};", "{text}" }
    }
}

/// The song's chart: the band's PDF (every page) or the Keyflow render.
#[component]
fn ChartView() -> Element {
    let st = use_context::<GigState>();
    let mut keyflow = st.keyflow;
    let mut fit_page = st.fit_page;
    let size = try_use_context::<guitar::ui::WindowSize>();
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
    let has_keyflow = song.charts.iter().any(|c| is_keyflow(&c.label));
    let has_pdf = song.charts.iter().any(|c| !is_keyflow(&c.label));
    // The page's height for "Page": the window less the bars around it.
    let page_h = size.map_or(560.0, |s| (s.0.peek().1 - 48.0 - 52.0 - 56.0 - 52.0 - 24.0).max(240.0));
    let img_style = if fit_page() {
        format!("height: {page_h:.0}px; width: auto; max-width: 100%; display: block; margin: 0 auto 10px; background: #fff; border-radius: 4px;")
    } else {
        "width: 100%; height: auto; display: block; margin-bottom: 10px; background: #fff; border-radius: 4px;".to_string()
    };
    let seg = |on: bool, enabled: bool| {
        format!(
            "height: 32px; padding: 0 14px; display: flex; align-items: center; border: none; border-radius: 5px; \
             background: {}; color: {}; font-size: 14px; font-weight: {}; font-family: {FONT}; cursor: pointer;",
            if on { "#2a2a31" } else { "transparent" },
            if !enabled { "#4a4a52" } else if on { INK } else { INK_3 },
            if on { 750 } else { 600 },
        )
    };
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; background: {MAIN}; color: {INK}; font-family: {FONT};",
            div { style: "flex-shrink: 0; min-height: 52px; display: flex; align-items: center; gap: 8px; padding: 6px 14px; border-bottom: 1px solid {RULE}; box-sizing: border-box;",
                span { style: "flex-shrink: 0; font-size: 13px; color: {INK_3};", "{song.number}" }
                span { style: "flex-shrink: 0; font-size: 18px; font-weight: 800;", "{song.title}" }
                if !song.artist.is_empty() {
                    span { style: "flex-shrink: 0; font-size: 13px; color: {INK_2};", "{song.artist}" }
                }
                if !song.key.is_empty() { {fact(&format!("Key {}", song.key), "#1c1917", "#fde68a")} }
                if !song.tempo.is_empty() { {fact(&format!("{} bpm", song.tempo), "#1c1917", "#fde68a")} }
                if !song.guitar.is_empty() {
                    span { style: "min-width: 0; overflow: hidden; white-space: nowrap; font-size: 13px; padding: 3px 8px; border-radius: 5px; background: #052e16; color: #bbf7d0;",
                        "Guitar: {song.guitar}"
                    }
                }
                span { style: "flex: 1;" }
                div { style: "flex-shrink: 0; display: flex; padding: 3px; gap: 2px; border-radius: 7px; background: #08080a;",
                    button { style: seg(!fit_page(), true), onclick: move |_| fit_page.set(false), "Width" }
                    button { style: seg(fit_page(), true), onclick: move |_| fit_page.set(true), "Page" }
                }
                div { style: "flex-shrink: 0; display: flex; padding: 3px; gap: 2px; border-radius: 7px; background: #08080a;",
                    button { style: seg(!keyflow(), has_pdf), onclick: move |_| keyflow.set(false), "PDF" }
                    button { style: seg(keyflow(), has_keyflow), onclick: move |_| keyflow.set(true), "Keyflow" }
                }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 10px 14px;",
                if pages.is_empty() {
                    div { style: "padding: 24px; color: {INK_3}; font-size: 14px;",
                        if keyflow() { "No Keyflow chart for this song." } else { "No PDF chart for this song." }
                    }
                }
                for (k, (label, uri)) in pages.into_iter().enumerate() {
                    img { key: "{k}-{label}", src: "{uri}", style: "{img_style}" }
                }
                if !song.notes.is_empty() {
                    div { style: "padding: 4px 2px 12px; font-size: 13px; color: {INK_2};", "{song.notes}" }
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
                "Studio: pick a song in Setlist to hear its reference."
            }
        };
    };
    let d = (st.dur)();
    let p = (st.pos)();
    let looped = (st.looped)();
    let mut parts = spans(&song.sections, d);
    if parts.is_empty() && d > 0.0 {
        parts.push(Span { name: "Full song".to_string(), start: 0.0, end: d, section: None });
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
                if !song.tempo.is_empty() { {fact(&format!("{} bpm", song.tempo), "#1c1917", "#fde68a")} }
                if !song.guitar.is_empty() { {fact(&format!("Guitar: {}", song.guitar), "#052e16", "#bbf7d0")} }
                if !song.audio_kind.is_empty() { {fact(&song.audio_kind, "#18181b", INK_3)} }
            }
            div { style: "font-size: 12px; color: {INK_3};",
                "The full Session app opens here with the unified shell; for now this plays the song's reference."
            }
        }
    }
}

// ── Setlist ─────────────────────────────────────────────────────────────────

/// The gig's set, in the sidebar.
#[component]
fn GigSetlist() -> Element {
    let st = use_context::<GigState>();
    let Some(gig) = st.gig() else {
        return rsx! {};
    };
    let at = (st.song_at)();
    rsx! {
        div { style: "flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: {FONT}; color: {INK};",
            div { style: "flex-shrink: 0; padding: 10px 14px 8px; display: flex; flex-direction: column; gap: 3px; border-bottom: 1px solid {RULE};",
                div { style: "display: flex; align-items: baseline; gap: 8px;",
                    span { style: "font-size: 16px; font-weight: 800;", "{gig.name}" }
                    span { style: "font-size: 12px; color: {INK_3};", "{gig.songs.len()} songs" }
                }
                for (i, line) in gig.dates.iter().take(1).enumerate() {
                    span { key: "{i}", style: "font-size: 11px; color: {INK_2};", "{line}" }
                }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: auto; padding: 4px 6px;",
                for (i, s) in gig.songs.iter().enumerate() {
                    {
                        let on = at == Some(i);
                        let bg = if on { "#0c4a6e" } else { "transparent" };
                        let rail = if on { LIVE } else { "transparent" };
                        rsx! {
                            button {
                                key: "{i}",
                                style: "width: 100%; min-height: 48px; text-align: left; border: none; border-radius: 6px; padding: 6px 10px 6px 8px; \
                                        margin-bottom: 2px; background: {bg}; color: {INK}; cursor: pointer; display: flex; gap: 10px; \
                                        align-items: center; border-left: 3px solid {rail}; box-sizing: border-box; font-family: {FONT};",
                                onclick: move |_| st.select(i),
                                span { style: "width: 22px; flex-shrink: 0; font-size: 12px; color: {INK_3}; text-align: right;", "{s.number}" }
                                div { style: "display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1;",
                                    span { style: "font-size: 15px; font-weight: 650; white-space: nowrap; overflow: hidden;", "{s.title}" }
                                    span { style: "font-size: 11px; color: {INK_2}; white-space: nowrap; overflow: hidden;",
                                        if s.artist.is_empty() { "—" } else { "{s.artist}" }
                                    }
                                }
                                div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 2px; flex-shrink: 0;",
                                    span { style: "font-size: 11px; color: #fde68a;", "{s.key}" }
                                    span { style: "font-size: 11px; color: {INK_3};",
                                        if s.audio.is_some() { "{s.tempo}" } else { "chart only" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sec(name: &str, start: f64, end: f64) -> Section {
        Section { name: name.to_string(), start, end }
    }

    #[test]
    fn spans_keep_the_gaps_between_sections() {
        let s = spans(&[sec("Intro", 2.0, 10.0), sec("Verse", 10.0, 30.0)], 40.0);
        let got: Vec<_> = s.iter().map(|x| (x.name.as_str(), x.start, x.end, x.section)).collect();
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
    fn no_sections_no_spans() {
        assert!(spans(&[], 100.0).is_empty());
    }

    #[test]
    fn a_section_past_the_end_is_cut_to_it() {
        let s = spans(&[sec("Out", 5.0, 50.0)], 20.0);
        assert_eq!(s.last().map(|x| x.end), Some(20.0));
    }
}
