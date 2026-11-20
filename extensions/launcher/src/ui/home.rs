//! Home: what needs you now, live and in the studio. Tonight's set (its
//! room, call time, your rig, how ready it is), this week (the next of each
//! event you play and the rehearsals booked), what waits on your review, and
//! a shelf to jump back into.

use dioxus::prelude::*;

use super::data::{self, Day, Studio};
use super::parts::{Card, Cover, Head, Icon, Lines, ROW, Ready, Rule, day_cal};
use super::{INK, INK3, LIVE, Nav, PAGE, Route, hsl, push, stacked, use_studio};

#[component]
pub fn HomeView() -> Element {
    let studio = use_studio();
    let nav: Nav = use_signal(Vec::new);
    let s = studio.read();
    let root = root(&s, nav);
    rsx! { div { style: "{PAGE}", {stacked(&s, nav, root)} } }
}

fn root(s: &Studio, nav: Nav) -> Element {
    let today = s.today;
    let (y, m, d) = today.civil();
    let date = format!(
        "{}, {} {d}",
        data::WEEKDAYS.get(today.weekday()).copied().unwrap_or(""),
        data::MONTHS_LONG
            .get(m.saturating_sub(1))
            .copied()
            .unwrap_or("")
    );
    let _ = y;
    let mine = s.mine();
    let next = mine.first().copied();
    // This week: the next of each event you play after the featured one, and the rehearsals.
    let mut seen: Vec<&str> = next.map(|w| w.series.as_str()).into_iter().collect();
    let mut week: Vec<(Day, Element)> = mine
        .iter()
        .skip(1)
        .filter(|w| w.day.since(today) < 8)
        .filter(|w| {
            let fresh = !seen.contains(&w.series.as_str());
            if fresh {
                seen.push(w.series.as_str());
            }
            fresh
        })
        .filter_map(|w| s.series(&w.series).map(|sr| (w, sr)))
        .map(|(w, sr)| {
            let id = w.id.clone();
            (w.day, rsx! {
                button { style: "{ROW}", onclick: move |_| push(nav, Route::Week(id.clone())),
                    {day_cal(w.day)}
                    div { style: "width: 36px; height: 36px; border-radius: 8px; overflow: hidden; flex-shrink: 0; background: #000;",
                        Cover { art: sr.logo.clone(), hue: sr.hue, radius: 8 }
                    }
                    div { style: "flex: 1; min-width: 0;",
                        div { style: "font-size: 16px; font-weight: 620; color: {INK};", "{sr.name}" }
                        div { style: "font-size: 12px; color: #85858e; display: flex; gap: 4px; align-items: center;",
                            Icon { name: "pin", size: 11, stroke: 2.0 }
                            "{sr.room} · "
                            span { style: "color: rgba(240,194,75,0.6);", "Call {sr.call}" }
                        }
                    }
                    Ready { n: w.places.len(), of: sr.size }
                }
            })
        })
        .collect();
    for r in s
        .rehearsals
        .iter()
        .filter(|r| (0..8).contains(&r.day.since(today)))
    {
        let id = r.week.clone();
        let name = s
            .week(&r.week)
            .and_then(|w| s.series(&w.series))
            .map_or_else(String::new, |x| x.name.clone());
        week.push((r.day, rsx! {
            button { style: "{ROW}", onclick: move |_| push(nav, Route::Week(id.clone())),
                {day_cal(r.day)}
                div { style: "width: 36px; height: 36px; border-radius: 8px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; \
                              background: rgba(45,212,191,0.12); color: #2dd4bf;",
                    Icon { name: "mic", size: 17, stroke: 2.0 }
                }
                Lines { title: format!("Rehearsal · {name}"), sub: format!("{} · {}", r.room, r.time) }
            }
        }));
    }
    week.sort_by_key(|(d, _)| *d);
    let review: Vec<&data::Track> = s
        .tracks
        .iter()
        .filter(|t| t.open() > 0 || t.when.ends_with('h') || t.when == "Yesterday")
        .take(3)
        .collect();
    let loaded = s.rigs.iter().find(|r| r.loaded);
    rsx! {
        super::parts::Title { text: "Home" }
        div { style: "font-size: 13px; font-weight: 600; color: {INK3}; margin: 2px 2px 10px;", "{date}" }
        if let Some(w) = next {
            if let Some(sr) = s.series(&w.series) {
                {tonight(s, w, sr, loaded, nav)}
            }
        }
        if !week.is_empty() {
            Head { text: "This week" }
            Card { for (k, (_, el)) in week.into_iter().enumerate() { if k > 0 { Rule { left: 66 } } {el} } }
        }
        if !review.is_empty() {
            Head { text: "To review" }
            Card {
                for (k, t) in review.iter().enumerate() {
                    if k > 0 { Rule { left: 66 } }
                    {review_row(s, t, nav)}
                }
            }
        }
        Head { text: "Jump back in" }
        div { style: "display: flex; gap: 12px; overflow-x: scroll; margin: 0 -20px; padding: 0 20px 6px;",
            for show in s.shows.iter() {
                {shelf(show.art.clone(), show.hue, &show.title, "Setlist", Route::Show(show.id.clone()), nav)}
            }
            for r in s.records.iter() {
                {shelf(r.art.clone(), r.hue, &r.title, &r.what, Route::Record(r.id.clone()), nav)}
            }
        }
    }
}

