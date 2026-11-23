//! Records: every album and single as its cover; an album's tracks, each
//! with its latest version and status; a track's review (Samply's way):
//! its versions, its waveform with a pin for each comment, and the
//! timestamped comments to resolve or answer at the playhead.

use dioxus::prelude::*;

use super::data::{self, Studio, Track};
use super::parts::{Back, Card, Cover, Hero, Icon, ROW, Rule, Stage};
use super::{
    INK, INK2, INK3, LIVE, Nav, OK, PAGE, Route, SURFACE2, hsl, push, stacked, use_studio,
};

#[component]
pub fn RecordsView() -> Element {
    let studio = use_studio();
    let nav: Nav = use_signal(Vec::new);
    let s = studio.read();
    let root = root(&s, nav);
    rsx! { div { class: "studio-launcher", style: "{PAGE}", document::Style { {super::BOX} } {stacked(&s, nav, root)} } }
}

fn root(s: &Studio, nav: Nav) -> Element {
    // Albums and singles alike, the last worked on first.
    let mut cells: Vec<Element> = Vec::new();
    for r in &s.records {
        let tracks = s.tracks_of(&r.id);
        let open: usize = tracks.iter().map(|t| t.open()).sum();
        let id = r.id.clone();
        cells.push(cell(
            r.art.clone(),
            r.hue,
            &r.title,
            &r.artist,
            r.status,
            &format!("{} tracks", tracks.len()),
            open,
            Route::Record(id),
            nav,
        ));
    }
    for t in s.tracks.iter().filter(|t| t.record.is_none()) {
        let kind = if t.cover { "Cover single" } else { "Single" };
        cells.insert(
            1.min(cells.len()),
            cell(
                t.art.clone(),
                t.hue,
                &t.title,
                &t.artist,
                t.stage,
                kind,
                t.open(),
                Route::Track(t.id.clone()),
                nav,
            ),
        );
    }
    rsx! {
        super::parts::TopBar { title: "Records",
            super::parts::Joined {
                super::parts::JTool { icon: "search", label: "Search records" }
                super::parts::Hair {}
                super::parts::JTool { icon: "plus", label: "New record" }
            }
        }
        div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 22px 16px; margin-top: 6px;",
            for (k, c) in cells.into_iter().enumerate() { div { key: "{k}", style: "min-width: 0;", {c} } }
        }
    }
}

