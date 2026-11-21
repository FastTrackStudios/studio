//! A song: its session, live. The tempo and meter, the form (its
//! sections, laid end to end in bars), and the chart, all in the song's
//! session document: the same document a DAW session of the song opens,
//! edited here by everyone who has the song open at once.

use dioxus::prelude::*;
use session_sync::SessionModel;

use crate::data::{self, FIFTHS};
use crate::icons::{I, Icon};
use crate::pieces::{Bar, Cover, Page, Tool};
use crate::songs::{self, Entry, Section, Song, use_library, use_song};
use crate::state::use_app;
use crate::Route;

/// Kinds of section a form is made of, for the add buttons.
const KINDS: [&str; 7] = ["Intro", "Verse", "Pre-Chorus", "Chorus", "Bridge", "Instrumental", "Outro"];

#[component]
pub fn SongPage(id: String) -> Element {
    let app = use_app();
    let lib = use_library();
    let entry = lib.get(&id);
    let song = use_song(&id, entry.clone());
    let Some(e) = entry else {
        return rsx! {
            Page {
                Bar { back: Route::Library { tag: "all".into() }, title: "Not found" }
                div { class: "body",
                    p { class: "empty", "No song {id} in the library." }
                }
            }
        };
    };
    let hue = data::str_hue(e.artist.as_deref().unwrap_or(&e.title));
    let play = e.builtin();
    let tools = rsx! {
        HereNow { song: song.clone() }
        if let Some(s) = play {
            Tool {
                label: "Play",
                icon: I::Play,
                onclick: move |_| app.play(vec![s], "Library".into(), s.title),
            }
        }
    };
    rsx! {
        Page { tint: Some(hue),
            Bar { back: Route::Library { tag: "all".into() }, title: e.title.clone(), tools }
            div { class: "body songpage",
                match song.model() {
                    None => rsx! {
                        p { class: "empty", "Opening the song's session…" }
                    },
                    Some(m) => rsx! {
                        Head { entry: e.clone(), song: song.clone(), model: m.clone(), lib_put: lib.clone() }
                        Form { song: song.clone(), model: m.clone() }
                        Chart { song: song.clone(), chart: m.chart.clone() }
                        Footer { song: song.clone() }
                    },
                }
            }
        }
    }
}

/// Who else has the song open.
#[component]
fn HereNow(song: Song) -> Element {
    let others = song.others();
    rsx! {
        span { class: "herenow",
            for (i , h) in others.iter().enumerate() {
                span {
                    key: "{i}",
                    class: "hereav",
                    style: "--h:{h.hue}",
                    title: "{h.name}",
                    "{h.name.chars().next().unwrap_or('?')}"
                }
            }
        }
    }
}

#[component]
fn Head(entry: Entry, song: Song, model: SessionModel, lib_put: songs::Library) -> Element {
    let bpm = songs::bpm(&model);
    let (beats, unit) = songs::meter(&model);
    let total = songs::sections(&model).iter().map(|s| s.bars).sum::<u32>();
    let secs = f64::from(total) * songs::bar_secs(&model);
    let key = model
        .project
        .ext
        .get("studio")
        .and_then(|x| x.get("key"))
        .cloned()
        .or(entry.key.clone())
        .unwrap_or_default();
    let set_key = {
        let (song, lib, entry) = (song.clone(), lib_put.clone(), entry.clone());
        move |k: String| {
            song.change(|m| {
                m.project.ext.entry("studio".into()).or_default().insert("key".into(), k.clone());
            });
            lib.put(&Entry { key: Some(k).filter(|k| !k.is_empty()), ..entry.clone() });
        }
    };
    let set_tempo = {
        let (song, lib, entry) = (song.clone(), lib_put.clone(), entry.clone());
        move |(b, beats, unit): (f64, u32, u32)| {
            if !(20.0..=400.0).contains(&b) {
                return;
            }
            song.change(|m| songs::set_tempo(m, b, beats, unit));
            lib.put(&Entry { tempo: Some(b.round() as u32), ..entry.clone() });
        }
    };
    let set_tempo2 = set_tempo.clone();
    rsx! {
        div { class: "songhead",
            Cover { title: entry.title.clone(), size: 72, hue: data::str_hue(entry.artist.as_deref().unwrap_or(&entry.title)) }
            div { class: "shmain",
                span { class: "shtitle", "{entry.title}" }
                if let Some(a) = &entry.artist {
                    span { class: "s1", "{a}" }
                }
                span { class: "s1", "{total} bars · {songs::clock(secs)}" }
            }
        }
        div { class: "facts",
            label { class: "fact",
                span { "Key" }
                select {
                    value: "{key}",
                    onchange: move |ev| set_key(ev.value()),
                    option { value: "", "—" }
                    for k in FIFTHS {
                        option { key: "{k}", value: "{k}", selected: k == key, "{k}" }
                    }
                }
            }
            label { class: "fact",
                span { "Tempo" }
                input {
                    r#type: "number",
                    inputmode: "decimal",
                    min: "20",
                    max: "400",
                    value: "{bpm}",
                    onchange: move |ev| {
                        if let Ok(b) = ev.value().parse::<f64>() {
                            set_tempo((b, beats, unit));
                        }
                    },
                }
            }
            label { class: "fact",
                span { "Meter" }
                select {
                    onchange: move |ev| {
                        let (b, u) = match ev.value().as_str() {
                            "3/4" => (3, 4),
                            "6/8" => (6, 8),
                            "2/4" => (2, 4),
                            _ => (4, 4),
                        };
                        set_tempo2((bpm, b, u));
                    },
                    for mtr in ["4/4", "3/4", "6/8", "2/4"] {
                        option { key: "{mtr}", value: mtr, selected: mtr == format!("{beats}/{unit}"), "{mtr}" }
                    }
                }
            }
        }
    }
}

