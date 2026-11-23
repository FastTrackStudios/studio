//! The arrangement of the song playing (the session's main view): its sections over a
//! time ruler, its tracks with their clips, and a progress bar and a
//! playhead that follow Now Playing. A press or a drag on either seeks.
//!
//! Drawn from plain elements, so it renders the same in Blitz and in a
//! browser: every lane is a row whose clips are placed in percent of the
//! song's length (no measured widths), the tracks scroll in one
//! `overflow-y: scroll` pane, and the playhead is a component of its own
//! so only it redraws as the song plays. The model is
//! [`crate::arrangement`], read from the song's one file.

use std::rc::Rc;

use dioxus::prelude::*;

use crate::arrangement::{Arrangement, Lane};
use crate::state::use_app;

/// A model as a prop: equal when it is the same one, so the lanes are
/// drawn once per song.
#[derive(Clone)]
pub struct Model(pub Rc<Arrangement>);

impl PartialEq for Model {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The arrangement of the song Now Playing holds: the session's main view.
#[component]
pub fn ArrangementView() -> Element {
    let app = use_app();
    let Some(n) = (app.np)().filter(|n| n.song().is_some()) else {
        return rsx! {
            p { class: "pline", "Open a set, and its songs' arrangements show here." }
        };
    };
    let folder = n.place().map(|p| p.folder.clone()).unwrap_or_default();
    let bpm = n.song().and_then(|s| s.tempo).unwrap_or(120);
    rsx! {
        div { class: "arrpage",
            Loaded { key: "{n.i}/{folder}", i: n.i, folder: folder.clone(), bpm }
        }
    }
}

/// The song's model, read off the UI thread, then drawn.
#[component]
fn Loaded(i: usize, folder: String, bpm: u32) -> Element {
    let app = use_app();
    let model = use_resource(use_reactive!(|folder, bpm| async move {
        load(folder, f64::from(bpm)).await
    }));
    // Now Playing's length is the arrangement's once it is known, so the
    // playheads and the engine's position agree.
    use_effect(move || {
        if let Some(Ok(m)) = &*model.read() {
            let len = m.0.len;
            let mut np = app.np;
            if np.peek().as_ref().is_some_and(|n| n.i == i && n.dur < len) {
                np.with_mut(|n| {
                    if let Some(n) = n {
                        n.dur = len;
                    }
                });
            }
        }
    });
    match &*model.read() {
        None => rsx! {
            p { class: "pline", "Reading the song's arrangement…" }
        },
        Some(Err(e)) => rsx! {
            p { class: "pline", "No arrangement for this song: {e}" }
        },
        Some(Ok(m)) => rsx! {
            ProgressBar { model: m.clone() }
            Lanes { model: m.clone() }
        },
    }
}

#[cfg(feature = "native")]
async fn load(folder: String, bpm: f64) -> Result<Model, String> {
    if folder.is_empty() {
        return Err("it has no song folder on this computer".into());
    }
    let (tx, rx) = futures_channel::oneshot::channel();
    std::thread::spawn(move || {
        let mut span = crate::trace::Span::root("studio.arrangement.open");
        let read = crate::arrangement::of_folder(&folder, bpm);
        match &read {
            Ok((_, a)) => {
                span.set("studio.arrangement.tracks", a.tracks.len() as u64);
                span.set("studio.arrangement.shown", a.shown().len() as u64);
                span.set(
                    "studio.arrangement.clips",
                    a.tracks.iter().map(|t| t.clips.len()).sum::<usize>() as u64,
                );
                span.set("studio.arrangement.sections", a.sections.len() as u64);
                span.set("studio.arrangement.len_s", a.len);
                span.set("studio.arrangement.outcome", "read");
            }
            Err(e) => {
                span.set("studio.arrangement.outcome", "failed");
                span.fail(e.clone());
            }
        }
        drop(span);
        let _ = tx.send(read.map(|(_, a)| a));
    });
    // The Rc is made here, on the UI thread (it is not Send).
    match rx.await {
        Ok(Ok(a)) => Ok(Model(Rc::new(a))),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("the reader stopped".into()),
    }
}

#[cfg(not(feature = "native"))]
async fn load(folder: String, bpm: f64) -> Result<Model, String> {
    let _ = (folder, bpm);
    Err("the arrangement shows in the desktop app for now".into())
}

/// Seconds at a pointer's `x` (client) over an element `rect`.
fn at_x(x: f64, rect: &dioxus::html::geometry::PixelsRect, len: f64) -> Option<f64> {
    (rect.width() > 0.0).then(|| ((x - rect.min_x()) / rect.width()).clamp(0.0, 1.0) * len)
}

/// A press or drag over `el` seeks to where it is.
fn seek_at(el: Signal<Option<Rc<MountedData>>>, x: f64, len: f64) {
    let app = consume_context::<crate::state::AppState>();
    spawn(async move {
        let Some(el) = el() else { return };
        if let Ok(rect) = el.get_client_rect().await
            && let Some(t) = at_x(x, &rect, len)
        {
            let mut seek = app.seek;
            seek.set(Some(t));
        }
    });
}

/// A colour for a section, from its name.
fn section_hue(name: &str) -> u32 {
    let n = name.to_ascii_lowercase();
    let key = n.split_whitespace().next().unwrap_or("");
    match key {
        "intro" | "count" => 210,
        "verse" => 160,
        "pre" | "pre-chorus" | "prechorus" => 45,
        "chorus" => 330,
        "bridge" => 270,
        "outro" | "ending" => 200,
        "solo" | "inst" | "instrumental" | "interlude" => 20,
        _ => {
            key.bytes()
                .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)))
                % 360
        }
    }
}

