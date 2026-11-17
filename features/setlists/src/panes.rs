//! Pieces every song pane shares, and the Setlist view.
//!
//! **One clock.** Every pane follows setlists' source on Architect's
//! playhead ([`crate::now`]): the song playing is its item, the time its
//! time. That source is the deck's own clock, or Studio's engine's
//! transport while one runs (`crate::engine_clock`). A pane reads the
//! state a few times a second ([`use_playhead`]) and carries the time on
//! between reads, so the bar, the beat and the line move every frame.
//! Setlists' own views read the source directly; Studio's Chart
//! (`studio-chart`) and Lyrics (`studio-lyrics`) extensions read it as
//! `studio.setlists`' `now` ([`crate::now::Reader`]), each as itself.
//!
//! **Which song** ([`shown`]). The one playing; with nothing on the
//! playhead, the song the pane was opened on (its state, a note's path:
//! what `EXT_VIEW_OPEN` hands it, or the song's tab). An arrangement plays
//! its own chart (`Song#Arrangement`: the chart under Arrangements/<name>/).
//!
//! [`SetlistView`]: the running order, its sets as labels, each song a row
//! (number, its key's tile, title over artist, length), the song playing
//! marked; a row pressed plays it.

use std::time::Duration;

use dioxus::prelude::*;

use crate::host::{self, SetlistRow, SongRow};
use crate::keys::KeyLabel;
use crate::layout::SongRef;
use crate::now::{self, Now, Reader};
use crate::workspace::Song;
use crate::workspace_view::{Palette, clock_text, engrave, sleep};

/// How often a pane reads the playhead.
const POLL: Duration = Duration::from_millis(150);
/// How often its time moves while it plays.
const FRAME: Duration = Duration::from_millis(40);
/// Narrower than this, a pane is a phone's: a chart across its width.
pub const NARROW: f64 = 600.0;

/// The playhead as a pane reads it: its state, the time now, and how it
/// was read (to seek through the same door).
#[derive(Clone, PartialEq)]
pub struct Playhead {
    pub state: Signal<Option<Now>>,
    pub t: Signal<f64>,
    pub reader: Reader,
}

impl Playhead {
    /// Plays from `t` seconds of item `id` (the song shown). While
    /// Studio's engine is the clock it is refused: Studio is played in
    /// Studio.
    pub fn seek(&self, id: String, t: f64) {
        let reader = self.reader.clone();
        spawn(async move {
            if let Err(error) = reader.seek(&id, t).await {
                tracing::debug!(%error, "setlists: a seek refused");
            }
        });
    }
}

/// Follows setlists' source on the playhead, read by `reader`.
#[must_use]
pub fn use_playhead(reader: Reader) -> Playhead {
    // Studio's engine, when one runs, is the clock (once per process).
    #[cfg(all(
        feature = "engine-clock",
        not(any(target_arch = "wasm32", target_os = "ios"))
    ))]
    use_hook(crate::engine_clock::start);
    let mut state = use_signal(|| None::<Now>);
    let mut t = use_signal(|| 0.0_f64);
    let polling = reader.clone();
    use_future(move || {
        let reader = polling.clone();
        async move {
            loop {
                let read = reader.state().await;
                if *state.peek() != read {
                    state.set(read);
                }
                sleep(POLL).await;
            }
        }
    });
    use_future(move || async move {
        loop {
            sleep(FRAME).await;
            let at = state.peek().as_ref().map_or(0.0, |n| n.at(now::now_ms()));
            if (at - *t.peek()).abs() > 0.005 {
                t.set(at);
            }
        }
    });
    Playhead { state, t, reader }
}

/// The note the pane was opened on: its state (a vault path, from
/// `EXT_VIEW_OPEN`), else the song tab's note.
#[must_use]
pub fn use_note() -> Option<String> {
    // A process view drawn remotely has no workspace: only the deck's
    // context then. Whether there is one never changes for a pane.
    let pane =
        try_use_context::<architect_workspace::Host>().map(|_| architect_workspace::use_pane());
    let deck = architect_workspace::use_deck_context();
    pane.map(|p| p.state())
        .map(|s| s.trim().to_owned())
        .filter(|s| s.ends_with(".md") || s.contains(".md#"))
        .or_else(|| deck.map(|c| c.id))
}

/// The song a pane shows: the one playing (its note, its arrangement),
/// else the song note it was opened on; `true` when it is the one
/// playing.
#[must_use]
pub fn shown(now: Option<&Now>, note: Option<&str>) -> Option<(String, Option<String>, bool)> {
    if let Some((path, arrangement)) = now.and_then(Now::song) {
        return Some((path, arrangement, true));
    }
    let note = note?;
    let (path, arrangement) = now::song_of(note)?;
    SongRef::from_link_target(&path).map(|_| (path, arrangement, false))
}

/// A song's title from its note's path.
#[must_use]
pub fn title_of(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".md")
        .to_owned()
}

/// The item id of a song shown.
#[must_use]
pub fn item_of(path: &str, arrangement: Option<&str>) -> String {
    arrangement.map_or_else(|| path.to_owned(), |a| format!("{path}#{a}"))
}

