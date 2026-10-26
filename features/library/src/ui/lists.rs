//! The sidebar's lists: every setlist, song, album, session and rig in
//! the chosen space, as tables a finger can work.

use dioxus::prelude::*;
use lucide_dioxus::{Guitar, Mic, Piano, Plus};

use super::home::Page;
use super::kit::*;
use super::{Route, use_library, use_nav, use_wide};
use crate::model::{Found, RigKind, SpaceId, Stage, Sync, clock, minutes};

/// A list page's title, what it holds, and its one action.
#[component]
pub(crate) fn ListHeader(title: &'static str, detail: String, action: &'static str) -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: flex-end; gap: 16px; padding: 24px 0 18px;",
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px;",
                span { style: "{TITLE}", "{title}" }
                span { style: "{META} color: {INK_3};", "{detail}" }
            }
            // The action is drawn now, wired when the docs are (creating a
            // doc is the source's job, not the screen's).
            Button { label: action.to_owned(), weight: Weight::Secondary, icon: rsx! { Plus { size: 16, color: INK } }, on_press: move |()| {} }
        }
    }
}

/// A table's column headers. `columns` is the grid template every row of
/// the table shares.
#[component]
pub(crate) fn Columns(columns: String, names: Vec<&'static str>) -> Element {
    rsx! {
        div {
            style: "display: grid; grid-template-columns: {columns}; column-gap: 16px; align-items: center; \
                    height: 32px; padding: 0 12px; border-bottom: 1px solid {RULE_STRONG}; {COLUMN} color: {INK_3};",
            for (i, n) in names.into_iter().enumerate() {
                span { key: "{i}", style: "{ONE_LINE}", "{n}" }
            }
        }
    }
}

/// One table row: a press that goes to `route`.
#[component]
pub(crate) fn Row(columns: String, route: Route, children: Element) -> Element {
    let nav = use_nav();
    rsx! {
        button {
            class: "lp",
            style: "width: 100%; min-height: {ROW + 4}px; display: grid; grid-template-columns: {columns}; \
                    column-gap: 16px; align-items: center; padding: 8px 12px; border: none; \
                    border-bottom: 1px solid {RULE}; background: transparent; color: {INK}; \
                    font-family: {FONT}; text-align: left;",
            onclick: move |_| nav.go(route),
            {children}
        }
    }
}

/// A title with a line of detail under it, for a table's first column.
#[component]
pub(crate) fn Named(
    title: String,
    detail: String,
    #[props(default)] colour: Option<String>,
) -> Element {
    rsx! {
        div { style: "display: flex; align-items: center; gap: 12px; min-width: 0;",
            if let Some(c) = colour { Swatch { colour: c, height: 32 } }
            div { style: "min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                span { style: "{BODY} {ONE_LINE}", "{title}" }
                span { style: "{META} color: {INK_3}; {ONE_LINE}", "{detail}" }
            }
        }
    }
}

fn in_space(chosen: Option<SpaceId>, s: SpaceId) -> bool {
    chosen.is_none_or(|x| x == s)
}