#[allow(clippy::too_many_arguments)] // A cell's facts, each shown.
fn cell(
    art: Option<String>,
    hue: u16,
    title: &str,
    artist: &str,
    stage: usize,
    kind: &str,
    open: usize,
    route: Route,
    nav: Nav,
) -> Element {
    rsx! {
        button {
            style: "display: flex; flex-direction: column; width: 100%; text-align: left; background: none; border: none; color: inherit; padding: 0;",
            onclick: move |_| push(nav, route.clone()),
            div { style: "position: relative; width: 100%; padding-top: 100%; border-radius: 12px; overflow: hidden; background: #111; border: 1px solid #222228;",
                div { style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0;", Cover { art, hue, radius: 0 } }
                if open > 0 {
                    span { style: "position: absolute; right: 8px; bottom: 8px; display: flex; align-items: center; gap: 3px; height: 22px; padding: 0 8px; \
                                   border-radius: 11px; font-size: 11.5px; font-weight: 700; background: rgba(0,0,0,0.75); color: {LIVE};",
                        Icon { name: "message", size: 11, stroke: 2.4 }
                        "{open}"
                    }
                }
            }
            span { style: "font-size: 15px; font-weight: 600; color: {INK}; margin-top: 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{title}" }
            span { style: "font-size: 13.5px; color: {INK3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{artist}" }
            div { style: "display: flex; align-items: center; gap: 8px; margin-top: 4px;",
                Stage { stage, small: true }
                span { style: "font-size: 11.5px; color: {INK3};", "{kind}" }
            }
        }
    }
}

// ── An album ──────────────────────────────────────────────────────────────

pub fn record_page(s: &Studio, id: &str, nav: Nav, back: EventHandler<()>) -> Element {
    let Some(r) = s.record(id) else {
        return rsx! { Back { title: "Not found", onback: back } };
    };
    let tracks = s.tracks_of(&r.id);
    let secs: u32 = tracks
        .iter()
        .filter_map(|t| t.latest())
        .map(|v| v.secs)
        .sum();
    let open: usize = tracks.iter().map(|t| t.open()).sum();
    let chip = rsx! { Stage { stage: r.status } };
    let count = format!("{} tracks · {}", tracks.len(), data::clock(secs));
    let hot = if open > 0 {
        format!("{open} open")
    } else {
        String::new()
    };
    rsx! {
        super::parts::Tint { hue: r.hue }
        Back { title: "", onback: back, tools: super::parts::page_tools("download", "Download") }
        Hero { art: r.art.clone(), hue: r.hue, title: r.title.clone(), over: r.what.clone(), sub: r.artist.clone(), chip, count, hot }
        div { style: "height: 12px;" }
        Card {
            for (k, t) in tracks.iter().enumerate() {
                if k > 0 { Rule { left: 48 } }
                {track_row(t, nav)}
            }
        }
    }
}

fn track_row(t: &Track, nav: Nav) -> Element {
    let id = t.id.clone();
    let v = t.latest();
    let open = t.open();
    let by = v.map_or_else(String::new, |v| format!("{} · {}", v.who, v.when));
    rsx! {
        button { key: "{t.id}", style: "{ROW} height: 70px; box-sizing: border-box;", onclick: move |_| push(nav, Route::Track(id.clone())),
            span { style: "width: 22px; text-align: center; font-size: 14px; font-weight: 600; color: #80808a; flex-shrink: 0;", "{t.no}" }
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px;",
                span { style: "font-size: 15.5px; font-weight: 620; color: {INK}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{t.title}" }
                span { style: "font-size: 13px; color: #85858e;", "{by}" }
                if open > 0 {
                    span { style: "display: flex; align-items: center; gap: 4px; font-size: 11px; color: rgba(240,194,75,0.85);",
                        Icon { name: "message", size: 11, stroke: 2.0 }
                        "{open} open"
                    }
                }
            }
            div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 6px;",
                if let Some(v) = v {
                    span { style: "height: 24px; padding: 0 9px; border-radius: 7px; display: flex; align-items: center; font-size: 12.5px; font-weight: 700; background: #1d1d23; color: {INK};",
                        "{v.what} v{v.n}"
                    }
                }
                Stage { stage: t.stage, small: true }
            }
        }
    }
}

// ── A track's review ──────────────────────────────────────────────────────

pub fn track_page(s: &Studio, id: &str, back: EventHandler<()>) -> Element {
    let Some(t) = s.track(id) else {
        return rsx! { Back { title: "Not found", onback: back } };
    };
    let rec = t.record.as_deref().and_then(|r| s.record(r));
    let kicker = rec.map_or_else(
        || {
            if t.cover {
                "Cover single".to_owned()
            } else {
                "Single".to_owned()
            }
        },
        |r| format!("{} · Track {}", r.title, t.no),
    );
    rsx! {
        Back { title: "", onback: back, tools: super::parts::page_tools("folder", "Open its session") }
        div { style: "display: flex; align-items: center; gap: 12px; margin: 2px 0 14px;",
            Cover { art: s.art_of(t), hue: t.hue, size: 56, radius: 10 }
            div { style: "min-width: 0;",
                div { style: "font-size: 11.5px; font-weight: 700; color: {INK3};", "{kicker}" }
                div { style: "font-size: 24px; font-weight: 760; letter-spacing: -0.02em; color: {INK};", "{t.title}" }
                div { style: "font-size: 13px; color: {INK3};", "{t.artist}" }
            }
        }
        Review { track: t.clone() }
    }
}

