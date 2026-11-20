//! Setlists: the recurring events as their logos, ringed by how ready the
//! next set is, and the shows; a recurring event's page, a week's set, a
//! show, and every setlist there is.

use dioxus::prelude::*;

use super::data::{self, Day, Place, Studio, Week};
use super::parts::{
    Back, Cal, Card, Cover, Head, Hero, Icon, Key, Lines, ROW, Ready, Rule, Where, day_cal,
};
use super::{INK, INK3, LIVE, Nav, PAGE, Route, push, stacked, use_studio};

#[component]
pub fn SetlistsView() -> Element {
    let studio = use_studio();
    let nav: Nav = use_signal(Vec::new);
    let s = studio.read();
    let root = root(&s, nav);
    rsx! { div { style: "{PAGE}", {stacked(&s, nav, root)} } }
}

fn root(s: &Studio, nav: Nav) -> Element {
    // The recurring events by folder, each folder's in the order they next meet.
    let mut folders: Vec<String> = s.series.iter().map(|x| x.folder.clone()).collect();
    folders.dedup();
    let upcoming: Vec<&data::Show> = s
        .shows
        .iter()
        .filter(|x| x.day.is_none_or(|d| d >= s.today) && x.month.is_none())
        .collect();
    let past: Vec<&data::Show> = s
        .shows
        .iter()
        .filter(|x| !upcoming.iter().any(|u| u.id == x.id))
        .collect();
    rsx! {
        super::parts::TopBar { title: "Setlists",
            super::parts::Joined {
                super::parts::JTool { icon: "library", label: "Song library" }
                super::parts::Hair {}
                super::parts::JTool { icon: "more", label: "Filter" }
            }
            super::parts::Tool { icon: "plus", label: "New setlist" }
        }
        for (k, folder) in folders.iter().enumerate() {
            Head {
                key: "{folder}",
                text: format!("{folder} · Recurring"),
                link: (k == 0).then(|| "Every setlist".to_owned()),
                onlink: move |()| push(nav, Route::Every),
            }
            div { style: "display: grid; grid-template-columns: repeat(5, 1fr); gap: 14px 4px;",
                for sr in sorted_series(s, folder) {
                    {event(s, sr, nav)}
                }
            }
        }
        if !upcoming.is_empty() {
            Head { text: "Upcoming shows" }
            for show in upcoming { {show_card(show, nav, false)} }
        }
        if !past.is_empty() {
            Head { text: "Past shows" }
            for show in past { {show_card(show, nav, true)} }
        }
    }
}

fn sorted_series<'a>(s: &'a Studio, folder: &str) -> Vec<&'a data::Series> {
    let mut v: Vec<&data::Series> = s.series.iter().filter(|x| x.folder == folder).collect();
    v.sort_by_key(|x| s.next_of(&x.id).map_or(Day(i64::MAX), |w| w.day));
    v
}

fn event(s: &Studio, sr: &data::Series, nav: Nav) -> Element {
    let next = s.next_of(&sr.id);
    let (n, of) = next.map_or((0, sr.size), |w| (w.places.len(), sr.size));
    let when = next
        .map(|w| match w.day.since(s.today) {
            0 => "Today".to_owned(),
            1..=6 => w.day.wd3().to_owned(),
            _ => w.day.short(),
        })
        .unwrap_or_default();
    let col = if n >= of {
        "#22c55e"
    } else if n > 0 {
        LIVE
    } else {
        INK3
    };
    let id = sr.id.clone();
    rsx! {
        button {
            key: "{sr.id}",
            style: "display: flex; flex-direction: column; align-items: center; gap: 2px; background: none; border: none; color: inherit; min-width: 0; padding: 0;",
            onclick: move |_| push(nav, Route::Series(id.clone())),
            div { style: "position: relative; width: 66px; height: 66px; display: flex; align-items: center; justify-content: center; margin-bottom: 4px;",
                super::parts::LogoRing { n, of, size: 66 }
                div { style: "width: 52px; height: 52px; border-radius: 26px; overflow: hidden; background: #000;",
                    Cover { art: sr.logo.clone(), hue: sr.hue, radius: 26 }
                }
            }
            span { style: "font-size: 12px; font-weight: 650; color: {INK}; max-width: 100%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{sr.name}" }
            span { style: "font-size: 11px; color: {INK3};",
                "{when} · "
                b { style: "color: {col}; font-weight: 650;", "{n}/{of}" }
            }
        }
    }
}

