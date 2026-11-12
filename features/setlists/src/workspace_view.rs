//! A song's workspace, drawn: Session's Overview for a song note
//! ([`crate::workspace`] is the model).
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │ progress bar                                 │
//! ├──────────────────────┬───────────────────────┤
//! │ chart editor         │                       │
//! ├──────────────────────┤ chart                 │
//! │ lyrics               │ (full height)         │
//! └──────────────────────┴───────────────────────┘
//! ```
//!
//! The panels are Session's: the progress bar is
//! `session::ui::components::progress::SongProgressBar` (what Session's
//! `ProgressBar` draws), the lyrics `session::ui::components::lyrics`
//! (Session's lyrics panel, its Performer view here), the editor
//! `session::ui::components::chart_text` (Session's chart editor's pane).
//! The chart is the engraving the share page draws (`crate::follow`), a
//! page at a time with the bar at the playhead marked: Session's chart
//! panel is a Blitz widget painting into the window's own scene and
//! listening to the window's events, which a process extension's view
//! (drawn remotely) cannot carry; pages as images and bars as boxes can.
//!
//! Everything follows one [`Clock`]; a click on a section, a bar or a
//! lyric line seeks it. Each pause in typing re-engraves the chart on a
//! thread of its own and saves the `.kf` back to the song.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_util::StreamExt as _;
use session::ui::components::chart_text::{ChartText, ChartTextStatus};
use session::ui::components::lyrics::{
    ColoredSpan, LyricsChoice, LyricsView, View as LyricsMode, Words,
};
use session::ui::components::progress::{ProgressSection, SongProgressBar};

use crate::follow::{PlacedBar, cursor_colors};
use crate::workspace::{Arrangement, Clock, Form, Loaded, LyricsSource, Pane, Song, lyrics_source};

const RULE: &str = "#2a2c31";
const TEXT: &str = "#e5e7eb";
const DIM: &str = "#8b9099";
const ACCENT: &str = "#3aa0ff";

/// How long typing pauses before the chart is engraved again.
const PAUSE: Duration = Duration::from_millis(350);

/// How often the play position is read while it plays.
const FRAME: Duration = Duration::from_millis(33);

/// A song engraved, as the panes take it: compared by identity, so a pane
/// draws again when a new engraving arrives and not on every frame.
#[derive(Clone)]
pub struct Engraved(pub Arc<Song>, pub Arc<Vec<String>>);

impl PartialEq for Engraved {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// What the engraver sends back for one text.
struct Reply {
    text: String,
    result: Result<(Song, Vec<String>, Option<String>), String>,
}

/// Each page of `song` as an image source.
#[must_use]
pub fn page_sources(song: &Song) -> Vec<String> {
    use base64::Engine as _;
    song.chart
        .pages
        .iter()
        .map(|svg| {
            format!(
                "data:image/svg+xml;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(svg)
            )
        })
        .collect()
}

/// Engrave `text`: the song, its pages as images, and its chart's lyric
/// lane as LRC.
fn engrave(text: &str) -> Result<(Song, Vec<String>, Option<String>), String> {
    let song = Song::engrave(text).map_err(|e| e.to_string())?;
    let pages = page_sources(&song);
    let lane = crate::follow::chart_lrc(text).ok().flatten();
    Ok((song, pages, lane))
}

/// The engraver: takes the latest text once typing has paused, engraves
/// it, and replies. A thread of its own natively; on a page (no threads),
/// in place.
fn start(reply: UnboundedSender<Reply>) -> std::sync::mpsc::Sender<String> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = std::thread::Builder::new()
            .name("setlists-engrave".into())
            .spawn(move || {
                while let Ok(mut text) = rx.recv() {
                    // Coalesce: whatever arrives before the pause replaces it.
                    while let Ok(newer) = rx.recv_timeout(PAUSE) {
                        text = newer;
                    }
                    let result = engrave(&text);
                    if reply.unbounded_send(Reply { text, result }).is_err() {
                        break;
                    }
                }
            });
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(100).await;
                let Some(text) = rx.try_iter().last() else {
                    continue;
                };
                let result = engrave(&text);
                if reply.unbounded_send(Reply { text, result }).is_err() {
                    break;
                }
            }
        });
    }
    tx
}