/// The whole song as one bar: its sections, how far it has played, and a
/// press or drag anywhere on it moves there.
#[component]
fn ProgressBar(model: Model) -> Element {
    let app = use_app();
    let el = use_signal(|| None::<Rc<MountedData>>);
    let mut dragging = use_signal(|| false);
    let len = model.0.len;
    let now = (app.np)().map(|n| n.secs().0).unwrap_or(0.0);
    let p = (now / len * 100.0).clamp(0.0, 100.0);
    let sections = model.0.sections.clone();
    rsx! {
        div {
            class: "arrprog",
            onmounted: move |e| {
                let mut el = el;
                el.set(Some(e.data()));
            },
            onmousedown: move |e: MouseEvent| {
                dragging.set(true);
                seek_at(el, e.client_coordinates().x, len);
            },
            onmousemove: move |e: MouseEvent| {
                if dragging() {
                    seek_at(el, e.client_coordinates().x, len);
                }
            },
            onmouseup: move |_| dragging.set(false),
            onmouseleave: move |_| dragging.set(false),
            for s in sections {
                div {
                    key: "{s.start}",
                    class: "arrprogsec",
                    style: "left:{s.start / len * 100.0}%; width:{(s.end - s.start) / len * 100.0}%; --h:{section_hue(&s.name)};",
                    span { "{s.name}" }
                }
            }
            div { class: "arrprogfill", style: "width:{p}%;" }
            div { class: "arrprogknob", style: "left:{p}%;" }
        }
        div { class: "arrprogtimes",
            span { "{crate::songs::clock(now)}" }
            span { "{crate::songs::clock(len)}" }
        }
    }
}

/// Every `n` bars, so the ruler has at most about 32 numbers.
fn bar_step(a: &Arrangement) -> u32 {
    let bar = 60.0 / a.bpm.max(20.0) * f64::from(a.beats_per_bar.max(1));
    let bars = (a.len / bar).ceil() as u32;
    [1, 2, 4, 8, 16, 32, 64]
        .into_iter()
        .find(|s| bars / s <= 32)
        .unwrap_or(64)
}