fn show_card(show: &data::Show, nav: Nav, past: bool) -> Element {
    let id = show.id.clone();
    let (top, big) = show.day.map_or_else(
        || show.month.clone().unwrap_or_default(),
        |d| (d.month().to_owned(), d.day_of_month().to_string()),
    );
    let op = if past { "0.7" } else { "1" };
    let sub = format!(
        "{}{} songs{}",
        if show.folder.is_empty() {
            String::new()
        } else {
            format!("{} · ", show.folder)
        },
        show.songs(),
        if show.secs() > 0 {
            format!(" · {}", data::clock(show.secs()))
        } else {
            String::new()
        }
    );
    rsx! {
        button {
            key: "{show.id}",
            style: "display: flex; align-items: center; gap: 14px; width: 100%; padding: 10px; margin-bottom: 10px; border-radius: 18px; \
                    background: #111114; border: 1px solid #1c1c21; color: inherit; text-align: left; opacity: {op};",
            onclick: move |_| push(nav, Route::Show(id.clone())),
            Cover { art: show.art.clone(), hue: show.hue, size: 72, radius: 12 }
            div { style: "flex: 1; min-width: 0;",
                div { style: "font-size: 17px; font-weight: 700; color: {INK};", "{show.title}" }
                div { style: "font-size: 12.5px; color: {INK3};", "{sub}" }
            }
            Cal { top, big }
        }
    }
}

// ── A recurring event ─────────────────────────────────────────────────────

pub fn series_page(s: &Studio, id: &str, nav: Nav, back: EventHandler<()>) -> Element {
    let Some(sr) = s.series(id) else {
        return rsx! { Back { title: "Not found", onback: back } };
    };
    let mut weeks: Vec<&Week> = s.weeks.iter().filter(|w| w.series == sr.id).collect();
    weeks.sort_by_key(|w| w.day);
    let up: Vec<&Week> = weeks.iter().copied().filter(|w| w.day >= s.today).collect();
    let past: Vec<&Week> = weeks
        .iter()
        .rev()
        .copied()
        .filter(|w| w.day < s.today)
        .collect();
    let days = sr
        .days
        .iter()
        .map(|d| data::WEEKDAYS.get(*d).copied().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" and ");
    let facts =
        rsx! { Where { room: sr.room.clone(), call: sr.call.clone(), starts: sr.starts.clone() } };
    rsx! {
        super::parts::Tint { hue: sr.hue }
        Back { title: "", onback: back, tools: super::parts::page_tools("calendar", "Book a rehearsal") }
        Hero { art: sr.logo.clone(), hue: sr.hue, title: sr.name.clone(), sub: format!("{} · Every {days}", sr.folder),
            count: format!("{} songs a set · {} sets so far", sr.size, past.len()), facts }
        Head { text: "Coming up" }
        Card { for (k, w) in up.iter().enumerate() { if k > 0 { Rule { left: 66 } } {week_row(w, sr.size, true, nav)} } }
        Head { text: "Before" }
        Card { for (k, w) in past.iter().enumerate() { if k > 0 { Rule { left: 66 } } {week_row(w, sr.size, false, nav)} } }
    }
}