/// A wait of `d`, natively on a timer thread, on a page the browser's.
async fn sleep(d: Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    futures_timer::Delay::new(d).await;
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(u32::try_from(d.as_millis()).unwrap_or(u32::MAX)).await;
}

/// A ticker: `()` every `every`, as long as it is listened to.
fn ticks(every: Duration) -> futures_channel::mpsc::UnboundedReceiver<()> {
    let (tick, ticks) = futures_channel::mpsc::unbounded::<()>();
    spawn(async move {
        while tick.unbounded_send(()).is_ok() {
            sleep(every).await;
        }
    });
    ticks
}

/// A node's size, measured once mounted and again every so often (a pane
/// resizes when a divider or the window moves, and nothing says so). Stays
/// (0, 0) where a view cannot measure (a process view drawn remotely).
/// A node to measure, once mounted.
type Node = Signal<Option<Rc<MountedData>>>;

fn use_measured() -> (Signal<(f64, f64)>, Node) {
    let mut size = use_signal(|| (0.0_f64, 0.0_f64));
    let node = use_signal(|| None::<Rc<MountedData>>);
    use_future(move || async move {
        let mut ticks = ticks(Duration::from_millis(300));
        while ticks.next().await.is_some() {
            let Some(node) = node.peek().clone() else {
                continue;
            };
            if let Ok(rect) = node.get_client_rect().await {
                let now = (rect.size.width, rect.size.height);
                if *size.peek() != now {
                    size.set(now);
                }
            }
        }
    });
    (size, node)
}

/// The panes laid out (`Arrangement`): the progress across the top, then
/// the editor over the lyrics on the left and the chart, the full height,
/// on the right; or, narrow, one column that scrolls.
#[component]
pub fn WorkspaceLayout(
    arrangement: Arrangement,
    progress: Element,
    editor: Element,
    chart: Element,
    lyrics: Element,
) -> Element {
    let pane = |p: Pane| match p {
        Pane::Progress => progress.clone(),
        Pane::Editor => editor.clone(),
        Pane::Chart => chart.clone(),
        Pane::Lyrics => lyrics.clone(),
    };
    match arrangement {
        Arrangement::Columns { left, right } => {
            let left_count = left.len();
            rsx! {
                div {
                    "data-workspace": "columns",
                    style: "position:absolute; top:0; left:0; right:0; bottom:0; display:flex; \
                            flex-direction:column; gap:10px; padding:10px; box-sizing:border-box;",
                    div { style: "flex:none;", "data-pane": "Progress", {progress.clone()} }
                    div {
                        style: "flex:1; min-height:0; display:flex; gap:10px;",
                        // Every column is `height:100%`, not the row's
                        // stretch: what is inside is placed absolutely, and
                        // Blitz lays it out against the column's height
                        // BEFORE the stretch (Session's `OverviewLayout`).
                        div {
                            style: "position:relative; flex:1; min-width:0; height:100%; display:flex; \
                                    flex-direction:column; gap:10px;",
                            for p in left {
                                div {
                                    key: "{p:?}",
                                    "data-pane": "{p:?}",
                                    style: "position:relative; flex:1; min-height:0; border-radius:8px; \
                                            overflow:hidden; border:1px solid {RULE}; \
                                            {left_share(p, left_count)}",
                                    {pane(p)}
                                }
                            }
                        }
                        for p in right {
                            div {
                                key: "{p:?}",
                                "data-pane": "{p:?}",
                                style: "position:relative; flex:1; min-width:0; height:100%; border-radius:8px; \
                                        overflow:hidden; border:1px solid {RULE};",
                                {pane(p)}
                            }
                        }
                    }
                }
            }
        }
        Arrangement::Stack(panes) => rsx! {
            div {
                "data-workspace": "stack",
                style: "position:absolute; top:0; left:0; right:0; bottom:0; overflow-y:scroll; \
                        display:flex; flex-direction:column; gap:8px; padding:8px; box-sizing:border-box;",
                for p in panes {
                    div {
                        key: "{p:?}",
                        "data-pane": "{p:?}",
                        style: "position:relative; flex:none; width:100%; {stacked(p)}",
                        {pane(p)}
                    }
                }
            }
        },
    }
}