/// The next name for a section of this kind (`Verse 3`).
fn next_name(list: &[Section], kind: &str) -> String {
    if matches!(kind, "Verse") {
        let n = list.iter().filter(|s| s.name.starts_with(kind)).count() + 1;
        format!("{kind} {n}")
    } else {
        kind.to_string()
    }
}

#[component]
fn Form(song: Song, model: SessionModel) -> Element {
    let list = songs::sections(&model);
    let bar = songs::bar_secs(&model);
    let mut picked = use_signal(|| None::<String>);
    let others = song.others();
    let mut pick = {
        let song = song.clone();
        move |id: Option<String>| {
            song.say_section(id.as_deref());
            picked.set(id);
        }
    };
    // Rewrite the form from a changed list of (id, name, bars).
    let relay = {
        let song = song.clone();
        move |f: Box<dyn FnOnce(&mut Vec<(String, String, u32)>)>| {
            song.change(|m| {
                let mut l: Vec<(String, String, u32)> =
                    songs::sections(m).into_iter().map(|s| (s.id, s.name, s.bars)).collect();
                f(&mut l);
                songs::lay_out(m, &l);
            });
        }
    };
    let sel = picked().and_then(|id| list.iter().find(|s| s.id == id).cloned());
    rsx! {
        h2 { class: "songh", "Form" }
        div { class: "form",
            for s in list.iter() {
                {
                    let id = s.id.clone();
                    let mut pick = pick.clone();
                    let on = picked().as_deref() == Some(s.id.as_str());
                    let here: Vec<_> = others.iter().filter(|o| o.section.as_deref() == Some(s.id.as_str())).cloned().collect();
                    rsx! {
                        button {
                            key: "{s.id}",
                            class: if on { "sect on" } else { "sect" },
                            style: "flex-grow:{s.bars};--h:{songs::section_hue(&s.name)}",
                            title: "{s.name}: bar {s.start_bar + 1}, {s.bars} bars",
                            onclick: move |_| pick(if on { None } else { Some(id.clone()) }),
                            span { class: "sname", "{songs::short(&s.name)}" }
                            span { class: "sbars", "{s.bars}" }
                            if !here.is_empty() {
                                span { class: "shere",
                                    for (i , h) in here.iter().enumerate() {
                                        span { key: "{i}", class: "hereav tiny", style: "--h:{h.hue}", "{h.name.chars().next().unwrap_or('?')}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        div { class: "addrow",
            for kind in KINDS {
                {
                    let relay = relay.clone();
                    let list = list.clone();
                    let mut pick = pick.clone();
                    let after = picked();
                    rsx! {
                        button {
                            key: "{kind}",
                            class: "addsect",
                            style: "--h:{songs::section_hue(kind)}",
                            onclick: move |_| {
                                let id = songs::new_id();
                                let name = next_name(&list, kind);
                                let bars = if matches!(kind, "Intro" | "Outro") { 4 } else { 8 };
                                let (id2, after) = (id.clone(), after.clone());
                                relay(Box::new(move |l| {
                                    let at = after.and_then(|a| l.iter().position(|x| x.0 == a)).map_or(l.len(), |i| i + 1);
                                    l.insert(at, (id2, name, bars));
                                }));
                                pick(Some(id));
                            },
                            Icon { name: I::Plus, size: 12, w: 2.6 }
                            "{kind}"
                        }
                    }
                }
            }
        }
        if let Some(s) = sel {
            SectionEditor { key: "{s.id}", section: s, relay: Callback::new(move |f| relay(f)), onclose: move |_| pick(None) }
        }
        div { class: "sectlist",
            for s in list.iter() {
                div { key: "{s.id}", class: "srow",
                    span { class: "sdot", style: "--h:{songs::section_hue(&s.name)}" }
                    span { class: "sname", "{s.name}" }
                    span { class: "s1", "bar {s.start_bar + 1}" }
                    span { class: "s1", "{songs::clock(f64::from(s.start_bar) * bar)}" }
                    span { class: "sbars", "{s.bars} bars" }
                }
            }
        }
    }
}

type Edit = Box<dyn FnOnce(&mut Vec<(String, String, u32)>)>;

#[component]
fn SectionEditor(section: Section, relay: Callback<Edit>, onclose: EventHandler<()>) -> Element {
    let mut name = use_signal(|| section.name.clone());
    let id = section.id.clone();
    let edit = move |f: fn(&mut Vec<(String, String, u32)>, usize)| {
        let id = id.clone();
        relay.call(Box::new(move |l| {
            if let Some(i) = l.iter().position(|x| x.0 == id) {
                f(l, i);
            }
        }));
    };
    let id2 = section.id.clone();
    let rename = move |v: String| {
        let id = id2.clone();
        relay.call(Box::new(move |l| {
            if let Some(x) = l.iter_mut().find(|x| x.0 == id) {
                x.1 = v;
            }
        }));
    };
    let id3 = section.id.clone();
    let del = edit.clone();
    rsx! {
        div { class: "secteditor", style: "--h:{songs::section_hue(&section.name)}",
            input {
                class: "sename",
                value: "{name}",
                aria_label: "Section name",
                oninput: move |ev| name.set(ev.value()),
                onchange: move |ev| {
                    let v = ev.value().trim().to_string();
                    if !v.is_empty() {
                        rename(v);
                    }
                },
            }
            div { class: "sebtns",
                button { class: "rb", aria_label: "Fewer bars", onclick: { let e = edit.clone(); move |_| e(|l, i| l[i].2 = l[i].2.saturating_sub(1).max(1)) }, "−" }
                span { class: "sebars", "{section.bars} bars" }
                button { class: "rb", aria_label: "More bars", onclick: { let e = edit.clone(); move |_| e(|l, i| l[i].2 += 1) }, "+" }
                button { class: "rb", aria_label: "Earlier", onclick: { let e = edit.clone(); move |_| e(|l, i| if i > 0 { l.swap(i, i - 1) }) }, "←" }
                button { class: "rb", aria_label: "Later", onclick: { let e = edit.clone(); move |_| e(|l, i| if i + 1 < l.len() { l.swap(i, i + 1) }) }, "→" }
                button {
                    class: "rb",
                    aria_label: "Repeat",
                    onclick: move |_| {
                        let id = id3.clone();
                        relay.call(Box::new(move |l| {
                            if let Some(i) = l.iter().position(|x| x.0 == id) {
                                let copy = (songs::new_id(), l[i].1.clone(), l[i].2);
                                l.insert(i + 1, copy);
                            }
                        }));
                    },
                    "⧉"
                }
                button { class: "rb danger", aria_label: "Remove", onclick: move |_| { del(|l, i| { l.remove(i); }); onclose.call(()); }, "✕" }
            }
        }
    }
}

#[component]
fn Chart(song: Song, chart: String) -> Element {
    // What this box last wrote: when the document holds anything else,
    // someone else typed, and the box takes it (so the next keystroke
    // diffs against their text, not over it).
    let mut draft = use_signal(|| chart.clone());
    let mut last = use_signal(|| chart.clone());
    if *last.peek() != chart {
        last.set(chart.clone());
        draft.set(chart.clone());
    }
    rsx! {
        h2 { class: "songh", "Chart" }
        textarea {
            class: "chart",
            spellcheck: false,
            value: "{draft}",
            oninput: move |ev| {
                let v = ev.value();
                draft.set(v.clone());
                last.set(v.clone());
                song.change(move |m| m.chart = v);
            },
        }
    }
}

#[component]
fn Footer(song: Song) -> Element {
    let peers = song.version().len();
    rsx! {
        p { class: "hint songfoot",
            "This song is a session document (schema {session_sync::SCHEMA}): its tempo, sections and chart, the same document a DAW session of it opens. "
            "Edits from {peers} "
            if peers == 1 { "device" } else { "devices" }
            ", merged field by field."
        }
    }
}
