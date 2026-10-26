//! An album (its tracks, each an arrangement and the session that
//! produces it, and how far each has come) and a session (a `.session`:
//! its song, its stage, and the way into Session).

use dioxus::prelude::*;
use lucide_dioxus::FolderOpen;

use super::home::Page;
use super::kit::*;
use super::lists::StageBar;
use super::song::Panel;
use super::{Open, Route, open, use_library, use_nav, use_wide};
use crate::model::{AlbumId, Anchor, SessionId, Stage, clock};

#[component]
pub(crate) fn AlbumPage(id: AlbumId) -> Element {
    let lib = use_library();
    let nav = use_nav();
    let wide = use_wide();
    let l = lib.read();
    let Some(album) = l.album(id).cloned() else {
        return rsx! { Page { p { style: "{BODY} color: {INK_2}; padding-top: 40px;", "This album was deleted or has not arrived yet." } } };
    };
    let stages: Vec<Option<Stage>> = album
        .tracks
        .iter()
        .map(|t| t.session.and_then(|p| l.session(p)).map(|p| p.stage))
        .collect();
    let total: f32 = album
        .tracks
        .iter()
        .filter_map(|t| {
            let s = l.song(t.song)?;
            Some(s.seconds(s.arrangement(t.arrangement)?))
        })
        .sum();
    let in_project = l.project_of_album(id).map(|p| p.id);
    let without = stages.iter().filter(|s| s.is_none()).count();
    let columns = if wide {
        "36px minmax(0, 3fr) 64px minmax(0, 2fr) 110px"
    } else {
        "36px minmax(0, 1fr) 110px"
    };
    rsx! {
        Page {
            div { style: "display: flex; align-items: center; gap: 24px; padding: 24px 0 20px;",
                // The cover: the songs' colours, until there is artwork.
                div { style: "width: 132px; height: 132px; border-radius: 12px; overflow: hidden; flex-shrink: 0; \
                              display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); border: 1px solid {RULE_STRONG};",
                    for (i, t) in album.tracks.iter().take(4).enumerate() {
                        span { key: "{i}", style: "background: {l.song(t.song).map_or(DIM, |s| s.colour)}; opacity: 0.85;" }
                    }
                }
                div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 8px;",
                    super::project::InProject { id: in_project }
                    span { style: "{TITLE}", "{album.title}" }
                    span { style: "{META} color: {INK_3};", "{album.artist} · {album.year} · {album.tracks.len()} tracks · {clock(total)}" }
                    div { style: "max-width: 420px; margin-top: 6px;", StageBar { stages: stages.clone() } }
                    if without > 0 { span { style: "{META} color: {WARN};", "{without} tracks have no session yet" } }
                }
            }
            div { style: "display: grid; grid-template-columns: {columns}; column-gap: 16px; align-items: center; \
                          height: 32px; padding: 0 12px; border-bottom: 1px solid {RULE_STRONG}; {COLUMN} color: {INK_3};",
                span {} span { "Track" }
                if wide { span { "Length" } span { "Session" } }
                span { "Stage" }
            }
            for (i, t) in album.tracks.iter().enumerate() {
                {
                    let song = l.song(t.song).cloned();
                    let arr = song.as_ref().and_then(|s| s.arrangement(t.arrangement).cloned());
                    let session = t.session.and_then(|p| l.session(p)).cloned();
                    let length = song.as_ref().zip(arr.as_ref()).map(|(s, a)| clock(s.seconds(a))).unwrap_or_default();
                    let route = t.session.map_or(Route::Song(t.song, Some(t.arrangement)), Route::Session);
                    rsx! {
                        button { key: "{i}", class: "lp",
                            style: "width: 100%; min-height: {ROW + 4}px; display: grid; grid-template-columns: {columns}; \
                                    column-gap: 16px; align-items: center; padding: 8px 12px; border: none; \
                                    border-bottom: 1px solid {RULE}; background: transparent; color: {INK}; \
                                    font-family: {FONT}; text-align: left;",
                            onclick: move |_| nav.go(route),
                            span { style: "{META} color: {INK_3}; text-align: right;", "{i + 1}" }
                            div { style: "display: flex; align-items: center; gap: 12px; min-width: 0;",
                                Swatch { colour: song.as_ref().map_or(DIM, |s| s.colour).to_owned(), height: 32 }
                                div { style: "min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                                    span { style: "{BODY} {ONE_LINE}", "{song.as_ref().map_or(\"Missing song\", |s| s.title.as_str())}" }
                                    span { style: "{META} color: {INK_3};", "{arr.as_ref().map_or(\"\", |a| a.name.as_str())}" }
                                }
                            }
                            if wide {
                                span { style: "{META} color: {INK_2};", "{length}" }
                                span { style: "{META} color: {INK_2}; {ONE_LINE}",
                                    "{session.as_ref().map_or(\"None yet\", |p| p.title.as_str())}"
                                }
                            }
                            div {
                                match &session {
                                    Some(p) => rsx! { Chip { text: p.stage.label().to_owned(), strong: matches!(p.stage, Stage::Mixing | Stage::Mastering) } },
                                    None => rsx! { span { style: "{META} color: {INK_3};", "Not started" } },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn SessionPage(id: SessionId) -> Element {
    let lib = use_library();
    let nav = use_nav();
    let wide = use_wide();
    let l = lib.read();
    let Some(session) = l.session(id).cloned() else {
        return rsx! { Page { p { style: "{BODY} color: {INK_2}; padding-top: 40px;", "This session was deleted or has not arrived yet." } } };
    };
    let song = session.song.and_then(|s| l.song(s)).cloned();
    let threads: Vec<_> = l
        .threads_on(&Anchor::Session(id))
        .into_iter()
        .cloned()
        .collect();
    let stages = [
        Stage::Writing,
        Stage::Tracking,
        Stage::Editing,
        Stage::Mixing,
        Stage::Mastering,
        Stage::Released,
    ];
    let at = stages.iter().position(|s| *s == session.stage).unwrap_or(0);
    let in_project = l.project_of_session(id).map(|p| p.id);
    let direction = if wide { "row" } else { "column" };
    rsx! {
        Page {
            div { style: "display: flex; align-items: flex-end; gap: 16px; padding: 24px 0 20px; flex-wrap: wrap;",
                div { style: "flex: 1 1 360px; min-width: 0; display: flex; flex-direction: column; gap: 8px;",
                    super::project::InProject { id: in_project }
                    span { style: "{TITLE}", "{session.title}" }
                    span { style: "{META} color: {INK_3};",
                        "{session.tracks} tracks · last changed by {session.edited.by} on {session.edited.device}, {session.edited.when}"
                    }
                }
                Button { label: "Open in Session".to_owned(), weight: Weight::Primary,
                    icon: rsx! { FolderOpen { size: 16, color: "#fff" } }, on_press: move |()| open(Open::Session(id)) }
            }
            // Where it is: every stage, the current one picked.
            div { style: "display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 6px;",
                for (i, s) in stages.iter().enumerate() {
                    {
                        let (bg, ink, border) = if i == at { (PICK_BG, INK, PICK) } else if i < at { (SHEET_2, INK_2, RULE) } else { ("transparent", INK_3, RULE) };
                        rsx! {
                            div { key: "{i}", style: "height: 48px; border-radius: 10px; border: 1px solid {border}; background: {bg}; \
                                                     display: flex; align-items: center; justify-content: center; color: {ink}; \
                                                     {META} font-weight: 650;",
                                "{s.label()}"
                            }
                        }
                    }
                }
            }
            div { style: "display: flex; flex-direction: {direction}; gap: 18px; margin-top: 22px; align-items: flex-start;",
                div { style: "flex: 1 1 0; min-width: 0; width: 100%;",
                    Panel { title: "Song",
                        match &song {
                            Some(s) => {
                                let sid = s.id;
                                rsx! {
                                    button { class: "lp",
                                        style: "min-height: 56px; display: flex; align-items: center; gap: 12px; border: none; border-top: 1px solid {RULE}; \
                                                background: transparent; color: {INK}; font-family: {FONT}; text-align: left; padding: 0;",
                                        onclick: move |_| nav.go(Route::Song(sid, None)),
                                        Swatch { colour: s.colour.to_owned(), height: 32 }
                                        div { style: "flex: 1; display: flex; flex-direction: column; gap: 2px;",
                                            span { style: "{BODY}", "{s.title}" }
                                            span { style: "{META} color: {INK_3};", "{s.arrangements.len()} arrangements · {s.artist}" }
                                        }
                                    }
                                }
                            }
                            None => rsx! {
                                span { style: "{META} color: {INK_3}; line-height: 1.5; padding: 12px 0; border-top: 1px solid {RULE};",
                                    "Not a song yet. Make it one to give it arrangements, charts and a place on setlists."
                                }
                            },
                        }
                    }
                }
                div { style: if wide { "flex: 0 0 380px; min-width: 0;" } else { "width: 100%;" },
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
