//! Home: what is on next and how ready it is, what is coming up, what
//! changed and who changed it, and the conversations still open.

use dioxus::prelude::*;
use lucide_dioxus::{FolderOpen, ListMusic, MessageSquare, Music, Pause, Play};

use super::kit::*;
use super::{Open, Route, Viewport, open, use_library, use_nav, use_wide};
use crate::model::{Anchor, Library, Listen, Piece, Ready, Setlist, minutes};

/// What a list row needs to say about a doc and where it goes.
pub(crate) struct Labelled {
    pub title: String,
    pub detail: String,
    pub route: Route,
}

/// What a thread is about, said the way a person would.
pub(crate) fn anchor_label(lib: &Library, anchor: &Anchor) -> Option<Labelled> {
    Some(match *anchor {
        Anchor::Song(id) => {
            let s = lib.song(id)?;
            Labelled {
                title: s.title.clone(),
                detail: "Song".into(),
                route: Route::Song(id, None),
            }
        }
        Anchor::Arrangement(id, a) => {
            let s = lib.song(id)?;
            let arr = s.arrangement(a)?;
            Labelled {
                title: s.title.clone(),
                detail: format!("{} arrangement", arr.name),
                route: Route::Song(id, Some(a)),
            }
        }
        Anchor::Setlist(id) => {
            let s = lib.setlist(id)?;
            Labelled {
                title: s.title.clone(),
                detail: "Setlist".into(),
                route: Route::Setlist(id, None),
            }
        }
        Anchor::Entry(id, e) => {
            let s = lib.setlist(id)?;
            let entry = s.entries.iter().find(|x| x.id == e)?;
            let what = match &entry.piece {
                Piece::Song { song, .. } => lib
                    .song(*song)
                    .map_or_else(String::new, |s| s.title.clone()),
                Piece::Other { title, .. } => title.clone(),
            };
            Labelled {
                title: what,
                detail: format!("in {}", s.title),
                route: Route::Setlist(id, Some(e)),
            }
        }
        Anchor::Album(id) => {
            let a = lib.album(id)?;
            Labelled {
                title: a.title.clone(),
                detail: "Album".into(),
                route: Route::Album(id),
            }
        }
        Anchor::Session(id) => {
            let p = lib.session(id)?;
            Labelled {
                title: p.title.clone(),
                detail: format!("Session · {}", p.stage.label()),
                route: Route::Session(id),
            }
        }
        Anchor::Project(id) => {
            let p = lib.project(id)?;
            Labelled {
                title: p.title.clone(),
                detail: format!("Project · {}", p.kind.label()),
                route: Route::Project(id),
            }
        }
    })
}

/// A page's scrolling column, centred and capped so a desk does not
/// stretch it into lines too long to read.
#[component]
pub(crate) fn Page(children: Element) -> Element {
    let wide = use_wide();
    let pad = if wide { 32 } else { 20 };
    rsx! {
        div {
            style: "flex: 1; min-height: 0; overflow-y: scroll;",
            div {
                style: "max-width: 1180px; margin: 0 auto; padding: 8px {pad}px 48px; box-sizing: border-box;",
                {children}
            }
        }
    }
}