/// The ruler, the sections and the tracks with their clips, under one
/// playhead; a press or drag over them seeks.
#[component]
fn Lanes(model: Model) -> Element {
    let a = model.0.clone();
    let len = a.len;
    let el = use_signal(|| None::<Rc<MountedData>>);
    let mut dragging = use_signal(|| false);
    let shown: Vec<Lane> = a.shown().into_iter().cloned().collect();
    let hidden = a.tracks.len() - shown.len();
    let clips: usize = shown.iter().map(|t| t.clips.len()).sum();
    let bars = a.bars(bar_step(&a));
    let sections = a.sections.clone();
    rsx! {
        p { class: "pline",
            "{shown.len()} tracks · {clips} clips · {a.sections.len()} sections · {a.bpm:.0} bpm {a.beats_per_bar}/4"
            if hidden > 0 {
                " · {hidden} empty tracks folded away"
            }
        }
        div { class: "arrscroll",
            div { class: "arrgrid",
                div { class: "arrnames",
                    div { class: "arrnh arrrulerh", "Bar" }
                    div { class: "arrnh arrsech", "Sections" }
                    for (k , t) in shown.iter().enumerate() {
                        div {
                            key: "{k}",
                            class: if t.folder { "arrname tfolder" } else { "arrname" },
                            style: "padding-left:{8 + t.depth * 12}px;",
                            span {
                                class: "arrsw",
                                style: "background:{t.color.clone().unwrap_or_else(|| \"#52525b\".into())};",
                            }
                            span { class: "arrnm", "{t.name}" }
                        }
                    }
                }
                div {
                    class: "arrtl",
                    onmounted: move |e| {
                        let mut el = el;
                        el.set(Some(e.data()));
                    },
                    onmousedown: move |e: MouseEvent| {
                        dragging.set(true);
                        seek_at(el, e.client_coordinates().x, len);
                    },
                    onmousemove: move |e: MouseEvent| {
                        if dragging() {
                            seek_at(el, e.client_coordinates().x, len);
                        }
                    },
                    onmouseup: move |_| dragging.set(false),
                    onmouseleave: move |_| dragging.set(false),
                    div { class: "arrruler",
                        for (at , n) in bars {
                            div { key: "{n}", class: "arrtick", style: "left:{at / len * 100.0}%;",
                                span { "{n}" }
                            }
                        }
                    }
                    div { class: "arrsecs",
                        for s in sections {
                            div {
                                key: "{s.start}",
                                class: "arrsec",
                                style: "left:{s.start / len * 100.0}%; width:{(s.end - s.start) / len * 100.0}%; --h:{section_hue(&s.name)};",
                                span { "{s.name}" }
                            }
                        }
                    }
                    for (k , t) in shown.iter().enumerate() {
                        div { key: "{k}", class: if t.folder { "arrlane tfolder" } else { "arrlane" },
                            for (j , c) in t.clips.iter().enumerate() {
                                div {
                                    key: "{j}",
                                    class: if c.midi { "arrclip midi" } else if c.muted { "arrclip muted" } else { "arrclip" },
                                    style: "left:{c.start / len * 100.0}%; width:{c.len / len * 100.0}%; background:{c.color.clone().unwrap_or_else(|| \"#3f6f8f\".into())};",
                                    span { "{c.name}" }
                                }
                            }
                        }
                    }
                    Playhead { len }
                }
            }
        }
    }
}

/// The line where the song is, over the lanes: the only part that redraws
/// while it plays.
#[component]
fn Playhead(len: f64) -> Element {
    let app = use_app();
    let now = (app.np)().map(|n| n.secs().0).unwrap_or(0.0);
    let p = (now / len.max(1.0) * 100.0).clamp(0.0, 100.0);
    rsx! {
        div { class: "arrph", style: "left:{p}%;" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ruler_keeps_to_about_32_numbers() {
        let a = Arrangement {
            len: 240.0,
            bpm: 120.0,
            beats_per_bar: 4,
            ..Default::default()
        };
        // 120 bars: every 4.
        assert_eq!(bar_step(&a), 4);
        assert!(a.bars(bar_step(&a)).len() <= 33);
    }

    #[test]
    fn sections_have_steady_colours() {
        assert_eq!(section_hue("Chorus 2"), section_hue("chorus"));
        assert_ne!(section_hue("Verse 1"), section_hue("Chorus"));
    }
}