/// `text` engraved off the view's thread (a page has none: in place).
///
/// # Errors
///
/// It does not parse, or a page did not draw.
pub async fn engrave_async(text: String) -> Result<(Song, Vec<String>, Option<String>), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = futures_channel::oneshot::channel();
        std::thread::Builder::new()
            .name("setlists-pane-engrave".into())
            .spawn(move || {
                let _ = tx.send(engrave(&text));
            })
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|_| "the engraver stopped".to_owned())?
    }
    #[cfg(target_arch = "wasm32")]
    {
        engrave(&text)
    }
}

/// Where the clock comes from, said in a pane's bar.
#[must_use]
pub fn clock_said(following: bool) -> &'static str {
    if !following {
        "not playing"
    } else if now::engine_drives() {
        "following Studio"
    } else {
        "on the playhead"
    }
}

/// A pane filling its pane.
pub const FILL: &str = "position:absolute; top:0; left:0; right:0; bottom:0;";

/// A pane's message, centred.
#[must_use]
pub fn said(p: &Palette, text: &str) -> Element {
    rsx! {
        div {
            style: "{FILL} display:flex; align-items:center; justify-content:center; padding:24px; \
                    box-sizing:border-box; text-align:center; color:{p.ink_2}; font-size:13px; \
                    background:{p.desk}; font-family:system-ui, sans-serif;",
            "data-pane-empty": "true",
            "{text}"
        }
    }
}

// ---------------------------------------------------------------------------
// Setlist
// ---------------------------------------------------------------------------

/// Which setlist a pane shows: the note it was opened on when that is a
/// setlist, else the one whose songs are on the playhead, else the first.
#[must_use]
pub fn setlist_for<'a>(
    sets: &'a [SetlistRow],
    note: Option<&str>,
    now: Option<&Now>,
) -> Option<&'a SetlistRow> {
    if let Some(note) = note
        && let Some(set) = sets.iter().find(|s| s.path == note)
    {
        return Some(set);
    }
    let playing = now.and_then(|n| n.item.as_deref());
    if let Some(item) = playing
        && let Some(set) = sets
            .iter()
            .find(|s| s.songs.iter().any(|at| now::item_id(at) == item))
    {
        return Some(set);
    }
    sets.first()
}

/// The running order, the song playing marked; a row plays its song.
#[allow(non_snake_case)]
pub fn SetlistView() -> Element {
    let palette = Palette::of(&architect_theme::use_resolved());
    let note = use_note();
    let playhead = use_playhead(Reader::Own);
    let data = use_resource(|| async {
        let sets = host::setlists().await.ok()?;
        let songs = host::songs().await.unwrap_or_default();
        Some((sets, songs))
    });
    let state = playhead.state.read().clone();
    let Some((sets, songs)) = data.read().clone().flatten() else {
        return said(&palette, "Reading the setlists…");
    };
    let Some(set) = setlist_for(&sets, note.as_deref(), state.as_ref()).cloned() else {
        return said(
            &palette,
            "No setlists yet: STUDIO_SETLISTS_CREATE makes one.",
        );
    };
    rsx! { SetlistOf { key: "{set.path}", set, songs, state, palette } }
}

