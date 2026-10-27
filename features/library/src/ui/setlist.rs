//! A setlist: what the show plays in order, the rigs attached to it (each
//! a part, one per rig per player), the sound each part plays for each
//! entry and where that sound comes from, and the conversation.

use dioxus::prelude::*;
use lucide_dioxus::{
    ChevronDown, ChevronUp, Guitar, Headphones, MessageSquare, Mic, Piano, Play, Plus, Share2,
    Trash2,
};
use uuid::Uuid;

use super::home::Page;
use super::kit::*;
use super::{Open, Route, edit, open, use_library, use_nav, use_wide};
use crate::model::{
    Anchor, ArrangementId, Edit, Entry, EntryId, From, Library, Part, PartId, Piece, Ready, RigId,
    RigKind, SetlistId, SongId, clock, minutes, sound,
};

/// What one row of the set says.
#[derive(Clone, PartialEq)]
struct Line {
    entry: Entry,
    title: String,
    detail: String,
    colour: String,
    key: Option<(String, bool)>,
    bpm: Option<(String, bool)>,
    starts: String,
    length: String,
    chart: Option<Ready>,
    sound: Option<(String, From)>,
    comments: usize,
}

fn lines(l: &Library, set: SetlistId, part: Option<&Part>) -> Vec<Line> {
    let Some(set) = l.setlist(set) else {
        return Vec::new();
    };
    let rig = part.and_then(|p| l.rig(p.rig));
    let mut at = 0.0;
    set.entries
        .iter()
        .map(|e| {
            let seconds = l.entry_seconds(e);
            let starts = clock(at);
            at += seconds;
            let comments = l
                .threads_on(&Anchor::Entry(set.id, e.id))
                .iter()
                .map(|t| t.comments.len())
                .sum();
            let sound = part.zip(rig).map(|(p, r)| {
                let s = sound(p, r, e);
                (s.patch, s.from)
            });
            match &e.piece {
                Piece::Song { song, arrangement } => {
                    let s = l.song(*song);
                    let a = s.and_then(|s| s.arrangement(*arrangement));
                    Line {
                        entry: e.clone(),
                        title: s.map_or_else(|| "Missing song".into(), |s| s.title.clone()),
                        detail: a.map_or_else(String::new, |a| a.name.clone()),
                        colour: s.map_or(DIM, |s| s.colour).to_owned(),
                        key: a.map(|a| {
                            e.key
                                .as_ref()
                                .map_or_else(|| (a.key.0.clone(), false), |k| (k.0.clone(), true))
                        }),
                        bpm: a.map(|a| {
                            e.bpm.map_or_else(
                                || (format!("{:.0}", a.time.bpm), false),
                                |b| (format!("{b:.0}"), true),
                            )
                        }),
                        starts,
                        length: clock(seconds),
                        chart: a.map(|a| a.chart),
                        sound,
                        comments,
                    }
                }
                Piece::Other { kind, title, .. } => Line {
                    entry: e.clone(),
                    title: title.clone(),
                    detail: kind.label().to_owned(),
                    colour: "transparent".into(),
                    key: None,
                    bpm: None,
                    starts,
                    length: clock(seconds),
                    chart: None,
                    sound,
                    comments,
                },
            }
        })
        .collect()
}

