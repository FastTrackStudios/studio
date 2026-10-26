//! A song: the hub. Its arrangements as tabs, each one's facts and form
//! over the song's shared sections, the chart's shape, the preset each of
//! my rigs plays for it, everywhere it is used, its files, and the
//! conversation.

use dioxus::prelude::*;
use lucide_dioxus::{AudioWaveform, Disc3, FileMusic, FolderOpen, ListMusic, Plus, Share2};

use super::home::Page;
use super::kit::*;
use super::{Open, Route, open, use_library, use_nav, use_wide};
use crate::model::{Anchor, ArrangementId, AssetKind, Library, Piece, Song, SongId, Step, clock};

/// A section's colour by what it is, so a form reads at a glance.
fn section_colour(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.starts_with("chorus") {
        "#60a5fa"
    } else if n.starts_with("verse") {
        "#34d399"
    } else if n.starts_with("pre") {
        "#fbbf24"
    } else if n.starts_with("bridge") {
        "#c084fc"
    } else {
        "#71717a"
    }
}

#[component]
pub(crate) fn SongPage(id: SongId, arrangement: Option<ArrangementId>) -> Element {
    let lib = use_library();
    let mut nav = use_nav();
    let wide = use_wide();
    let l = lib.read();
    let Some(song) = l.song(id).cloned() else {
        return rsx! { Page { p { style: "{BODY} color: {INK_2}; padding-top: 40px;", "This song was deleted or has not arrived yet." } } };
    };
    let current = arrangement
        .and_then(|a| song.arrangement(a))
        .unwrap_or(&song.arrangements[0])
        .clone();
    let bars = song.bars(&current);
    let length = clock(song.seconds(&current));
    let credits = [song.artist.clone(), song.writers.join(", ")]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let ccli = song
        .ccli
        .clone()
        .map(|c| format!("CCLI {c}"))
        .unwrap_or_default();
    let direction = if wide { "row" } else { "column" };
    let session = current.session;
    let tabs: Vec<(ArrangementId, String, String)> = song
        .arrangements
        .iter()
        .map(|a| (a.id, a.name.clone(), clock(song.seconds(a))))
        .collect();
    rsx! {
        Page {
            div { style: "display: flex; align-items: flex-end; gap: 16px; padding: 24px 0 18px; flex-wrap: wrap;",
                div { style: "flex: 1 1 360px; min-width: 0; display: flex; align-items: center; gap: 16px;",
                    span { style: "width: 6px; height: 56px; border-radius: 3px; background: {song.colour}; flex-shrink: 0;" }
                    div { style: "min-width: 0; display: flex; flex-direction: column; gap: 6px;",
                        span { style: "{TITLE}", "{song.title}" }
                        span { style: "{META} color: {INK_3};", "{credits}" }
                        div { style: "display: flex; gap: 6px; flex-wrap: wrap; align-items: center;",
                            for t in song.tags.iter() { Chip { key: "{t}", text: t.clone() } }
                            if !ccli.is_empty() { span { style: "{META} color: {INK_3}; margin-left: 4px;", "{ccli}" } }
                        }
                    }
                }
                div { style: "display: flex; gap: 10px;",
                    if let Some(p) = session {
                        Button { label: "Open session".to_owned(), weight: Weight::Secondary,
                            icon: rsx! { FolderOpen { size: 16, color: INK } }, on_press: move |()| open(Open::Session(p)) }
                    }
                    Button { label: "Share chart".to_owned(), weight: Weight::Secondary, icon: rsx! { Share2 { size: 16, color: INK } }, on_press: move |()| {} }
                }
            }
            // The arrangements: one song, several ways to play it.
            div { style: "display: flex; gap: 8px; flex-wrap: wrap; border-bottom: 1px solid {RULE_STRONG}; padding-bottom: 12px;",
                for (aid, name, len) in tabs {
                    {
                        let on = aid == current.id;
                        let (bg, border, ink) = if on { (PICK_BG, PICK, INK) } else { ("transparent", RULE_STRONG, INK_2) };
                        rsx! {
                            button {
                                key: "{aid.0}",
                                class: "lp",
                                style: "height: 48px; padding: 0 16px; border-radius: 10px; border: 1px solid {border}; \
                                        background: {bg}; color: {ink}; font-family: {FONT}; display: flex; \
                                        align-items: center; gap: 10px;",
                                onclick: move |_| nav.route.set(Route::Song(id, Some(aid))),
                                span { style: "font-size: 15px; font-weight: 650;", "{name}" }
                                span { style: "{META} color: {INK_3};", "{len}" }
                            }
                        }
                    }
                }
                button {
                    class: "lp",
                    style: "height: 48px; padding: 0 14px; border-radius: 10px; border: 1px dashed {DIM}; background: transparent; \
                            color: {INK_2}; font-family: {FONT}; display: flex; align-items: center; gap: 8px; font-size: 15px;",
                    Plus { size: 16, color: INK_2 }
                    "Arrangement"
                }
            }
            div { style: "display: flex; flex-direction: {direction}; gap: 28px; align-items: flex-start;",
                div { style: "flex: 1 1 0; min-width: 0; width: 100%;",
                    // The arrangement's facts, in one quiet line of figures.
                    div { style: "display: flex; flex-wrap: wrap; gap: 36px; padding: 22px 0 6px;",
                        Fact { label: "Key", value: current.key.0.clone() }
                        Fact { label: "Tempo", value: format!("{:.0} BPM", current.time.bpm) }
                        Fact { label: "Meter", value: format!("{}/4", current.time.beats_per_bar) }
                        Fact { label: "Length", value: length.clone() }
                        Fact { label: "Bars", value: bars.to_string() }
                    }
                    Heading { text: "Form".to_owned() }
                    FormStrip { song: song.clone(), form: current.form.clone() }
                    Heading { text: "Chart".to_owned(),
                        trailing: rsx! { ReadyMark { what: "Keyflow chart", ready: current.chart } } }
                    ChartOutline { song: song.clone(), form: current.form.clone(), home_key: current.key.0.clone() }
                }
                div { style: if wide { "flex: 0 0 340px; min-width: 0;" } else { "width: 100%;" },
                    SongAside { song: song.clone(), arrangement: current.id }
                }
            }
        }
    }
}