#[component]
pub(crate) fn Home() -> Element {
    let lib = use_library();
    let nav = use_nav();
    let wide = use_wide();
    let space = (nav.space)();
    let l = lib.read();
    let in_space = |s: crate::model::SpaceId| space.is_none_or(|x| x == s);

    let tonight = l
        .setlists
        .iter()
        .find(|s| s.tonight && in_space(s.space))
        .cloned();
    // The dated sets first; then the rest, so a library whose sets carry
    // no date yet still shows them here.
    let mut coming: Vec<Setlist> = l
        .setlists
        .iter()
        .filter(|s| !s.tonight && in_space(s.space))
        .cloned()
        .collect();
    coming.sort_by_key(|s| s.when.is_none());

    // What changed, newest first, across kinds.
    let mut changed: Vec<(u32, Labelled, String, &'static str)> = Vec::new();
    for s in l.songs.iter().filter(|s| in_space(s.space)) {
        changed.push((
            s.edited.age,
            Labelled {
                title: s.title.clone(),
                detail: "Song".into(),
                route: Route::Song(s.id, None),
            },
            format!("{} · {}", s.edited.by, s.edited.when),
            "song",
        ));
    }
    for p in l.sessions.iter().filter(|p| in_space(p.space)) {
        changed.push((
            p.edited.age,
            Labelled {
                title: p.title.clone(),
                detail: format!("Session · {}", p.stage.label()),
                route: Route::Session(p.id),
            },
            format!("{} · {}", p.edited.by, p.edited.when),
            "session",
        ));
    }
    for s in l.setlists.iter().filter(|s| in_space(s.space)) {
        changed.push((
            s.edited.age,
            Labelled {
                title: s.title.clone(),
                detail: "Setlist".into(),
                route: Route::Setlist(s.id, None),
            },
            format!("{} · {}", s.edited.by, s.edited.when),
            "setlist",
        ));
    }
    changed.sort_by_key(|c| c.0);
    changed.truncate(6);

    let threads: Vec<(Labelled, String, String, usize)> = l
        .threads
        .iter()
        .filter(|t| !t.resolved)
        .filter_map(|t| {
            let last = t.comments.last()?;
            Some((
                anchor_label(&l, &t.anchor)?,
                last.author.clone(),
                last.text.clone(),
                t.comments.len(),
            ))
        })
        .collect();

    let columns = if wide {
        "grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); column-gap: 32px;"
    } else {
        "grid-template-columns: minmax(0, 1fr);"
    };
    // The app's ways in (Studio: a new project, joining by a link, what
    // was opened lately), first.
    let slot = try_consume_context::<super::Slots>().and_then(|s| s.home);
    rsx! {
        Page {
            if let Some(slot) = slot {
                {slot.call(())}
            }
            if let Some(set) = tonight {
                Tonight { set }
            }
            NewMixes {}
            div {
                style: "display: grid; {columns}",
                div {
                    Heading { text: "Coming up".to_owned(), trailing: rsx! { TextButton { label: "All setlists".to_owned(), on_press: move |()| nav.jump(Route::Setlists) } } }
                    div { style: "border-top: 1px solid {RULE};",
                        for s in coming {
                            UpcomingRow { key: "{s.id.0}", set: s }
                        }
                    }
                }
                div {
                    Heading { text: "Recently changed".to_owned() }
                    div { style: "border-top: 1px solid {RULE};",
                        for (i, (_, label, by, kind)) in changed.into_iter().enumerate() {
                            ChangedRow { key: "{i}", title: label.title, detail: label.detail, by, kind, route: label.route }
                        }
                    }
                }
            }
            Heading { text: "Open conversations".to_owned() }
            div { style: "border-top: 1px solid {RULE};",
                for (i, (label, author, text, n)) in threads.into_iter().enumerate() {
                    ThreadRow { key: "{i}", title: label.title, detail: label.detail, author, text, count: n, route: label.route }
                }
            }
        }
    }
}

/// Tonight's set: what it is, how ready this iPad is for it, and one
/// press to play it on each of my rigs.
#[component]
fn Tonight(set: Setlist) -> Element {
    let lib = use_library();
    let nav = use_nav();
    let wide = use_wide();
    let l = lib.read();
    let length = minutes(l.setlist_seconds(&set));
    let songs: Vec<_> = set
        .entries
        .iter()
        .filter_map(|e| match e.piece {
            // This show's key where it moves the song.
            Piece::Song { song, arrangement } => l.song(song).and_then(|s| {
                let a = s.arrangement(arrangement)?;
                Some((s, a, e.key.as_ref().unwrap_or(&a.key).0.clone()))
            }),
            Piece::Other { .. } => None,
        })
        .collect();
    let charts = songs
        .iter()
        .filter(|(_, a, _)| a.chart == Ready::Here)
        .count();
    let audio = songs
        .iter()
        .filter(|(_, a, _)| a.audio == Ready::Here)
        .count();
    let total = songs.len();
    let mine: Vec<(crate::model::PartId, String, String)> = l
        .parts_of(set.id)
        .into_iter()
        .filter(|p| p.player == l.me)
        .filter_map(|p| {
            let rig = l.rig(p.rig)?;
            Some((p.id, rig.kind.label().to_owned(), rig.profile.clone()))
        })
        .collect();
    let others: Vec<String> = l
        .parts_of(set.id)
        .into_iter()
        .filter(|p| p.player != l.me)
        .filter_map(|p| Some(format!("{} · {}", l.rig(p.rig)?.kind.label(), p.player)))
        .collect();
    let preview: Vec<(String, &'static str, String)> = songs
        .iter()
        .take(6)
        .map(|(s, _, key)| (s.title.clone(), s.colour, key.clone()))
        .collect();
    let when = set.when.clone().unwrap_or_default();
    let venue = set
        .venue
        .clone()
        .map(|v| format!(" · {v}"))
        .unwrap_or_default();
    let pieces = set.entries.len();
    let id = set.id;
    let direction = if wide { "row" } else { "column" };
    // Beside the facts when there is room; under them, as tall as its
    // rows, when upright.
    let preview_box = if wide {
        "flex: 0 0 340px;"
    } else {
        "flex: 0 0 auto;"
    };
    let charts_line = if charts == total {
        "Every chart".to_owned()
    } else {
        format!("{charts} of {total} charts")
    };
    let audio_line = if audio == total {
        "All audio on iPad".to_owned()
    } else {
        format!("{audio} of {total} tracks on iPad")
    };
    let charts_ink = if charts == total { INK_2 } else { WARN };
    let audio_ink = if audio == total { INK_2 } else { INK_3 };
    rsx! {
        div {
            style: "margin-top: 20px; border-radius: 16px; border: 1px solid rgba(34,197,94,0.35); \
                    background: linear-gradient(180deg, rgba(34,197,94,0.08), rgba(34,197,94,0.02)); \
                    padding: 24px; display: flex; flex-direction: {direction}; gap: 28px;",
            div {
                style: "flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 10px;",
                div { style: "display: flex; align-items: center; gap: 8px; color: {LIVE}; {META} font-weight: 700;",
                    span { style: "width: 8px; height: 8px; border-radius: 4px; background: {LIVE};" }
                    "{when}{venue}"
                }
                button {
                    class: "lp",
                    style: "align-self: flex-start; border: none; background: transparent; padding: 0; margin: 0; \
                            color: {INK}; font-family: {FONT}; text-align: left; border-radius: 6px;",
                    onclick: move |_| nav.go(Route::Setlist(id, None)),
                    span { style: "{TITLE}", "{set.title}" }
                }
                div { style: "display: flex; flex-wrap: wrap; align-items: center; gap: 14px; color: {INK_2}; {META}",
                    span { "{pieces} pieces · {length}" }
                    span { style: "color: {charts_ink};", "{charts_line}" }
                    span { style: "color: {audio_ink};", "{audio_line}" }
                }
                div { style: "display: flex; flex-wrap: wrap; gap: 10px; margin-top: 8px;",
                    for (i, (part, kind, _profile)) in mine.into_iter().enumerate() {
                        Button {
                            key: "{part.0}",
                            label: format!("Play {kind}"),
                            weight: if i == 0 { Weight::Primary } else { Weight::Secondary },
                            icon: rsx! { Play { size: 16, color: if i == 0 { "#fff" } else { INK }, fill: if i == 0 { "#fff" } else { "none" } } },
                            on_press: move |()| open(Open::Part(part)),
                        }
                    }
                    Button { label: "Rehearse".to_owned(), weight: Weight::Secondary, on_press: move |()| open(Open::Rehearse(id)) }
                }
                if !others.is_empty() {
                    div { style: "{META} color: {INK_3}; margin-top: 2px;", "Also on this set: {others.join(\", \")}" }
                }
            }
            div {
                style: "{preview_box} min-width: 0; display: flex; flex-direction: column;",
                for (i, (title, colour, key)) in preview.into_iter().enumerate() {
                    div {
                        key: "{i}",
                        style: "height: 40px; display: flex; align-items: center; gap: 12px; border-bottom: 1px solid rgba(255,255,255,0.06);",
                        span { style: "width: 18px; {META} color: {INK_3}; text-align: right;", "{i + 1}" }
                        Swatch { colour, height: 18 }
                        span { style: "flex: 1; {BODY} font-size: 14px; {ONE_LINE}", "{title}" }
                        span { style: "{META} color: {INK_2};", "{key}" }
                    }
                }
            }
        }
    }
}

#[component]
fn UpcomingRow(set: Setlist) -> Element {
    let lib = use_library();
    let nav = use_nav();
    let l = lib.read();
    let length = minutes(l.setlist_seconds(&set));
    let parts = l.parts_of(set.id).len();
    let id = set.id;
    let when = set.when.clone().unwrap_or_default();
    let venue = set.venue.clone().unwrap_or_default();
    let n = set.entries.len();
    rsx! {
        button {
            class: "lp",
            style: "width: 100%; height: {ROW + 8}px; display: flex; align-items: center; gap: 14px; padding: 0 8px; \
                    border: none; border-bottom: 1px solid {RULE}; background: transparent; color: {INK}; \
                    font-family: {FONT}; text-align: left;",
            onclick: move |_| nav.go(Route::Setlist(id, None)),
            ListMusic { size: 20, color: INK_3 }
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                span { style: "{BODY} {ONE_LINE}", "{set.title}" }
                span { style: "{META} color: {INK_3}; {ONE_LINE}", "{when} · {venue}" }
            }
            div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 3px;",
                span { style: "{META} color: {INK_2};", "{n} · {length}" }
                span { style: "{META} color: {INK_3};", if parts == 0 { "No rigs" } else if parts == 1 { "1 rig" } else { "{parts} rigs" } }
            }
            SyncMark { sync: set.sync, quiet: true }
        }
    }
}

