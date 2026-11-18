//! The launcher's view: the vault's setlists down the left with what was
//! opened lately and the rigs, the chosen setlist's running order on the
//! right. Two columns that wrap into one where the pane is narrow (a
//! phone, a side dock), with no media query: Blitz draws it as a browser
//! does.
//!
//! Inline styles over the host's theme variables only (a linked view may
//! hold no `style` node).

use std::time::Duration;

use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::find::Rigs;
use crate::model::{Recent, clock};
use crate::{Found, Loaded};

const INK: &str = "var(--foreground, #e6e8eb)";
const DIM: &str = "var(--muted-foreground, #8b9099)";
const LINE: &str = "var(--border, #23262b)";
const WASH: &str = "var(--accent, #26292f)";
const GROUND: &str = "var(--background, #101114)";
const SIDE: &str = "var(--sidebar, #17181c)";
/// Ready: the theme's "safe" green. Not ready: a dim outline.
const READY: &str = "var(--signal-safe, #3ecf8e)";

/// What the engine has open, as the view says it.
#[derive(Clone, PartialEq, Default)]
struct Engine {
    up: bool,
    /// The open song's session path.
    path: Option<String>,
}

async fn engine_now() -> Engine {
    let Ok(state) = crate::host::run("DAW_STATE", json!({})).await else {
        return Engine::default();
    };
    Engine {
        up: state.get("up").and_then(Value::as_bool).unwrap_or(false),
        path: state
            .pointer("/session/path")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

/// What a press said, shown along the foot until the next one.
#[derive(Clone, PartialEq)]
enum Said {
    Nothing,
    Working(String),
    Done(String),
    Refused(String),
}

#[component]
pub fn LauncherView() -> Element {
    let mut loaded = use_signal(|| None::<Result<Loaded, String>>);
    let mut engine = use_signal(Engine::default);
    let mut recent = use_signal(Vec::<Recent>::new);
    let rigs = use_signal(crate::find::rigs);
    let mut chosen = use_signal(|| None::<String>);
    let mut said = use_signal(|| Said::Nothing);

    // The vault read again every few seconds; a signal set only when what
    // was read differs, so an unchanged vault redraws nothing.
    use_future(move || async move {
        loop {
            let now = crate::load().await;
            if loaded.peek().as_ref() != Some(&now) {
                loaded.set(Some(now));
            }
            let now = engine_now().await;
            if *engine.peek() != now {
                engine.set(now);
            }
            let now = crate::recent().await.unwrap_or_default();
            if *recent.peek() != now {
                recent.set(now);
            }
            futures_timer::Delay::new(Duration::from_secs(4)).await;
        }
    });

    let lists: Vec<Found> = match loaded() {
        None => return rsx! { Note { text: "Reading the vault…" } },
        Some(Err(why)) => return rsx! { Note { text: "The vault could not be read: {why}" } },
        Some(Ok(l)) => l.setlists,
    };
    let open = chosen()
        .and_then(|path| lists.iter().find(|f| f.setlist.path == path).cloned())
        .or_else(|| lists.first().cloned());
    let open_path = open.as_ref().map(|f| f.setlist.path.clone());

    let mut press = move |what: String, action: &'static str, args: Value| {
        said.set(Said::Working(what.clone()));
        spawn(async move {
            let heard = crate::host::run(action, args).await;
            said.set(match heard {
                Ok(out) => Said::Done(done(&what, &out)),
                Err(why) => Said::Refused(why),
            });
            engine.set(engine_now().await);
        });
    };

    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; flex-direction:column; min-width:0; min-height:0; \
                    background:{GROUND}; color:{INK}; font-size:13px; line-height:1.35;",
            div {
                style: "flex:1; min-height:0; overflow-y:scroll; display:flex; flex-wrap:wrap; align-content:flex-start;",
                // The left column: what there is to open.
                div {
                    style: "flex:1 1 250px; min-width:0; max-width:100%; background:{SIDE}; \
                            padding:14px 0 18px; box-sizing:border-box; align-self:stretch;",
                    Heading { text: "Setlists" }
                    if lists.is_empty() {
                        Quiet { text: "No setlist in this vault yet. A setlist is a note of type setlist in Setlists/." }
                    }
                    for found in lists.iter() {
                        SetlistRow {
                            key: "{found.setlist.path}",
                            found: found.clone(),
                            on: open_path.as_deref() == Some(found.setlist.path.as_str()),
                            onpick: move |path: String| chosen.set(Some(path)),
                        }
                    }
                    Heading { text: "Recent" }
                    if recent().is_empty() {
                        Quiet {
                            text: if engine().up {
                                "Nothing opened yet."
                            } else {
                                "Studio's engine is not running, so it cannot say."
                            }
                        }
                    }
                    for (i, r) in recent().into_iter().take(8).enumerate() {
                        RecentRow {
                            key: "{i}",
                            recent: r.clone(),
                            onopen: move |r: Recent| {
                                press(r.name.clone(), "DAW_OPEN", json!({ "path": r.path }));
                            },
                        }
                    }
                    Heading { text: "Rigs" }
                    RigList { rigs: rigs() }
                }
                // The right column: the chosen setlist.
                div {
                    style: "flex:3 1 420px; min-width:0; max-width:100%; padding:14px 0 24px; box-sizing:border-box;",
                    match open {
                        Some(found) => rsx! {
                            SetlistPage {
                                found: found.clone(),
                                playing: engine().path,
                                onsetlist: move |title: String| {
                                    press(title.clone(), "STUDIO_LAUNCHER_OPEN_SETLIST", json!({ "setlist": title }));
                                },
                                onsong: move |(setlist, song): (String, String)| {
                                    press(song.clone(), "STUDIO_LAUNCHER_OPEN_SONG", json!({ "song": song, "setlist": setlist }));
                                },
                            }
                        },
                        None => rsx! { Quiet { text: "Choose a setlist." } },
                    }
                }
            }
            Foot { said: said(), engine: engine() }
        }
    }
}