#[component]
fn Fact(label: &'static str, value: String) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 4px;",
            span { style: "{COLUMN} color: {INK_3};", "{label}" }
            span { style: "font-size: 22px; font-weight: 700; letter-spacing: -0.01em;", "{value}" }
        }
    }
}

/// The form as one bar: each step a block as wide as the bars it plays.
#[component]
fn FormStrip(song: Song, form: Vec<Step>) -> Element {
    let blocks: Vec<(String, u32, u8, &'static str)> = form
        .iter()
        .filter_map(|step| {
            let s = song.section(step.section)?;
            (s.bars > 0).then(|| {
                (
                    s.name.clone(),
                    s.bars * u32::from(step.times),
                    step.times,
                    section_colour(&s.name),
                )
            })
        })
        .collect();
    rsx! {
        div { style: "display: flex; gap: 3px; height: 56px;",
            for (i, (name, bars, times, colour)) in blocks.into_iter().enumerate() {
                div {
                    key: "{i}",
                    style: "flex: {bars} 1 0; min-width: 0; border-radius: 8px; background: {SHEET_2}; \
                            border: 1px solid {RULE}; border-top: 3px solid {colour}; padding: 6px 8px; \
                            display: flex; flex-direction: column; justify-content: center; gap: 2px; overflow: hidden;",
                    span { style: "font-size: 12px; font-weight: 700; {ONE_LINE}", "{name}" }
                    span { style: "font-size: 11px; color: {INK_3}; {ONE_LINE}",
                        if times > 1 { "×{times} · {bars}" } else { "{bars}" }
                    }
                }
            }
        }
    }
}

/// The chart's shape for this arrangement: each section in order, its bars
/// as cells, four to a line, the way the band reads it. (The engraved
/// Keyflow chart opens from here once the chart panel mounts in the
/// library.)
#[component]
fn ChartOutline(song: Song, form: Vec<Step>, home_key: String) -> Element {
    let rows: Vec<(String, &'static str, u32, u8)> = form
        .iter()
        .filter_map(|step| {
            let s = song.section(step.section)?;
            (s.bars > 0).then(|| (s.name.clone(), section_colour(&s.name), s.bars, step.times))
        })
        .collect();
    rsx! {
        div { style: "border-radius: 14px; background: #f4f4f1; color: #18181b; padding: 22px 26px; \
                      display: flex; flex-direction: column; gap: 14px;",
            div { style: "display: flex; align-items: baseline; gap: 12px;",
                span { style: "font-size: 20px; font-weight: 750; font-family: Georgia, 'Times New Roman', serif;", "{song.title}" }
                span { style: "flex: 1;" }
                span { style: "font-size: 13px; font-weight: 600; color: #52525b;", "Key of {home_key}" }
            }
            for (i, (name, colour, bars, times)) in rows.into_iter().enumerate() {
                div { key: "{i}", style: "display: flex; flex-direction: column; gap: 6px;",
                    span { style: "font-size: 11px; font-weight: 800; letter-spacing: 0.08em; text-transform: uppercase; color: #3f3f46; \
                                   display: flex; align-items: center; gap: 8px;",
                        span { style: "width: 10px; height: 10px; border-radius: 2px; background: {colour};" }
                        if times > 1 { "{name} ×{times}" } else { "{name}" }
                    }
                    div { style: "display: grid; grid-template-columns: repeat(8, minmax(0, 1fr)); border-left: 2px solid #18181b;",
                        for b in 0..bars.min(16) {
                            span { key: "{b}", style: "height: 30px; border-right: 1px solid #a1a1aa; border-bottom: 1px solid #e4e4e7;" }
                        }
                    }
                }
            }
        }
    }
}

/// Beside the song: my rigs' presets for this arrangement, where it is
/// used, its files, the conversation.
#[component]
fn SongAside(song: Song, arrangement: ArrangementId) -> Element {
    let lib = use_library();
    let nav = use_nav();
    let l = lib.read();
    let presets: Vec<(String, String, &'static str)> = l
        .rigs
        .iter()
        .filter(|r| r.owner == l.me)
        .map(|r| match r.preset(song.id, arrangement) {
            Some(p) if p.arrangement.is_some() => (
                r.kind.label().to_owned(),
                p.patch.clone(),
                "this arrangement",
            ),
            Some(p) => (
                r.kind.label().to_owned(),
                p.patch.clone(),
                "every arrangement",
            ),
            None => (
                r.kind.label().to_owned(),
                r.default_patch.clone(),
                "profile default",
            ),
        })
        .collect();
    let used = used_in(&l, song.id);
    let threads: Vec<_> = l
        .threads_on(&Anchor::Song(song.id))
        .into_iter()
        .cloned()
        .collect();
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 18px; margin-top: 22px;",
            Panel { title: "Your rigs",
                for (i, (rig, patch, scope)) in presets.into_iter().enumerate() {
                    div { key: "{i}", style: "min-height: 52px; display: flex; align-items: center; gap: 12px; border-top: 1px solid {RULE};",
                        span { style: "{META} color: {INK_2}; width: 56px;", "{rig}" }
                        div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                            span { style: "{BODY} font-size: 14px; {ONE_LINE}", "{patch}" }
                            span { style: "font-size: 11px; font-weight: 650; color: {INK_3};", "{scope}" }
                        }
                    }
                }
            }
            Panel { title: "Used in",
                for (i, (kind, title, detail, route)) in used.into_iter().enumerate() {
                    button { key: "{i}", class: "lp",
                        style: "min-height: 52px; display: flex; align-items: center; gap: 12px; border: none; border-top: 1px solid {RULE}; \
                                background: transparent; color: {INK}; font-family: {FONT}; text-align: left; padding: 0;",
                        onclick: move |_| nav.go(route),
                        match kind {
                            0 => rsx! { ListMusic { size: 18, color: INK_3 } },
                            1 => rsx! { Disc3 { size: 18, color: INK_3 } },
                            _ => rsx! { FolderOpen { size: 18, color: INK_3 } },
                        }
                        div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px;",
                            span { style: "{BODY} font-size: 14px; {ONE_LINE}", "{title}" }
                            span { style: "{META} color: {INK_3}; {ONE_LINE}", "{detail}" }
                        }
                    }
                }
            }
            Panel { title: "Files",
                for (i, a) in song.assets.iter().enumerate() {
                    div { key: "{i}", style: "min-height: 44px; display: flex; align-items: center; gap: 12px; border-top: 1px solid {RULE};",
                        match a.kind {
                            AssetKind::Chart | AssetKind::Lyrics | AssetKind::Pdf => rsx! { FileMusic { size: 17, color: INK_3 } },
                            _ => rsx! { AudioWaveform { size: 17, color: INK_3 } },
                        }
                        span { style: "flex: 1; min-width: 0; {META} color: {INK}; {ONE_LINE}", "{a.name}" }
                        span { style: "{META} color: {INK_3};", "{size(a.bytes)}" }
                        ReadyMark { what: "", ready: a.ready }
                    }
                }
            }
            Panel { title: "Conversation",
                for (i, t) in threads.iter().enumerate() {
                    for (j, c) in t.comments.iter().enumerate() {
                        div { key: "{i}-{j}", style: "display: flex; gap: 10px; align-items: flex-start; padding-top: 12px; border-top: 1px solid {RULE};",
                            Avatar { name: c.author.clone(), size: 28 }
                            div { style: "flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; padding-bottom: 10px;",
                                span { style: "{META} color: {INK_2}; font-weight: 650;", "{c.author} " span { style: "color: {INK_3}; font-weight: 500;", "{c.when}" } }
                                span { style: "font-size: 14px; line-height: 1.45;", "{c.text}" }
                            }
                        }
                    }
                }
                if threads.is_empty() { span { style: "{META} color: {INK_3}; padding-top: 10px; border-top: 1px solid {RULE};", "No comments yet." } }
            }
        }
    }
}