/// A track's versions and comments, the playhead and the comment box.
#[component]
fn Review(track: Track) -> Element {
    let mut chosen = use_signal(|| track.versions.len().saturating_sub(1));
    let mut pos = use_signal(|| 0.18f32);
    let mut playing = use_signal(|| false);
    let mut draft = use_signal(String::new);
    let mut added = use_signal(Vec::<data::Comment>::new);
    let mut resolved = use_signal(Vec::<String>::new);
    let Some(v) = track.versions.get(chosen()).cloned() else {
        return rsx! { div { style: "color: {INK3};", "No version yet: bounce one from the session." } };
    };
    let mut comments: Vec<data::Comment> = v.comments.iter().cloned().chain(added()).collect();
    for c in &mut comments {
        if resolved().contains(&c.text) {
            c.open = false;
        }
    }
    comments.sort_by(|a, b| a.at.total_cmp(&b.at));
    let open = comments.iter().filter(|c| c.open).count();
    let at = |p: f32| data::clock(secs_at(v.secs, p));
    // The waveform: the same for the same title.
    let bars: Vec<f32> = (0..64u16)
        .map(|k| {
            let x = f32::from(k) / 6.0 + f32::from(u16::try_from(track.title.len()).unwrap_or(0));
            0.2 + 0.8 * (x.sin().abs() * (0.55 + 0.45 * ((x * 7.3).cos().abs())))
        })
        .collect();
    let played = pos();
    let here = at(played);
    let total = data::clock(v.secs);
    let initial = v.who.chars().next().unwrap_or(' ');
    let head_style = format!(
        "position: absolute; top: 18px; bottom: 0; width: 2px; left: {}%; background: {LIVE};",
        played * 100.0
    );
    rsx! {
        div { style: "display: flex; gap: 6px; overflow-x: scroll; margin-bottom: 10px;",
            for (k, ver) in track.versions.iter().enumerate() {
                button {
                    key: "{k}",
                    style: if k == chosen() { format!("flex-shrink: 0; height: 32px; padding: 0 12px; border-radius: 16px; border: none; font-size: 13px; font-weight: 650; background: {INK}; color: #000;") }
                           else { format!("flex-shrink: 0; height: 32px; padding: 0 12px; border-radius: 16px; border: none; font-size: 13px; font-weight: 650; background: {SURFACE2}; color: {INK2};") },
                    onclick: move |_| chosen.set(k),
                    "{ver.what} v{ver.n}"
                }
            }
        }
        div { style: "display: flex; align-items: center; gap: 8px; font-size: 13px; color: #909099; margin-bottom: 14px;",
            span { style: "width: 22px; height: 22px; border-radius: 11px; display: flex; align-items: center; justify-content: center; font-size: 10px; \
                           font-weight: 700; background: {hsl(30, 45, 30)}; color: {INK};", "{initial}" }
            "{v.who} · {v.when}"
            span { style: "margin-left: auto;", Stage { stage: track.stage } }
        }
        // The waveform; a tap moves the playhead there.
        div {
            style: "position: relative; height: 92px; display: flex; align-items: center; gap: 2px; padding-top: 22px; box-sizing: border-box;",
            onclick: move |e: MouseEvent| {
                let x = e.element_coordinates().x;
                // The box is the page's width less its padding: near enough for a playhead.
                let w = 350.0f64;
                pos.set(((x / w).clamp(0.0, 1.0) as f32).min(1.0));
            },
            for (k, h) in bars.iter().enumerate() {
                div { key: "{k}", style: if (f32::from(u16::try_from(k).unwrap_or(0)) / 64.0) < played { format!("flex: 1; height: {}%; border-radius: 1.5px; background: {INK};", h * 100.0) }
                                      else { format!("flex: 1; height: {}%; border-radius: 1.5px; background: #3a3a42;", h * 100.0) } }
            }
            div { style: "{head_style}" }
            for (k, c) in comments.iter().enumerate() {
                button {
                    key: "p{k}",
                    style: format!("position: absolute; top: 0; left: {}%; width: 22px; height: 22px; margin-left: -11px; border-radius: 11px; border: none; \
                                    font-size: 10px; font-weight: 800; color: #fff; background: {}; opacity: {};",
                                   c.at * 100.0, hsl(name_h(&c.who), 50, 42), if c.open { 1.0 } else { 0.35 }),
                    onclick: { let a = c.at; move |e: MouseEvent| { e.stop_propagation(); pos.set(a); } },
                    {c.who.chars().next().map(String::from).unwrap_or_default()}
                }
            }
        }
        div { style: "display: flex; align-items: center; justify-content: space-between; margin: 8px 0 4px;",
            span { style: "width: 48px; font-size: 12px; color: #909099;", "{here}" }
            button {
                style: "width: 52px; height: 52px; border-radius: 26px; border: none; background: {INK}; color: #000; display: flex; align-items: center; justify-content: center;",
                onclick: move |_| playing.toggle(),
                Icon { name: if playing() { "pause" } else { "play" }, size: 22, stroke: 2.0, fill: true }
            }
            span { style: "width: 48px; text-align: right; font-size: 12px; color: #909099;", "{total}" }
        }
        div { style: "font-size: 13px; font-weight: 650; color: {INK3}; margin: 18px 2px 6px;",
            if comments.is_empty() { "No comments on this version" } else { "{comments.len()} comments · {open} open" }
        }
        for (k, c) in comments.iter().enumerate() {
            div { key: "c{k}", style: if k > 0 { "display: flex; align-items: flex-start; gap: 10px; padding: 9px 0; border-top: 1px solid #1f1f24;" } else { "display: flex; align-items: flex-start; gap: 10px; padding: 9px 0;" },
                button {
                    style: "flex-shrink: 0; height: 24px; padding: 0 8px; border-radius: 7px; border: none; font-size: 12px; font-weight: 700; \
                            background: rgba(240,194,75,0.12); color: {LIVE};",
                    onclick: { let a = c.at; move |_| pos.set(a) },
                    {at(c.at)}
                }
                div { style: "flex: 1; min-width: 0;",
                    div { style: "font-size: 13px; font-weight: 650; color: {INK};", "{c.who}" }
                    div { style: if c.open { format!("font-size: 15px; color: {INK};") } else { format!("font-size: 15px; color: {INK3};") }, "{c.text}" }
                }
                if c.open {
                    button {
                        style: "width: 30px; height: 30px; border-radius: 15px; border: none; background: none; box-shadow: inset 0 0 0 1px #34343b; color: {INK3}; \
                                display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
                        onclick: { let text = c.text.clone(); move |_| resolved.write().push(text.clone()) },
                        Icon { name: "check", size: 14, stroke: 2.4 }
                    }
                } else {
                    span { style: "width: 30px; display: flex; justify-content: center; color: {OK}; opacity: 0.6;", Icon { name: "check", size: 13, stroke: 2.4 } }
                }
            }
        }
        div { style: "display: flex; align-items: center; gap: 8px; margin-top: 10px; padding: 6px 6px 6px 12px; border-radius: 22px; background: {SURFACE2};",
            span { style: "font-size: 12px; font-weight: 700; color: {LIVE};", "{here}" }
            input {
                style: "flex: 1; min-width: 0; background: none; border: none; color: {INK}; font-size: 15px; outline: none;",
                placeholder: "Comment at {here}…",
                value: "{draft}",
                oninput: move |e| draft.set(e.value()),
            }
            button {
                style: "width: 32px; height: 32px; border-radius: 16px; border: none; background: #2dd4bf; color: #042f2a; display: flex; align-items: center; justify-content: center;",
                onclick: move |_| {
                    let text = draft().trim().to_owned();
                    if !text.is_empty() {
                        added.write().push(data::Comment { at: pos(), who: "Cody".into(), text, open: true });
                        draft.set(String::new());
                    }
                },
                Icon { name: "arrow-up", size: 16, stroke: 2.4 }
            }
        }
    }
}

fn secs_at(total: u32, p: f32) -> u32 {
    let s = f32::from(u16::try_from(total).unwrap_or(u16::MAX)) * p.clamp(0.0, 1.0);
    format!("{s:.0}").parse().unwrap_or(0)
}

fn name_h(name: &str) -> u16 {
    match name {
        "Cody" => 30,
        "Jake" => 200,
        "Mia" => 320,
        "Sam" => 140,
        n => data::name_hue(n),
    }
}
