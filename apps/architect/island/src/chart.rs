//! The song view (web bundle only, studio#491): a song note's chart and
//! its timed lyrics, following this extension's source in the app's
//! playhead, as the retired share page drew them (studio#406).
//!
//! The chart is the song's `kf` fence engraved here, in the frame
//! (`setlists::follow::follow_pages`: the Master Rhythm pages, text as
//! outlines, each bar placed and timed on the chart's own clock). Over
//! each page an overlay of the same size marks the bar being played (the
//! wash of Session's playback cursor) and the beat's line gliding across
//! it, and takes a press on a bar as a seek there. The lyrics are its
//! timed lines, the one being sung marked, a line pressed seeks to it.
//! While the song plays, the bar or line is kept in view.
//!
//! What it reads and asks is [`crate::song`]'s.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use serde_json::Value;
use setlists::follow::{FollowPages, cursor_colors, follow_pages};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;

use crate::song::{self, Due, Pending, Played, at_or_before, following, position};
use crate::views::Ask;

/// The view's sheet: the frame's `style-src 'unsafe-inline'` takes it.
const SHEET: &str = r#"
.sv { font-family: var(--font, system-ui, -apple-system, sans-serif); color: var(--foreground, CanvasText);
      box-sizing: border-box; width: 100%; padding: 0 0 12px; }
.sv * { box-sizing: border-box; }
.sv-tabs { display: flex; gap: 4px; padding: 4px; margin: 0 0 8px; border-radius: 10px; position: sticky; top: 0; z-index: 2;
           background: var(--background, Canvas); border: 1px solid var(--border, GrayText); width: max-content; }
.sv-tab { border: 0; border-radius: 7px; padding: 6px 14px; font: inherit; font-size: 13px; font-weight: 500;
          background: transparent; color: var(--muted-foreground, GrayText); cursor: pointer; min-height: 32px; }
.sv-tab[aria-pressed="true"] { background: var(--accent, Highlight); color: var(--accent-foreground, HighlightText); }
.sv-hint { font-size: 12px; color: var(--muted-foreground, GrayText); margin: 0 0 8px; }
.sv-pages { display: flex; flex-direction: column; gap: 12px; }
.sv-page { position: relative; width: 100%; background: #fff; border-radius: 6px; overflow: hidden;
           box-shadow: 0 0 0 1px rgba(0,0,0,.08); }
.sv-paper > svg { display: block; width: 100%; height: auto; }
.sv-over { position: absolute; left: 0; top: 0; width: 100%; height: 100%; }
.sv-bar { fill: transparent; cursor: pointer; }
.sv-bar:hover { fill: rgba(128,128,128,.12); }
.sv-lines { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
.sv-line { display: block; width: 100%; text-align: left; border: 0; background: transparent; color: var(--muted-foreground, GrayText);
           font: inherit; font-size: 18px; line-height: 1.35; padding: 8px 10px; border-radius: 8px; cursor: pointer; }
.sv-line:hover { background: rgba(128,128,128,.12); }
.sv-line[data-current] { color: var(--foreground, CanvasText); font-weight: 700;
                         background: rgba(128,128,128,.10); }
.sv-line[data-past] { opacity: .7; }
.sv-empty { padding: 16px 0; color: var(--muted-foreground, GrayText); }
"#;

/// The frame's `document`.
fn document() -> Option<JsValue> {
    js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("document"))
        .ok()
        .filter(JsValue::is_object)
}

fn call(target: &JsValue, name: &str, args: &[JsValue]) -> Option<JsValue> {
    let f = js_sys::Reflect::get(target, &JsValue::from_str(name))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    let list = js_sys::Array::new();
    for a in args {
        list.push(a);
    }
    f.apply(target, &list).ok()
}

fn number(v: &JsValue, key: &str) -> f64 {
    js_sys::Reflect::get(v, &JsValue::from_str(key))
        .ok()
        .and_then(|n| n.as_f64())
        .unwrap_or(0.0)
}

/// How long the view leaves the scroll alone after the person scrolled
/// it themselves (a wheel, a touch, a key), ms: following the song never
/// takes the page from under a finger looking for a bar.
const HANDS_OFF_MS: f64 = 4_000.0;