fn week_row(w: &Week, size: usize, ahead: bool, nav: Nav) -> Element {
    let id = w.id.clone();
    let sub = if w.places.is_empty() {
        "No songs yet".to_owned()
    } else {
        w.places
            .iter()
            .take(3)
            .map(|p| p.song.title.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    rsx! {
        button { key: "{w.id}", style: "{ROW}", onclick: move |_| push(nav, Route::Week(id.clone())),
            {day_cal(w.day)}
            Lines { title: w.day.short(), sub }
            if ahead { Ready { n: w.places.len(), of: size } } else { span { style: "color: {INK3};", Icon { name: "chev", size: 16 } } }
        }
    }
}

// ── A week's set ──────────────────────────────────────────────────────────

pub fn week_page(s: &Studio, id: &str, back: EventHandler<()>) -> Element {
    let Some(w) = s.week(id) else {
        return rsx! { Back { title: "Not found", onback: back } };
    };
    let Some(sr) = s.series(&w.series) else {
        return rsx! {};
    };
    let facts = rsx! {
        Cal { top: w.day.month().to_owned(), big: w.day.day_of_month().to_string() }
        Where { room: sr.room.clone(), call: sr.call.clone(), starts: sr.starts.clone() }
    };
    let missing = sr.size.saturating_sub(w.places.len());
    let sub = if missing > 0 {
        format!("{} · {} of {} songs", sr.folder, w.places.len(), sr.size)
    } else {
        sr.folder.clone()
    };
    let count = format!("{} songs · {}", w.places.len(), data::clock(w.secs()));
    // Its two sections: the Top Set's two songs, the Bottom Set's rest.
    let split = 2usize.min(w.places.len().max(sr.size));
    let top: Vec<Option<&Place>> = (0..split).map(|i| w.places.get(i)).collect();
    let bottom: Vec<Option<&Place>> = (split..sr.size.max(w.places.len()))
        .map(|i| w.places.get(i))
        .collect();
    let (top_n, bottom_n) = (
        top.iter().flatten().count(),
        bottom.iter().flatten().count(),
    );
    rsx! {
        super::parts::Tint { hue: sr.hue }
        Back { title: "", onback: back, tools: super::parts::page_tools("download", "Download") }
        Hero { art: sr.logo.clone(), hue: sr.hue, title: sr.name.clone(), sub, count, facts }
        Head { text: "Top set", note: songs(top_n) }
        Card { for (k, p) in top.iter().enumerate() { if k > 0 { Rule {} } {place_row(*p)} } }
        Head { text: "Bottom set", note: songs(bottom_n) }
        Card { for (k, p) in bottom.iter().enumerate() { if k > 0 { Rule {} } {place_row(*p)} } }
    }
}

/// A song in a set: its art, title and artist, who leads it; its key, and
/// its arrangement under it when it is not the song's own.
fn place_row(p: Option<&Place>) -> Element {
    let Some(p) = p else {
        return rsx! {
            div { style: "{ROW} height: 70px; box-sizing: border-box;",
                div { style: "width: 48px; height: 48px; border-radius: 9px; box-shadow: inset 0 0 0 1px #34343b; color: {INK3}; \
                              display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
                    Icon { name: "plus", size: 18, stroke: 2.0 }
                }
                Lines { title: "Add a song", sub: "From the Library" }
            }
        };
    };
    let leads: Vec<(&str, String, bool)> = [("mic", p.vocal.clone()), ("guitar", p.guitar.clone())]
        .into_iter()
        .filter_map(|(icon, who)| who.map(|w| (icon, w.clone(), w == "Cody")))
        .collect();
    rsx! {
        div { style: "{ROW} height: 70px; box-sizing: border-box;",
            Cover { art: p.song.art.clone(), hue: data::name_hue(&p.song.artist), size: 48, radius: 9 }
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px;",
                span { style: "font-size: 15.5px; font-weight: 620; color: {INK}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.song.title}" }
                span { style: "font-size: 13px; color: #85858e; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{p.song.artist}" }
                div { style: "display: flex; gap: 10px; height: 15px; align-items: center; margin-top: 2px;",
                    for (icon, who, me) in leads {
                        span { key: "{icon}", style: if me { "display: flex; align-items: center; gap: 3px; font-size: 11px; font-weight: 600; color: hsl(30, 70%, 68%);" }
                                    else { "display: flex; align-items: center; gap: 3px; font-size: 11px; color: #85858e;" },
                            Icon { name: icon, size: 10, stroke: 2.0 }
                            if me { "You" } else { "{who}" }
                        }
                    }
                }
            }
            div { style: "display: flex; flex-direction: column; align-items: flex-end; justify-content: space-between; align-self: stretch; padding: 3px 0;",
                Key { value: p.key().map(str::to_owned), from: p.song.key.clone() }
                span { style: if p.arrangement.is_some() { "display: flex; align-items: center; gap: 4px; font-size: 10.5px; font-weight: 600; color: hsl(45, 80%, 65%);" }
                              else { "display: flex; align-items: center; font-size: 10.5px; color: #6a6a72;" },
                    Icon { name: "branch", size: 11, stroke: 2.2 }
                    "{p.arrangement.clone().unwrap_or_default()}"
                }
            }
        }
    }
}

// ── A show ────────────────────────────────────────────────────────────────

pub fn show_page(s: &Studio, id: &str, back: EventHandler<()>) -> Element {
    let Some(show) = s.show(id) else {
        return rsx! { Back { title: "Not found", onback: back } };
    };
    let (top, big) = show.day.map_or_else(
        || show.month.clone().unwrap_or_default(),
        |d| (d.month().to_owned(), d.day_of_month().to_string()),
    );
    let facts = rsx! { Cal { top, big } };
    let count = format!(
        "{} songs{}",
        show.songs(),
        if show.secs() > 0 {
            format!(" · {}", data::clock(show.secs()))
        } else {
            String::new()
        }
    );
    let note = show.note.clone();
    let title = show.title.clone();
    rsx! {
        super::parts::Tint { hue: show.hue }
        Back { title: "", onback: back, tools: super::parts::page_tools("download", "Download") }
        Hero { art: show.art.clone(), hue: show.hue, title: show.title.clone(), sub: show.folder.clone(), count, facts }
        // The setlist itself: its note, with its Setlist, Chart, Lyrics
        // and More (arrangement, mixer, lighting) tabs.
        if let Some(note) = note {
            button {
                style: "margin: 12px 0 4px; height: 44px; border-radius: 22px; border: none; background: {INK}; color: #000; font-weight: 650; font-size: 15px;",
                // Its page, and its songs in the engine (the arrangement's
                // and the mixer's).
                onclick: move |_| {
                    super::open_setlist(&note, &title);
                    super::open_note(&note);
                },
                "Open setlist"
            }
        }
        if show.sections.is_empty() {
            div { style: "color: {INK3}; font-size: 14px; padding: 24px 2px;", "No songs yet." }
        }
        for (k, (name, places)) in show.sections.iter().enumerate() {
            Head { key: "{k}", text: name.clone().unwrap_or_else(|| "Songs".into()), note: songs(places.len()) }
            Card { for (i, p) in places.iter().enumerate() { if i > 0 { Rule {} } {place_row(Some(p))} } }
        }
    }
}

// ── Every setlist ─────────────────────────────────────────────────────────

pub fn every_page(s: &Studio, nav: Nav, back: EventHandler<()>) -> Element {
    // Every set that exists: the weeks with songs, this week's, and the shows; newest first.
    let mut rows: Vec<(Day, Element)> = s
        .weeks
        .iter()
        .filter(|w| !w.places.is_empty() || (0..7).contains(&w.day.since(s.today)))
        .filter_map(|w| s.series(&w.series).map(|sr| (w, sr)))
        .map(|(w, sr)| {
            let id = w.id.clone();
            let sub = if w.places.is_empty() { "Not planned yet".to_owned() } else { format!("{} songs · {}", w.places.len(), w.places.iter().take(2).map(|p| p.song.title.as_str()).collect::<Vec<_>>().join(", ")) };
            let ahead = w.day >= s.today;
            (w.day, rsx! {
                button { style: "{ROW}", onclick: move |_| push(nav, Route::Week(id.clone())),
                    {day_cal(w.day)}
                    div { style: "width: 36px; height: 36px; border-radius: 8px; overflow: hidden; flex-shrink: 0; background: #000;",
                        Cover { art: sr.logo.clone(), hue: sr.hue, radius: 8 }
                    }
                    Lines { title: sr.name.clone(), sub }
                    if ahead { Ready { n: w.places.len(), of: sr.size } }
                }
            })
        })
        .collect();
    for show in &s.shows {
        if let Some(day) = show.day {
            let id = show.id.clone();
            rows.push((
                day,
                rsx! {
                    button { style: "{ROW}", onclick: move |_| push(nav, Route::Show(id.clone())),
                        {day_cal(day)}
                        Cover { art: show.art.clone(), hue: show.hue, size: 36, radius: 8 }
                        Lines { title: show.title.clone(), sub: format!("{} songs", show.songs()) }
                    }
                },
            ));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    let mut months: Vec<(i64, usize)> = Vec::new();
    for (d, _) in &rows {
        let (y, m, _) = d.civil();
        if months.last() != Some(&(y, m)) {
            months.push((y, m));
        }
    }
    rsx! {
        Back { title: "Every setlist", onback: back }
        for (y, m) in months {
            Head { key: "{y}-{m}", text: format!("{} {y}", data::MONTHS_LONG.get(m.saturating_sub(1)).copied().unwrap_or("")) }
            Card {
                for (k, (_, el)) in rows.iter().filter(|(d, _)| { let (yy, mm, _) = d.civil(); yy == y && mm == m }).enumerate() {
                    if k > 0 { Rule { left: 66 } }
                    {el.clone()}
                }
            }
        }
    }
}

/// `1 song`, `3 songs`.
fn songs(n: usize) -> String {
    if n == 1 {
        "1 song".to_owned()
    } else {
        format!("{n} songs")
    }
}