/// Where a song is played and produced: (kind, title, detail, route),
/// kind 0 a setlist, 1 an album, 2 a session.
fn used_in(l: &Library, song: SongId) -> Vec<(u8, String, String, Route)> {
    let mut used = Vec::new();
    for s in l.setlists_with(song) {
        let arrangement = s.entries.iter().find_map(|e| match e.piece {
            Piece::Song {
                song: id,
                arrangement,
            } if id == song => l.song(id)?.arrangement(arrangement).map(|a| a.name.clone()),
            _ => None,
        });
        used.push((
            0,
            s.title.clone(),
            format!("Setlist · {}", arrangement.unwrap_or_default()),
            Route::Setlist(s.id, None),
        ));
    }
    for a in l.albums_with(song) {
        used.push((
            1,
            a.title.clone(),
            format!("Album · {}", a.year),
            Route::Album(a.id),
        ));
    }
    for p in l.sessions.iter().filter(|p| p.song == Some(song)) {
        used.push((
            2,
            p.title.clone(),
            format!("Session · {}", p.stage.label()),
            Route::Session(p.id),
        ));
    }
    used
}

/// A titled block of the aside.
#[component]
pub(crate) fn Panel(title: &'static str, children: Element) -> Element {
    rsx! {
        div { style: "border-radius: 14px; border: 1px solid {RULE_STRONG}; background: {SHEET}; padding: 14px 16px 6px; \
                      display: flex; flex-direction: column;",
            span { style: "{HEADING} font-size: 15px; padding-bottom: 10px;", "{title}" }
            {children}
        }
    }
}