/// What to say once `what` opened: which songs a setlist left out.
fn done(what: &str, out: &Value) -> String {
    let left: Vec<&str> = out
        .get("left_out")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if left.is_empty() {
        format!("Opening {what} in the engine.")
    } else {
        format!(
            "Opening {what} without {} with no session: {}.",
            left.len(),
            left.join(", ")
        )
    }
}

#[component]
fn Note(text: String) -> Element {
    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; align-items:center; justify-content:center; \
                    background:{GROUND}; color:{DIM}; font-size:13px; padding:24px; box-sizing:border-box;",
            "{text}"
        }
    }
}

#[component]
fn Heading(text: String) -> Element {
    rsx! {
        div { style: "padding:14px 16px 6px; font-size:11px; font-weight:600; color:{DIM};", "{text}" }
    }
}

#[component]
fn Quiet(text: String) -> Element {
    rsx! {
        div { style: "padding:2px 16px 8px; color:{DIM}; font-size:12px;", "{text}" }
    }
}

#[component]
fn SetlistRow(found: Found, on: bool, onpick: EventHandler<String>) -> Element {
    let list = &found.setlist;
    let count = list.entries().count();
    let (length, unknown) = list.length();
    let fill = if on { WASH } else { "transparent" };
    let mut facts = vec![format!("{count} songs")];
    if length > 0.0 {
        facts.push(if unknown == 0 {
            clock(length)
        } else {
            format!("{} and {unknown} untimed", clock(length))
        });
    }
    facts.push(format!("{} with a session", found.playable()));
    let facts = facts.join(", ");
    let path = list.path.clone();
    rsx! {
        div {
            style: "margin:0 8px; padding:7px 8px; border-radius:6px; background:{fill}; cursor:pointer;",
            onclick: move |_| onpick.call(path.clone()),
            div { style: "display:flex; align-items:baseline; gap:8px; min-width:0;",
                div { style: "flex:1; min-width:0; font-weight:600; overflow:hidden; white-space:nowrap;", "{list.title}" }
                if let Some(date) = &list.date {
                    div { style: "flex:none; color:{DIM}; font-size:11px;", "{date}" }
                }
            }
            div { style: "color:{DIM}; font-size:11px; margin-top:1px;", "{facts}" }
        }
    }
}

