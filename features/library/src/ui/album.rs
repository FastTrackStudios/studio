//! An album: its tracks, each an arrangement and the session that
//! produces it, and how far each has come.

use dioxus::prelude::*;
use lucide_dioxus::{Play, Shuffle};

use super::home::Page;
use super::kit::*;
use super::lists::StageBar;
use super::player::use_player;
use super::{Route, use_library, use_nav, use_wide};
use crate::model::{AlbumId, Stage, clock};

#[component]
pub(crate) fn AlbumPage(id: AlbumId) -> Element {
    let lib = use_library();
    let player = use_player();
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
    let queue = l.album_queue(id);
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
                    if !queue.is_empty() {
                        div { style: "display: flex; gap: 10px; margin-top: 8px;",
                            Button { label: "Play".to_owned(), weight: Weight::Primary,
                                icon: rsx! { Play { size: 16, color: "#fff", fill: "#fff" } },
                                on_press: { let q = queue.clone(); let t = album.title.clone(); move |()| player.play(q.clone(), 0, &t) } }
                            Button { label: "Shuffle".to_owned(), weight: Weight::Secondary,
                                icon: rsx! { Shuffle { size: 16, color: INK } },
                                on_press: { let q = queue.clone(); let t = album.title.clone(); move |()| { player.play(q.clone(), 0, &t); player.toggle_shuffle(); } } }
                        }
                    }
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
