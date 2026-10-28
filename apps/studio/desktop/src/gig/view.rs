//! The gig page on the shared studio shell (`app_shell::ui`): the set in
//! the left sidebar, the song's facts in the top drawer, its chart in main,
//! and the transport and practice controls in the bottom drawer
//! (docs/design/unified-shell/spec.md §6, "Setlist / practice view").
//!
//! Each region is a props-less panel registered under `gig.*`, reading the
//! page's state ([`GigState`]) from context, so the same panels can move to
//! another region, or into Session's shell, without being rewritten.

use std::time::Duration;

use app_shell::ui::{Panel, PanelConstraints, PanelRegistry, Region, ShellFrame};
use dioxus::prelude::*;

use super::player::{output_devices, player};
use super::{Gig, Song, clock, image_uri, list_gigs};

/// Cells in the progress bar — each one a seek target (no hit-testing
/// needed, which Blitz and a WebView do differently).
const CELLS: usize = 120;

const SPEEDS: &[u32] = &[50, 60, 70, 75, 80, 85, 90, 100];

const ACCENT: &str = "#38bdf8";

/// The scope the gig page's layout is saved under.
const SCOPE: &str = "signal.gig";

fn chip(active: bool) -> &'static str {
    if active {
        "border: 1px solid #38bdf8; background: #0c4a6e; color: #e0f2fe; border-radius: 8px; \
         padding: 6px 10px; font-size: 13px; font-weight: 600; cursor: pointer;"
    } else {
        "border: 1px solid #27272a; background: #18181b; color: #d4d4d8; border-radius: 8px; \
         padding: 6px 10px; font-size: 13px; font-weight: 600; cursor: pointer;"
    }
}

const BIG: &str = "border: none; background: #0ea5e9; color: #04121c; border-radius: 10px; \
                   padding: 10px 22px; font-size: 16px; font-weight: 800; cursor: pointer; min-width: 110px;";

fn seconds_of(frac: f64, duration: f64) -> f64 {
    (frac * duration).clamp(0.0, duration.max(0.0))
}

