//! The dashboard: a signed-in person's sets and songs, across every org
//! the app has open, each a press away from the views a link uses.
//!
//! The app's home view (`SiteIsland::home`, Architect's `app::home`). It
//! is handed the vaults' typed notes as rows (`note.home`: `{person,
//! orgs: [{slug, rows: [{path, type, properties}]}], recent: [path]}`),
//! never a note's text, and reads them with the model the Project
//! Launcher reads them with (`setlists::rows`): one model of sets and
//! songs, two surfaces. A press asks the app to open the note
//! (`NOTE_OPEN`): in a browser that is the setlist's page or the song's
//! chart and lyrics, where on a desk the launcher opens the set in the
//! engine.
//!
//! [`board_of`] is the reading (tested natively); the rest draws it.

use serde_json::Value;
use setlists::rows::{Row, Setlist, Song, clock, setlists};

/// One org's sets and songs, as the dashboard lists them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Org {
    pub slug: String,
    /// Setlists dated today or later, the soonest first.
    pub coming: Vec<Setlist>,
    /// Setlists dated before today, the latest first, then the undated.
    pub earlier: Vec<Setlist>,
    /// Song libraries by name, each with its songs by title.
    pub libraries: Vec<(String, Vec<Song>)>,
    /// Setlists with a share link: the setlist's path, its title, the code.
    pub shared: Vec<(String, String, String)>,
}

/// Everything the dashboard shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Board {
    pub person: String,
    pub orgs: Vec<Org>,
    /// Notes opened lately: path and title, newest first.
    pub recent: Vec<(String, String)>,
}

impl Board {
    /// Whether there is nothing to list yet (the vaults are still read).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.orgs
            .iter()
            .all(|o| o.coming.is_empty() && o.earlier.is_empty() && o.libraries.is_empty())
    }
}

/// The library a song note is in, by its path
/// (`Libraries/<Library>/<Song>/<Song>.md`); "Songs" for one elsewhere.
#[must_use]
pub fn library_of(path: &str) -> String {
    let mut parts = path.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(root), Some(library), Some(_)) if root == setlists::layout::LIBRARIES => {
            library.to_owned()
        }
        _ => "Songs".to_owned(),
    }
}

fn rows_of(org: &Value, kind: &str) -> Vec<Row> {
    org.get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|r| {
            r.get("type")
                .and_then(Value::as_str)
                .is_some_and(|t| t.eq_ignore_ascii_case(kind))
        })
        .filter_map(|r| {
            Some(Row {
                path: r.get("path")?.as_str()?.to_owned(),
                values: r.get("properties")?.as_object()?.clone(),
            })
        })
        .collect()
}

/// The dashboard for `note` (the view's `note` prop) on `today`
/// (`YYYY-MM-DD`: a setlist dated today is still coming up).
#[must_use]
pub fn board_of(note: &Value, today: &str) -> Board {
    let Some(home) = note.get("home") else {
        return Board::default();
    };
    let mut titles: Vec<(String, String)> = Vec::new();
    let orgs = home
        .get("orgs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|org| {
            let (set_rows, song_rows) = (rows_of(org, "setlist"), rows_of(org, "song"));
            let shared = set_rows
                .iter()
                .filter_map(|r| {
                    let code = r.values.get("share")?.as_str()?.trim();
                    let title = r
                        .values
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or(&r.path);
                    (!code.is_empty()).then(|| (r.path.clone(), title.to_owned(), code.to_owned()))
                })
                .collect();
            let all = setlists(&set_rows, &song_rows);
            let (mut coming, mut earlier): (Vec<Setlist>, Vec<Setlist>) = all
                .into_iter()
                .partition(|s| s.date.as_deref().is_some_and(|d| d >= today));
            coming.sort_by(|a, b| a.date.cmp(&b.date).then(a.title.cmp(&b.title)));
            // The latest first; the undated after every dated one.
            earlier.sort_by(|a, b| b.date.cmp(&a.date).then(a.title.cmp(&b.title)));
            let mut songs: Vec<Song> = song_rows.iter().map(Song::of).collect();
            songs.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
            let mut libraries: Vec<(String, Vec<Song>)> = Vec::new();
            for song in songs {
                let name = library_of(&song.path);
                titles.push((song.path.clone(), song.title.clone()));
                match libraries.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, list)) => list.push(song),
                    None => libraries.push((name, vec![song])),
                }
            }
            libraries.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
            for s in coming.iter().chain(&earlier) {
                titles.push((s.path.clone(), s.title.clone()));
            }
            Org {
                slug: org
                    .get("slug")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                coming,
                earlier,
                libraries,
                shared,
            }
        })
        .collect();
    let recent = home
        .get("recent")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|p| titles.iter().find(|(path, _)| path == p).cloned())
        .collect();
    Board {
        person: home
            .get("person")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        orgs,
        recent,
    }
}