thread_local! {
    /// When the person last scrolled the frame (ms since the epoch).
    static SCROLLED: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// Note the person's own scrolling, once per frame: a wheel, a touch
/// moving, a key (their own, never the view's `scrollBy`).
fn watch_scrolling() {
    let on = Closure::<dyn FnMut()>::new(|| SCROLLED.with(|s| s.set(js_sys::Date::now())));
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"passive".into(), &JsValue::TRUE);
    for event in ["wheel", "touchmove", "keydown"] {
        let _ = call(
            &js_sys::global(),
            "addEventListener",
            &[
                JsValue::from_str(event),
                on.as_ref().clone(),
                opts.clone().into(),
            ],
        );
    }
    // Lives as long as the frame.
    on.forget();
}

/// Keep the element `id` in view: scrolled to a third of the frame's
/// height when it is outside its middle, smoothly; nothing otherwise, nor
/// for [`HANDS_OFF_MS`] after the person scrolled.
fn keep_in_view(id: &str) {
    if js_sys::Date::now() - SCROLLED.with(std::cell::Cell::get) < HANDS_OFF_MS {
        return;
    }
    let Some(doc) = document() else { return };
    let Some(el) =
        call(&doc, "getElementById", &[JsValue::from_str(id)]).filter(JsValue::is_object)
    else {
        return;
    };
    let Some(rect) = call(&el, "getBoundingClientRect", &[]) else {
        return;
    };
    let height = number(&js_sys::global(), "innerHeight");
    if height <= 0.0 {
        return;
    }
    let (top, bottom) = (number(&rect, "top"), number(&rect, "bottom"));
    if top >= height * 0.12 && bottom <= height * 0.82 {
        return;
    }
    let by = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&by, &"top".into(), &JsValue::from_f64(top - height / 3.0));
    let _ = js_sys::Reflect::set(&by, &"behavior".into(), &"smooth".into());
    let _ = call(&js_sys::global(), "scrollBy", &[by.into()]);
}

/// The chart's sections as played, for its queue item.
fn played(chart: &FollowPages) -> Vec<Played> {
    let mut out: Vec<Played> = Vec::new();
    let mut last = None;
    for b in &chart.bars {
        if last == Some(b.section) {
            if let Some(p) = out.last_mut() {
                p.end = b.end;
            }
            continue;
        }
        last = Some(b.section);
        let (name, color) = chart.sections.get(b.section).cloned().unwrap_or_default();
        out.push(Played {
            name,
            start: b.start,
            end: b.end,
            color: song::hex_color(&color),
        });
    }
    out
}

/// The chart's tempo and beats a bar, as its header says them.
fn tempo(source: &str) -> Option<(f64, u8)> {
    let chart = keyflow::parse(source).ok()?;
    let bpm = chart.tempo.as_ref()?.bpm;
    let meter = chart
        .initial_time_signature
        .as_ref()
        .or(chart.time_signature.as_ref())
        .map(ToString::to_string)
        .unwrap_or_default();
    crate::queue::tempo_of(&format!("{bpm} bpm · {meter}"))
}

/// A chart engraved, shared by the view's renders: the same one while its
/// song is (compared by identity, never page by page).
#[derive(Clone)]
struct Engraved(Rc<FollowPages>);

impl PartialEq for Engraved {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// What the view shows: the chart or the lyrics.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    Chart,
    Lyrics,
}