fn usize_f64(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The gig page's state, shared by its panels through context.
///
/// All signals (and one handler), so it is `Copy`: a panel destructures
/// what it needs.
#[derive(Clone, Copy)]
struct GigState {
    gigs: Signal<Vec<Gig>>,
    gig_at: Signal<usize>,
    song_at: Signal<Option<usize>>,
    chart_at: Signal<usize>,
    pos: Signal<f64>,
    dur: Signal<f64>,
    playing: Signal<bool>,
    loading: Signal<bool>,
    status: Signal<String>,
    device: Signal<String>,
    speed: Signal<f64>,
    volume: Signal<f32>,
    looped: Signal<Option<(f64, f64)>>,
    mark_a: Signal<Option<f64>>,
    /// Back to the rig menu, when the host has one.
    on_back: Option<EventHandler<()>>,
}

impl GigState {
    fn new(on_back: Option<EventHandler<()>>) -> Self {
        let gigs = list_gigs();
        let at = super::remembered_index(&gigs);
        Self {
            gigs: Signal::new(gigs),
            gig_at: Signal::new(at),
            song_at: Signal::new(None),
            chart_at: Signal::new(0),
            pos: Signal::new(0.0),
            dur: Signal::new(0.0),
            playing: Signal::new(false),
            loading: Signal::new(false),
            status: Signal::new(String::new()),
            device: Signal::new(String::new()),
            speed: Signal::new(player().speed()),
            volume: Signal::new(player().volume()),
            looped: Signal::new(None),
            mark_a: Signal::new(None),
            on_back,
        }
    }

    /// The gig on show (subscribes the caller).
    fn gig(&self) -> Option<Gig> {
        let at = *self.gig_at.read();
        self.gigs.read().get(at).cloned()
    }

    /// The song picked in it (subscribes the caller).
    fn song(&self) -> Option<Song> {
        let at = (*self.song_at.read())?;
        self.gig().and_then(|g| g.songs.get(at).cloned())
    }

    fn song_count(&self) -> usize {
        self.gig().map_or(0, |g| g.songs.len())
    }

    /// Pick song `i`: its chart, no loop, its track loaded (or nothing).
    fn select(mut self, i: usize) {
        self.song_at.set(Some(i));
        self.chart_at.set(0);
        self.looped.set(None);
        self.mark_a.set(None);
        player().set_loop(None);
        let audio = self
            .gigs
            .read()
            .get(*self.gig_at.peek())
            .and_then(|g| g.songs.get(i))
            .and_then(|s| s.audio.clone());
        match audio {
            Some(path) => player().load(&path),
            None => player().unload(),
        }
    }

    /// Show another gig of this machine's.
    fn pick_gig(mut self, i: usize) {
        self.gig_at.set(i);
        self.song_at.set(None);
        player().unload();
    }

    /// Loop a range of the track (and start from its top), or stop looping.
    fn set_loop(mut self, range: Option<(f64, f64)>) {
        player().set_loop(range);
        self.looped.set(range);
        if let Some((a, _)) = range {
            player().seek(a);
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

fn use_gig() -> GigState {
    use_context::<GigState>()
}

/// The gig page's panels, by region (spec §1.1 ids, namespaced `gig.`).
fn registry() -> PanelRegistry {
    PanelRegistry::new()
        .with(
            Panel::new("gig.setlist", "Set", Region::Left, setlist_panel)
                .with_constraints(PanelConstraints::new().with_min_width(240.0)),
        )
        .with(Panel::new("gig.song", "Song", Region::Top, song_panel))
        .with(Panel::new("gig.chart", "Chart", Region::Main, chart_panel))
        .with(
            Panel::new("gig.practice", "Practice", Region::Bottom, practice_panel)
                .with_constraints(PanelConstraints::new().with_min_height(112.0)),
        )
}

/// The gig workspace. `on_back` (if any) returns to the rig menu.
#[component]
pub fn GigWorkspace(on_back: Option<EventHandler<()>>) -> Element {
    let st = use_context_provider(|| GigState::new(on_back));
    app_shell::ui::use_shell_state(SCOPE);
    app_shell::ui::use_panel_registry(registry);

    // Mirror the player into the page, ten times a second.
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
            let dv = p.device();
            if *device.peek() != dv {
                device.set(dv);
            }
            architect::platform::sleep(Duration::from_millis(100)).await;
        }
    });

    let mut gigs = st.gigs;
    if st.gig().is_none() {
        return rsx! {
            div { style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; \
                          gap: 10px; color: #a1a1aa; padding: 24px; text-align: center; background: #0b0b0d;",
                span { style: "font-size: 18px; font-weight: 700; color: #e4e4e7;", "No gig on this machine yet" }
                span { style: "font-size: 13px;", "Put a gig folder (with its gig.json) in" }
                code { style: "font-size: 13px; color: #e4e4e7;", "{super::gigs_root().display()}" }
                div { style: "display: flex; gap: 8px; margin-top: 8px;",
                    if let Some(back) = on_back {
                        button { style: chip(false), onclick: move |_| back.call(()), "← Rigs" }
                    }
                    button { style: chip(false), onclick: move |_| gigs.set(list_gigs()), "Look again" }
                }
            }
        };
    }

    let song_at = st.song_at;
    rsx! {
        div {
            style: "flex: 1; min-width: 0; min-height: 0; display: flex; background: #0b0b0d; color: #e4e4e7; \
                    font-family: Inter, -apple-system, sans-serif;",
            tabindex: 0,
            onkeydown: move |e: KeyboardEvent| match e.key() {
                Key::Character(c) if c == " " => player().toggle(),
                Key::ArrowLeft => player().seek((player().position() - 5.0).max(0.0)),
                Key::ArrowRight => player().seek(player().position() + 5.0),
                Key::ArrowUp => {
                    if let Some(i) = song_at().and_then(|i| i.checked_sub(1)) {
                        st.select(i);
                    }
                }
                Key::ArrowDown => {
                    let next = song_at().map_or(0, |i| i.saturating_add(1));
                    if next < st.song_count() {
                        st.select(next);
                    }
                }
                _ => {}
            },
            ShellFrame {}
        }
    }
}

// ── Left: the set ───────────────────────────────────────────────────────────

fn setlist_panel() -> Element {
    let st = use_gig();
    let Some(gig) = st.gig() else { return rsx! {} };
    let gigs = st.gigs;
    let gig_at = st.gig_at;
    let song_at = st.song_at;
    rsx! {
        div { style: "flex: 1; display: flex; flex-direction: column; min-height: 0;",
            div { style: "padding: 4px 14px 10px; display: flex; flex-direction: column; gap: 4px; \
                          border-bottom: 1px solid #1f1f23;",
                div { style: "display: flex; align-items: center; gap: 8px;",
                    if let Some(back) = st.on_back {
                        button { style: chip(false), onclick: move |_| back.call(()), "←" }
                    }
                    span { style: "font-size: 17px; font-weight: 800;", "{gig.name}" }
                }
                for (i, d) in gig.dates.iter().enumerate() {
                    span { key: "{i}", style: "font-size: 11px; color: #a1a1aa;", "{d}" }
                }
                if !gig.note.is_empty() {
                    span { style: "font-size: 11px; color: #71717a;", "{gig.note}" }
                }
                if gigs.read().len() > 1 {
                    div { style: "display: flex; gap: 6px; flex-wrap: wrap; margin-top: 4px;",
                        for (i, g) in gigs.read().iter().enumerate() {
                            button {
                                key: "{i}",
                                style: chip(i == gig_at()),
                                onclick: move |_| st.pick_gig(i),
                                "{g.name}"
                            }
                        }
                    }
                }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: scroll; padding: 6px;",
                for (i, s) in gig.songs.iter().enumerate() {
                    button {
                        key: "{i}",
                        style: if song_at() == Some(i) {
                            "width: 100%; text-align: left; border: none; border-radius: 8px; padding: 8px 10px; \
                             margin-bottom: 2px; background: #0c4a6e; color: #f0f9ff; cursor: pointer; \
                             display: flex; gap: 10px; align-items: center;"
                        } else {
                            "width: 100%; text-align: left; border: none; border-radius: 8px; padding: 8px 10px; \
                             margin-bottom: 2px; background: transparent; color: #e4e4e7; cursor: pointer; \
                             display: flex; gap: 10px; align-items: center;"
                        },
                        onclick: move |_| st.select(i),
                        span { style: "width: 22px; font-size: 12px; color: #71717a; text-align: right;", "{s.number}" }
                        div { style: "display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1;",
                            span { style: "font-size: 14px; font-weight: 650;", "{s.title}" }
                            span { style: "font-size: 11px; color: #a1a1aa;",
                                if s.artist.is_empty() { "—" } else { "{s.artist}" }
                            }
                        }
                        div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 2px;",
                            span { style: "font-size: 11px; color: #d4d4d8;", "{s.key}" }
                            span { style: "font-size: 11px; color: #71717a;",
                                if s.audio.is_some() { "{s.tempo}" } else { "chart only" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Top: the song's facts ───────────────────────────────────────────────────

fn song_panel() -> Element {
    let st = use_gig();
    let Some(song) = st.song() else {
        return rsx! {
            div { style: "padding: 8px 18px; font-size: 12px; color: #71717a;",
                "Space plays and pauses · ← → skip 5 s · ↑ ↓ change song"
            }
        };
    };
    rsx! {
        div { style: "padding: 6px 18px 8px; display: flex; flex-wrap: wrap; align-items: baseline; gap: 6px 14px;",
            span { style: "font-size: 22px; font-weight: 800;", "{song.title}" }
            span { style: "font-size: 14px; color: #a1a1aa;", "{song.artist}" }
            if !song.key.is_empty() {
                span { style: "font-size: 13px; padding: 2px 8px; border-radius: 6px; background: #1c1917; color: #fde68a;",
                    "Key {song.key}"
                }
            }
            if !song.tempo.is_empty() {
                span { style: "font-size: 13px; padding: 2px 8px; border-radius: 6px; background: #1c1917; color: #fde68a;",
                    "{song.tempo} bpm"
                }
            }
            span { style: "font-size: 13px; padding: 2px 8px; border-radius: 6px; background: #172554; color: #bfdbfe;",
                "Profile: {song.profile.label()}"
            }
            if !song.guitar.is_empty() {
                span { style: "font-size: 13px; padding: 2px 8px; border-radius: 6px; background: #052e16; color: #bbf7d0;",
                    "Guitar: {song.guitar}"
                }
            }
            if !song.notes.is_empty() {
                span { style: "font-size: 12px; color: #a1a1aa;", "{song.notes}" }
            }
        }
    }
}

// ── Main: the chart ─────────────────────────────────────────────────────────

fn chart_panel() -> Element {
    let st = use_gig();
    let mut chart_at = st.chart_at;
    let Some(song) = st.song() else {
        return rsx! {
            div { style: "flex: 1; display: flex; flex-direction: column; align-items: center; \
                          justify-content: center; gap: 8px; color: #a1a1aa;",
                span { style: "font-size: 18px; font-weight: 700; color: #e4e4e7;", "Pick a song" }
                span { style: "font-size: 13px;", "Its chart opens here, with its practice track ready to play." }
            }
        };
    };
    let charts = song.charts.clone();
    let tab = chart_at().min(charts.len().saturating_sub(1));
    let chart_src = charts.get(tab).and_then(|c| image_uri(&c.path));
    let pdf = song.pdf.clone();
    rsx! {
        div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
            div { style: "padding: 8px 18px 6px; display: flex; gap: 6px; align-items: center;",
                for (k, c) in charts.iter().enumerate() {
                    button { key: "{k}", style: chip(k == tab), onclick: move |_| chart_at.set(k), "{c.label}" }
                }
                if let Some(pdf) = pdf {
                    if cfg!(target_os = "macos") {
                        button {
                            style: chip(false),
                            onclick: move |_| open_file(&pdf),
                            "Open PDF"
                        }
                    }
                }
            }
            div { style: "flex: 1; min-height: 0; overflow-y: scroll; padding: 0 18px 18px;",
                match chart_src {
                    Some(src) => rsx! {
                        img { src: "{src}", style: "width: 100%; height: auto; border-radius: 6px; background: #fff;" }
                    },
                    None => rsx! {
                        div { style: "padding: 24px; color: #71717a; font-size: 13px;", "No chart for this song." }
                    },
                }
            }
        }
    }
}

// ── Bottom: transport and practice ──────────────────────────────────────────

#[allow(clippy::too_many_lines)] // one panel, laid out top to bottom
fn practice_panel() -> Element {
    let st = use_gig();
    let GigState {
        pos,
        dur,
        playing,
        loading,
        status,
        device,
        speed,
        volume,
        looped,
        mut mark_a,
        ..
    } = st;
    let Some(song) = st.song() else {
        return rsx! {
            div { style: "padding: 10px 18px; font-size: 13px; color: #71717a;",
                "Pick a song to practise along with it."
            }
        };
    };
    if song.audio.is_none() {
        return rsx! {
            div { style: "padding: 10px 18px; font-size: 13px; color: #fbbf24;",
                "No practice audio for this one yet — the chart only. (Recording to come from the MD/band.)"
            }
        };
    }
    let d = dur();
    let p = pos();
    let current = song.sections.iter().position(|s| p >= s.start && p < s.end);
    let pct = (speed() * 100.0).round();
    let vol = (f64::from(volume()) * 100.0).round();
    let loop_text = looped().map(|(a, b)| format!("Loop {} – {}", clock(a), clock(b)));
    rsx! {
        div { style: "display: flex; flex-direction: column; padding-bottom: 4px;",
            div { style: "padding: 4px 18px 8px; display: flex; flex-wrap: wrap; align-items: center; gap: 8px;",
                button { style: chip(false), onclick: move |_| player().seek(0.0), "⏮" }
                button { style: chip(false), onclick: move |_| player().seek((player().position() - 5.0).max(0.0)), "−5s" }
                button { style: BIG, onclick: move |_| player().toggle(),
                    if loading() { "Loading…" } else if playing() { "Pause" } else { "Play" }
                }
                button { style: chip(false), onclick: move |_| player().seek(player().position() + 5.0), "+5s" }
                span { style: "font-size: 15px; font-variant-numeric: tabular-nums; min-width: 92px; padding: 0 6px;",
                    "{clock(p)} / {clock(d)}"
                }
                span { style: "font-size: 12px; color: #71717a; margin-left: 6px;", "Speed" }
                for s in SPEEDS.iter().copied() {
                    button {
                        key: "{s}",
                        style: chip((pct - f64::from(s)).abs() < 0.5),
                        onclick: move |_| st.set_speed(f64::from(s) / 100.0),
                        "{s}%"
                    }
                }
                button { style: chip(false), onclick: move |_| st.set_speed((speed() - 0.05).max(0.25)), "−" }
                span { style: "font-size: 13px; min-width: 40px; text-align: center;", "{pct}%" }
                button { style: chip(false), onclick: move |_| st.set_speed((speed() + 0.05).min(2.0)), "+" }
            }
            // Progress: a row of seek targets, sections shaded.
            div { style: "margin: 0 18px; height: 26px; display: flex; border-radius: 6px; overflow: hidden; \
                          background: #18181b; cursor: pointer;",
                for i in 0..CELLS {
                    {
                        let from = seconds_of(usize_f64(i) / usize_f64(CELLS), d);
                        let mid = seconds_of((usize_f64(i) + 0.5) / usize_f64(CELLS), d);
                        let played = d > 0.0 && mid <= p;
                        let odd = song.sections.iter().position(|s| mid >= s.start && mid < s.end)
                            .is_some_and(|k| k % 2 == 1);
                        let in_loop = looped().is_some_and(|(a, b)| mid >= a && mid < b);
                        let bg = match (played, odd) {
                            (true, false) => "#0284c7",
                            (true, true) => ACCENT,
                            (false, false) => "#27272a",
                            (false, true) => "#3f3f46",
                        };
                        let edge = if in_loop { "border-top: 3px solid #f59e0b;" } else { "" };
                        rsx! {
                            div {
                                key: "{i}",
                                style: "flex: 1; height: 100%; background: {bg}; {edge}",
                                onclick: move |_| player().seek(from),
                            }
                        }
                    }
                }
            }
            // Sections and the loop.
            div { style: "padding: 8px 18px; display: flex; flex-wrap: wrap; gap: 6px; align-items: center;",
                for (k, s) in song.sections.iter().enumerate() {
                    {
                        let (a, b, name) = (s.start, s.end, s.name.clone());
                        let looping_this = looped().is_some_and(|(x, y)| (x - a).abs() < 0.01 && (y - b).abs() < 0.01);
                        rsx! {
                            div { key: "{k}", style: "display: flex;",
                                button {
                                    style: chip(current == Some(k)),
                                    onclick: move |_| player().seek(a),
                                    "{name}"
                                }
                                button {
                                    style: chip(looping_this),
                                    title: "Loop this section",
                                    onclick: move |_| st.set_loop((!looping_this).then_some((a, b))),
                                    "↻"
                                }
                            }
                        }
                    }
                }
                span { style: "width: 10px;" }
                button { style: chip(mark_a().is_some()), onclick: move |_| mark_a.set(Some(player().position())),
                    if let Some(a) = mark_a() { "A {clock(a)}" } else { "Set A" }
                }
                button { style: chip(false), onclick: move |_| {
                    if let Some(a) = mark_a() {
                        let b = player().position();
                        let (a, b) = if b > a { (a, b) } else { (b, a) };
                        if b - a > 0.5 {
                            st.set_loop(Some((a, b)));
                        }
                    }
                }, "Set B" }
                if let Some(text) = loop_text {
                    span { style: "font-size: 12px; color: #fbbf24;", "{text}" }
                    button { style: chip(false), onclick: move |_| {
                        st.set_loop(None);
                        mark_a.set(None);
                    }, "Loop off" }
                }
                span { style: "flex: 1;" }
                span { style: "font-size: 12px; color: #71717a;", "Vol" }
                button { style: chip(false), onclick: move |_| st.set_volume((volume() - 0.1).max(0.0)), "−" }
                span { style: "font-size: 12px; min-width: 36px; text-align: center;", "{vol}%" }
                button { style: chip(false), onclick: move |_| st.set_volume((volume() + 0.1).min(2.0)), "+" }
                button {
                    style: chip(false),
                    title: "Next output device",
                    onclick: move |_| {
                        let all = output_devices();
                        if all.is_empty() {
                            return;
                        }
                        let now = device();
                        let next = all.iter().position(|n| *n == now)
                            .map_or(0, |i| (i + 1) % all.len());
                        if let Some(name) = all.get(next) {
                            player().open_device(Some(name.clone()));
                        }
                    },
                    "Out: {device}"
                }
            }
            if !status().is_empty() {
                div { style: "padding: 0 18px 6px; font-size: 12px; color: #f87171;", "{status}" }
            }
            div { style: "padding: 0 18px 6px; font-size: 11px; color: #52525b;", "{song.audio_kind}" }
        }
    }
}

/// Open a file in its default app (the PDF in Preview).
fn open_file(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = std::process::Command::new("open").arg(path).spawn() {
            tracing::warn!("gig: open {}: {e}", path.display());
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = path;
}