fn tonight(
    s: &Studio,
    w: &data::Week,
    sr: &data::Series,
    loaded: Option<&data::Rig>,
    nav: Nav,
) -> Element {
    let id = w.id.clone();
    let when = match w.day.since(s.today) {
        0 => "Tonight".to_owned(),
        1 => "Tomorrow".to_owned(),
        _ => data::WEEKDAYS
            .get(w.day.weekday())
            .copied()
            .unwrap_or("")
            .to_owned(),
    };
    let wash = hsl(sr.hue, 40, 12);
    rsx! {
        button {
            style: "display: flex; flex-direction: column; gap: 12px; width: 100%; padding: 14px; border-radius: 20px; text-align: left; color: inherit; \
                    background: linear-gradient(160deg, {wash}, #111114 70%); border: 1px solid #22222a;",
            onclick: move |_| push(nav, Route::Week(id.clone())),
            div { style: "display: flex; align-items: center; gap: 14px;",
                div { style: "width: 64px; height: 64px; border-radius: 14px; overflow: hidden; flex-shrink: 0; background: #000;",
                    Cover { art: sr.logo.clone(), hue: sr.hue, radius: 14 }
                }
                div { style: "flex: 1; min-width: 0;",
                    div { style: "font-size: 12px; font-weight: 700; color: {hsl(sr.hue, 70, 75)};", "{when} · {sr.starts}" }
                    div { style: "font-size: 26px; font-weight: 760; letter-spacing: -0.02em; color: {INK};", "{sr.name}" }
                    div { style: "font-size: 13px; color: {INK3};", "{sr.folder} · {w.places.len()} songs" }
                }
            }
            div { style: "display: flex; align-items: center; gap: 14px; font-size: 13px; color: #85858e;",
                span { style: "display: flex; align-items: center; gap: 5px;", Icon { name: "pin", size: 13, stroke: 2.0 } "{sr.room}" }
                span { style: "display: flex; align-items: center; gap: 5px; color: rgba(240,194,75,0.7);", Icon { name: "clock", size: 13, stroke: 2.0 } "Call {sr.call}" }
                if let Some(r) = loaded {
                    span { style: "display: flex; align-items: center; gap: 5px; color: {hsl(r.hue, 70, 72)};", Icon { name: r.icon, size: 13, stroke: 2.0 } "{r.name}" }
                }
                span { style: "margin-left: auto;", Ready { n: w.places.len(), of: sr.size } }
            }
        }
    }
}

fn review_row(s: &Studio, t: &data::Track, nav: Nav) -> Element {
    let id = t.id.clone();
    let v = t.latest();
    let sub = v.map_or_else(String::new, |v| {
        format!("{} v{} · {} · {}", v.what, v.n, v.who, v.when)
    });
    let open = t.open();
    rsx! {
        button { key: "{t.id}", style: "{ROW}", onclick: move |_| push(nav, Route::Track(id.clone())),
            Cover { art: s.art_of(t), hue: t.hue, size: 40, radius: 8 }
            Lines { title: t.title.clone(), sub }
            if open > 0 {
                span { style: "display: flex; align-items: center; gap: 4px; height: 24px; padding: 0 8px; border-radius: 12px; font-size: 12px; \
                               font-weight: 700; background: rgba(240,194,75,0.14); color: {LIVE};",
                    Icon { name: "message", size: 13, stroke: 2.0 }
                    "{open}"
                }
            } else {
                span { style: "font-size: 11px; font-weight: 700; color: #2dd4bf; background: rgba(45,212,191,0.13); padding: 3px 8px; border-radius: 9px;", "New" }
            }
        }
    }
}

fn shelf(
    art: Option<String>,
    hue: u16,
    title: &str,
    kind: &str,
    route: Route,
    nav: Nav,
) -> Element {
    rsx! {
        button {
            style: "flex-shrink: 0; width: 124px; display: flex; flex-direction: column; gap: 2px; text-align: left; background: none; border: none; color: inherit; padding: 0;",
            onclick: move |_| push(nav, route.clone()),
            div { style: "width: 124px; height: 124px; border-radius: 12px; overflow: hidden; margin-bottom: 6px; background: #111;",
                Cover { art, hue, radius: 12 }
            }
            span { style: "font-size: 14px; font-weight: 620; color: {INK}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{title}" }
            span { style: "font-size: 12px; color: {INK3};", "{kind}" }
        }
    }
}
