//! A setlist file from Drive, as a set: its acts and songs, each song's
//! chart, and the way to play it. And the chart of the song playing,
//! following the recording bar by bar.

use dioxus::prelude::*;

use crate::Route;
#[cfg_attr(not(feature = "drive"), allow(unused_imports))]
use crate::icons::{I, Icon};
use crate::pieces::{Bar, Page};
use crate::state::use_app;

/// The spaces to look in: the org Studio is in, then every other.
#[cfg(feature = "drive")]
fn spaces() -> Vec<String> {
    let app = use_app();
    let mut v = vec![(app.here)()];
    for o in (app.orgs)() {
        if !v.contains(&o) {
            v.push(o);
        }
    }
    v
}

#[component]
pub fn DriveSet(space: String, location: String, path: Vec<String>) -> Element {
    let path = path.join("/");
    let title = crate::setfile::name_of(path.rsplit('/').next().unwrap_or(&path));
    rsx! {
        Page {
            Bar { back: Some(Route::Setlists {}), title: Some(title.clone()) }
            div { class: "body",
                Loaded { space, location, path }
            }
        }
    }
}

#[cfg(not(feature = "drive"))]
#[component]
fn Loaded(space: String, location: String, path: String) -> Element {
    let _ = (space, location, path);
    rsx! {
        p { class: "pline", "A setlist file opens in the browser build, through Drive." }
    }
}