#[component]
pub(crate) fn Setlists() -> Element {
    let lib = use_library();
    let nav = use_nav();
    let wide = use_wide();
    let space = (nav.space)();
    let l = lib.read();
    let sets: Vec<_> = l
        .setlists
        .iter()
        .filter(|s| in_space(space, s.space))
        .collect();
    let columns = if wide {
        "minmax(0, 3fr) minmax(0, 2fr) 90px 120px 96px"
    } else {
        "minmax(0, 1fr) 90px 96px"
    }
    .to_owned();
    let names = if wide {
        vec!["Setlist", "When", "Length", "Rigs", "Sync"]
    } else {
        vec!["Setlist", "Length", "Sync"]
    };
    rsx! {
        Page {
            ListHeader { title: "Setlists", detail: format!("{} setlists · what each show plays, in order", sets.len()), action: "New setlist" }
            Columns { columns: columns.clone(), names }
            for s in sets {
                Row { key: "{s.id.0}", columns: columns.clone(), route: Route::Setlist(s.id, None),
                    Named {
                        title: s.title.clone(),
                        detail: format!("{} pieces · {}", s.entries.len(), l.space(s.space).map_or("", |x| x.name.as_str())),
                        colour: Some(if s.tonight { LIVE.to_owned() } else { DIM.to_owned() }),
                    }
                    if wide {
                        div { style: "min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                            span { style: "{META} color: {INK_2}; {ONE_LINE}", "{s.when.clone().unwrap_or_else(|| \"No date\".into())}" }
                            span { style: "{META} color: {INK_3}; {ONE_LINE}", "{s.venue.clone().unwrap_or_default()}" }
                        }
                    }
                    span { style: "{META} color: {INK_2};", "{minutes(l.setlist_seconds(s))}" }
                    if wide {
                        div { style: "display: flex; align-items: center; gap: -6px;",
                            for p in l.parts_of(s.id) {
                                span { key: "{p.id.0}", style: "margin-right: -6px; border: 2px solid {DESK}; border-radius: 16px; display: flex;",
                                    Avatar { name: p.player.clone(), size: 26 }
                                }
                            }
                            if l.parts_of(s.id).is_empty() { span { style: "{META} color: {INK_3};", "None yet" } }
                        }
                    }
                    SyncMark { sync: s.sync }
                }
            }
        }
    }
}

#[component]
pub(crate) fn Songs() -> Element {
    let lib = use_library();
    let nav = use_nav();
    let wide = use_wide();
    let space = (nav.space)();
    let l = lib.read();
    let mut songs: Vec<_> = l
        .songs
        .iter()
        .filter(|s| in_space(space, s.space))
        .collect();
    songs.sort_by(|a, b| a.title.cmp(&b.title));
    let columns = if wide {
        "minmax(0, 3fr) minmax(0, 2fr) 56px 64px 64px minmax(0, 2fr) 96px"
    } else {
        "minmax(0, 1fr) 56px 64px 96px"
    }
    .to_owned();
    let names = if wide {
        vec![
            "Song",
            "Arrangements",
            "Key",
            "BPM",
            "Length",
            "Ready",
            "Sync",
        ]
    } else {
        vec!["Song", "Key", "BPM", "Sync"]
    };
    rsx! {
        Page {
            ListHeader { title: "Songs", detail: format!("{} songs · each with its arrangements, charts and audio", songs.len()), action: "New song" }
            Columns { columns: columns.clone(), names }
            for s in songs {
                {
                    let main = &s.arrangements[0];
                    let names: Vec<String> = s.arrangements.iter().map(|a| a.name.clone()).collect();
                    rsx! {
                        Row { key: "{s.id.0}", columns: columns.clone(), route: Route::Song(s.id, None),
                            Named { title: s.title.clone(), detail: s.artist.clone(), colour: Some(s.colour.to_owned()) }
                            if wide {
                                span { style: "{META} color: {INK_2}; {ONE_LINE}", "{names.join(\" · \")}" }
                            }
                            span { style: "{BODY} color: {INK_2};", "{main.key}" }
                            span { style: "{META} color: {INK_2};", "{main.time.bpm:.0}" }
                            if wide {
                                span { style: "{META} color: {INK_2};", "{clock(s.seconds(main))}" }
                                div { style: "display: flex; gap: 12px; min-width: 0; overflow: hidden;",
                                    ReadyMark { what: "Chart", ready: main.chart }
                                    ReadyMark { what: "Audio", ready: main.audio }
                                }
                            }
                            SyncMark { sync: s.sync }
                        }
                    }
                }
            }
        }
    }
}

/// An album's tracks as one bar: each track a segment, lit by how far its
/// session has come (none yet is a dim outline).
#[component]
pub(crate) fn StageBar(stages: Vec<Option<Stage>>) -> Element {
    let colour = |s: Option<Stage>| match s {
        None => "transparent",
        Some(Stage::Writing | Stage::Tracking) => "#334155",
        Some(Stage::Editing) => "#475569",
        Some(Stage::Mixing) => "#60a5fa",
        Some(Stage::Mastering) => "#a78bfa",
        Some(Stage::Released) => LIVE,
    };
    rsx! {
        div { style: "display: flex; gap: 3px; height: 10px; min-width: 0;",
            for (i, s) in stages.into_iter().enumerate() {
                span { key: "{i}", style: "flex: 1 1 0; min-width: 0; border-radius: 3px; background: {colour(s)}; border: 1px solid {RULE_STRONG};" }
            }
        }
    }
}

#[component]
pub(crate) fn Sessions() -> Element {
    let lib = use_library();
    let nav = use_nav();
    let wide = use_wide();
    let space = (nav.space)();
    let l = lib.read();
    let mut sessions: Vec<_> = l
        .sessions
        .iter()
        .filter(|p| in_space(space, p.space))
        .collect();
    sessions.sort_by_key(|p| p.edited.age);
    let columns = if wide {
        "minmax(0, 3fr) 110px 72px minmax(0, 2fr) 96px"
    } else {
        "minmax(0, 1fr) 110px 96px"
    }
    .to_owned();
    let names = if wide {
        vec!["Session", "Stage", "Tracks", "Last change", "Sync"]
    } else {
        vec!["Session", "Stage", "Sync"]
    };
    rsx! {
        Page {
            ListHeader { title: "Sessions", detail: format!("{} sessions · full sessions, in the studio", sessions.len()), action: "New session" }
            Columns { columns: columns.clone(), names }
            for p in sessions {
                Row { key: "{p.id.0}", columns: columns.clone(), route: Route::Session(p.id),
                    Named {
                        title: p.title.clone(),
                        detail: p.song.and_then(|s| l.song(s)).map_or_else(|| "Not a song yet".to_owned(), |s| format!("Song · {}", s.title)),
                        colour: p.song.and_then(|s| l.song(s)).map(|s| s.colour.to_owned()),
                    }
                    div { Chip { text: p.stage.label().to_owned(), strong: matches!(p.stage, Stage::Mixing | Stage::Mastering) } }
                    if wide {
                        span { style: "{META} color: {INK_2};", "{p.tracks}" }
                        span { style: "{META} color: {INK_3}; {ONE_LINE}", "{p.edited.by} on {p.edited.device} · {p.edited.when}" }
                    }
                    SyncMark { sync: p.sync }
                }
            }
        }
    }
}

#[component]
pub(crate) fn Rigs() -> Element {
    let lib = use_library();
    let wide = use_wide();
    let l = lib.read();
    let columns = if wide {
        "minmax(0, 3fr) minmax(0, 2fr) 90px 110px 96px"
    } else {
        "minmax(0, 1fr) 110px 96px"
    }
    .to_owned();
    let names = if wide {
        vec!["Rig", "Profile", "Patches", "Song presets", "Sync"]
    } else {
        vec!["Rig", "Song presets", "Sync"]
    };
    let rigs: Vec<_> = l.rigs.iter().filter(|r| r.owner == l.me).collect();
    rsx! {
        Page {
            ListHeader { title: "Rigs", detail: format!("{} rigs · your sounds, and the preset each song wants", rigs.len()), action: "Add rig" }
            Columns { columns: columns.clone(), names }
            for r in rigs.iter() {
                Row { key: "{r.id.0}", columns: columns.clone(), route: Route::Rigs,
                    div { style: "display: flex; align-items: center; gap: 12px; min-width: 0;",
                        span { style: "width: 36px; height: 36px; border-radius: 10px; background: {SHEET_2}; border: 1px solid {RULE}; \
                                       display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
                            match r.kind {
                                RigKind::Keys => rsx! { Piano { size: 18, color: INK_2 } },
                                RigKind::Vocals => rsx! { Mic { size: 18, color: INK_2 } },
                                _ => rsx! { Guitar { size: 18, color: INK_2 } },
                            }
                        }
                        div { style: "min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                            span { style: "{BODY}", "{r.name}" }
                            span { style: "{META} color: {INK_3};", "Default: {r.default_patch}" }
                        }
                    }
                    if wide {
                        span { style: "{META} color: {INK_2};", "{r.profile}" }
                        span { style: "{META} color: {INK_2};", "{r.patches}" }
                    }
                    span { style: "{META} color: {INK_2};", "{r.presets.len()} songs" }
                    SyncMark { sync: r.sync }
                }
            }
            if rigs.iter().all(|r| r.sync != Sync::Cloud) {
                p { style: "{META} color: {INK_3}; line-height: 1.5; margin-top: 18px; max-width: 640px;",
                    "A rig's song presets follow a song into every setlist. Attach a rig to a setlist to change the sound for one show only."
                }
            }
        }
    }
}

/// What the search found, grouped by kind, each a press that opens it.
#[component]
pub(crate) fn Results() -> Element {
    let lib = use_library();
    let nav = use_nav();
    let q = (nav.query)();
    let l = lib.read();
    let found = l.search(&q);
    let rows: Vec<(&'static str, String, String, Route)> = found
        .iter()
        .filter_map(|f| {
            Some(match *f {
                Found::Project(id) => {
                    let p = l.project(id)?;
                    (
                        "Projects",
                        p.title.clone(),
                        format!("{} · {}", p.kind.label(), p.status.label()),
                        Route::Project(id),
                    )
                }
                Found::Setlist(id) => {
                    let s = l.setlist(id)?;
                    (
                        "Setlists",
                        s.title.clone(),
                        s.when.clone().unwrap_or_else(|| "No date".into()),
                        Route::Setlist(id, None),
                    )
                }
                Found::Song(id) => {
                    let s = l.song(id)?;
                    (
                        "Songs",
                        s.title.clone(),
                        s.artist.clone(),
                        Route::Song(id, None),
                    )
                }
                Found::Session(id) => {
                    let s = l.session(id)?;
                    (
                        "Sessions",
                        s.title.clone(),
                        s.stage.label().to_owned(),
                        Route::Session(id),
                    )
                }
            })
        })
        .collect();
    let columns = "minmax(0, 1fr)".to_owned();
    let count = rows.len();
    let kinds: Vec<&'static str> = rows.iter().map(|r| r.0).collect();
    rsx! {
        Page {
            div { style: "display: flex; flex-direction: column; gap: 6px; padding: 24px 0 8px;",
                span { style: "{TITLE}", "“{q.trim()}”" }
                span { style: "{META} color: {INK_3};",
                    if count == 0 { "Nothing matches. Try a song's title, an artist, a venue." } else if count == 1 { "1 match" } else { "{count} matches" }
                }
            }
            for (i, (kind, title, detail, route)) in rows.into_iter().enumerate() {
                div { key: "{i}",
                    if i == 0 || kinds[i - 1] != kind {
                        Heading { text: kind.to_owned() }
                    }
                    Row { columns: columns.clone(), route,
                        Named { title, detail }
                    }
                }
            }
        }
    }
}
