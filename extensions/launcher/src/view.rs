//! The launcher's view: one setlist as a list of songs, in the grammar of
//! Architect's media list (`architect_media::ui::MediaList`, the list a
//! setlist's own page plays from): each set's name, then each row's
//! number, tile, title over artist, and length. The tile is the song's
//! key on the song's colour, as the setlist's share page draws it
//! (`setlists::row::tile_hue`). Nothing else is on a row: it is a list to
//! look at and load from. What differs from the media list is what a
//! press does: it opens the song in Studio's engine instead of playing
//! its recording, so a song with no tracks on this machine is greyed, as
//! a row with nothing to play is there.
//!
//! The view draws no navigation of its own. Which setlist it shows is
//! chosen in Architect's navigator (`crate::nav`) or in the row of
//! setlist names at its head, so the same view is the desk's pane, the
//! window's and the phone's page inside Architect's shell.
//!
//! Inline styles over the host's theme variables only (a linked view may
//! hold no `style` node).

use std::time::Duration;

use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::Found;
use crate::model::clock;

const INK: &str = "var(--foreground, #e6e8eb)";
const DIM: &str = "var(--muted-foreground, #8b9099)";
const LINE: &str = "var(--border, #23262b)";
const WASH: &str = "var(--accent, #26292f)";
const GROUND: &str = "var(--background, #101114)";
/// The pill and the open song: the theme's "safe" green.
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
    let mut loaded = use_signal(|| None::<Result<Vec<Found>, String>>);
    let mut engine = use_signal(Engine::default);
    let mut chosen = use_signal(crate::chosen);
    let mut said = use_signal(|| Said::Nothing);

    // The choice made elsewhere (the navigator, an action) twice a second;
    // the vault and the engine every few. A signal is set only when what
    // was read differs, so nothing redraws while nothing changes.
    use_future(move || async move {
        let mut tick = 0u32;
        loop {
            let now = crate::chosen();
            if *chosen.peek() != now {
                chosen.set(now);
            }
            if tick.is_multiple_of(8) {
                let now = crate::load().await.map(|l| l.setlists);
                if loaded.peek().as_ref() != Some(&now) {
                    loaded.set(Some(now));
                }
                let now = engine_now().await;
                if *engine.peek() != now {
                    engine.set(now);
                }
            }
            tick = tick.wrapping_add(1);
            futures_timer::Delay::new(Duration::from_millis(500)).await;
        }
    });

    let lists: Vec<Found> = match loaded() {
        None => return rsx! { Note { text: "Reading the vault…" } },
        Some(Err(why)) => return rsx! { Note { text: "The vault could not be read: {why}" } },
        Some(Ok(lists)) => lists,
    };
    if lists.is_empty() {
        return rsx! {
            Note { text: "No setlist in this vault yet. A setlist is a note of type setlist in Setlists/." }
        };
    }
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
                style: "flex:1; min-height:0; overflow-y:scroll; padding:16px 20px 24px; box-sizing:border-box;",
                // The setlists, to switch between where the navigator is not showing.
                if lists.len() > 1 {
                    div { style: "display:flex; flex-wrap:wrap; gap:6px; margin-bottom:14px;",
                        for found in lists.iter() {
                            {
                                let on = open_path.as_deref() == Some(found.setlist.path.as_str());
                                let (fill, ink) = if on { (WASH, INK) } else { ("transparent", DIM) };
                                let path = found.setlist.path.clone();
                                rsx! {
                                    button {
                                        key: "{found.setlist.path}",
                                        style: "height:28px; padding:0 12px; border:1px solid {LINE}; border-radius:999px; \
                                                background:{fill}; color:{ink}; font-size:12px; font-weight:600; cursor:pointer;",
                                        onclick: move |_| {
                                            crate::choose(path.clone());
                                            chosen.set(Some(path.clone()));
                                        },
                                        "{found.setlist.title}"
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(found) = open {
                    SetlistPage {
                        found,
                        playing: engine().path,
                        onsetlist: move |title: String| {
                            press(title.clone(), "STUDIO_LAUNCHER_OPEN_SETLIST", json!({ "setlist": title }));
                        },
                        onsong: move |(setlist, song): (String, String)| {
                            press(song.clone(), "STUDIO_LAUNCHER_OPEN_SONG", json!({ "song": song, "setlist": setlist }));
                        },
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
            "Opening {what} without {} with no tracks: {}.",
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
    // The media list's summary line: items, time, how many open.
    let mut facts = vec![format!("{count} songs")];
    if length > 0.0 {
        facts.push(if unknown == 0 {
            clock(length)
        } else {
            format!("{} timed, {unknown} not", clock(length))
        });
    }
    if let Some(date) = &list.date {
        facts.push(date.clone());
    }
    let facts = facts.join(" · ");
    let can = found.file.is_some() || playable > 0;
    let button = if can {
        format!("background:{READY}; color:#06140d; cursor:pointer;")
    } else {
        format!("background:{WASH}; color:{DIM};")
    };
    let label = if found.file.is_some() || playable == count {
        "Open set".to_owned()
    } else if playable > 0 {
        format!("Open {playable} with tracks")
    } else {
        "No tracks yet".to_owned()
    };
    let title = list.title.clone();
    let mut at = 0usize;
    rsx! {
        div { style: "font-size:22px; font-weight:700; line-height:1.2;", "{list.title}" }
        // The list's head: its pill, then what it holds.
        div { style: "display:flex; flex-wrap:wrap; align-items:center; gap:8px 12px; padding:10px 0 6px; min-width:0;",
            button {
                style: "flex:none; height:36px; padding:0 16px; border:none; border-radius:999px; font-size:14px; font-weight:600; {button}",
                disabled: !can,
                onclick: move |_| onsetlist.call(title.clone()),
                "{label}"
            }
            div { style: "flex:1 1 160px; min-width:0; color:{DIM};", "{facts}" }
        }
        for (s, set) in list.sets.iter().enumerate() {
            div { key: "{s}",
                if let Some(name) = &set.name {
                    div { style: "padding:18px 0 6px; font-size:13px; font-weight:650; color:{DIM};", "{name}" }
                }
                for entry in set.entries.iter() {
                    {
                        let number = at + 1;
                        let session = found.sessions.get(at).cloned().flatten();
                        at += 1;
                        let song = entry.song.as_ref();
                        let now = session.is_some()
                            && session.as_ref().and_then(|p| p.to_str()) == playing.as_deref();
                        let key = song.and_then(crate::model::Song::keys);
                        let hue = setlists::row::tile_hue(&entry.label);
                        let about = [
                            song.and_then(|x| x.artist.clone()),
                            song.and_then(|x| x.tempo).map(|t| format!("{t:.0} bpm")),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ");
                        let length = song.and_then(|x| x.length).map(clock).unwrap_or_default();
                        let fill = if now { WASH } else { "transparent" };
                        let ink = if now { READY } else { INK };
                        // A song with no tracks is greyed, as a row with
                        // nothing to play is in the media list.
                        let fade = if session.is_some() { "1" } else { "0.55" };
                        let cursor = if session.is_some() { "pointer" } else { "default" };
                        let tile_size = if key.as_ref().is_some_and(|k| k.chars().count() > 2) { 12 } else { 16 };
                        let (name, label) = (list.title.clone(), entry.label.clone());
                        rsx! {
                            div {
                                key: "{number}",
                                style: "display:flex; align-items:center; gap:12px; min-width:0; min-height:56px; \
                                        padding:4px; border-radius:8px; background:{fill}; cursor:{cursor};",
                                onclick: move |_| {
                                    if session.is_some() {
                                        onsong.call((name.clone(), label.clone()));
                                    }
                                },
                                div { style: "flex:none; width:22px; text-align:right; color:{DIM}; font-size:13px; opacity:{fade};", "{number}" }
                                div {
                                    style: "flex:none; width:48px; height:48px; border-radius:6px; background:hsl({hue} 42% 40%); \
                                            color:#ffffff; display:flex; align-items:center; justify-content:center; \
                                            font-size:{tile_size}px; font-weight:700; opacity:{fade};",
                                    {key.unwrap_or_else(|| "♪".to_owned())}
                                }
                                div { style: "flex:1; min-width:0; opacity:{fade};",
                                    div { style: "color:{ink}; font-size:16px; font-weight:500; line-height:1.3; overflow:hidden; white-space:nowrap;", "{entry.label}" }
                                    div { style: "color:{DIM}; font-size:13px; overflow:hidden; white-space:nowrap;",
                                        if song.is_none() { "its note is not in the library" } else { "{about}" }
                                    }
                                }
                                div { style: "flex:none; color:{DIM}; font-size:13px; opacity:{fade};", "{length}" }
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
        engine
            .path
            .as_deref()
            .and_then(|p| p.rsplit('/').next())
            .map_or_else(
                || "Engine: nothing open".to_owned(),
                |name| format!("Engine: {name}"),
            )
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