#[component]
pub(crate) fn SetlistPage(id: SetlistId, entry: Option<EntryId>) -> Element {
    let lib = use_library();
    let wide = use_wide();
    // Hooks before any early return: a set that arrives later must find
    // the same hooks.
    let mut chosen = use_signal(|| None::<PartId>);
    let mut editing = use_signal(|| false);
    let player = super::player::use_player();
    let l = lib.read();
    let listen = l.setlist_queue(id);
    let Some(set) = l.setlist(id).cloned() else {
        return rsx! { Page { p { style: "{BODY} color: {INK_2}; padding-top: 40px;", "This setlist was deleted or has not arrived yet." } } };
    };
    let parts: Vec<Part> = l.parts_of(id).into_iter().cloned().collect();
    // The part whose sounds the table shows: the one picked, else mine
    // (first).
    let picked = chosen().or_else(|| parts.first().map(|p| p.id));
    let part = picked.and_then(|c| parts.iter().find(|p| p.id == c));
    let rows = lines(&l, id, part);
    let length = minutes(l.setlist_seconds(&set));
    let space = l
        .space(set.space)
        .map(|s| s.name.clone())
        .unwrap_or_default();
    let mine: Option<(PartId, String)> = parts
        .iter()
        .find(|p| p.player == l.me)
        .and_then(|p| Some((p.id, l.rig(p.rig)?.kind.label().to_owned())));
    let when = [set.when.clone(), set.venue.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let selected = entry
        .and_then(|e| rows.iter().find(|r| r.entry.id == e))
        .cloned();
    let in_project = l.project_of_setlist(id).map(|p| p.id);
    let direction = if wide { "row" } else { "column" };
    let edit_label = if editing() { "Done" } else { "Edit" };
    rsx! {
        Page {
            div { style: "display: flex; align-items: flex-end; gap: 16px; padding: 24px 0 6px; flex-wrap: wrap;",
                div { style: "flex: 1 1 360px; min-width: 0; display: flex; flex-direction: column; gap: 8px;",
                    if set.tonight {
                        span { style: "display: flex; align-items: center; gap: 8px; color: {LIVE}; {META} font-weight: 700;",
                            span { style: "width: 8px; height: 8px; border-radius: 4px; background: {LIVE};" }
                            "{when}"
                        }
                    } else if !when.is_empty() {
                        span { style: "{META} color: {INK_2}; font-weight: 650;", "{when}" }
                    }
                    super::project::InProject { id: in_project }
                    span { style: "{TITLE}", "{set.title}" }
                    span { style: "{META} color: {INK_3};", "{set.entries.len()} pieces · {length} · {space} · edited by {set.edited.by}, {set.edited.when}" }
                }
                div { style: "display: flex; gap: 10px;",
                    if let Some((part, kind)) = mine {
                        Button { label: format!("Play {kind}"), weight: Weight::Primary,
                            icon: rsx! { Play { size: 16, color: "#fff", fill: "#fff" } },
                            on_press: move |()| open(Open::Part(part)) }
                    }
                    if !listen.is_empty() {
                        Button { label: "Listen".to_owned(), weight: Weight::Secondary,
                            icon: rsx! { Headphones { size: 16, color: INK } },
                            on_press: { let t = set.title.clone(); move |()| player.play(listen.clone(), 0, &t) } }
                    }
                    Button { label: "Rehearse".to_owned(), weight: Weight::Secondary, on_press: move |()| open(Open::Rehearse(id)) }
                    Button { label: "Share".to_owned(), weight: Weight::Secondary, icon: rsx! { Share2 { size: 16, color: INK } }, on_press: move |()| {} }
                }
            }
            Heading { text: "Rigs on this set".to_owned() }
            div { style: "display: flex; flex-wrap: wrap; gap: 10px;",
                for p in parts.iter().cloned() {
                    PartTile { key: "{p.id.0}", on: picked == Some(p.id), part: p.clone(), on_press: move |()| chosen.set(Some(p.id)) }
                }
                AttachTile { setlist: id }
            }
            div { style: "display: flex; flex-direction: {direction}; gap: 28px; align-items: flex-start; margin-top: 8px;",
                div { style: "flex: 1 1 0; min-width: 0; width: 100%;",
                    Heading { text: "Running order".to_owned(),
                        trailing: rsx! { TextButton { label: edit_label.to_owned(), on_press: move |()| editing.toggle() } } }
                    if editing() {
                        EditOrder { id, rows }
                    } else {
                        EntryTable { id, rows, wide, selected: entry, part_chosen: part.is_some() }
                    }
                }
                div { style: if wide { "flex: 0 0 340px; min-width: 0;" } else { "width: 100%;" },
                    Aside { id, line: selected }
                }
            }
        }
    }
}

fn rig_icon(kind: RigKind, colour: &'static str) -> Element {
    match kind {
        RigKind::Keys => rsx! { Piano { size: 20, color: colour } },
        RigKind::Vocals => rsx! { Mic { size: 20, color: colour } },
        _ => rsx! { Guitar { size: 20, color: colour } },
    }
}

/// One attached rig: what it is, whose, and how it stands. Picked, the
/// table shows its sounds.
#[component]
fn PartTile(part: Part, on: bool, on_press: EventHandler<()>) -> Element {
    let lib = use_library();
    let l = lib.read();
    let Some(rig) = l.rig(part.rig) else {
        return rsx! {};
    };
    let mine = part.player == l.me;
    let set = part.setlist;
    let changed = part.sounds.len();
    let (border, bg) = if on {
        (PICK, PICK_BG)
    } else {
        (RULE_STRONG, "transparent")
    };
    let detail = if changed == 0 {
        "Song presets".to_owned()
    } else {
        format!("{changed} set for this show")
    };
    let kind = rig.kind;
    let name = rig.kind.label();
    let profile = rig.profile.clone();
    let _ = set;
    rsx! {
        button {
            class: "lp",
            style: "min-width: 220px; height: 72px; display: flex; align-items: center; gap: 12px; padding: 0 14px; \
                    border-radius: 12px; border: 1px solid {border}; background: {bg}; color: {INK}; \
                    font-family: {FONT}; text-align: left;",
            onclick: move |_| on_press.call(()),
            span { style: "width: 40px; height: 40px; border-radius: 10px; background: {SHEET_2}; border: 1px solid {RULE}; \
                           display: flex; align-items: center; justify-content: center; flex-shrink: 0;",
                {rig_icon(kind, if on { PICK } else { INK_2 })}
            }
            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                span { style: "{BODY}", "{name} · {profile}" }
                span { style: "{META} color: {INK_3}; {ONE_LINE}", "{detail}" }
            }
            div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 6px;",
                Avatar { name: part.player.clone(), size: 24 }
                if mine { SyncMark { sync: part.sync, quiet: true } }
            }
        }
    }
}

