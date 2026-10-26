//! Projects: Task's containers (a run of services, a show, a record),
//! each with its people and its tasks, and the setlists, sessions and
//! album attached to it. The list, and one project's page.

use dioxus::prelude::*;
use lucide_dioxus::{AudioWaveform, Check, Disc3, ListMusic, Mic, Radio};

use super::home::Page;
use super::kit::*;
use super::lists::{Columns, ListHeader, Named, Row, StageBar};
use super::song::Panel;
use super::{Route, use_library, use_nav, use_wide};
use crate::model::{Anchor, Project, ProjectId, ProjectKind, Stage, Status, clock, minutes};

fn kind_icon(kind: ProjectKind, colour: &'static str) -> Element {
    match kind {
        ProjectKind::Live => rsx! { Radio { size: 18, color: colour } },
        ProjectKind::Recording => rsx! { Mic { size: 18, color: colour } },
        ProjectKind::Other => rsx! { ListMusic { size: 18, color: colour } },
    }
}

/// What a project holds, in a few words: "2 setlists · 4 sessions · album".
fn contents(p: &Project) -> String {
    let mut parts = Vec::new();
    match p.setlists.len() {
        0 => {}
        1 => parts.push("1 setlist".to_owned()),
        n => parts.push(format!("{n} setlists")),
    }
    match p.sessions.len() {
        0 => {}
        1 => parts.push("1 session".to_owned()),
        n => parts.push(format!("{n} sessions")),
    }
    if p.album.is_some() {
        parts.push("album".to_owned());
    }
    if parts.is_empty() {
        "Nothing attached yet".to_owned()
    } else {
        parts.join(" · ")
    }
}

/// A short bar of how many of a project's tasks are done.
#[component]
fn Progress(done: usize, total: usize) -> Element {
    let pct = if total == 0 { 0 } else { done * 100 / total };
    rsx! {
        div { style: "display: flex; align-items: center; gap: 8px; min-width: 0;",
            div { style: "flex: 1; min-width: 0; height: 6px; border-radius: 3px; background: {SHEET_2}; border: 1px solid {RULE}; overflow: hidden;",
                div { style: "width: {pct}%; height: 100%; background: {LIVE};" }
            }
            span { style: "{META} color: {INK_2}; white-space: nowrap;", "{done}/{total}" }
        }
    }
}

