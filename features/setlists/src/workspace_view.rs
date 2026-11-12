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
//! (drawn remotely) cannot carry, and a linked one has no API for
//! (FastTrackStudios/architect#591); pages as images and bars as boxes can.
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
use crate::workspace::{
    Arrangement, ChartAccess, ChartFrom, Clock, Form, Loaded, LyricsSource, Pane, Reread, Saving,
    Song, Unsaved, Write, keep_unsaved, lyrics_source,
};

const RULE: &str = "#2a2c31";
const DIM: &str = "#8b9099";

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
    /// The panes' border: the theme's rule ([`Palette`]).
    #[props(default = RULE.to_owned())]
    rule: String,
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
                                            overflow:hidden; border:1px solid {rule}; \
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
                                        overflow:hidden; border:1px solid {rule};",
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
    /// The app's colours (the pane around the page; the page stays paper).
    #[props(default)]
    palette: Option<Palette>,
) -> Element {
    let desk = palette
        .as_ref()
        .map_or_else(|| "#1a1b1e".to_owned(), |p| p.desk.clone());
    let ink_2 = palette
        .as_ref()
        .map_or_else(|| DIM.to_owned(), |p| p.ink_2.clone());
    let button = palette
        .as_ref()
        .map_or_else(|| SMALL_BUTTON.to_owned(), Palette::button);
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
        format!(
            "position:relative; width:100%; display:flex; flex-direction:column; background:{desk};"
        )
    } else {
        format!(
            "position:absolute; top:0; left:0; width:100%; height:100%; display:flex; \
             flex-direction:column; align-items:center; background:{desk};"
        )
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
                        height:28px; font-size:11px; color:{ink_2};",
                button {
                    style: "{button}",
                    onclick: move |_| held.set(Some(page.saturating_sub(1))),
                    "\u{2039}"
                }
                span { "Page {page + 1} of {count}" }
                button {
                    style: "{button}",
                    onclick: move |_| held.set(Some(page.saturating_add(1).min(last))),
                    "\u{203A}"
                }
                if held().is_some() {
                    button {
                        style: "{button}",
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

/// The workspace's colours: the app's theme tokens.
///
/// Each is the token's CSS variable (what a process view's sub-document has
/// at `:root`), falling back to the theme Architect resolves here (the
/// built-in default where there is no store above: a process view drawn
/// remotely).
#[derive(Clone, PartialEq, Eq)]
pub struct Palette {
    desk: String,
    sheet: String,
    ink: String,
    ink_2: String,
    rule: String,
    pill: String,
    brand: String,
    brand_ink: String,
    danger: String,
    /// The sheet is light: the editor's syntax colours for it.
    light: bool,
}

impl Palette {
    fn of(r: &architect_theme::Resolved) -> Self {
        let v = |token: &str| format!("var(--{token}, {})", r.value(token));
        Self {
            desk: v("desk"),
            sheet: v("sheet"),
            ink: v("ink"),
            ink_2: v("ink-2"),
            rule: v("rule"),
            pill: v("pill"),
            brand: v("brand"),
            brand_ink: v("brand-ink"),
            danger: v("danger"),
            light: r.color("sheet").luminance() > 0.5,
        }
    }

    /// The editor's custom properties, from the theme, on its pane only.
    fn editor_vars(&self) -> String {
        format!(
            "--background: {}; --foreground: {}; --muted: {}; --muted-foreground: {}; \
             --border: {}; --primary: {};",
            self.sheet, self.ink, self.pill, self.ink_2, self.rule, self.brand
        )
    }

    fn button(&self) -> String {
        format!(
            "display:flex; align-items:center; justify-content:center; gap:4px; height:22px; \
             min-width:22px; padding:0 8px; border-radius:5px; border:1px solid {}; \
             background:{}; color:{}; font-size:12px; cursor:pointer;",
            self.rule, self.pill, self.ink
        )
    }
}

/// `lyrics` as the panel shows them, against `song`'s parts when it is
/// engraved; without a chart, the whole song as one part, so the lines
/// still show and follow.
fn words_of(
    lyrics: Option<&LyricsSource>,
    song: Option<&Song>,
    title: &str,
    artist: Option<&str>,
) -> Words {
    let (lines, regions) = song.map_or_else(
        || {
            let mut lines = lyrics.map(|s| s.lyrics(f64::MAX)).unwrap_or_default();
            let end = lines.lines.last().map_or(0.0, |l| l.start + LAST_LINE_HOLD);
            if let Some(last) = lines.lines.last_mut() {
                last.end = end;
            }
            let whole = ColoredSpan {
                span: session::lyrics::SectionSpan {
                    name: "Song".to_owned(),
                    start: 0.0,
                    end,
                },
                color: None,
            };
            (lines, vec![whole])
        },
        |song| {
            (
                lyrics.map(|s| song.lyrics(s)).unwrap_or_default(),
                song.spans()
                    .into_iter()
                    .map(|(span, color)| ColoredSpan {
                        span,
                        color: Some(color),
                    })
                    .collect(),
            )
        },
    );
    Words::new(lines, regions, title.to_owned(), artist.map(str::to_owned))
}

/// How long the last lyric line is held without a chart to end it.
const LAST_LINE_HOLD: f64 = 6.0;

/// What a reply to an edit leaves the workspace saying.
fn note(status: &mut Signal<(ChartTextStatus, String)>, kind: ChartTextStatus, text: String) {
    status.set((kind, text));
}

/// The workspace over a song loaded from its note: the progress, the
/// chart's text and the lyrics, and the chart, following one clock.
///
/// `form` and `at` fix the form and the position (a picture of it); left
/// out, the form follows the pane's width and the song starts at 0.
#[allow(clippy::too_many_lines)]
#[component]
pub fn SongWorkspace(
    loaded: Loaded,
    #[props(default)] form: Option<Form>,
    #[props(default)] at: Option<f64>,
) -> Element {
    use_context_provider(|| LyricsChoice::of(LyricsMode::Performer));
    let palette = Palette::of(&architect_theme::use_resolved());
    let origin = use_hook(web_time::Instant::now);
    let opened = use_hook(|| {
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    });
    let wall = move || origin.elapsed().as_secs_f64();
    // Until a chart is engraved (or with none), the song ends with its
    // lyrics.
    let lyrics_end = loaded.lyrics.as_ref().and_then(|l| {
        l.lyrics(f64::MAX)
            .lines
            .last()
            .map(|line| line.start + LAST_LINE_HOLD)
    });
    let mut clock = use_signal(|| {
        let mut c = Clock::new(lyrics_end);
        c.seek(at.unwrap_or(0.0), 0.0);
        c
    });
    let mut now = use_signal(|| at.unwrap_or(0.0));
    let mut engraved = use_signal(|| None::<Engraved>);
    let mut chart_error = use_signal(|| None::<String>);
    // The lyrics, with or without a chart: they never wait on it.
    let mut words = use_signal(|| {
        loaded
            .lyrics
            .as_ref()
            .map(|l| words_of(Some(l), None, &loaded.title, loaded.artist.as_deref()))
    });
    // Where the chart came from, and so whether it may be edited and saved.
    let mut access = use_signal(|| loaded.chart.clone());
    let first_status = match loaded.chart.editable() {
        Err(why) => (ChartTextStatus::Warn, why),
        Ok(()) if loaded.chart.text.trim().is_empty() => {
            (ChartTextStatus::Idle, "this song has no chart".to_owned())
        }
        Ok(()) => (ChartTextStatus::Idle, "engraving…".to_owned()),
    };
    let mut status = use_signal(|| first_status);
    // Saving over the `.kf`: only for a chart read from it.
    let mut saving = use_signal(|| {
        matches!(loaded.chart.from, ChartFrom::File).then(|| Saving::new(loaded.chart.text.clone()))
    });
    // A save refused, the way on being to read the `.kf` again: `Some(true)`
    // it could not be read (the app may need the vault's folder), `Some(false)`
    // it changed since it was read.
    let mut stale = use_signal(|| None::<bool>);
    let mut state = use_signal(|| editor_state::EditorState::new(loaded.source()));
    let mut editor_open = use_signal(|| None::<bool>);
    let (size, mut node) = use_measured();
    let form = form.unwrap_or_else(|| Form::of(size().0));

    let lrc = match &loaded.lyrics {
        Some(LyricsSource::Lrc(text)) => Some(text.clone()),
        _ => None,
    };
    let (title, artist) = (loaded.title.clone(), loaded.artist.clone());
    let replies = use_coroutine(move |mut rx: UnboundedReceiver<Reply>| {
        let lrc = lrc.clone();
        let (title, artist) = (title.clone(), artist.clone());
        async move {
            while let Some(reply) = rx.next().await {
                let (song, pages, lane) = match reply.result {
                    Ok(done) => done,
                    Err(why) => {
                        if engraved.peek().is_none() {
                            chart_error.set(Some(why.clone()));
                        }
                        note(
                            &mut status,
                            ChartTextStatus::Warn,
                            format!("not engraved: {why}"),
                        );
                        continue;
                    }
                };
                chart_error.set(None);
                let (_, end) = song.span();
                clock.write().set_end(Some(end));
                // One lyrics source: the LRC, else the chart's lane (as
                // edited).
                let source = lyrics_source(lrc.as_deref(), None)
                    .or_else(|| lane.map(LyricsSource::ChartLane));
                words.set(
                    source
                        .as_ref()
                        .map(|s| words_of(Some(s), Some(&song), &title, artist.as_deref())),
                );
                let bars = song.chart.bars.len();
                engraved.set(Some(Engraved(Arc::new(song), Arc::new(pages))));
                save(
                    &reply.text,
                    bars,
                    opened,
                    access,
                    &mut saving,
                    &mut status,
                    &mut stale,
                )
                .await;
            }
        }
    });
    let worker = use_hook(|| {
        let tx = start(replies.tx());
        if !loaded.source().trim().is_empty() {
            let _ = tx.send(loaded.source().to_owned());
        }
        tx
    });
    // Each change of text goes to the engraver, which waits for the pause.
    let mut last = use_signal(|| loaded.source().to_owned());
    let sending = worker.clone();
    use_effect(move || {
        let text = state.read().doc.to_string();
        if *last.peek() != text {
            last.set(text.clone());
            let _ = sending.send(text);
        }
    });
    // Read the `.kf` again: after the app refused it (asking for the
    // vault's folder first), or to leave the note's read-only copy. The
    // editor then holds what the file holds, and saving starts from it.
    let file = loaded.chart.file.clone();
    let unsaved_to = loaded.chart.file.clone();
    let reread = use_callback(move |ask: bool| {
        let Some(file) = file.clone() else { return };
        let unsaved_to = unsaved_to.clone().unwrap_or_default();
        spawn(async move {
            let mut read = crate::host::read_vault_file(&file).await;
            if read.is_err() && ask && crate::host::allow_vault().await.is_ok() {
                read = crate::host::read_vault_file(&file).await;
            }
            let was_editable = access.peek().editable().is_ok();
            let next = access.peek().clone().reread(read);
            match next.editable() {
                Ok(()) => {
                    let current = state.peek().doc.to_string();
                    let prev = saving.peek().clone();
                    // An edit not saved (the file changed under it) is
                    // kept: written to the extension's folder before the
                    // editor takes the file's text, and said where. Not
                    // written, the editor keeps it and nothing is read.
                    let edit = Unsaved::of(
                        was_editable,
                        &current,
                        &next.text,
                        prev.as_ref().map(Saving::base),
                        || crate::workspace::unsaved_path(&unsaved_to, opened, now_secs()),
                    );
                    let kept = match keep_unsaved(edit, |path, text| async move {
                        crate::host::write_own_file(&path, &text).await
                    })
                    .await
                    {
                        Reread::Take { kept } => kept,
                        Reread::Hold { why } => {
                            note(&mut status, ChartTextStatus::Warn, why);
                            return;
                        }
                    };
                    stale.set(None);
                    if !was_editable || current != next.text {
                        // The file's text, not the copy (nor an edit made
                        // against another version): the editor starts from it.
                        state.set(editor_state::EditorState::new(&next.text));
                    }
                    // Saving starts from the file; the file as opened stays
                    // the one backed up.
                    saving.set(Some(prev.map_or_else(
                        || Saving::new(next.text.clone()),
                        |s| s.reread(next.text.clone()),
                    )));
                    let said = kept.map_or_else(
                        || format!("read {} again", base(&file)),
                        |path| {
                            format!(
                                "read {} again; your unsaved edit is kept in the extension's folder: {path}",
                                base(&file)
                            )
                        },
                    );
                    note(&mut status, ChartTextStatus::Done, said);
                }
                Err(why) => note(&mut status, ChartTextStatus::Warn, why),
            }
            access.set(next);
        });
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
    let editable = access.read().editable().is_ok();
    let read_only = matches!(access.read().from, ChartFrom::NoteCopy { .. });
    let asking = saving.read().as_ref().is_some_and(|s| s.asking().is_some());
    let end = song
        .as_ref()
        .map(|s| s.0.span().1)
        .or(lyrics_end)
        .unwrap_or_default();
    let back_to = song.as_ref().map(|s| s.0.back_from(t)).unwrap_or_default();
    let on_to = song.as_ref().and_then(|s| s.0.forward_from(t));
    let button = palette.button();
    let p = palette.clone();

    let progress = rsx! {
        div { style: "display:flex; flex-direction:column; gap:6px;",
            div {
                style: "display:flex; align-items:center; gap:8px; height:28px; color:{p.ink}; font-size:13px;",
                button { style: "{button}", title: "Back a section",
                    onclick: move |_| seek.call(back_to),
                    lucide_dioxus::SkipBack { size: 14, color: "currentColor" } }
                button {
                    style: "{button} min-width:56px; background:{p.brand}; color:{p.brand_ink}; border-color:{p.brand}; font-weight:700;",
                    "data-transport": if playing { "playing" } else { "paused" },
                    onclick: move |_| {
                        clock.write().toggle(wall());
                    },
                    if playing {
                        lucide_dioxus::Pause { size: 14, color: "currentColor" }
                    } else {
                        lucide_dioxus::Play { size: 14, color: "currentColor" }
                    }
                }
                button { style: "{button}", title: "On a section",
                    onclick: move |_| if let Some(to) = on_to { seek.call(to) },
                    lucide_dioxus::SkipForward { size: 14, color: "currentColor" } }
                span { style: "color:{p.ink_2}; font-variant-numeric:tabular-nums;",
                    "{clock_text(t)} / {clock_text(end)}" }
                span { style: "flex:1; min-width:0; overflow:hidden; white-space:nowrap; font-weight:700; padding-left:6px;",
                    "{loaded.title}" }
                if !loaded.shared.is_empty() {
                    span {
                        "data-shared": "{loaded.shared.len()}",
                        title: "A share page draws the chart the song's note carries, written when the song was imported: an edit here reaches the .kf, not that copy, until the app can rewrite a note's body (FastTrackStudios/architect#492).",
                        style: "flex:none; font-size:11px; color:{p.ink_2}; border:1px solid {p.rule}; border-radius:5px; padding:1px 6px;",
                        "Shared: its page shows the chart as imported"
                    }
                }
                if asking {
                    button {
                        style: "{button} border-color:{p.danger}; color:{p.danger};",
                        "data-confirm": "empty-chart",
                        title: "Save the chart as it is now, with no bars, over the saved one",
                        // The editor's text as it is now, never a copy taken
                        // when the question was asked: if it changed since,
                        // the question no longer stands and nothing is saved.
                        onclick: move |_| {
                            let current = state.peek().doc.to_string();
                            let yes = saving.write().as_mut().is_some_and(|s| s.confirm(&current));
                            if yes {
                                let _ = worker.send(current);
                            } else {
                                note(&mut status, ChartTextStatus::Idle,
                                    "the chart changed since: nothing to save anyway".to_owned());
                            }
                        },
                        "Save anyway"
                    }
                }
                if read_only || stale().is_some() {
                    button {
                        style: "{button}",
                        "data-reread": "kf",
                        title: "Read the song's .kf again (asking for the vault's folder if the app needs it) before anything is written; an edit not saved is kept in the extension's folder",
                        onclick: move |_| reread.call(true),
                        if stale() == Some(true) && !read_only { "Allow saving" } else { "Read the .kf again" }
                    }
                }
                button {
                    style: "{button}",
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
        ChartText {
            state,
            message,
            status: status_kind,
            editable,
            vars: palette.editor_vars(),
            light: palette.light,
        }
    };
    let empty_chart = if loaded.source().trim().is_empty() {
        Some("This song has no chart.".to_owned())
    } else {
        chart_error().map(|why| format!("The chart does not engrave: {why}"))
    };
    let chart = song.map_or_else(
        || rsx! {
            div { style: "position:absolute; top:0; left:0; width:100%; height:100%; display:flex; \
                          align-items:center; justify-content:center; padding:16px; box-sizing:border-box; \
                          text-align:center; color:{p.ink_2}; font-size:12px; background:{p.desk}; min-height:200px;",
                "data-chart-empty": if empty_chart.is_some() { "true" } else { "false" },
                {empty_chart.clone().unwrap_or_else(|| "Engraving the chart…".to_owned())}
            }
        },
        |song| rsx! { ChartPages { song, now: t, on_seek: seek, across: form == Form::Narrow, palette: Some(palette.clone()) } },
    );
    let lyrics = words().map_or_else(
        || rsx! {
            div { style: "position:absolute; top:0; left:0; width:100%; height:100%; display:flex; \
                          align-items:center; justify-content:center; padding:16px; box-sizing:border-box; \
                          text-align:center; color:{p.ink_2}; font-size:12px; background:{p.sheet};",
                "No timed lyrics for this song: give it an LRC, or its chart a timed lyric lane ([lyrics lrc])."
            }
        },
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
            style: "position:absolute; top:0; left:0; right:0; bottom:0; background:{p.desk}; color:{p.ink}; \
                    font-family:system-ui, sans-serif;",
            onmounted: move |e| node.set(Some(e.data())),
            WorkspaceLayout { arrangement, progress, editor, chart, lyrics, rule: palette.rule }
        }
    }
}

/// Save an engraved edit `text` (of `bars` bars) to the `.kf`.
///
/// As
/// [`Saving::decide`] says: never the note's copy, never over a file that
/// changed or could not be read again, never a chart with no bars over one
/// with bars without the person's yes; the file as opened backed up first,
/// once, in the extension's own folder. The write is the host's
/// `write_file`, which truncates and then writes: not atomic
/// (FastTrackStudios/architect#590), hence the backup.
// Its future holds the workspace's signals: it runs on the view's own
// executor (the coroutine), never sent to another thread.
#[allow(clippy::too_many_arguments, clippy::future_not_send)]
async fn save(
    text: &str,
    bars: usize,
    opened: u64,
    access: Signal<ChartAccess>,
    saving: &mut Signal<Option<Saving>>,
    status: &mut Signal<(ChartTextStatus, String)>,
    stale: &mut Signal<Option<bool>>,
) {
    let chart = access.peek().clone();
    if let Err(why) = chart.editable() {
        note(status, ChartTextStatus::Warn, why);
        return;
    }
    let (Some(file), ChartFrom::File) = (chart.file.clone(), &chart.from) else {
        note(
            status,
            ChartTextStatus::Warn,
            "engraved — the song has no .kf to save to".to_owned(),
        );
        return;
    };
    let Some(mut save) = saving.peek().clone() else {
        return;
    };
    // A question about another text (and a yes to it) no longer stands.
    save.now_saving(text);
    if save.base() == text {
        // What the file holds (its first engraving): nothing to write.
        save.knows_bars(bars);
        saving.set(Some(save));
        note(status, ChartTextStatus::Done, "engraved".to_owned());
        return;
    }
    // Read again before anything is written.
    let on_disk = crate::host::read_vault_file(&file).await;
    let decision = save.decide(on_disk.as_deref().map_err(String::as_str), text, bars);
    // The question (or its end) is kept with the rest of the saving state.
    saving.set(Some(save.clone()));
    match decision {
        Write::Unchanged => note(status, ChartTextStatus::Done, "engraved".to_owned()),
        Write::Refuse { why, reread } => {
            stale.set(reread.then_some(on_disk.is_err()));
            note(status, ChartTextStatus::Warn, why);
        }
        Write::Confirm(question) => note(status, ChartTextStatus::Warn, question),
        Write::Save { backup } => {
            // Never through a symbolic link: the host's `write_file`
            // follows it, wherever it points (FastTrackStudios/architect#592).
            match crate::host::link_on_path(&file).await {
                Ok(None) => {}
                Ok(Some(link)) => {
                    note(
                        status,
                        ChartTextStatus::Warn,
                        format!(
                            "not saved: {link} is a symbolic link, and a write would follow it \
                             wherever it points (outside the vault, maybe)"
                        ),
                    );
                    return;
                }
                Err(why) => {
                    note(status, ChartTextStatus::Warn, format!("not saved: {why}"));
                    return;
                }
            }
            if backup {
                let path = crate::workspace::backup_path(&file, opened);
                if let Err(why) = crate::host::write_own_file(&path, save.base()).await {
                    note(
                        status,
                        ChartTextStatus::Warn,
                        format!("not saved: the .kf could not be backed up first ({why})"),
                    );
                    return;
                }
            }
            match crate::host::write_vault_file(&file, text).await {
                Ok(()) => {
                    save.saved(text.to_owned(), bars);
                    saving.set(Some(save));
                    stale.set(None);
                    note(
                        status,
                        ChartTextStatus::Done,
                        format!("saved to {}", base(&file)),
                    );
                }
                Err(why) => {
                    stale.set(Some(true));
                    note(
                        status,
                        ChartTextStatus::Warn,
                        format!("engraved, not saved: {why}"),
                    );
                }
            }
        }
    }
}

/// Seconds since the epoch, now.
fn now_secs() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A vault path's file name.
fn base(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}