/// The song view, over the app's props and the note it handed.
#[component]
pub fn SongView(page: Signal<Value>, note: Signal<Value>, ask: Ask) -> Element {
    let now_ms = js_sys::Date::now;
    let song = use_memo(move || song::song_of_note(&note.read()));
    // Engraved once a song's chart is known, after the first paint (the
    // engraving takes a moment; the tabs and lyrics show at once).
    let chart = use_resource(move || async move {
        let source = song().and_then(|s| s.chart)?;
        gloo_timers::future::TimeoutFuture::new(16).await;
        let pages = follow_pages(&source)
            .map(|p| Engraved(Rc::new(p)))
            .map_err(|e| e.to_string());
        Some((pages, tempo(&source)))
    });
    let mut tab = use_signal(|| None::<Tab>);
    use_hook(watch_scrolling);
    // Where the song is, moved on between the app's updates while it plays.
    let mut at = use_signal(|| None::<f64>);
    let pending = use_hook(|| Rc::new(RefCell::new(None::<Pending>)));
    {
        let pending = pending.clone();
        use_future(move || {
            let pending = pending.clone();
            async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(50).await;
                    let Some(s) = song.peek().clone() else {
                        continue;
                    };
                    let props = page.peek();
                    let wait = *pending.borrow();
                    if let Some(p) = wait {
                        match song::due(&props, &s, p, now_ms()) {
                            Due::Now((action, args)) => {
                                *pending.borrow_mut() = None;
                                ask.call((action.to_owned(), args));
                            }
                            Due::Wait => {}
                            Due::Dropped => *pending.borrow_mut() = None,
                        }
                    }
                    let t = following(&props, &s).then(|| position(&props, now_ms()));
                    drop(props);
                    if *at.peek() != t {
                        at.set(t);
                    }
                }
            }
        });
    }
    let on_seek = use_callback(move |t: f64| {
        {
            let Some(s) = song.peek().clone() else { return };
            let (sections, beat) = match &*chart.peek() {
                Some(Some((Ok(c), beat))) => (played(&c.0), *beat),
                Some(Some((Err(_), beat))) => (Vec::new(), *beat),
                _ => (Vec::new(), None),
            };
            let (asks, wait) = song::seek(&page.peek(), &s, t, now_ms(), || {
                song::source_of(&s, &sections, beat, t)
            });
            *pending.borrow_mut() = wait;
            for (action, args) in asks {
                ask.call((action.to_owned(), args));
            }
            // Shown at once; the app's next update confirms it.
            if following(&page.peek(), &s) {
                at.set(Some(t));
            }
        }
    });
    let Some(s) = song() else {
        return rsx! {
            document::Style { {SHEET} }
            div { class: "sv", p { class: "sv-empty", "No song." } }
        };
    };
    let has_chart = s.chart.is_some();
    let has_lines = !s.lines.is_empty();
    let shown = tab().unwrap_or(if has_chart { Tab::Chart } else { Tab::Lyrics });
    let playing = page
        .read()
        .get("playing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let hint = match (&s.recording, at()) {
        (None, _) => Some("No recording to follow for this song yet."),
        (Some(_), None) => Some(if shown == Tab::Chart {
            "Tap a bar to play the song from there."
        } else {
            "Tap a line to play the song from there."
        }),
        _ => None,
    };
    let t = at();
    rsx! {
        document::Style { {SHEET} }
        div { class: "sv", "data-testid": "studio-song", "data-following": if t.is_some() { "true" } else { "false" },
            if has_chart && has_lines {
                div { class: "sv-tabs", role: "group", "aria-label": "Show",
                    button { r#type: "button", class: "sv-tab", "aria-pressed": "{shown == Tab::Chart}",
                        onclick: move |_| tab.set(Some(Tab::Chart)), "Chart" }
                    button { r#type: "button", class: "sv-tab", "aria-pressed": "{shown == Tab::Lyrics}",
                        onclick: move |_| tab.set(Some(Tab::Lyrics)), "Lyrics" }
                }
            }
            if let Some(h) = hint {
                p { class: "sv-hint", "{h}" }
            }
            match shown {
                Tab::Chart => match &*chart.read() {
                    Some(Some((Ok(pages), _))) => rsx! {
                        Chart { pages: pages.clone(), t, playing, on_seek }
                    },
                    Some(Some((Err(why), _))) => rsx! { p { class: "sv-empty", "The chart did not engrave: {why}" } },
                    Some(None) => rsx! { p { class: "sv-empty", "No chart for this song." } },
                    None => rsx! { p { class: "sv-empty", "Engraving the chart…" } },
                },
                Tab::Lyrics => rsx! {
                    Lines { lines: s.lines, t, playing, on_seek }
                },
            }
        }
    }
}