#[component]
fn ChangedRow(
    title: String,
    detail: String,
    by: String,
    kind: &'static str,
    route: Route,
) -> Element {
    let nav = use_nav();
    rsx! {
        button {
            class: "lp",
            style: "width: 100%; height: {ROW + 8}px; display: flex; align-items: center; gap: 14px; padding: 0 8px; \
                    border: none; border-bottom: 1px solid {RULE}; background: transparent; color: {INK}; \
                    font-family: {FONT}; text-align: left;",
            onclick: move |_| nav.go(route),
            match kind {
                "song" => rsx! { Music { size: 20, color: INK_3 } },
                "session" => rsx! { FolderOpen { size: 20, color: INK_3 } },
                _ => rsx! { ListMusic { size: 20, color: INK_3 } },
            }
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                span { style: "{BODY} {ONE_LINE}", "{title}" }
                span { style: "{META} color: {INK_3}; {ONE_LINE}", "{detail}" }
            }
            span { style: "{META} color: {INK_3}; white-space: nowrap;", "{by}" }
        }
    }
}

#[component]
fn ThreadRow(
    title: String,
    detail: String,
    author: String,
    text: String,
    count: usize,
    route: Route,
) -> Element {
    let nav = use_nav();
    rsx! {
        button {
            class: "lp",
            style: "width: 100%; display: flex; align-items: flex-start; gap: 14px; padding: 14px 8px; \
                    border: none; border-bottom: 1px solid {RULE}; background: transparent; color: {INK}; \
                    font-family: {FONT}; text-align: left;",
            onclick: move |_| nav.go(route),
            Avatar { name: author.clone(), size: 32 }
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px;",
                div { style: "display: flex; gap: 8px; align-items: baseline; min-width: 0;",
                    span { style: "{BODY} white-space: nowrap;", "{title}" }
                    span { style: "{META} color: {INK_3}; {ONE_LINE}", "{detail}" }
                }
                span { style: "font-size: 14px; line-height: 1.45; color: {INK_2};", "{author}: {text}" }
            }
            span { style: "display: flex; align-items: center; gap: 5px; {META} color: {INK_3};",
                MessageSquare { size: 15, color: INK_3 }
                "{count}"
            }
        }
    }
}

