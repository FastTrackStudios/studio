//! The launcher's view: one setlist as Architect's media list
//! (`architect_media::ui::MediaList`, the list a setlist's own page plays
//! from), given the launcher's press: the pill opens the set and a row
//! opens its song in Studio's engine, where the page's list plays a
//! recording. A song's tile is its key on its colour, the setlist share
//! page's rule (`setlists::row::tile_hue`), over its cover's corner when
//! it has one. A song with no tracks on this machine is greyed, as a row
//! with nothing to play is.
//!
//! The view draws no navigation of its own. Which setlist it shows is
//! chosen in Architect's navigator (`crate::nav`) or in the row of
//! setlist names at its head, so the same view is the desk's pane, the
//! window's and the phone's page inside Architect's shell.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use architect_media::ui::{MediaList, Sections};
use architect_media::{Item, Row, Tile};
use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::model::{Entry, clock};
use crate::{Found, Loaded, layout};

const INK: &str = "var(--foreground, #e6e8eb)";
const DIM: &str = "var(--muted-foreground, #8b9099)";
const LINE: &str = "var(--border, #23262b)";
const WASH: &str = "var(--accent, #26292f)";
const GROUND: &str = "var(--background, #101114)";

/// What a press said, shown along the foot until the next one.
#[derive(Clone, PartialEq)]
enum Said {
    Nothing,
    Working(String),
    Done(String),
    Refused(String),
}

/// The covers read so far, by their file: a cover is read once.
static COVERS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

fn cover(file: &str) -> Option<String> {
    let mut covers = COVERS.lock().unwrap_or_else(PoisonError::into_inner);
    covers
        .get_or_insert_with(HashMap::new)
        .entry(file.to_owned())
        .or_insert_with(|| crate::find::picture_url(Path::new(file)))
        .clone()
}

/// A place of a setlist as a row of the list: `id` its place from 0.
fn row(id: usize, entry: &Entry, session: Option<&Path>, vault: Option<&str>) -> Row {
    let song = entry.song.as_ref();
    let about: Vec<String> = [
        song.and_then(|s| s.artist.clone()),
        song.and_then(|s| s.tempo).map(|t| format!("{t:.0} bpm")),
        entry.remark.clone(),
    ]
    .into_iter()
    .flatten()
    .collect();
    Row {
        id: id.to_string(),
        number: id.saturating_add(1),
        item: Item {
            path: entry.note.clone(),
            title: entry.label.clone(),
            subtitle: if song.is_none() {
                Some("its note is not in the library".to_owned())
            } else {
                (!about.is_empty()).then(|| about.join(" · "))
            },
            duration: song.and_then(|s| s.length),
            ..Item::default()
        },
        // What the row loads: its session. None greys it.
        media: session.and_then(Path::to_str).map(str::to_owned),
        art: song
            .and_then(|s| s.art.as_deref())
            .zip(vault)
            .map(|(art, vault)| format!("{vault}/{art}")),
        tile: Some(Tile {
            text: song
                .and_then(crate::model::Song::keys)
                .unwrap_or_else(|| "♪".to_owned()),
            hue: setlists::row::tile_hue(&entry.label),
        }),
    }
}

/// `found` as the list's sections: its sets, each place a row.
fn sections(found: &Found, vault: Option<&str>) -> Sections {
    let mut places = found.sessions.iter().enumerate();
    found
        .setlist
        .sets
        .iter()
        .map(|set| {
            let rows = set
                .entries
                .iter()
                .filter_map(|entry| {
                    let (id, session) = places.next()?;
                    Some(row(id, entry, session.as_deref(), vault))
                })
                .collect();
            (set.name.clone(), rows)
        })
        .collect()
}

/// The place of `found` whose session is the engine's `playing`.
fn current(found: &Found, playing: Option<&str>) -> Option<String> {
    let playing = Path::new(playing?).with_extension("");
    found
        .sessions
        .iter()
        .position(|s| {
            s.as_deref()
                .is_some_and(|p| p.with_extension("") == playing)
        })
        .map(|at| at.to_string())
}