/// Attach another rig to this set: pressed, it lists my rigs that are not
/// on it yet, and a rig pressed there becomes a part.
#[component]
fn AttachTile(setlist: SetlistId) -> Element {
    let lib = use_library();
    let mut open = use_signal(|| false);
    let l = lib.read();
    let me = l.me.clone();
    let free: Vec<(RigId, RigKind, String)> = l
        .rigs
        .iter()
        .filter(|r| r.owner == l.me)
        .filter(|r| {
            !l.parts
                .iter()
                .any(|p| p.setlist == setlist && p.rig == r.id && p.player == l.me)
        })
        .map(|r| (r.id, r.kind, r.profile.clone()))
        .collect();
    let tile = "height: 72px; display: flex; align-items: center; gap: 10px; padding: 0 18px; \
                border-radius: 12px; background: transparent; font-family: {FONT};";
    rsx! {
        if open() {
            for (rig, kind, profile) in free.iter().cloned() {
                {
                    let me = me.clone();
                    rsx! {
                        button {
                            key: "{rig.0}",
                            class: "lp",
                            style: "{tile} border: 1px solid {PICK}; color: {INK}; {BODY}",
                            onclick: move |_| {
                                edit(Edit::AttachRig { setlist, rig, player: me.clone(), part: PartId(Uuid::new_v4()) });
                                open.set(false);
                            },
                            {rig_icon(kind, PICK)}
                            "{kind.label()} · {profile}"
                        }
                    }
                }
            }
            if free.is_empty() {
                span { style: "{META} color: {INK_3}; align-self: center; padding: 0 8px;", "Every rig of yours is on this set." }
            }
            button {
                class: "lp",
                style: "{tile} border: 1px solid {RULE_STRONG}; color: {INK_2}; {BODY}",
                onclick: move |_| open.set(false),
                "Cancel"
            }
        } else {
            button {
                class: "lp",
                style: "{tile} border: 1px dashed {DIM}; color: {INK_2}; {BODY}",
                onclick: move |_| open.set(true),
                Plus { size: 18, color: INK_2 }
                "Attach rig"
            }
        }
    }
}