/// The newest mixes, as a music app's shelf: the artwork plays it (and
/// queues the rest after it), the words open its Review.
#[component]
fn NewMixes() -> Element {
    let lib = use_library();
    let nav = use_nav();
    let player = super::player::use_player();
    let wide = use_wide();
    let Viewport(size) = use_context::<Viewport>();
    let l = lib.read();
    let queue = l.new_mixes();
    if queue.is_empty() {
        return rsx! {};
    }
    let now = player.current();
    let playing = (player.playing)();
    let cards: Vec<(usize, Listen, String, String, &'static str, String)> = queue
        .iter()
        .enumerate()
        .filter_map(|(i, listen)| {
            let (session, mix) = l.mix(*listen)?;
            let (title, artist, colour) = l.listen_label(*listen)?;
            let _ = artist;
            Some((
                i,
                *listen,
                title,
                format!("Mix v{} · {}", mix.n, session.edited.when),
                colour,
                mix.note.clone(),
            ))
        })
        .collect();
    let columns = if wide {
        4
    } else if size().0 >= 700.0 {
        3
    } else {
        2
    };
    rsx! {
        Heading { text: "New mixes".to_owned(),
            trailing: rsx! { TextButton { label: "Play all".to_owned(), on_press: {
                let queue = queue.clone();
                move |()| player.play(queue.clone(), 0, "New mixes")
            } } } }
        div { style: "display: grid; grid-template-columns: repeat({columns}, minmax(0, 1fr)); gap: 20px;",
            for (i, listen, title, line, colour, note) in cards {
                {
                    let queue = queue.clone();
                    let here = now == Some(listen);
                    let (icon_play, label) = if here && playing { (false, "Pause") } else { (true, "Play") };
                    rsx! {
                        div { key: "{i}", style: "min-width: 0; display: flex; flex-direction: column; gap: 10px;",
                            div { style: "position: relative; width: 100%; padding-top: 100%; border-radius: 12px; overflow: hidden; \
                                          background: linear-gradient(145deg, {colour} 0%, {colour}99 45%, #18181b 100%);",
                                button { class: "lp", "aria-label": "{label} {title}",
                                    style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%; border: none; background: transparent; padding: 0;",
                                    onclick: move |_| if here { player.toggle() } else { player.play(queue.clone(), i, "New mixes") },
                                    span { style: "position: absolute; left: 14px; right: 14px; bottom: 14px; text-align: left; font-family: {FONT}; \
                                                   font-size: 20px; font-weight: 800; letter-spacing: -0.02em; line-height: 1.05; color: rgba(255,255,255,0.92);",
                                        "{title}"
                                    }
                                    span { style: "position: absolute; right: 12px; top: 12px; width: 44px; height: 44px; border-radius: 22px; \
                                                   background: rgba(12,12,15,0.55); display: flex; align-items: center; justify-content: center;",
                                        if icon_play { Play { size: 20, color: "#fff", fill: "#fff" } } else { Pause { size: 20, color: "#fff", fill: "#fff" } }
                                    }
                                }
                            }
                            button { class: "lp",
                                style: "border: none; background: transparent; padding: 2px 0; margin: 0; display: flex; flex-direction: column; align-items: flex-start; gap: 3px; \
                                        color: {INK}; font-family: {FONT}; text-align: left; border-radius: 6px; min-width: 0;",
                                onclick: move |_| nav.go(Route::Session(listen.session)),
                                span { style: "{BODY} {ONE_LINE}", "{title}" }
                                span { style: "{META} color: {INK_3}; {ONE_LINE}", "{line}" }
                                span { style: "{META} color: {INK_3}; {ONE_LINE}", "{note}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