#[component]
pub fn LauncherView() -> Element {
    let mut loaded = use_signal(|| None::<Result<Loaded, String>>);
    let mut playing = use_signal(|| None::<String>);
    let mut chosen = use_signal(crate::chosen);
    let mut saved = use_signal(Vec::<String>::new);
    // The layout Open set applies: none until one is picked, then the
    // setlist's own.
    let mut picked = use_signal(|| None::<Option<String>>);
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
                picked.set(None);
            }
            if tick.is_multiple_of(8) {
                let now = crate::load().await;
                if loaded.peek().as_ref() != Some(&now) {
                    loaded.set(Some(now));
                }
                let now = crate::follow::session().await.map(|s| s.path);
                if *playing.peek() != now {
                    playing.set(now);
                }
                let now = crate::arrange::saved_names().await;
                if *saved.peek() != now {
                    saved.set(now);
                }
            }
            tick = tick.wrapping_add(1);
            futures_timer::Delay::new(Duration::from_millis(500)).await;
        }
    });

    let all = match loaded() {
        None => return rsx! { Note { text: "Reading the vault…" } },
        Some(Err(why)) => return rsx! { Note { text: "The vault could not be read: {why}" } },
        Some(Ok(all)) => all,
    };
    if all.setlists.is_empty() {
        return rsx! {
            Note { text: "No setlist in this vault yet. A setlist is a note of type setlist in Setlists/." }
        };
    }
    let Some(found) = chosen()
        .and_then(|path| {
            all.setlists
                .iter()
                .find(|f| f.setlist.path == path)
                .cloned()
        })
        .or_else(|| all.setlists.first().cloned())
    else {
        return rsx! { Note { text: "Choose a setlist." } };
    };
    let list = found.setlist.clone();

    let mut press = move |what: String, action: &'static str, args: Value| {
        said.set(Said::Working(what.clone()));
        // Run to the end even when this view goes meanwhile (an open
        // closes the launcher's window; in a browser the home steps aside
        // for the song opened): what it says is shown only while it is
        // there.
        dioxus::core::spawn_forever(async move {
            let heard = crate::host::run(action, args).await;
            if let Ok(mut now) = said.try_write() {
                *now = match heard {
                    Ok(out) => Said::Done(done(&what, &out)),
                    Err(why) => Said::Refused(why),
                };
            }
            let session = crate::follow::session().await.map(|s| s.path);
            if let Ok(mut now) = playing.try_write() {
                *now = session;
            }
        });
    };

    let count = list.entries().count();
    let playable = found.playable();
    let (length, unknown) = list.length();
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
    let head = if found.file.is_some() || playable == count {
        "Open set".to_owned()
    } else if playable > 0 {
        format!("Open {playable} with tracks")
    } else {
        "No tracks yet".to_owned()
    };
    let layout_now = picked().unwrap_or_else(|| list.layout.clone());
    let layouts = layout::BUILT_IN
        .iter()
        .map(|l| (*l).to_owned())
        .chain(saved());
    let (title, songs) = (list.title.clone(), found.clone());
    let layout_for_open = layout_now.clone();
    let art = use_callback(|file: String| cover(&file));

    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; flex-direction:column; min-width:0; min-height:0; \
                    background:{GROUND}; color:{INK}; font-size:13px; line-height:1.35;",
            div {
                style: "flex:1; min-height:0; overflow-y:scroll; padding:16px 20px 24px; box-sizing:border-box;",
                // The setlists, to switch between where the navigator is not showing.
                if all.setlists.len() > 1 {
                    div { style: "display:flex; flex-wrap:wrap; gap:6px; margin-bottom:14px;",
                        for other in all.setlists.iter() {
                            Pill {
                                key: "{other.setlist.path}",
                                label: other.setlist.title.clone(),
                                on: other.setlist.path == list.path,
                                onpick: {
                                    let path = other.setlist.path.clone();
                                    move |()| {
                                        crate::choose(path.clone());
                                        chosen.set(Some(path.clone()));
                                        picked.set(None);
                                    }
                                },
                            }
                        }
                    }
                }
                div { style: "font-size:22px; font-weight:700; line-height:1.2; margin-bottom:8px;", "{list.title}" }
                // The layout Open set lays the panes out in.
                div { style: "display:flex; flex-wrap:wrap; align-items:center; gap:6px; margin-bottom:6px;",
                    div { style: "color:{DIM}; font-size:12px; margin-right:2px;", "Opens in" }
                    Pill {
                        label: "As it is".to_owned(),
                        on: layout_now.is_none(),
                        onpick: move |()| picked.set(Some(None)),
                    }
                    for name in layouts {
                        Pill {
                            key: "{name}",
                            label: name.clone(),
                            on: layout_now.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(&name)),
                            onpick: move |()| picked.set(Some(Some(name.clone()))),
                        }
                    }
                }
                MediaList {
                    source: "app:studio-launcher".to_owned(),
                    name: list.title,
                    sections: sections(&found, all.vault_root.as_deref()),
                    art: Some(art),
                    head: Some(head),
                    sum: Some(facts.join(" · ")),
                    current: current(&found, playing().as_deref()),
                    on_head: move |()| {
                        let mut args = json!({ "setlist": title });
                        if let (Some(layout), Some(fields)) = (&layout_for_open, args.as_object_mut()) {
                            fields.insert("layout".to_owned(), json!(layout));
                        }
                        press(title.clone(), "STUDIO_LAUNCHER_OPEN_SETLIST", args);
                    },
                    on_press: move |id: String| {
                        let place = id.parse::<usize>().ok().and_then(|at| songs.setlist.entries().nth(at));
                        if let Some(entry) = place {
                            press(
                                entry.label.clone(),
                                "STUDIO_LAUNCHER_OPEN_SONG",
                                json!({ "song": entry.label, "setlist": songs.setlist.title }),
                            );
                        }
                    },
                }
            }
            Foot { said: said(), playing: playing() }
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
        format!("Opened {what}.")
    } else {
        format!(
            "Opened {what} without {} with no tracks: {}.",
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

/// One of a row of choices: the one on is filled.
#[component]
fn Pill(label: String, on: bool, onpick: EventHandler<()>) -> Element {
    let (fill, ink) = if on {
        (WASH, INK)
    } else {
        ("transparent", DIM)
    };
    rsx! {
        button {
            style: "height:28px; padding:0 12px; border:1px solid {LINE}; border-radius:999px; \
                    background:{fill}; color:{ink}; font-size:12px; font-weight:600; cursor:pointer;",
            onclick: move |_| onpick.call(()),
            "{label}"
        }
    }
}

#[component]
fn Foot(said: Said, playing: Option<String>) -> Element {
    let (text, ink) = match &said {
        Said::Nothing => (String::new(), DIM),
        Said::Working(what) => (format!("Opening {what}…"), DIM),
        Said::Done(text) => (text.clone(), INK),
        Said::Refused(why) => (why.clone(), "var(--destructive, #e5534b)"),
    };
    let state = playing
        .as_deref()
        .and_then(|p| Path::new(p).file_stem())
        .and_then(|s| s.to_str())
        .map_or_else(
            || "Engine: nothing open".to_owned(),
            |name| format!("Engine: {name}"),
        );
    rsx! {
        div {
            style: "flex:none; display:flex; align-items:center; gap:12px; min-width:0; padding:6px 16px; \
                    border-top:1px solid {LINE}; font-size:12px;",
            div { style: "flex:1; min-width:0; color:{ink}; overflow:hidden; white-space:nowrap;", "{text}" }
            div { style: "flex:none; color:{DIM};", "{state}" }
        }
    }
}