/// A setlist's line under its title: its date, its songs, how long.
#[must_use]
pub fn set_line(set: &Setlist) -> String {
    let songs = set.entries().count();
    let (seconds, unknown) = set.length();
    let mut parts = Vec::new();
    if let Some(date) = &set.date {
        parts.push(date.clone());
    }
    parts.push(format!("{songs} song{}", if songs == 1 { "" } else { "s" }));
    if seconds > 0.0 {
        parts.push(format!(
            "{}{}",
            clock(seconds),
            if unknown > 0 { "+" } else { "" }
        ));
    }
    parts.join(", ")
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use view::Home;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod view {
    use dioxus::prelude::*;
    use serde_json::{Value, json};
    use setlists::rows::{Setlist, Song};

    use super::{Board, Org, board_of, set_line};
    use crate::views::Ask;

    const CSS: &str = r"
.hv { font-family: var(--font, system-ui, sans-serif); color: var(--foreground, CanvasText); padding: 2px 12px 28px; }
.hv-top { padding: 6px 2px 14px; }
.hv-top h1 { margin: 0; font-size: 26px; line-height: 1.15; letter-spacing: -0.02em; font-weight: 700; }
.hv-top p { margin: 6px 0 0; font-size: 14px; color: var(--muted-foreground, GrayText); }
.hv-cols { display: grid; grid-template-columns: minmax(0, 1fr); gap: 8px 40px; align-items: start; }
@media (min-width: 900px) { .hv-cols { grid-template-columns: minmax(0, 5fr) minmax(0, 6fr); } }
.hv-org { margin: 18px 2px 0; font-size: 13px; font-weight: 650; color: var(--muted-foreground, GrayText); }
.hv h2 { margin: 22px 2px 6px; font-size: 15px; font-weight: 650; display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
.hv h2 span { font-size: 13px; font-weight: 400; color: var(--muted-foreground, GrayText); }
.hv ul { list-style: none; margin: 0; padding: 0; }
.hv-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: 12px; width: 100%; min-height: 56px;
  padding: 8px 10px; margin: 0; border: 0; border-radius: 12px; background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
.hv-row:hover { background: rgba(128, 128, 128, .12); }
.hv-row:focus-visible { outline: 2px solid var(--accent, Highlight); outline-offset: -2px; }
.hv-name { display: block; font-size: 16px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.hv-sub { display: block; margin-top: 2px; font-size: 13.5px; color: var(--muted-foreground, GrayText); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.hv-side { font-size: 13px; color: var(--muted-foreground, GrayText); font-variant-numeric: tabular-nums; text-align: right; white-space: nowrap; }
.hv-has { display: flex; gap: 5px; justify-content: flex-end; margin-top: 3px; }
.hv-has span { font-size: 11.5px; line-height: 1; padding: 3px 6px; border-radius: 6px; border: 1px solid var(--border, rgba(128,128,128,.35)); color: var(--muted-foreground, GrayText); }
.hv-has span[data-on] { border-color: transparent; background: color-mix(in srgb, var(--accent, Highlight) 16%, transparent); color: var(--foreground, CanvasText); }
.hv-next .hv-name { font-size: 18px; font-weight: 650; }
.hv-next { background: color-mix(in srgb, var(--accent, Highlight) 10%, transparent); }
.hv-empty { margin: 6px 2px; font-size: 14px; color: var(--muted-foreground, GrayText); }
";

    fn today() -> String {
        let d = js_sys::Date::new_0();
        format!(
            "{:04}-{:02}-{:02}",
            d.get_full_year(),
            d.get_month() + 1,
            d.get_date()
        )
    }

    #[component]
    fn SetRow(set: Setlist, next: bool, ask: Ask) -> Element {
        let path = set.path.clone();
        rsx! {
            li {
                button { r#type: "button", class: if next { "hv-row hv-next" } else { "hv-row" }, "data-testid": "home-setlist",
                    onclick: move |_| ask.call(("NOTE_OPEN".to_owned(), json!({ "path": path }))),
                    span { style: "min-width: 0;",
                        span { class: "hv-name", "{set.title}" }
                        span { class: "hv-sub", "{set_line(&set)}" }
                    }
                }
            }
        }
    }

    #[component]
    fn SongRow(song: Song, ask: Ask) -> Element {
        let path = song.path.clone();
        let key = song.keys().unwrap_or_default();
        let tempo = song
            .tempo
            .map(|t| format!("{t:.0} bpm"))
            .unwrap_or_default();
        let side = [key, tempo]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        let said = format!(
            "{}{}: {}, {}, {}",
            song.title,
            song.artist
                .as_ref()
                .map(|a| format!(", {a}"))
                .unwrap_or_default(),
            if song.chart {
                "has a chart"
            } else {
                "no chart"
            },
            if song.lyrics {
                "synced lyrics"
            } else {
                "no synced lyrics"
            },
            if song.recording {
                "a recording"
            } else {
                "no recording"
            },
        );
        rsx! {
            li {
                button { r#type: "button", class: "hv-row", "data-testid": "home-song", "aria-label": "{said}",
                    onclick: move |_| ask.call(("NOTE_OPEN".to_owned(), json!({ "path": path }))),
                    span { style: "min-width: 0;", "aria-hidden": "true",
                        span { class: "hv-name", "{song.title}" }
                        span { class: "hv-sub", {song.artist.clone().unwrap_or_default()} }
                    }
                    span { "aria-hidden": "true",
                        span { class: "hv-side", "{side}" }
                        span { class: "hv-has",
                            span { "data-on": if song.chart { "" }, "Chart" }
                            span { "data-on": if song.lyrics { "" }, "Lyrics" }
                            span { "data-on": if song.recording { "" }, "Audio" }
                        }
                    }
                }
            }
        }
    }

    #[component]
    fn Sets(org: Org, ask: Ask) -> Element {
        rsx! {
            if !org.coming.is_empty() {
                h2 { "Coming up" span { "{org.coming.len()}" } }
                ul { for (i, set) in org.coming.iter().cloned().enumerate() {
                    SetRow { key: "{set.path}", set, next: i == 0, ask }
                } }
            }
            if !org.earlier.is_empty() {
                h2 { if org.coming.is_empty() { "Setlists" } else { "Earlier" } span { "{org.earlier.len()}" } }
                ul { for set in org.earlier.iter().cloned() {
                    SetRow { key: "{set.path}", set, next: false, ask }
                } }
            }
            if org.coming.is_empty() && org.earlier.is_empty() {
                h2 { "Setlists" }
                p { class: "hv-empty", "No setlists in this vault yet." }
            }
            if !org.shared.is_empty() {
                h2 { "Shared by link" span { "{org.shared.len()}" } }
                ul { for (path, title, code) in org.shared.iter().cloned() {
                    li { key: "{code}",
                        button { r#type: "button", class: "hv-row", "data-testid": "home-shared",
                            onclick: move |_| ask.call(("NOTE_OPEN".to_owned(), json!({ "path": path }))),
                            span { style: "min-width: 0;",
                                span { class: "hv-name", "{title}" }
                                span { class: "hv-sub", "/s/{code}/" }
                            }
                        }
                    }
                } }
            }
        }
    }

    #[component]
    fn Libraries(org: Org, ask: Ask) -> Element {
        rsx! {
            for (name, songs) in org.libraries.iter().cloned() {
                h2 { key: "h{name}", "{name}" span { "{songs.len()} songs" } }
                ul { for song in songs {
                    SongRow { key: "{song.path}", song, ask }
                } }
            }
            if org.libraries.is_empty() {
                h2 { "Song libraries" }
                p { class: "hv-empty", "No songs in this vault yet." }
            }
        }
    }

    /// The dashboard.
    #[component]
    pub fn Home(note: Signal<Value>, ask: Ask) -> Element {
        let board: Board = board_of(&note.read(), &today());
        let many = board.orgs.len() > 1;
        let first = board
            .person
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_owned();
        rsx! {
            document::Style { {CSS} }
            div { class: "hv", "data-testid": "studio-home", "data-orgs": "{board.orgs.len()}",
                header { class: "hv-top",
                    h1 { if first.is_empty() { "Your sets and songs" } else { "{first}'s sets and songs" } }
                    if board.is_empty() {
                        p { "Reading your vaults…" }
                    }
                }
                if !board.recent.is_empty() {
                    h2 { "Recently opened" }
                    ul { for (path, title) in board.recent.iter().cloned() {
                        li { key: "{path}",
                            button { r#type: "button", class: "hv-row", "data-testid": "home-recent",
                                onclick: move |_| ask.call(("NOTE_OPEN".to_owned(), json!({ "path": path }))),
                                span { class: "hv-name", "{title}" }
                            }
                        }
                    } }
                }
                for org in board.orgs.iter().cloned() {
                    if many {
                        div { key: "o{org.slug}", class: "hv-org", "{org.slug}" }
                    }
                    div { class: "hv-cols",
                        div { Sets { org: org.clone(), ask } }
                        div { Libraries { org, ask } }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn note() -> Value {
        json!({ "home": {
            "person": "Ada Lovelace",
            "recent": ["Libraries/Christmas/Joy/Joy.md", "Gone.md"],
            "orgs": [{ "slug": "acme", "rows": [
                { "path": "Setlists/Eve.md", "type": "setlist", "properties": {
                    "title": "Christmas Eve", "date": "2026-12-24", "share": "abcdefghijkmnpqr",
                    "songs": ["[[Libraries/Christmas/Joy/Joy|Joy]]", "[[Libraries/Christmas/Night/Night|Night]]"] } },
                { "path": "Setlists/Recital.md", "type": "setlist", "properties": { "title": "Recital", "date": "2026-05-02", "songs": [] } },
                { "path": "Setlists/Someday.md", "type": "setlist", "properties": { "title": "Someday", "songs": [] } },
                { "path": "Setlists/Today.md", "type": "Setlist", "properties": { "title": "Tonight", "date": "2026-11-18", "songs": [] } },
                { "path": "Libraries/Christmas/Night/Night.md", "type": "song", "properties": { "title": "Night", "length": 200.0 } },
                { "path": "Libraries/Christmas/Joy/Joy.md", "type": "song", "properties": {
                    "title": "Joy", "artist": "Trad.", "key": "D", "tempo": 120.0, "length": 100.0,
                    "chart": "[[Joy.kf]]", "lyrics": "[[Joy.lrc]]", "media": ["[[renders/joy.ogg]]"] } },
                { "path": "Libraries/Rock/Fire/Fire.md", "type": "song", "properties": { "title": "Fire" } },
                { "path": "Notes/Todo.md", "type": "task", "properties": { "title": "Todo" } },
            ] }],
        } })
    }

    #[test]
    fn setlists_are_upcoming_first_then_the_latest() {
        let board = board_of(&note(), "2026-11-18");
        let org = &board.orgs[0];
        let titles = |sets: &[Setlist]| sets.iter().map(|s| s.title.clone()).collect::<Vec<_>>();
        // Today's is still coming up, before the one in December.
        assert_eq!(titles(&org.coming), ["Tonight", "Christmas Eve"]);
        // Then what has been, the undated last.
        assert_eq!(titles(&org.earlier), ["Recital", "Someday"]);
        assert_eq!(set_line(&org.coming[1]), "2026-12-24, 2 songs, 5:00");
        assert_eq!(
            org.shared,
            [(
                "Setlists/Eve.md".to_owned(),
                "Christmas Eve".to_owned(),
                "abcdefghijkmnpqr".to_owned()
            )]
        );
    }

    #[test]
    fn songs_are_in_their_libraries_with_what_each_has() {
        let board = board_of(&note(), "2026-11-18");
        let libraries = &board.orgs[0].libraries;
        assert_eq!(
            libraries
                .iter()
                .map(|(n, s)| (n.as_str(), s.len()))
                .collect::<Vec<_>>(),
            [("Christmas", 2), ("Rock", 1)]
        );
        let joy = &libraries[0].1[0];
        assert_eq!(joy.title, "Joy");
        assert!(joy.chart && joy.lyrics && joy.recording);
        let night = &libraries[0].1[1];
        assert!(!night.chart && !night.lyrics && !night.recording);
        assert_eq!(library_of("Loose/Song.md"), "Songs");
    }

    #[test]
    fn recents_are_the_notes_still_there_and_nothing_is_not_a_board() {
        let board = board_of(&note(), "2026-11-18");
        assert_eq!(
            board.recent,
            [(
                "Libraries/Christmas/Joy/Joy.md".to_owned(),
                "Joy".to_owned()
            )]
        );
        assert_eq!(board.person, "Ada Lovelace");
        assert!(!board.is_empty());
        assert!(board_of(&json!({}), "2026-11-18").is_empty());
        assert!(
            board_of(
                &json!({ "home": { "orgs": [{ "slug": "a", "rows": [] }] } }),
                "2026-11-18"
            )
            .is_empty()
        );
    }
}