#[component]
pub(crate) fn Projects() -> Element {
    let lib = use_library();
    let nav = use_nav();
    let wide = use_wide();
    let space = (nav.space)();
    let l = lib.read();
    let mut projects: Vec<_> = l
        .projects
        .iter()
        .filter(|p| space.is_none_or(|s| s == p.space))
        .collect();
    // Active first, then what is being planned, then the rest.
    projects.sort_by_key(|p| match p.status {
        Status::Active => 0,
        Status::Planning => 1,
        Status::OnHold => 2,
        Status::Done => 3,
    });
    let columns = if wide {
        "minmax(0, 3fr) 100px minmax(0, 1.5fr) 130px 104px 96px"
    } else {
        "minmax(0, 1fr) 120px 96px"
    }
    .to_owned();
    let names = if wide {
        vec!["Project", "Status", "Holds", "Tasks", "People", "Sync"]
    } else {
        vec!["Project", "Tasks", "Sync"]
    };
    rsx! {
        Page {
            ListHeader { title: "Projects", detail: format!("{} projects · what the band is working on, kept in Task", projects.len()), action: "New project" }
            Columns { columns: columns.clone(), names }
            for p in projects {
                {
                    let done = p.todos.iter().filter(|t| t.done).count();
                    let total = p.todos.len();
                    let when = [p.kind.label().to_owned(), p.when.clone().unwrap_or_default()]
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" · ");
                    rsx! {
                        Row { key: "{p.id.0}", columns: columns.clone(), route: Route::Project(p.id),
                            div { style: "display: flex; align-items: center; gap: 12px; min-width: 0;",
                                span { style: "width: 36px; height: 36px; border-radius: 10px; background: {SHEET_2}; border: 1px solid {RULE}; \
                                               display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
                                    {kind_icon(p.kind, INK_2)}
                                }
                                Named { title: p.title.clone(), detail: when }
                            }
                            if wide {
                                div { Chip { text: p.status.label().to_owned(), strong: p.status == Status::Active } }
                                span { style: "{META} color: {INK_2}; {ONE_LINE}", "{contents(p)}" }
                            }
                            Progress { done, total }
                            if wide {
                                div { style: "display: flex; align-items: center;",
                                    for (i, person) in p.people.iter().take(4).enumerate() {
                                        span { key: "{i}", style: "margin-right: -6px; border: 2px solid {DESK}; border-radius: 16px; display: flex;",
                                            Avatar { name: person.clone(), size: 26 }
                                        }
                                    }
                                }
                            }
                            SyncMark { sync: p.sync }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn ProjectPage(id: ProjectId) -> Element {
    let lib = use_library();
    let wide = use_wide();
    let l = lib.read();
    let Some(p) = l.project(id).cloned() else {
        return rsx! { Page { p { style: "{BODY} color: {INK_2}; padding-top: 40px;", "This project was deleted or has not arrived yet." } } };
    };
    let place = [p.when.clone(), p.venue.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let done = p.todos.iter().filter(|t| t.done).count();
    let total = p.todos.len();
    let setlists: Vec<(crate::model::SetlistId, String, String, bool)> = p
        .setlists
        .iter()
        .filter_map(|s| l.setlist(*s))
        .map(|s| {
            let detail = format!(
                "{} · {} pieces · {}",
                s.when.clone().unwrap_or_else(|| "No date".into()),
                s.entries.len(),
                minutes(l.setlist_seconds(s))
            );
            (s.id, s.title.clone(), detail, s.tonight)
        })
        .collect();
    let sessions: Vec<(crate::model::SessionId, String, String, Stage)> = p
        .sessions
        .iter()
        .filter_map(|s| l.session(*s))
        .map(|s| {
            (
                s.id,
                s.title.clone(),
                format!("{} tracks · {} · {}", s.tracks, s.edited.by, s.edited.when),
                s.stage,
            )
        })
        .collect();
    let album = p.album.and_then(|a| l.album(a)).map(|a| {
        let stages: Vec<Option<Stage>> = a
            .tracks
            .iter()
            .map(|t| t.session.and_then(|s| l.session(s)).map(|s| s.stage))
            .collect();
        let total: f32 = a
            .tracks
            .iter()
            .filter_map(|t| {
                let s = l.song(t.song)?;
                Some(s.seconds(s.arrangement(t.arrangement)?))
            })
            .sum();
        (
            a.id,
            a.title.clone(),
            format!("{} tracks · {}", a.tracks.len(), clock(total)),
            stages,
        )
    });
    let threads: Vec<_> = l
        .threads_on(&Anchor::Project(id))
        .into_iter()
        .cloned()
        .collect();
    let direction = if wide { "row" } else { "column" };
    let kind = p.kind;
    rsx! {
        Page {
            div { style: "display: flex; align-items: flex-end; gap: 16px; padding: 24px 0 18px; flex-wrap: wrap;",
                div { style: "flex: 1 1 360px; min-width: 0; display: flex; flex-direction: column; gap: 8px;",
                    div { style: "display: flex; align-items: center; gap: 8px; {META} color: {INK_2}; font-weight: 650;",
                        {kind_icon(kind, INK_2)}
                        "{p.kind.label()} · {p.status.label()}"
                    }
                    span { style: "{TITLE}", "{p.title}" }
                    span { style: "{META} color: {INK_3};", "{place}" }
                }
                div { style: "display: flex; align-items: center; gap: 4px;",
                    for (i, person) in p.people.iter().enumerate() {
                        span { key: "{i}", style: "margin-left: -6px; border: 2px solid {DESK}; border-radius: 20px; display: flex;",
                            Avatar { name: person.clone(), size: 34 }
                        }
                    }
                }
            }
            div { style: "display: flex; flex-direction: {direction}; gap: 28px; align-items: flex-start;",
                div { style: "flex: 1 1 0; min-width: 0; width: 100%;",
                    if !setlists.is_empty() {
                        Heading { text: "Setlists".to_owned() }
                        div { style: "border-top: 1px solid {RULE};",
                            for (sid, title, detail, tonight) in setlists {
                                AttachedRow { key: "{sid.0}", title, detail, live: tonight, route: Route::Setlist(sid, None),
                                    icon: rsx! { ListMusic { size: 20, color: if tonight { LIVE } else { INK_3 } } } }
                            }
                        }
                    }
                    if let Some((aid, title, detail, stages)) = album {
                        Heading { text: "Album".to_owned() }
                        div { style: "border-top: 1px solid {RULE};",
                            AttachedRow { title, detail, live: false, route: Route::Album(aid),
                                icon: rsx! { Disc3 { size: 20, color: INK_3 } } }
                            div { style: "padding: 10px 8px 4px;", StageBar { stages } }
                        }
                    }
                    if !sessions.is_empty() {
                        Heading { text: "Sessions".to_owned() }
                        div { style: "border-top: 1px solid {RULE};",
                            for (sid, title, detail, stage) in sessions {
                                AttachedRow { key: "{sid.0}", title, detail: format!("{} · {detail}", stage.label()), live: false, route: Route::Session(sid),
                                    icon: rsx! { AudioWaveform { size: 20, color: INK_3 } } }
                            }
                        }
                    }
                    if p.setlists.is_empty() && p.sessions.is_empty() && p.album.is_none() {
                        p { style: "{META} color: {INK_3}; line-height: 1.5; padding-top: 24px;",
                            "Nothing attached yet. Add a setlist for a show, or a session to record in."
                        }
                    }
                }
                div { style: if wide { "flex: 0 0 360px; min-width: 0; display: flex; flex-direction: column; gap: 18px; margin-top: 22px;" } else { "width: 100%; display: flex; flex-direction: column; gap: 18px;" },
                    Panel { title: "Tasks",
                        div { style: "padding-bottom: 10px;", Progress { done, total } }
                        for (i, t) in p.todos.iter().enumerate() {
                            {
                                let (box_bg, box_border, ink) = if t.done { (LIVE, LIVE, INK_3) } else { ("transparent", DIM, INK) };
                                let strike = if t.done { "text-decoration: line-through;" } else { "" };
                                let meta = [t.who.clone(), t.due.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ");
                                rsx! {
                                    div { key: "{i}", style: "min-height: 48px; display: flex; align-items: center; gap: 12px; border-top: 1px solid {RULE};",
                                        span { style: "width: 20px; height: 20px; border-radius: 6px; border: 1.5px solid {box_border}; background: {box_bg}; \
                                                       display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
                                            if t.done { Check { size: 14, color: "#04210f", stroke_width: 3 } }
                                        }
                                        span { style: "flex: 1; min-width: 0; font-size: 14px; line-height: 1.35; color: {ink}; {strike}", "{t.title}" }
                                        span { style: "{META} color: {INK_3}; white-space: nowrap;", "{meta}" }
                                    }
                                }
                            }
                        }
                    }
                    Panel { title: "Conversation",
                        for (i, t) in threads.iter().enumerate() {
                            for (j, c) in t.comments.iter().enumerate() {
                                div { key: "{i}-{j}", style: "display: flex; gap: 10px; align-items: flex-start; padding: 12px 0 10px; border-top: 1px solid {RULE};",
                                    Avatar { name: c.author.clone(), size: 28 }
                                    div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                                        span { style: "{META} color: {INK_2}; font-weight: 650;", "{c.author} " span { style: "color: {INK_3}; font-weight: 500;", "{c.when}" } }
                                        span { style: "font-size: 14px; line-height: 1.45;", "{c.text}" }
                                    }
                                }
                            }
                        }
                        if threads.is_empty() { span { style: "{META} color: {INK_3}; padding: 10px 0; border-top: 1px solid {RULE};", "No comments yet." } }
                    }
                }
            }
        }
    }
}

/// One thing attached to a project: a press that opens it.
#[component]
fn AttachedRow(title: String, detail: String, live: bool, route: Route, icon: Element) -> Element {
    let nav = use_nav();
    let title_ink = if live { LIVE } else { INK };
    rsx! {
        button {
            class: "lp",
            style: "width: 100%; min-height: {ROW + 8}px; display: flex; align-items: center; gap: 14px; padding: 8px; \
                    border: none; border-bottom: 1px solid {RULE}; background: transparent; color: {INK}; \
                    font-family: {FONT}; text-align: left;",
            onclick: move |_| nav.go(route),
            {icon}
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                span { style: "{BODY} {ONE_LINE} color: {title_ink};", "{title}" }
                span { style: "{META} color: {INK_3}; {ONE_LINE}", "{detail}" }
            }
        }
    }
}

/// "in Sunday nights, fall": the way back from what a project holds to it.
#[component]
pub(crate) fn InProject(id: Option<ProjectId>) -> Element {
    let lib = use_library();
    let nav = use_nav();
    let Some(p) = id.and_then(|id| lib.read().project(id).cloned()) else {
        return rsx! {};
    };
    let pid = p.id;
    rsx! {
        button {
            class: "lp",
            style: "align-self: flex-start; height: 28px; padding: 0 10px; margin-left: -10px; border: none; border-radius: 8px; \
                    background: transparent; color: {PICK}; font-family: {FONT}; {META} font-weight: 650; \
                    display: flex; align-items: center; gap: 6px;",
            onclick: move |_| nav.go(Route::Project(pid)),
            "{p.title}"
        }
    }
}