#[cfg(feature = "drive")]
#[component]
fn Loaded(space: String, location: String, path: String) -> Element {
    let app = use_app();
    let spaces = spaces();
    // This page, for the session's way back.
    let here = Route::DriveSet {
        space: space.clone(),
        location: location.clone(),
        path: path.split('/').map(str::to_string).collect(),
    };
    // A file on disk: read again when it changes (edited by hand) or when
    // Drive has found charts for it.
    let mut version = use_signal(|| (0_i64, 0_usize));
    {
        let (location, path, spaces) = (location.clone(), path.clone(), spaces.clone());
        use_future(move || {
            let (location, path, spaces) = (location.clone(), path.clone(), spaces.clone());
            async move {
                if location != crate::files::DISK {
                    return;
                }
                loop {
                    let v = crate::drive_sets::disk_version(&path, &spaces);
                    if *version.peek() != v {
                        version.set(v);
                    }
                    crate::state::sleep_ms(2000).await;
                }
            }
        });
    }
    let v = version();
    let set = use_resource(use_reactive!(|space, location, path, v| {
        let spaces = spaces.clone();
        let _ = v;
        async move { crate::drive_sets::load(&space, &location, &path, &spaces).await }
    }));
    let Some(res) = set() else {
        return rsx! {
            p { class: "pline", "Reading the setlist from its computer…" }
        };
    };
    let set = match res {
        Ok(s) => s,
        Err(e) => {
            return rsx! {
                p { class: "pline", "Could not read this setlist: {e}" }
            };
        }
    };
    let songs: usize = set.acts.iter().map(|a| a.songs.len()).sum();
    let charted: usize = set
        .acts
        .iter()
        .flat_map(|a| a.songs.iter())
        .filter(|s| s.place.chart.is_some())
        .count();
    let list = set.playlist();
    let name = set.name.clone();
    // A session an action asked for before the set was read.
    let asked = app.pending_session.peek().clone();
    if let Some((song, play)) = asked {
        let mut pending = app.pending_session;
        pending.set(None);
        let i = song
            .and_then(|s| {
                let s = s.to_lowercase();
                list.iter()
                    .position(|(x, _)| x.title.to_lowercase().contains(&s))
            })
            .unwrap_or(0);
        let (list, name, here) = (list.clone(), name.clone(), here.clone());
        spawn(async move {
            crate::session::enter(
                app,
                list,
                name,
                i,
                play,
                Some(here),
                crate::session::View::Arrangement,
            );
        });
    }
    // What an action picks a song of (STUDIO_ARRANGEMENT).
    {
        let shown = Some((name.clone(), list.clone()));
        if *app.shown_set.peek() != shown {
            let mut s = app.shown_set;
            s.set(shown);
        }
    }
    let mut n = 0usize;
    rsx! {
        div { class: "fsethead",
            h1 { "{set.name}" }
            p { class: "pline",
                "{songs} songs · {charted} with charts"
                if !set.location_name.is_empty() {
                    " · {set.location_name}"
                }
            }
            div { class: "fsetacts",
                button {
                    class: "playset",
                    onclick: {
                        let (list, name, here) = (list.clone(), name.clone(), here.clone());
                        move |_| crate::session::enter(app, list.clone(), name.clone(), 0, true, Some(here.clone()), crate::session::View::Arrangement)
                    },
                    Icon { name: I::Play, size: 14, w: 2.4 }
                    "Play the set"
                }
                button {
                    class: "playset quiet",
                    onclick: {
                        let (list, name, here) = (list.clone(), name.clone(), here.clone());
                        move |_| crate::session::enter(app, list.clone(), name.clone(), 0, false, Some(here.clone()), crate::session::View::Chart)
                    },
                    Icon { name: I::Chart, size: 14 }
                    "Chart"
                }
                button {
                    class: "playset quiet",
                    onclick: {
                        let (list, name, here) = (list.clone(), name.clone(), here.clone());
                        move |_| crate::session::enter(app, list.clone(), name.clone(), 0, false, Some(here.clone()), crate::session::View::Arrangement)
                    },
                    Icon { name: I::Layers, size: 14 }
                    "Arrangement"
                }
                crate::offline::OfflineButton { name: name.clone(), list: list.clone() }
            }
        }
        for act in set.acts.iter() {
            if let Some(a) = &act.name {
                div { class: "dayhead", "{a}" }
            }
            div { class: "agenda",
                for song in act.songs.iter() {
                    {
                        let i = n;
                        n += 1;
                        let (list, name) = (list.clone(), name.clone());
                        let (list2, name2, here2, here1) = (list.clone(), name.clone(), here.clone(), here.clone());
                        rsx! {
                            div { key: "{song.place.folder}", class: "agline",
                            button {
                                class: "ag",
                                onclick: move |_| crate::session::enter(app, list.clone(), name.clone(), i, true, Some(here1.clone()), crate::session::View::Arrangement),
                                span { class: "cal tpl", "{i + 1}" }
                                span { class: "mid",
                                    span { class: "t1", "{song.title}" }
                                    span { class: "s1",
                                        if let Some(c) = &song.place.chart {
                                            "Chart: {c.name}"
                                        } else {
                                            "No chart yet"
                                        }
                                    }
                                }
                                span { class: "chev",
                                    Icon { name: I::Play, size: 14 }
                                }
                            }
                            button {
                                class: "playset quiet agarr",
                                title: "Its arrangement",
                                onclick: move |_| crate::session::enter(app, list2.clone(), name2.clone(), i, false, Some(here2.clone()), crate::session::View::Arrangement),
                                Icon { name: I::Layers, size: 14 }
                                "Arrangement"
                            }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The chart of the song playing, following it: the bar sounding is lit
/// and kept in view, and a tap on a bar plays from there.
#[component]
pub fn NowChart() -> Element {
    let app = use_app();
    let title = (app.np)()
        .and_then(|n| n.song().map(|s| s.title.to_string()))
        .unwrap_or_else(|| "Chart".into());
    rsx! {
        Page {
            Bar { back: Some(Route::Setlists {}), title: Some(title) }
            div { class: "body",
                ChartView {}
            }
        }
    }
}

/// The chart of the song playing (the session's Chart view, and the
/// launcher's chart page).
#[component]
pub fn ChartView() -> Element {
    let app = use_app();
    let np = (app.np)();
    let Some(n) = np.filter(|n| n.song().is_some()) else {
        return rsx! {
            p { class: "pline", "Play a set, and its chart follows here." }
        };
    };
    let title = n.song().map(|s| s.title.to_string()).unwrap_or_default();
    let (now, _) = n.secs();
    let section = n.timeline.at(now).map(|s| s.name.clone());
    // A chart found beside the song (a `.kf`) is its own; a song with none
    // shows the one Keyflow generates from its session's markers, once the
    // page's engine has opened it.
    let chart = n.place().and_then(|p| {
        p.chart
            .clone()
            .map(Shown::Written)
            .or_else(|| crate::chart_generated::of(p).map(Shown::Generated))
    });
    let sections = n.timeline.sections.clone();
    rsx! {
        div { class: "chartnow",
            div { class: "chartnowhead",
                div {
                    b { "{title}" }
                    span { class: "pline",
                        "{crate::songs::clock(now)}"
                        if let Some(s) = &section {
                            " · {s}"
                        }
                    }
                }
            }
            match chart {
                Some(chart) => rsx! {
                    ChartFollow { chart, sections }
                },
                None => rsx! {
                    p { class: "pline", "No chart for this song yet: Studio looks for a Keyflow chart (.kf) named like its session folder, and makes one from the session's sections once it opens." }
                },
            }
        }
    }
}

/// The chart a song shows: its own, written (a `.kf` beside it), or the one
/// generated from its session's markers when it has none.
#[derive(Clone, Debug, PartialEq)]
enum Shown {
    Written(crate::files::Hit),
    Generated(crate::chart_generated::Generated),
}

impl Shown {
    /// What the page calls it.
    #[cfg_attr(not(feature = "drive"), allow(dead_code))]
    fn name(&self) -> &str {
        match self {
            Self::Written(hit) => &hit.name,
            Self::Generated(_) => "The generated chart",
        }
    }
}

#[cfg(not(feature = "drive"))]
#[component]
fn ChartFollow(chart: Shown, sections: Vec<crate::session_file::Section>) -> Element {
    let _ = (chart, sections);
    rsx! {}
}

/// Marks the bar and the beat sounding at `t` (seconds), scrolls the bar
/// into view when it changes, and sends a tapped bar's time back.
#[cfg(feature = "drive")]
const FOLLOW: &str = r#"
window.__kfSend = (v) => dioxus.send(v);
if (!window.__kfClick) {
  window.__kfClick = true;
  document.addEventListener('click', (e) => {
    const box = document.getElementById('nowchart');
    if (!box || !box.contains(e.target)) return;
    const r = e.target.closest('.kf-bar');
    if (r && window.__kfSend) window.__kfSend(parseFloat(r.getAttribute('data-at')));
  });
}
const mark = (box, sel, t) => {
  const els = box.querySelectorAll(sel);
  let lo = 0, hi = els.length - 1, k = -1;
  while (lo <= hi) {
    const m = (lo + hi) >> 1;
    if (parseFloat(els[m].getAttribute('data-at')) <= t) { k = m; lo = m + 1; } else { hi = m - 1; }
  }
  const was = box.querySelector(sel + '[aria-current]');
  const now = k >= 0 ? els[k] : null;
  if (was === now) return null;
  if (was) was.removeAttribute('aria-current');
  if (now) now.setAttribute('aria-current', 'time');
  return now;
};
while (true) {
  const m = await dioxus.recv();
  const box = document.getElementById('nowchart');
  if (!box) continue;
  const bar = mark(box, '.kf-bar', m.t);
  mark(box, '.kf-step', m.t);
  if (bar) bar.scrollIntoView({ block: 'center', behavior: 'smooth' });
}
"#;

#[cfg(feature = "drive")]
#[component]
fn ChartFollow(chart: Shown, sections: Vec<crate::session_file::Section>) -> Element {
    use crate::files_live::web;

    let app = use_app();
    let name = chart.name().to_owned();
    // Laid out for the window it is on: a phone's narrower, so it is drawn
    // near its own size (engraved again on crossing over, a phone turned).
    let width = crate::chart_sync::layout_width(app_shell::ui::use_shell().viewport().0);
    let engraved = use_resource(use_reactive!(|chart, sections, width| async move {
        let mut span = crate::trace::Span::root("studio.chart.show");
        span.set("studio.chart", chart.name().to_owned());
        // A generated chart's bars are timed to the session it came from.
        let (text, sections) = match chart {
            Shown::Written(hit) => {
                span.set("studio.chart.source", "chart");
                match web::bytes_in(Some(&span), "GET", &web::file_url(&hit), None).await? {
                    (200 | 206, b) => (String::from_utf8_lossy(&b).into_owned(), sections),
                    (code, _) => return Err(format!("its computer answered {code}")),
                }
            }
            Shown::Generated(g) => {
                span.set("studio.chart.source", "generated");
                (g.text, g.sections)
            }
        };
        let mut engraving = span.child("studio.chart.engrave");
        engraving.set("studio.chart.width", width);
        let e = crate::chart_sync::engrave(&text, &sections, width);
        if let Ok(e) = &e {
            engraving.set("studio.chart.timing", format!("{:?}", e.timing));
            engraving.set("studio.chart.bytes", e.svg.len() as u64);
        }
        e
    }));
    let fonts = use_hook(|| crate::chart_sync::font_css().unwrap_or_default());
    let eval = use_hook(|| document::eval(FOLLOW));
    // A tapped bar: play from it.
    {
        let eval = eval.clone();
        use_future(move || {
            let mut eval = eval.clone();
            async move {
                let mut seek = app.seek;
                while let Ok(t) = eval.recv::<f64>().await {
                    seek.set(Some(t));
                }
            }
        });
    }
    // Where the recording is, to the chart.
    {
        let eval = eval.clone();
        use_effect(move || {
            let t = (app.np)().map(|n| n.secs().0).unwrap_or(0.0);
            let _ = eval.send(serde_json::json!({ "t": t }));
        });
    }
    let body = match engraved() {
        None => rsx! {
            p { class: "pline", "Engraving {name}…" }
        },
        Some(Err(e)) => rsx! {
            p { class: "pline", "Could not show {name}: {e}" }
        },
        Some(Ok(e)) => {
            let how = match (&chart, e.timing) {
                (Shown::Generated(_), _) => {
                    "Made from the session's sections and markers: no chords until someone writes them"
                }
                (_, crate::chart_sync::Timing::Sections) => "Bars timed to the session's sections",
                (_, crate::chart_sync::Timing::Tempo) => {
                    "Bars timed by the chart's tempo (its form differs from the session's sections)"
                }
            };
            rsx! {
                p { class: "pline", "{name} · {how}" }
                for note in e.notes.iter() {
                    p { class: "pline chartnote", "{note}" }
                }
                div {
                    id: "nowchart",
                    class: "kfpaper",
                    "data-layout-width": "{width}",
                    dangerous_inner_html: "{e.svg}",
                }
            }
        }
    };
    rsx! {
        style { dangerous_inner_html: "{fonts}" }
        {body}
    }
}