/// How much of the left column a pane takes: the editor the larger share
/// over the lyrics.
const fn left_share(p: Pane, count: usize) -> &'static str {
    match (p, count) {
        (Pane::Editor, 2) => "flex-grow:3;",
        (Pane::Lyrics, 2) => "flex-grow:2;",
        _ => "",
    }
}

/// A pane's height in the one column.
const fn stacked(p: Pane) -> &'static str {
    match p {
        // The chart sets its own height: a page across the width.
        Pane::Progress | Pane::Chart => "",
        Pane::Editor => {
            "height:340px; border-radius:8px; overflow:hidden; border:1px solid #2a2c31;"
        }
        Pane::Lyrics => {
            "height:320px; border-radius:8px; overflow:hidden; border:1px solid #2a2c31;"
        }
    }
}

/// `seconds` as `m:ss`.
fn clock_text(seconds: f64) -> String {
    let whole = seconds.max(0.0).floor();
    let minutes = (whole / 60.0).floor();
    format!("{minutes:.0}:{:02.0}", minutes.mul_add(-60.0, whole))
}

/// The size a sheet `sheet` (w × h) takes fitted whole into `room` (w × h).
#[must_use]
pub fn fit_page(room: (f64, f64), sheet: (f64, f64)) -> (f64, f64) {
    let ratio = sheet.0 / sheet.1.max(f64::EPSILON);
    let by_height = (room.1 * ratio, room.1);
    if by_height.0 <= room.0 {
        by_height
    } else {
        (room.0, room.0 / ratio.max(f64::EPSILON))
    }
}

/// The chart a page at a time: the page the playhead is on (or the one
/// turned to), every bar on it a place to play from, the one being played
/// marked. `fit`: the page whole in the pane (measured); else across the
/// pane's width, its own height.
#[component]
pub fn ChartPages(
    song: Engraved,
    now: f64,
    on_seek: EventHandler<f64>,
    #[props(default)] across: bool,
) -> Element {
    let Engraved(song, sources) = song;
    let mut held = use_signal(|| None::<usize>);
    let (size, mut node) = use_measured();
    let count = song.chart.pages.len().max(1);
    let last = count.saturating_sub(1);
    let following = song.page_at(now).min(last);
    let page = held().map_or(following, |p| p.min(last));
    let current = song.bar_at(now).and_then(|i| song.bar(i)).copied();
    // The beat being played: Session's line at its start, on this page.
    let beat = song.beat_at(now).filter(|b| b.page == page).copied();
    let (wash, line) = cursor_colors();
    let (pw, ph) = (song.chart.width, song.chart.height);
    // The bars on the page, a box each (a repeat's passes share one: the
    // first is where a click goes).
    let mut boxes: Vec<PlacedBar> = Vec::new();
    for b in song.chart.bars.iter().filter(|b| b.page == page) {
        if !boxes.iter().any(|o| same_box(o, b)) {
            boxes.push(*b);
        }
    }
    let src = sources.get(page).cloned().unwrap_or_default();
    let (w, h) = size();
    let page_style = if across || w <= 0.0 || h <= 0.0 {
        format!("width:100%; aspect-ratio:{pw} / {ph};")
    } else {
        let (fw, fh) = fit_page((w - 16.0, h - 40.0), (pw, ph));
        format!("width:{fw:.1}px; height:{fh:.1}px;")
    };
    let outer = if across {
        "position:relative; width:100%; display:flex; flex-direction:column; background:#1a1b1e;"
    } else {
        "position:absolute; top:0; left:0; width:100%; height:100%; display:flex; \
         flex-direction:column; align-items:center; background:#1a1b1e;"
    };
    rsx! {
        div {
            style: "{outer}",
            "data-chart-page": "{page}",
            onmounted: move |e| node.set(Some(e.data())),
            div {
                // Clipped: a page's drawing runs past its box (the next
                // page beside it, in the engraver's scene), and Blitz does
                // not clip an image to its `viewBox`.
                style: "position:relative; flex:none; margin-top:8px; {page_style} overflow:hidden; \
                        box-shadow:0 2px 12px rgba(0,0,0,0.5); background:#ffffff;",
                img {
                    style: "position:absolute; top:0; left:0; width:100%; height:100%;",
                    src: "{src}",
                }
                for b in boxes {
                    {
                        let on = current.as_ref().is_some_and(|c| same_box(c, &b));
                        let fill = if on { wash.as_str() } else { "transparent" };
                        let at = b.start;
                        rsx! {
                            div {
                                key: "{b.number}-{b.pass}",
                                "data-bar": "{b.number}",
                                "data-at": "{at}",
                                "aria-current": if on { "time" } else { "false" },
                                style: "position:absolute; left:{b.x / pw * 100.0}%; top:{b.y / ph * 100.0}%; \
                                        width:{b.width / pw * 100.0}%; height:{b.height / ph * 100.0}%; \
                                        background:{fill}; border-radius:2px; cursor:pointer;",
                                onclick: move |_| on_seek.call(at),
                            }
                        }
                    }
                }
                if let Some(b) = beat {
                    div {
                        "data-beat": "{b.beat + 1}",
                        style: "position:absolute; left:{b.x / pw * 100.0}%; top:{b.y / ph * 100.0}%; \
                                width:{b.width / pw * 100.0}%; height:{b.height / ph * 100.0}%; \
                                background:{line}; pointer-events:none;",
                    }
                }
            }
            // The pages: turn by hand, or follow the song.
            div {
                style: "flex:none; display:flex; align-items:center; justify-content:center; gap:8px; \
                        height:28px; font-size:11px; color:{DIM};",
                button {
                    style: "{SMALL_BUTTON}",
                    onclick: move |_| held.set(Some(page.saturating_sub(1))),
                    "\u{2039}"
                }
                span { "Page {page + 1} of {count}" }
                button {
                    style: "{SMALL_BUTTON}",
                    onclick: move |_| held.set(Some(page.saturating_add(1).min(last))),
                    "\u{203A}"
                }
                if held().is_some() {
                    button {
                        style: "{SMALL_BUTTON}",
                        onclick: move |_| held.set(None),
                        "Follow"
                    }
                }
            }
        }
    }
}