/// The running order, editable: each piece moves up or down or comes out,
/// and a song from the library goes on the end.
#[component]
fn EditOrder(id: SetlistId, rows: Vec<Line>) -> Element {
    let lib = use_library();
    let mut adding = use_signal(|| false);
    let last = rows.len().saturating_sub(1);
    let songs: Vec<(SongId, String, &'static str, Vec<(ArrangementId, String)>)> = lib
        .read()
        .songs
        .iter()
        .map(|s| {
            (
                s.id,
                s.title.clone(),
                s.colour,
                s.arrangements
                    .iter()
                    .map(|a| (a.id, a.name.clone()))
                    .collect(),
            )
        })
        .collect();
    let icon_button = "width: 48px; height: 48px; border: none; border-radius: 10px; background: transparent; \
                       display: flex; align-items: center; justify-content: center;";
    rsx! {
        div { style: "border-top: 1px solid {RULE_STRONG};",
            for (i, line) in rows.into_iter().enumerate() {
                {
                    let entry = line.entry.id;
                    let song = !matches!(line.entry.piece, Piece::Other { .. });
                    let (up_ink, down_ink) = (if i == 0 { DIM } else { INK_2 }, if i == last { DIM } else { INK_2 });
                    rsx! {
                        div { key: "{entry.0}",
                            style: "min-height: {ROW}px; display: flex; align-items: center; gap: 12px; padding: 4px 4px 4px 12px; \
                                    border-bottom: 1px solid {RULE};",
                            span { style: "width: 22px; {META} color: {INK_3}; text-align: right;", "{i + 1}" }
                            if song { Swatch { colour: line.colour.clone(), height: 28 } }
                            else { span { style: "width: 4px; height: 28px; border-radius: 2px; border: 1px dashed {DIM}; flex-shrink: 0;" } }
                            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                                span { style: "{BODY} {ONE_LINE}", "{line.title}" }
                                span { style: "{META} color: {INK_3}; {ONE_LINE}", "{line.detail}" }
                            }
                            button { class: "lp", "aria-label": "Move up", disabled: i == 0, style: "{icon_button}",
                                onclick: move |_| edit(Edit::MoveEntry { setlist: id, entry, to: i.saturating_sub(1) }),
                                ChevronUp { size: 20, color: up_ink } }
                            button { class: "lp", "aria-label": "Move down", disabled: i == last, style: "{icon_button}",
                                onclick: move |_| edit(Edit::MoveEntry { setlist: id, entry, to: i + 1 }),
                                ChevronDown { size: 20, color: down_ink } }
                            button { class: "lp", "aria-label": "Remove", style: "{icon_button}",
                                onclick: move |_| edit(Edit::RemoveEntry { setlist: id, entry }),
                                Trash2 { size: 18, color: VOID } }
                        }
                    }
                }
            }
            if adding() {
                div { style: "padding: 8px 0;",
                    for (sid, title, colour, arrangements) in songs {
                        div { key: "{sid.0}", style: "min-height: {ROW}px; display: flex; align-items: center; gap: 12px; padding: 4px 12px; border-bottom: 1px solid {RULE}; flex-wrap: wrap;",
                            Swatch { colour: colour.to_owned(), height: 28 }
                            span { style: "flex: 1 1 160px; min-width: 0; {BODY} {ONE_LINE}", "{title}" }
                            for (aid, name) in arrangements {
                                button { key: "{aid.0}", class: "lp",
                                    style: "height: 40px; padding: 0 14px; border-radius: 10px; border: 1px solid {RULE_STRONG}; \
                                            background: transparent; color: {INK}; font-family: {FONT}; {META} font-weight: 650;",
                                    onclick: move |_| {
                                        edit(Edit::AddSong { setlist: id, entry: EntryId(Uuid::new_v4()), song: sid, arrangement: aid });
                                        adding.set(false);
                                    },
                                    "+ {name}"
                                }
                            }
                        }
                    }
                }
            }
            button {
                class: "lp",
                style: "margin-top: 10px; height: 48px; padding: 0 16px; border-radius: 10px; border: 1px dashed {DIM}; \
                        background: transparent; color: {INK_2}; font-family: {FONT}; {BODY} display: flex; align-items: center; gap: 8px;",
                onclick: move |_| adding.toggle(),
                Plus { size: 18, color: INK_2 }
                if adding() { "Close the library" } else { "Add a song" }
            }
        }
    }
}