#[component]
fn SetlistOf(
    set: SetlistRow,
    songs: Vec<SongRow>,
    state: Option<Now>,
    palette: Palette,
) -> Element {
    // With nothing on the playhead, the setlist goes on it (paused at its
    // first song), so the Chart and Lyrics tabs have a song. Once.
    let seed = set.clone();
    use_hook(move || {
        spawn(async move {
            if now::state().await.is_none()
                && let Err(error) = now::load(&seed, None).await
            {
                tracing::debug!(%error, "setlists: the playhead refused the setlist");
            }
        });
    });
    let p = palette;
    let playing = state.as_ref().and_then(|n| n.item.clone());
    let on_deck = state.as_ref().is_some_and(|n| n.playing);
    let model = set.setlist();
    let sections = model.sections();
    let engine = now::engine_drives();
    let mut rows = Vec::new();
    for section in &sections {
        if let Some(name) = section.name {
            rows.push(rsx! {
                div { key: "set-{section.start}",
                    style: "padding:14px 4px 6px; font-size:11px; letter-spacing:0.08em; text-transform:uppercase; \
                            color:{p.ink_2}; font-weight:700;",
                    "{name}"
                }
            });
        }
        for (i, at) in section.songs.iter().enumerate() {
            let number = section.start + i + 1;
            let song = songs
                .iter()
                .find(|s| s.at.library == at.library && s.at.title == at.title);
            let id = now::item_id(at);
            let current = playing.as_deref() == Some(id.as_str());
            let key = song.and_then(|s| {
                s.key.as_ref().map(|start| {
                    KeyLabel {
                        start: start.clone(),
                        end: s.key_end.clone(),
                        outside: s.key_outside,
                    }
                    .text()
                })
            });
            let artist = song.and_then(|s| s.artist.clone()).unwrap_or_default();
            let length = song
                .and_then(|s| s.length)
                .map(clock_text)
                .unwrap_or_default();
            let hue = crate::row::tile_hue(&at.title);
            let label = at.label();
            let row_bg = if current {
                p.pill.clone()
            } else {
                "transparent".to_owned()
            };
            let title_ink = if current {
                p.brand.clone()
            } else {
                p.ink.clone()
            };
            let (seed, play_id) = (set.clone(), id.clone());
            rows.push(rsx! {
                div {
                    key: "{id}",
                    "data-setlist-row": "{number}",
                    "aria-current": if current { "true" } else { "false" },
                    style: "display:flex; align-items:center; gap:12px; padding:6px 6px; border-radius:8px; \
                            background:{row_bg}; cursor:pointer;",
                    onclick: move |_| {
                        if engine {
                            return;
                        }
                        let (seed, id) = (seed.clone(), play_id.clone());
                        spawn(async move {
                            let loaded = now::load(&seed, Some(&id)).await;
                            if let Err(error) = loaded.map(drop) {
                                tracing::debug!(%error, "setlists: the playhead refused the setlist");
                                return;
                            }
                            if let Err(error) = now::go(&id, 0.0, true).await {
                                tracing::debug!(%error, "setlists: the playhead refused a song");
                            }
                        });
                    },
                    span { style: "flex:none; width:22px; text-align:right; font-size:13px; color:{p.ink_2}; \
                                   font-variant-numeric:tabular-nums;",
                        if current && on_deck { "\u{25B6}" } else { "{number}" }
                    }
                    div {
                        style: "flex:none; width:44px; height:44px; border-radius:6px; display:flex; align-items:center; \
                                justify-content:center; background:hsl({hue}, 45%, 32%); color:#ffffff; \
                                font-size:15px; font-weight:800;",
                        title: "Key",
                        if let Some(key) = key { "{key}" } else { "\u{266A}" }
                    }
                    div { style: "flex:1; min-width:0; display:flex; flex-direction:column; gap:2px;",
                        span { style: "font-size:15px; font-weight:600; color:{title_ink}; white-space:nowrap; overflow:hidden;", "{label}" }
                        span { style: "font-size:12px; color:{p.ink_2}; white-space:nowrap; overflow:hidden;", "{artist}" }
                    }
                    span { style: "flex:none; font-size:12px; color:{p.ink_2}; font-variant-numeric:tabular-nums;", "{length}" }
                }
            });
        }
    }
    let date = set.date.clone().unwrap_or_default();
    let count = set.songs.len();
    rsx! {
        div {
            "data-studio-setlist": "{set.title}",
            style: "{FILL} overflow-y:scroll; background:{p.desk}; color:{p.ink}; font-family:system-ui, sans-serif;",
            div { style: "padding:16px 14px 24px; box-sizing:border-box;",
                div { style: "font-size:22px; font-weight:800;", "{set.title}" }
                div { style: "font-size:12px; color:{p.ink_2}; padding:2px 0 6px;",
                    "{count} songs" if !date.is_empty() { " · {date}" }
                    if engine { " · following Studio" }
                }
                {rows.into_iter()}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const KNOW_ME: &str = "Libraries/JT/Know Me/Know Me.md";

    fn playing(id: &str) -> Now {
        Now::of(&json!({"item": {"id": id}, "time": 1.0, "at": 0.0, "playing": true}))
            .unwrap_or_default()
    }

    #[test]
    fn a_pane_shows_the_song_playing_else_the_song_it_opened_on() {
        let now = playing(&format!("{KNOW_ME}#Extended"));
        assert_eq!(
            shown(Some(&now), Some("Libraries/JT/Fire/Fire.md")),
            Some((KNOW_ME.to_owned(), Some("Extended".to_owned()), true))
        );
        assert_eq!(
            shown(None, Some("Libraries/JT/Fire/Fire.md")),
            Some(("Libraries/JT/Fire/Fire.md".to_owned(), None, false))
        );
        assert_eq!(
            shown(None, Some("Setlists/JT.md")),
            None,
            "a setlist is no song"
        );
        assert_eq!(shown(None, None), None);
        assert_eq!(
            item_of(KNOW_ME, Some("Extended")),
            format!("{KNOW_ME}#Extended")
        );
        assert_eq!(title_of(KNOW_ME), "Know Me");
    }

    #[test]
    fn the_setlist_shown_is_the_notes_else_the_one_playing() {
        let set = |path: &str, title: &str| SetlistRow {
            path: path.into(),
            title: title.into(),
            date: None,
            songs: vec![SongRef::new("JT", title, None)],
            sets: Vec::new(),
            share: None,
        };
        let sets = vec![set("Setlists/A.md", "A"), set("Setlists/B.md", "B")];
        let b = playing("Libraries/JT/B/B.md");
        assert_eq!(
            setlist_for(&sets, Some("Setlists/A.md"), Some(&b)).map(|s| &*s.title),
            Some("A")
        );
        assert_eq!(
            setlist_for(&sets, None, Some(&b)).map(|s| &*s.title),
            Some("B")
        );
        assert_eq!(setlist_for(&sets, None, None).map(|s| &*s.title), Some("A"));
    }
}