const SMALL_BUTTON: &str = "display:flex; align-items:center; justify-content:center; gap:4px; \
                            height:22px; min-width:22px; padding:0 8px; border-radius:5px; \
                            border:1px solid #2a2c31; background:#15161a; color:#e5e7eb; \
                            font-size:12px; cursor:pointer;";

fn same_box(a: &PlacedBar, b: &PlacedBar) -> bool {
    a.page == b.page && (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01
}

/// The song's progress: its parts, coloured, the playhead through them;
/// a click on a part plays from it. Session's progress bar.
#[component]
fn Progress(song: Option<Engraved>, now: f64, on_seek: EventHandler<f64>, labels: bool) -> Element {
    let Some(Engraved(song, _)) = song else {
        return rsx! { div { style: "height:56px;" } };
    };
    let (start, end) = song.span();
    let span = (end - start).max(f64::EPSILON);
    let percent = |t: f64| ((t - start) / span * 100.0).clamp(0.0, 100.0);
    let sections: Vec<ProgressSection> = song
        .parts
        .iter()
        .map(|p| ProgressSection {
            start_percent: percent(p.start),
            end_percent: percent(p.end),
            color: p.color.clone(),
            name: p.name.clone(),
            short_name: p.name.clone(),
            comment: None,
        })
        .collect();
    let starts: Vec<f64> = song.parts.iter().map(|p| p.start).collect();
    rsx! {
        SongProgressBar {
            progress: song.progress(now),
            sections,
            labels,
            on_section_click: move |i: usize| {
                if let Some(at) = starts.get(i) {
                    on_seek.call(*at);
                }
            },
        }
    }
}

/// The workspace over a song loaded from its note: the progress, the
/// chart's text and the lyrics, and the chart, following one clock.
///
/// `form` and `at` fix the form and the position (a picture of it); left
/// out, the form follows the pane's width and the song starts at 0.
#[component]
pub fn SongWorkspace(
    loaded: Loaded,
    #[props(default)] form: Option<Form>,
    #[props(default)] at: Option<f64>,
) -> Element {
    use_context_provider(|| LyricsChoice::of(LyricsMode::Performer));
    let origin = use_hook(web_time::Instant::now);
    let wall = move || origin.elapsed().as_secs_f64();
    let mut clock = use_signal(|| {
        let mut c = Clock::new(None);
        c.seek(at.unwrap_or(0.0), 0.0);
        c
    });
    let mut now = use_signal(|| at.unwrap_or(0.0));
    let mut engraved = use_signal(|| None::<Engraved>);
    let mut words = use_signal(|| None::<Words>);
    let mut status = use_signal(|| (ChartTextStatus::Idle, "engraving…".to_owned()));
    let mut refused = use_signal(|| false);
    let state = use_signal(|| editor_state::EditorState::new(&loaded.source));
    let mut editor_open = use_signal(|| None::<bool>);
    // What was last saved (or read): an engraving of anything else is an
    // edit, saved back to the song.
    let mut saved = use_signal(|| loaded.source.clone());
    let (size, mut node) = use_measured();
    let form = form.unwrap_or_else(|| Form::of(size().0));

    let lrc = match &loaded.lyrics {
        Some(LyricsSource::Lrc(text)) => Some(text.clone()),
        _ => None,
    };
    let chart_file = loaded.chart_file.clone();
    let (title, artist) = (loaded.title.clone(), loaded.artist.clone());
    let replies = use_coroutine(move |mut rx: UnboundedReceiver<Reply>| {
        let lrc = lrc.clone();
        let chart_file = chart_file.clone();
        let (title, artist) = (title.clone(), artist.clone());
        async move {
            while let Some(reply) = rx.next().await {
                let (song, pages, lane) = match reply.result {
                    Ok(done) => done,
                    Err(why) => {
                        status.set((ChartTextStatus::Warn, format!("not engraved: {why}")));
                        continue;
                    }
                };
                let (_, end) = song.span();
                clock.write().set_end(Some(end));
                // One lyrics source: the LRC, else the chart's lane (as
                // edited).
                let source = lyrics_source(lrc.as_deref(), None)
                    .or_else(|| lane.map(LyricsSource::ChartLane));
                let regions = song
                    .spans()
                    .into_iter()
                    .map(|(span, color)| ColoredSpan {
                        span,
                        color: Some(color),
                    })
                    .collect();
                let lyrics = source.map(|s| song.lyrics(&s)).unwrap_or_default();
                words.set(Some(Words::new(
                    lyrics,
                    regions,
                    title.clone(),
                    artist.clone(),
                )));
                engraved.set(Some(Engraved(Arc::new(song), Arc::new(pages))));
                if *saved.peek() == reply.text {
                    status.set((ChartTextStatus::Done, "engraved".to_owned()));
                    continue;
                }
                let Some(file) = chart_file.clone() else {
                    status.set((
                        ChartTextStatus::Warn,
                        "engraved — the song has no .kf to save to".to_owned(),
                    ));
                    continue;
                };
                match crate::host::write_vault_file(&file, &reply.text).await {
                    Ok(()) => {
                        saved.set(reply.text);
                        refused.set(false);
                        status.set((ChartTextStatus::Done, format!("saved to {}", base(&file))));
                    }
                    Err(why) => {
                        refused.set(true);
                        status.set((ChartTextStatus::Warn, format!("engraved, not saved: {why}")));
                    }
                }
            }
        }
    });
    let worker = use_hook(|| {
        let tx = start(replies.tx());
        let _ = tx.send(loaded.source.clone());
        tx
    });
    let again = worker.clone();
    // Each change of text goes to the engraver, which waits for the pause.
    let mut last = use_signal(|| loaded.source.clone());
    use_effect(move || {
        let text = state.read().doc.to_string();
        if *last.peek() != text {
            last.set(text.clone());
            let _ = worker.send(text);
        }
    });
    // The play position, read while it plays.
    use_future(move || async move {
        loop {
            sleep(FRAME).await;
            let t = clock.peek().now(wall());
            if (t - *now.peek()).abs() > 0.01 {
                now.set(t);
            }
        }
    });
    let seek = use_callback(move |t: f64| {
        clock.write().seek(t, wall());
        now.set(t);
    });
    let t = now();
    let playing = clock.read().playing(wall());
    let editor_shown = editor_open().unwrap_or(form == Form::Wide);
    let arrangement = Arrangement::of(form, editor_shown);
    let song = engraved();
    let (status_kind, message) = status();
    let end = song.as_ref().map_or(0.0, |s| s.0.span().1);
    let back_to = song.as_ref().map(|s| s.0.back_from(t)).unwrap_or_default();
    let on_to = song.as_ref().and_then(|s| s.0.forward_from(t));

    let progress = rsx! {
        div { style: "display:flex; flex-direction:column; gap:6px;",
            div {
                style: "display:flex; align-items:center; gap:8px; height:28px; color:{TEXT}; font-size:13px;",
                button { style: "{SMALL_BUTTON}", title: "Back a section",
                    onclick: move |_| seek.call(back_to),
                    lucide_dioxus::SkipBack { size: 14, color: TEXT } }
                button {
                    style: "{SMALL_BUTTON} min-width:56px; background:{ACCENT}; color:#0b0c0e; border-color:{ACCENT}; font-weight:700;",
                    "data-transport": if playing { "playing" } else { "paused" },
                    onclick: move |_| {
                        clock.write().toggle(wall());
                    },
                    if playing {
                        lucide_dioxus::Pause { size: 14, color: "#0b0c0e" }
                    } else {
                        lucide_dioxus::Play { size: 14, color: "#0b0c0e" }
                    }
                }
                button { style: "{SMALL_BUTTON}", title: "On a section",
                    onclick: move |_| if let Some(to) = on_to { seek.call(to) },
                    lucide_dioxus::SkipForward { size: 14, color: TEXT } }
                span { style: "color:{DIM}; font-variant-numeric:tabular-nums;",
                    "{clock_text(t)} / {clock_text(end)}" }
                span { style: "flex:1; min-width:0; overflow:hidden; white-space:nowrap; font-weight:700; padding-left:6px;",
                    "{loaded.title}" }
                if refused() {
                    button {
                        style: "{SMALL_BUTTON}",
                        title: "Pick this vault's folder so charts can be saved in it",
                        onclick: move |_| {
                            let again = again.clone();
                            spawn(async move {
                                if crate::host::allow_vault().await.is_ok() {
                                    refused.set(false);
                                    // Saved on the next engraving.
                                    saved.set(String::new());
                                    let text = state.peek().doc.to_string();
                                    let _ = again.send(text);
                                }
                            });
                        },
                        "Allow saving"
                    }
                }
                button {
                    style: "{SMALL_BUTTON}",
                    "data-toggle": "editor",
                    title: if editor_shown { "Hide the chart's text" } else { "Edit the chart as text" },
                    onclick: move |_| editor_open.set(Some(!editor_shown)),
                    if editor_shown { "Hide text" } else { "Edit chart" }
                }
            }
            Progress { song: song.clone(), now: t, on_seek: seek, labels: form == Form::Wide }
        }
    };
    let editor = rsx! {
        ChartText { state, message, status: status_kind }
    };
    let chart = song.map_or_else(
        || rsx! {
            div { style: "position:absolute; top:0; left:0; width:100%; height:100%; display:flex; \
                          align-items:center; justify-content:center; color:{DIM}; font-size:12px; background:#1a1b1e; \
                          min-height:200px;",
                "Engraving the chart…" }
        },
        |song| rsx! { ChartPages { song, now: t, on_seek: seek, across: form == Form::Narrow } },
    );
    let lyrics = words().map_or_else(
        || rsx! {},
        |words| rsx! {
            LyricsView {
                words,
                at: t,
                on_seek: seek,
                // Where a view cannot measure itself (a process view, drawn
                // remotely), the words still draw: at a pane's usual size.
                start_size: (640.0, 320.0),
                empty: "No timed lyrics for this song: give it an LRC, or its chart a timed lyric lane ([lyrics lrc]).",
            }
        },
    );
    rsx! {
        div {
            "data-studio-song": "{loaded.title}",
            style: "position:absolute; top:0; left:0; right:0; bottom:0; background:#0e0f12; color:{TEXT}; \
                    font-family:system-ui, sans-serif;",
            onmounted: move |e| node.set(Some(e.data())),
            WorkspaceLayout { arrangement, progress, editor, chart, lyrics }
        }
    }
}

/// A vault path's file name.
fn base(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}