#[component]
fn EntryTable(
    id: SetlistId,
    rows: Vec<Line>,
    wide: bool,
    selected: Option<EntryId>,
    part_chosen: bool,
) -> Element {
    let mut nav = use_nav();
    let columns = if wide {
        "36px minmax(0, 3fr) 52px 52px 56px minmax(0, 2fr) 36px"
    } else {
        "36px minmax(0, 1fr) 52px 56px minmax(0, 1fr)"
    };
    let names: Vec<&str> = if wide {
        vec!["", "Piece", "Key", "BPM", "Length", "Sound", ""]
    } else {
        vec!["", "Piece", "Key", "Length", "Sound"]
    };
    rsx! {
        div {
            style: "display: grid; grid-template-columns: {columns}; column-gap: 14px; align-items: center; \
                    height: 32px; padding: 0 12px; border-bottom: 1px solid {RULE_STRONG}; {COLUMN} color: {INK_3};",
            for (i, n) in names.into_iter().enumerate() { span { key: "{i}", "{n}" } }
        }
        for (i, line) in rows.into_iter().enumerate() {
            {
                let on = selected == Some(line.entry.id);
                let entry = line.entry.id;
                let song = !matches!(line.entry.piece, Piece::Other { .. });
                let bg = if on { PICK_BG } else { "transparent" };
                let title_ink = if song { INK } else { INK_2 };
                let edge = if on { PICK } else { "transparent" };
                rsx! {
                    button {
                        key: "{entry.0}",
                        class: "lp",
                        style: "width: 100%; min-height: {ROW + 4}px; display: grid; grid-template-columns: {columns}; \
                                column-gap: 14px; align-items: center; padding: 8px 12px; border: none; \
                                border-bottom: 1px solid {RULE}; background: {bg}; box-shadow: inset 2px 0 0 {edge}; \
                                color: {INK}; font-family: {FONT}; text-align: left;",
                        onclick: move |_| {
                            let to = Route::Setlist(id, if on { None } else { Some(entry) });
                            nav.route.set(to);
                        },
                        div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 2px;",
                            span { style: "{META} color: {INK_2}; font-weight: 650;", "{i + 1}" }
                            span { style: "font-size: 11px; color: {INK_3};", "{line.starts}" }
                        }
                        div { style: "display: flex; align-items: center; gap: 12px; min-width: 0;",
                            if song { Swatch { colour: line.colour.clone(), height: 32 } }
                            else { span { style: "width: 4px; height: 32px; border-radius: 2px; border: 1px dashed {DIM}; flex-shrink: 0;" } }
                            div { style: "min-width: 0; display: flex; flex-direction: column; gap: 3px;",
                                span { style: "{BODY} {ONE_LINE} color: {title_ink};", "{line.title}" }
                                span { style: "{META} color: {INK_3}; {ONE_LINE}",
                                    "{line.detail}"
                                    if line.chart == Some(Ready::Missing) { span { style: "color: {WARN};", " · no chart" } }
                                    if !line.entry.notes.is_empty() { " · {line.entry.notes}" }
                                }
                            }
                        }
                        match line.key.clone() {
                            Some((k, changed)) => rsx! { div { Chip { text: k, strong: changed } } },
                            None => rsx! { span {} },
                        }
                        if wide {
                            match line.bpm.clone() {
                                Some((b, changed)) => {
                                    let ink = if changed { PICK } else { INK_2 };
                                    rsx! { span { style: "{META} color: {ink}; font-weight: 650;", "{b}" } }
                                }
                                None => rsx! { span {} },
                            }
                        }
                        span { style: "{META} color: {INK_2};", "{line.length}" }
                        match line.sound.clone() {
                            Some((patch, from)) => rsx! { SoundCell { patch, from } },
                            None => rsx! { span { style: "{META} color: {INK_3};", if part_chosen { "" } else { "Attach a rig" } } },
                        }
                        if wide {
                            if line.comments > 0 {
                                span { style: "display: flex; align-items: center; gap: 4px; {META} color: {INK_2};",
                                    MessageSquare { size: 15, color: INK_2 }
                                    "{line.comments}"
                                }
                            } else { span {} }
                        }
                    }
                }
            }
        }
    }
}