/// The chart's pages, each with its overlay: the bars to press, the bar
/// being played and the beat's line.
#[component]
fn Chart(pages: Engraved, t: Option<f64>, playing: bool, on_seek: EventHandler<f64>) -> Element {
    let pages = &pages.0;
    let (wash, line) = cursor_colors();
    let starts: Vec<f64> = pages.bars.iter().map(|b| b.start).collect();
    let bar = t.and_then(|t| at_or_before(&starts, t)).filter(|i| {
        pages
            .bars
            .get(*i)
            .is_some_and(|b| t.is_some_and(|t| t < b.end))
    });
    let beat_ats: Vec<f64> = pages.beats.iter().map(|b| b.at).collect();
    let beat = t.and_then(|t| {
        let i = at_or_before(&beat_ats, t)?;
        let b = pages.beats.get(i)?;
        let end = b
            .until
            .or_else(|| pages.beats.get(i.saturating_add(1)).map(|n| n.at))
            .unwrap_or(b.at);
        let frac = if end > b.at {
            ((t - b.at) / (end - b.at)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some((b.page, b.glide.mul_add(frac, b.x), b.y, b.width, b.height))
    });
    let current = bar.and_then(|i| pages.bars.get(i).map(|b| (i, b.page)));
    use_effect(use_reactive!(|current, playing| {
        if playing && current.is_some() {
            keep_in_view("sv-current-bar");
        }
    }));
    let (w, h) = (pages.width, pages.height);
    rsx! {
        div { class: "sv-pages", "data-bar": current.map(|(i, _)| i.to_string()),
            for (n, svg) in pages.pages.iter().enumerate() {
                div { key: "p{n}", class: "sv-page", style: "aspect-ratio: {w} / {h};",
                    div { class: "sv-paper", dangerous_inner_html: "{svg}" }
                    svg { class: "sv-over", view_box: "0 0 {w} {h}", "aria-label": "Page {n + 1} of the chart",
                        for (i, b) in pages.bars.iter().enumerate().filter(|(_, b)| b.page == n) {
                            {
                                let start = b.start;
                                rsx! {
                                    rect { key: "b{i}", class: "sv-bar", x: "{b.x}", y: "{b.y}", width: "{b.width}", height: "{b.height}",
                                        "data-at": "{start:.3}", role: "button", "aria-label": "Bar {b.number}, play from here",
                                        onclick: move |_| on_seek.call(start),
                                    }
                                }
                            }
                        }
                        if let Some((i, page)) = current.filter(|(_, p)| *p == n) {
                            if let Some(b) = pages.bars.get(i) {
                                rect { id: "sv-current-bar", "data-testid": "chart-current-bar", "data-bar": "{b.number}",
                                    x: "{b.x}", y: "{b.y}", width: "{b.width}", height: "{b.height}", rx: "2",
                                    fill: "{wash}", pointer_events: "none", "data-page": "{page}" }
                            }
                        }
                        if let Some((_, x, y, bw, bh)) = beat.filter(|b| b.0 == n) {
                            rect { "data-testid": "chart-beat", x: "{x:.2}", y: "{y:.2}", width: "{bw:.2}", height: "{bh:.2}",
                                fill: "{line}", pointer_events: "none" }
                        }
                    }
                }
            }
        }
    }
}

/// The timed lines, the one being sung marked; a line pressed seeks there.
#[component]
fn Lines(
    lines: Vec<(f64, String)>,
    t: Option<f64>,
    playing: bool,
    on_seek: EventHandler<f64>,
) -> Element {
    if lines.is_empty() {
        return rsx! { p { class: "sv-empty", "No timed lyrics for this song yet." } };
    }
    let starts: Vec<f64> = lines.iter().map(|(at, _)| *at).collect();
    let current = t.and_then(|t| at_or_before(&starts, t));
    use_effect(use_reactive!(|current, playing| {
        if playing && current.is_some() {
            keep_in_view("sv-current-line");
        }
    }));
    rsx! {
        ol { class: "sv-lines", "data-testid": "song-lines", "data-line": current.map(|c| c.to_string()),
            for (i, (at, text)) in lines.iter().enumerate() {
                {
                    let at = *at;
                    let mm = (at / 60.0).floor();
                    let ss = mm.mul_add(-60.0, at).floor();
                    rsx! {
                        li { key: "l{i}",
                            button { r#type: "button", class: "sv-line",
                                id: if current == Some(i) { "sv-current-line" },
                                "data-current": if current == Some(i) { "" },
                                "data-past": if current.is_some_and(|c| i < c) { "" },
                                "data-at": "{at:.3}",
                                "aria-current": if current == Some(i) { "true" },
                                "aria-label": "{text}, at {mm}:{ss:02}, play from here",
                                onclick: move |_| on_seek.call(at),
                                "{text}"
                            }
                        }
                    }
                }
            }
        }
    }
}