#[component]
fn RecentRow(recent: Recent, onopen: EventHandler<Recent>) -> Element {
    let r = recent.clone();
    rsx! {
        div {
            style: "margin:0 8px; padding:5px 8px; border-radius:6px; cursor:pointer; display:flex; align-items:baseline; gap:8px; min-width:0;",
            onclick: move |_| onopen.call(r.clone()),
            div { style: "flex:1; min-width:0; overflow:hidden; white-space:nowrap;", "{recent.name}" }
            div { style: "flex:none; color:{DIM}; font-size:11px;", "{recent.when}" }
        }
    }
}

#[component]
fn RigList(rigs: Vec<Rigs>) -> Element {
    rsx! {
        if rigs.is_empty() {
            Quiet { text: "No saved rig on this machine (Signal keeps them in its config folder)." }
        }
        for rig in rigs.iter() {
            div { key: "{rig.instrument}", style: "padding:3px 16px;",
                div { style: "font-weight:600;", "{rig.instrument}" }
                div { style: "color:{DIM}; font-size:11px;",
                    if rig.profiles.is_empty() { "no profile saved" } else { "{rig.profiles.join(\", \")}" }
                }
            }
        }
        if !rigs.is_empty() {
            Quiet { text: "Listed only: loading a rig into the engine is not built yet." }
        }
    }
}

/// The five things a song is ready with, in the order of the marks.
const MARKS: [&str; 5] = ["Chart", "Lyrics", "Ref", "Tracks", "Lights"];