/// A part's sound for an entry, and where it came from: set for this show
/// (blue), the song's preset, or the profile's default (quiet).
#[component]
fn SoundCell(patch: String, from: From) -> Element {
    let (colour, word) = match from {
        From::Part => (PICK, "this show"),
        From::SongPreset => (INK_2, "song preset"),
        From::Profile => (INK_3, "default"),
    };
    let ink = if from == From::Profile { INK_2 } else { INK };
    rsx! {
        div { style: "min-width: 0; display: flex; flex-direction: column; gap: 3px;",
            span { style: "{META} font-weight: 650; color: {ink}; {ONE_LINE}", "{patch}" }
            span { style: "font-size: 11px; font-weight: 650; color: {colour}; {ONE_LINE}", "{word}" }
        }
    }
}

/// With no piece picked, the set at a glance: what is missing before it
/// can be played (charts, audio), what this show changes (keys, tempos),
/// and who plays what. Each line says the thing, not a placeholder.
#[component]
fn SetReadiness(id: SetlistId) -> Element {
    let lib = use_library();
    let l = lib.read();
    let Some(set) = l.setlist(id) else {
        return rsx! {};
    };
    let songs: Vec<(String, Ready, Ready)> = set
        .entries
        .iter()
        .filter_map(|e| match e.piece {
            Piece::Song { song, arrangement } => {
                let s = l.song(song)?;
                let a = s.arrangement(arrangement)?;
                Some((s.title.clone(), a.chart, a.audio))
            }
            Piece::Other { .. } => None,
        })
        .collect();
    let total = songs.len();
    let no_chart: Vec<String> = songs
        .iter()
        .filter(|s| s.1 != Ready::Here)
        .map(|s| s.0.clone())
        .collect();
    let no_audio: Vec<String> = songs
        .iter()
        .filter(|s| s.2 != Ready::Here)
        .map(|s| s.0.clone())
        .collect();
    let keys = set.entries.iter().filter(|e| e.key.is_some()).count();
    let tempos = set.entries.iter().filter(|e| e.bpm.is_some()).count();
    let rigs: Vec<(String, String)> = l
        .parts_of(id)
        .iter()
        .filter_map(|p| Some((p.player.clone(), l.rig(p.rig)?.kind.label().to_owned())))
        .collect();
    let line = |ok: bool| if ok { INK_2 } else { WARN };
    let chart_line = if no_chart.is_empty() {
        format!("All {total} charts")
    } else {
        format!("No chart: {}", no_chart.join(", "))
    };
    let audio_line = if no_audio.is_empty() {
        "All audio on this iPad".to_owned()
    } else {
        format!("Not on this iPad: {}", no_audio.join(", "))
    };
    let changes = match (keys, tempos) {
        (0, 0) => "Played as arranged".to_owned(),
        (k, 0) => format!("{k} in another key"),
        (0, t) => format!("{t} at another tempo"),
        (k, t) => format!("{k} in another key, {t} at another tempo"),
    };
    rsx! {
        span { style: "{HEADING}", "Before the show" }
        div { style: "display: flex; flex-direction: column;",
            ReadyLine { label: "Charts", text: chart_line, ink: line(no_chart.is_empty()) }
            ReadyLine { label: "Audio", text: audio_line, ink: line(no_audio.is_empty()) }
            ReadyLine { label: "Changes", text: changes, ink: INK_2 }
        }
        if !rigs.is_empty() {
            div { style: "display: flex; flex-wrap: wrap; gap: 8px;",
                for (i, (who, rig)) in rigs.into_iter().enumerate() {
                    span { key: "{i}", style: "height: 32px; padding: 0 10px 0 4px; border-radius: 16px; border: 1px solid {RULE_STRONG}; \
                                             display: flex; align-items: center; gap: 8px; {META} color: {INK_2};",
                        Avatar { name: who.clone(), size: 24 }
                        "{who} · {rig}"
                    }
                }
            }
        }
    }
}

