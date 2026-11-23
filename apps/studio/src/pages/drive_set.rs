//! A setlist file from Drive, as a set: its acts and songs, each song's
//! chart, and the way to play it. And the chart of the song playing,
//! following the recording bar by bar.

use dioxus::prelude::*;

use crate::Route;
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
    let set = use_resource(use_reactive!(|space, location, path| {
        let spaces = spaces.clone();
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
                        let (list, name) = (list.clone(), name.clone());
                        move |_| app.play_places(list.clone(), name.clone(), 0)
                    },
                    Icon { name: I::Play, size: 14, w: 2.4 }
                    "Play the set"
                }
                button {
                    class: "playset quiet",
                    onclick: move |_| {
                        navigator().push(Route::NowChart {});
                    },
                    Icon { name: I::Chart, size: 14 }
                    "Chart"
                }
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
                        rsx! {
                            button {
                                key: "{song.place.folder}",
                                class: "ag",
                                onclick: move |_| {
                                    app.play_places(list.clone(), name.clone(), i);
                                    navigator().push(Route::NowChart {});
                                },
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
    let mut app = use_app();
    let np = (app.np)();
    let Some(n) = np.filter(|n| n.song().is_some()) else {
        return rsx! {
            Page {
                Bar { back: Some(Route::Setlists {}), title: Some("Chart".to_string()) }
                div { class: "body",
                    p { class: "pline", "Play a set, and its chart follows here." }
                }
            }
        };
    };
    let title = n.song().map(|s| s.title.to_string()).unwrap_or_default();
    let (now, _) = n.secs();
    let section = n.timeline.at(now).map(|s| s.name.clone());
    let chart = n.place().and_then(|p| p.chart.clone());
    let sections = n.timeline.sections.clone();
    let playing = n.playing;
    rsx! {
        Page {
            Bar { back: Some(Route::Setlists {}), title: Some(title.clone()) }
            div { class: "body",
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
                        div { class: "fsetacts",
                            button {
                                class: "playset quiet",
                                aria_label: if playing { "Pause" } else { "Play" },
                                onclick: move |_| {
                                    app.np.with_mut(|n| {
                                        if let Some(n) = n {
                                            n.playing = !n.playing;
                                        }
                                    });
                                },
                                Icon { name: if playing { I::Pause } else { I::Play }, size: 14 }
                                if playing { "Pause" } else { "Play" }
                            }
                            button {
                                class: "playset quiet",
                                onclick: move |_| {
                                    app.np.with_mut(|n| {
                                        if let Some(n) = n {
                                            n.next();
                                        }
                                    });
                                },
                                Icon { name: I::Skip, size: 14 }
                                "Next song"
                            }
                        }
                    }
                    match chart {
                        Some(c) => rsx! {
                            ChartFollow { chart: c, sections }
                        },
                        None => rsx! {
                            p { class: "pline", "No chart for this song yet: Studio looks for a Keyflow chart (.kf) named like its session folder." }
                        },
                    }
                }
            }
        }
    }
}

#[cfg(not(feature = "drive"))]
#[component]
fn ChartFollow(chart: crate::files::Hit, sections: Vec<crate::session_file::Section>) -> Element {
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
fn ChartFollow(chart: crate::files::Hit, sections: Vec<crate::session_file::Section>) -> Element {
    use crate::files_live::web;

    let app = use_app();
    let engraved = use_resource(use_reactive!(|chart, sections| async move {
        let mut span = crate::trace::Span::root("studio.chart.show");
        span.set("studio.chart", chart.name.clone());
        let text = match web::bytes_in(Some(&span), "GET", &web::file_url(&chart), None).await? {
            (200 | 206, b) => String::from_utf8_lossy(&b).into_owned(),
            (code, _) => return Err(format!("its computer answered {code}")),
        };
        let mut engraving = span.child("studio.chart.engrave");
        let e = crate::chart_sync::engrave(&text, &sections);
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
            p { class: "pline", "Engraving {chart.name}…" }
        },
        Some(Err(e)) => rsx! {
            p { class: "pline", "Could not show {chart.name}: {e}" }
        },
        Some(Ok(e)) => {
            let how = match e.timing {
                crate::chart_sync::Timing::Sections => "Bars timed to the session's sections",
                crate::chart_sync::Timing::Tempo => {
                    "Bars timed by the chart's tempo (its form differs from the session's sections)"
                }
            };
            rsx! {
                p { class: "pline", "{chart.name} · {how}" }
                for note in e.notes.iter() {
                    p { class: "pline chartnote", "{note}" }
                }
                div { id: "nowchart", class: "kfpaper", dangerous_inner_html: "{e.svg}" }
            }
        }
    };
    rsx! {
        style { dangerous_inner_html: "{fonts}" }
        {body}
    }
}