#[component]
fn SetlistPage(
    found: Found,
    playing: Option<String>,
    onsetlist: EventHandler<String>,
    onsong: EventHandler<(String, String)>,
) -> Element {
    let list = &found.setlist;
    let count = list.entries().count();
    let playable = found.playable();
    let (length, unknown) = list.length();
    let mut facts = vec![format!("{count} songs")];
    if list.sets.len() > 1 {
        facts.push(format!("{} sets", list.sets.len()));
    }
    if length > 0.0 {
        facts.push(if unknown == 0 {
            clock(length)
        } else {
            format!("{} timed, {unknown} not", clock(length))
        });
    }
    let facts = facts.join(", ");
    let can = found.file.is_some() || playable > 0;
    let button = if can {
        format!("background:{INK}; color:{GROUND}; cursor:pointer;")
    } else {
        format!("background:{WASH}; color:{DIM};")
    };
    let label = if found.file.is_some() || playable == count {
        "Open setlist".to_owned()
    } else if playable > 0 {
        format!("Open the {playable} with a session")
    } else {
        "No session yet".to_owned()
    };
    let title = list.title.clone();
    let mut at = 0usize;
    rsx! {
        div { style: "padding:4px 20px 12px; display:flex; flex-wrap:wrap; align-items:flex-end; gap:10px 16px;",
            div { style: "flex:1 1 220px; min-width:0;",
                div { style: "font-size:20px; font-weight:650; line-height:1.2;", "{list.title}" }
                div { style: "color:{DIM}; margin-top:3px;",
                    "{facts}"
                    if let Some(date) = &list.date { ", {date}" }
                }
            }
            button {
                style: "flex:none; height:30px; padding:0 14px; border:none; border-radius:6px; font-size:13px; font-weight:600; {button}",
                disabled: !can,
                onclick: move |_| onsetlist.call(title.clone()),
                "{label}"
            }
        }
        // What the marks are, once, over their columns.
        div { style: "display:flex; align-items:center; gap:10px; padding:0 20px 4px; color:{DIM}; font-size:10px;",
            div { style: "flex:1; min-width:0;" }
            div { style: "flex:none; display:flex; gap:2px;",
                for mark in MARKS {
                    div { key: "{mark}", style: "width:38px; text-align:center;", "{mark}" }
                }
            }
        }
        for (s, set) in list.sets.iter().enumerate() {
            div { key: "{s}",
                if let Some(name) = &set.name {
                    div {
                        style: "margin:10px 20px 2px; padding-bottom:5px; border-bottom:1px solid {LINE}; \
                                font-size:12px; font-weight:600; color:{DIM};",
                        "{name}"
                    }
                }
                for entry in set.entries.iter() {
                    {
                        let number = at + 1;
                        let session = found.sessions.get(at).cloned().flatten();
                        let lights = found.lights.get(at).copied().unwrap_or(false);
                        at += 1;
                        let song = entry.song.as_ref();
                        let ready = [
                            song.is_some_and(|x| x.chart),
                            song.is_some_and(|x| x.lyrics),
                            song.is_some_and(|x| x.reference || x.recording),
                            session.is_some(),
                            lights,
                        ];
                        let now = session.is_some()
                            && session.as_ref().and_then(|p| p.to_str()) == playing.as_deref();
                        let about = [
                            song.and_then(|x| x.artist.clone()),
                            song.and_then(crate::model::Song::keys),
                            song.and_then(|x| x.tempo).map(|t| format!("{t:.0} bpm")),
                            song.and_then(|x| x.length).map(clock),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join("  ");
                        let fill = if now { WASH } else { "transparent" };
                        let ink = if song.is_some() { INK } else { DIM };
                        let cursor = if session.is_some() { "pointer" } else { "default" };
                        let (name, label) = (list.title.clone(), entry.label.clone());
                        rsx! {
                            div {
                                key: "{number}",
                                style: "margin:0 12px; padding:6px 8px; border-radius:6px; background:{fill}; \
                                        display:flex; align-items:center; gap:10px; min-width:0; cursor:{cursor};",
                                onclick: move |_| {
                                    if session.is_some() {
                                        onsong.call((name.clone(), label.clone()));
                                    }
                                },
                                div { style: "flex:none; width:20px; text-align:right; color:{DIM}; font-size:12px;", "{number}" }
                                div { style: "flex:1; min-width:0;",
                                    div { style: "color:{ink}; font-weight:550; overflow:hidden; white-space:nowrap;", "{entry.label}" }
                                    div { style: "color:{DIM}; font-size:11px; overflow:hidden; white-space:nowrap;",
                                        if song.is_none() { "its note is not in the library" } else { "{about}" }
                                    }
                                }
                                div { style: "flex:none; display:flex; gap:2px;",
                                    for (i, is) in ready.into_iter().enumerate() {
                                        div { key: "{i}", style: "width:38px; display:flex; justify-content:center;",
                                            div {
                                                style: if is {
                                                    format!("width:8px; height:8px; border-radius:4px; background:{READY};")
                                                } else {
                                                    format!("width:6px; height:6px; border-radius:4px; border:1px solid {DIM}; opacity:0.5;")
                                                }
                                            }
                                        }
                                    }
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
fn Foot(said: Said, engine: Engine) -> Element {
    let (text, ink) = match &said {
        Said::Nothing => (String::new(), DIM),
        Said::Working(what) => (format!("Opening {what}…"), DIM),
        Said::Done(text) => (text.clone(), INK),
        Said::Refused(why) => (why.clone(), "var(--destructive, #e5534b)"),
    };
    let state = if engine.up {
        match engine.path.as_deref().and_then(|p| p.rsplit('/').next()) {
            Some(name) => format!("Engine: {name}"),
            None => "Engine: nothing open".to_owned(),
        }
    } else {
        "Engine: not running".to_owned()
    };
    rsx! {
        div {
            style: "flex:none; display:flex; align-items:center; gap:12px; min-width:0; padding:6px 16px; \
                    border-top:1px solid {LINE}; font-size:12px;",
            div { style: "flex:1; min-width:0; color:{ink}; overflow:hidden; white-space:nowrap;", "{text}" }
            div { style: "flex:none; color:{DIM};", "{state}" }
        }
    }
}