#[component]
fn ReadyLine(label: &'static str, text: String, ink: &'static str) -> Element {
    rsx! {
        div { style: "min-height: 40px; display: flex; align-items: center; gap: 12px; border-top: 1px solid {RULE}; padding: 8px 0;",
            span { style: "{META} color: {INK_3}; width: 64px; flex-shrink: 0;", "{label}" }
            span { style: "font-size: 14px; line-height: 1.4; color: {ink};", "{text}" }
        }
    }
}

/// Beside the table: the picked entry (its sounds on every rig, its
/// notes, the way to its song) and the conversation about it; with
/// nothing picked, the set's own.
#[component]
fn Aside(id: SetlistId, line: Option<Line>) -> Element {
    let lib = use_library();
    let nav = use_nav();
    let l = lib.read();
    let anchor = match &line {
        Some(line) => Anchor::Entry(id, line.entry.id),
        None => Anchor::Setlist(id),
    };
    let sounds: Vec<(String, String, String, From)> = match &line {
        Some(line) => l
            .parts_of(id)
            .into_iter()
            .filter_map(|p| {
                let rig = l.rig(p.rig)?;
                let s = sound(p, rig, &line.entry);
                Some((
                    rig.kind.label().to_owned(),
                    p.player.clone(),
                    s.patch,
                    s.from,
                ))
            })
            .collect(),
        None => Vec::new(),
    };
    let song = line.as_ref().and_then(|line| match line.entry.piece {
        Piece::Song { song, arrangement } => Some((song, arrangement)),
        Piece::Other { .. } => None,
    });
    rsx! {
        div { style: "border-radius: 14px; border: 1px solid {RULE_STRONG}; background: {SHEET}; padding: 18px; \
                      display: flex; flex-direction: column; gap: 14px; margin-top: 24px;",
            match &line {
                Some(line) => rsx! {
                    div { style: "display: flex; flex-direction: column; gap: 4px;",
                        span { style: "{HEADING}", "{line.title}" }
                        span { style: "{META} color: {INK_3};", "{line.detail} · starts at {line.starts} · {line.length}" }
                    }
                    if !sounds.is_empty() {
                        div { style: "display: flex; flex-direction: column;",
                            for (i, (kind, player, patch, from)) in sounds.into_iter().enumerate() {
                                div { key: "{i}", style: "height: 48px; display: flex; align-items: center; gap: 10px; border-top: 1px solid {RULE};",
                                    Avatar { name: player.clone(), size: 24 }
                                    span { style: "{META} color: {INK_2}; width: 54px;", "{kind}" }
                                    div { style: "flex: 1; min-width: 0;", SoundCell { patch, from } }
                                }
                            }
                        }
                    }
                    if let Some((s, a)) = song {
                        Button { label: "Open song".to_owned(), weight: Weight::Secondary, on_press: move |()| nav.go(Route::Song(s, Some(a))) }
                    }
                },
                None => rsx! { SetReadiness { id } },
            }
            super::talk::Conversation { about: anchor.clone(), exact: true }
        }
    }
}
