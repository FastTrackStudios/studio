//! The setlist view's contract with the app that hosts it, in one place.
//!
//! The props it reads and the asks it makes, against Architect's
//! now-playing contract `architect.playhead` v0.1
//! (FastTrackStudios/architect#589): the setlist is the extension's
//! *source*, its songs the source's *queue*, and the Now Playing bar,
//! which is Architect's, plays it. A revision of that contract is a change
//! to this file alone (`docs/architect-distribution.md`, "The setlist
//! view", writes the shapes down).
//!
//! **Props in** (data only; the frame has no network and no credential):
//!
//! - the playhead's state, flat, as #589 gives a frame: `playing`, `time`,
//!   `duration`, `index`, `item` (`{id, …}`, the current item as given) and
//!   the rest ([`now_of`] reads `item.id`, `time`, `duration`, `playing`;
//!   with no `item`, nothing is playing);
//! - `setlist`, the list as a host that has resolved it gives it:
//!   `{title, sections: [{name, items: [Item]}]}`, an item
//!   `{id, number, title, artist, art, key, moved, length, recording,
//!   media}` ([`Item`]); a section with no name is the songs before the
//!   first set;
//! - or `note`, the setlist's note as the app's note host gives the view
//!   that claims its type: `{path, title, type, properties, body}`; the
//!   list is read from the body's ```` ```studio-setlist ```` fence
//!   (`setlists::setlist::Line`, the share page's own reader), the title
//!   from `title` or `properties.title`. `setlist` wins when both are
//!   given.
//!
//! **Asks out** (`architectPage.ask(action, args)`), on its own source
//! only (the host says whose):
//!
//! - [`PLAYHEAD_SET`] `{name, queue: [{id, title, subtitle, duration,
//!   media}]}`: the setlist's songs that have a recording, in running
//!   order ([`Setlist::source`]); asked when the list is first drawn and
//!   whenever it changes. It never says `play`, so it never takes the bar
//!   over, and a song playing stays playing (same `id`).
//! - [`PLAYHEAD_ITEM`] `{id, play: true}`: a row pressed plays its song.
//! - [`PLAYHEAD_PLAY`] / [`PLAYHEAD_PAUSE`] (no args): the playing row
//!   pressed again pauses, a paused one plays on.

use serde_json::{Map, Value, json};
use setlists::row::{row_key, signed, tile_hue};
use setlists::setlist::{Line, SETLIST_FENCE, section_line};

/// Sets (replaces) the extension's source: its name and its queue.
pub const PLAYHEAD_SET: &str = "PLAYHEAD_SET";
/// Makes one item of the queue current, by its id (and plays it).
pub const PLAYHEAD_ITEM: &str = "PLAYHEAD_ITEM";
/// Plays on.
pub const PLAYHEAD_PLAY: &str = "PLAYHEAD_PLAY";
/// Pauses.
pub const PLAYHEAD_PAUSE: &str = "PLAYHEAD_PAUSE";

/// The most rows drawn: a setlist longer than this is cut (a source's
/// queue may hold 500, and one ask is at most 64 KiB).
pub const MAX_ITEMS: usize = 200;

/// The contract's limits on what a source says (#589 v0.1).
const NAME_MAX: usize = 64;
const TEXT_MAX: usize = 200;
const ID_MAX: usize = 256;

/// One song of the list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Item {
    /// Its id in the queue: its note, `#` its arrangement (unique in the
    /// list but for a song played twice, which is `…@<number>` the second
    /// time).
    pub id: String,
    /// Its place in the running order, from 1.
    pub number: usize,
    pub title: String,
    /// Who it is by; empty when unknown.
    pub artist: String,
    /// Its art, only as a `data:` or `blob:` URL (the frame loads nothing
    /// else); none, the tile is its colour and its key.
    pub art: Option<String>,
    /// The key as played (`E`, `E*`, `E→F`); empty for none.
    pub key: String,
    /// The key said in words, for a screen reader.
    pub key_said: String,
    /// How far the setlist moves it from its chart's key (`+1`, `−3`).
    pub moved: Option<String>,
    /// Its tile's hue (from its title, as the share page's).
    pub hue: u32,
    /// Seconds; none when unknown.
    pub length: Option<f64>,
    /// Whether it has a recording (only those are in the queue).
    pub recording: bool,
    /// Its recording, a vault path, when known (what the app plays).
    pub media: Option<String>,
}

/// A set of the list: its name (none for the songs before the first set)
/// and its songs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Section {
    pub name: Option<String>,
    pub items: Vec<Item>,
}

/// The list the view draws.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Setlist {
    pub title: String,
    pub sections: Vec<Section>,
}

impl Setlist {
    /// Every song, in running order.
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.sections.iter().flat_map(|s| s.items.iter())
    }

    /// The [`PLAYHEAD_SET`] ask's args: the songs with a recording, each
    /// with its recording (a vault path) or at least its length, as the
    /// contract wants one or the other.
    #[must_use]
    pub fn source(&self) -> Value {
        let queue: Vec<Value> = self
            .items()
            .filter(|i| {
                i.recording && i.id.len() <= ID_MAX && (i.media.is_some() || i.length.is_some())
            })
            .map(|i| {
                let mut item = Map::new();
                item.insert("id".to_owned(), json!(i.id));
                item.insert("title".to_owned(), json!(cut(&i.title, TEXT_MAX)));
                if !i.artist.is_empty() {
                    item.insert("subtitle".to_owned(), json!(cut(&i.artist, TEXT_MAX)));
                }
                if let Some(l) = i.length {
                    item.insert("duration".to_owned(), json!(l));
                }
                if let Some(m) = &i.media {
                    item.insert("media".to_owned(), json!(m));
                }
                Value::Object(item)
            })
            .collect();
        let name = if self.title.is_empty() {
            "Setlist"
        } else {
            &self.title
        };
        json!({"name": cut(name, NAME_MAX), "queue": queue})
    }
}

/// `s`, at most `max` characters.
fn cut(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// A recording the app may play: a vault path (no scheme, not absolute, no
/// `..`), as the contract takes `media`; anything else is left out rather
/// than failing the whole source.
fn vault_path(p: &str) -> Option<String> {
    let p = p.trim();
    let ok = !p.is_empty()
        && !p.contains(':')
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.split('/').any(|seg| seg == "..");
    ok.then(|| p.to_owned())
}

/// Where the source is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Now {
    /// The item current (playing or paused), by its id.
    pub id: String,
    pub time: f64,
    pub duration: f64,
    pub playing: bool,
}

impl Now {
    /// How much of it has played, 0 to 1.
    #[must_use]
    pub fn progress(&self) -> f64 {
        if self.duration > 0.0 {
            (self.time / self.duration).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned()
}

fn seconds(v: &Value, key: &str) -> Option<f64> {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite() && *n >= 0.0)
}

/// Art the frame can draw: a `data:image/…` or `blob:` URL, nothing else.
fn art(v: &Value) -> Option<String> {
    let a = text(v, "art");
    let lower = a.to_ascii_lowercase();
    (lower.starts_with("data:image/") || lower.starts_with("blob:")).then_some(a)
}

/// The list in the app's props: `setlist` if given, else read from
/// `note`; none for neither.
#[must_use]
pub fn setlist_of(props: &Value) -> Option<Setlist> {
    if let Some(s) = props.get("setlist").filter(|s| s.is_object()) {
        return Some(given(s));
    }
    props.get("note").filter(|n| n.is_object()).map(from_note)
}

/// A resolved `setlist` prop.
fn given(s: &Value) -> Setlist {
    let mut number = 0_usize;
    let sections = s
        .get("sections")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|sec| Section {
                    name: Some(text(sec, "name")).filter(|n| !n.is_empty()),
                    items: sec
                        .get("items")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .map(|i| {
                                    number = number.saturating_add(1);
                                    given_item(i, number)
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    capped(Setlist {
        title: text(s, "title"),
        sections,
    })
}

fn given_item(i: &Value, number: usize) -> Item {
    let title = text(i, "title");
    let key = text(i, "key");
    let id = Some(text(i, "id"))
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| format!("@{number}"));
    Item {
        id,
        number: i
            .get("number")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(number),
        artist: text(i, "artist"),
        art: art(i),
        key_said: if key.is_empty() {
            String::new()
        } else {
            setlists::keys::KeyLabel::parse(&key)
                .map(|k| k.spoken())
                .unwrap_or_default()
        },
        key,
        moved: Some(text(i, "moved")).filter(|m| !m.is_empty()),
        hue: tile_hue(&title),
        title,
        length: seconds(i, "length").filter(|l| *l > 0.0),
        media: Some(text(i, "media")).and_then(|m| vault_path(&m)),
        recording: i.get("recording").and_then(Value::as_bool).unwrap_or(false)
            || i.get("media")
                .and_then(Value::as_str)
                .is_some_and(|m| !m.is_empty()),
    }
}

/// The list read from a setlist note's fence.
fn from_note(note: &Value) -> Setlist {
    let title = Some(text(note, "title"))
        .filter(|t| !t.is_empty())
        .or_else(|| {
            note.get("properties")
                .map(|p| text(p, "title"))
                .filter(|t| !t.is_empty())
        })
        .unwrap_or_default();
    let body = note.get("body").and_then(Value::as_str).unwrap_or("");
    let mut sections = vec![Section::default()];
    let mut seen = std::collections::BTreeSet::new();
    let mut inside = false;
    for line in body.lines() {
        if let Some(info) = line.trim_start().strip_prefix("```") {
            inside = !inside && info.split_whitespace().next() == Some(SETLIST_FENCE);
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(name) = section_line(line) {
            sections.push(Section {
                name: Some(name.to_owned()),
                items: Vec::new(),
            });
            continue;
        }
        let Some(row) = Line::parse(line) else {
            continue;
        };
        let mut id = row
            .arrangement
            .as_ref()
            .map_or_else(|| row.note.clone(), |a| format!("{}#{a}", row.note));
        if !seen.insert(id.clone()) {
            id = format!("{id}@{}", row.number);
        }
        let key = row_key(&row);
        let item = Item {
            id,
            number: row.number,
            artist: row.artist.clone().unwrap_or_default(),
            art: None,
            key: key.as_ref().map(|k| k.label.text()).unwrap_or_default(),
            key_said: key
                .as_ref()
                .map(setlists::row::RowKey::spoken)
                .unwrap_or_default(),
            moved: key.and_then(|k| k.moved).map(|(_, n)| signed(n)),
            hue: tile_hue(&row.label),
            title: row.label,
            length: row.length,
            media: row.recording.as_deref().and_then(vault_path),
            recording: row.recording.is_some(),
        };
        if let Some(last) = sections.last_mut() {
            last.items.push(item);
        }
    }
    sections.retain(|s| s.name.is_some() || !s.items.is_empty());
    capped(Setlist { title, sections })
}

/// At most [`MAX_ITEMS`] songs.
fn capped(mut s: Setlist) -> Setlist {
    let mut left = MAX_ITEMS;
    for sec in &mut s.sections {
        sec.items.truncate(left);
        left = left.saturating_sub(sec.items.len());
    }
    s
}

/// Where the source is, from the playhead's state in the props.
///
/// Its current `item`; none when nothing is current, or when the current
/// item has not started (a source just set is at its first item, paused
/// at 0: nothing is marked as playing until something plays).
#[must_use]
pub fn now_of(props: &Value) -> Option<Now> {
    let id = props.get("item").map(|i| text(i, "id")).unwrap_or_default();
    let now = Now {
        id,
        time: seconds(props, "time").unwrap_or(0.0),
        duration: seconds(props, "duration").unwrap_or(0.0),
        playing: props
            .get("playing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    (!now.id.is_empty() && (now.playing || now.time > 0.0)).then_some(now)
}

/// What pressing `item`'s row asks, given where the source is: the playing
/// song pauses, a paused one plays on, any other plays from its start; a
/// song with no recording asks nothing.
#[must_use]
pub fn press(item: &Item, now: Option<&Now>) -> Option<(&'static str, Value)> {
    if !item.recording {
        return None;
    }
    Some(match now {
        Some(n) if n.id == item.id && n.playing => (PLAYHEAD_PAUSE, Value::Null),
        Some(n) if n.id == item.id => (PLAYHEAD_PLAY, Value::Null),
        _ => (PLAYHEAD_ITEM, json!({"id": item.id, "play": true})),
    })
}

/// `m:ss`.
#[must_use]
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).floor();
    let m = (s / 60.0).floor();
    format!("{m}:{:02}", m.mul_add(-60.0, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "Intro words.\n\n```studio-setlist downloads\n\
        1 | Joy to the World | Libraries/C/Joy/Joy.md | D · 120 bpm | Libraries/C/Joy/Media/joy.ogg | Isaac Watts | 151.0 | D |\n\
        ## Tree Lighting\n\
        2 | Silent Night | Libraries/C/Silent/Silent.md#Choir | E · 60 bpm | | Franz Gruber | 200.0 | E | F\n\
        3 | Joy to the World | Libraries/C/Joy/Joy.md | | Libraries/C/Joy/Media/joy.ogg | | | |\n\
        ```\n";

    fn note() -> Value {
        json!({"note": {"path": "Setlists/Eve.md", "title": "", "type": "setlist",
                        "properties": {"title": "Christmas Eve"}, "body": NOTE}})
    }

    #[test]
    fn a_setlist_note_is_read_into_sets_and_rows() {
        let s = setlist_of(&note()).unwrap();
        assert_eq!(s.title, "Christmas Eve");
        assert_eq!(s.sections.len(), 2);
        assert_eq!(s.sections[0].name, None);
        assert_eq!(s.sections[1].name.as_deref(), Some("Tree Lighting"));
        let joy = &s.sections[0].items[0];
        assert_eq!(
            (
                joy.id.as_str(),
                joy.number,
                joy.artist.as_str(),
                joy.key.as_str()
            ),
            ("Libraries/C/Joy/Joy.md", 1, "Isaac Watts", "D")
        );
        assert!(joy.recording);
        assert_eq!(joy.length, Some(151.0));
        // A song with no recording stays listed; its key moved by the set.
        let silent = &s.sections[1].items[0];
        assert!(!silent.recording);
        assert_eq!(silent.id, "Libraries/C/Silent/Silent.md#Choir");
        assert_eq!(
            (silent.key.as_str(), silent.moved.as_deref()),
            ("F", Some("+1"))
        );
        // The same song twice: a second id.
        assert_eq!(s.sections[1].items[1].id, "Libraries/C/Joy/Joy.md@3");
    }

    #[test]
    fn the_source_is_the_songs_with_a_recording() {
        let q = setlist_of(&note()).unwrap().source();
        let ids: Vec<&str> = q["queue"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["Libraries/C/Joy/Joy.md", "Libraries/C/Joy/Joy.md@3"]);
        assert_eq!(q["name"], "Christmas Eve");
        assert_eq!(
            q["queue"][0],
            json!({"id": "Libraries/C/Joy/Joy.md", "title": "Joy to the World", "subtitle": "Isaac Watts",
                   "duration": 151.0, "media": "Libraries/C/Joy/Media/joy.ogg"})
        );
        // No artist, no length: its recording alone.
        assert_eq!(q["queue"][1].get("subtitle"), None);
        assert_eq!(q["queue"][1].get("duration"), None);
    }

    #[test]
    fn a_resolved_setlist_wins_and_takes_only_drawable_art() {
        let mut props = note();
        props["setlist"] = json!({"title": "Given", "sections": [{"name": "", "items": [
            {"id": "a", "title": "A", "artist": "X", "art": "data:image/png;base64,AA==", "key": "C", "length": 60, "recording": true},
            {"id": "b", "title": "B", "art": "https://elsewhere/b.png", "media": "https://elsewhere/b.ogg", "length": 5}
        ]}]});
        let s = setlist_of(&props).unwrap();
        assert_eq!(s.title, "Given");
        let items: Vec<&Item> = s.items().collect();
        assert_eq!(items[0].art.as_deref(), Some("data:image/png;base64,AA=="));
        assert_eq!(items[0].key_said, "Key of C");
        assert_eq!(items[1].art, None);
        // A URL is no vault path: listed, never sent as media.
        assert_eq!(items[1].media, None);
        assert!(items[1].recording);
        assert_eq!(
            s.source()["queue"][1],
            json!({"id": "b", "title": "B", "duration": 5.0})
        );
        assert_eq!(items[1].number, 2);
    }

    #[test]
    fn a_row_press_plays_pauses_or_plays_on() {
        let s = setlist_of(&note()).unwrap();
        let joy = s.sections[0].items[0].clone();
        let silent = s.sections[1].items[0].clone();
        assert_eq!(
            press(&joy, None),
            Some((
                PLAYHEAD_ITEM,
                json!({"id": "Libraries/C/Joy/Joy.md", "play": true})
            ))
        );
        let mut now = Now {
            id: joy.id.clone(),
            time: 3.0,
            duration: 151.0,
            playing: true,
        };
        assert_eq!(press(&joy, Some(&now)), Some((PLAYHEAD_PAUSE, Value::Null)));
        now.playing = false;
        assert_eq!(press(&joy, Some(&now)), Some((PLAYHEAD_PLAY, Value::Null)));
        assert_eq!(press(&silent, Some(&now)), None);
    }

    #[test]
    fn where_the_source_is() {
        // The playhead's state, flat (#589).
        let state = json!({"source": "studio.transport", "playing": true, "time": 30, "at": 1,
                           "duration": 120, "index": 0, "tracks": 2, "item": {"id": "a", "title": "A"}});
        let now = now_of(&state).unwrap();
        assert_eq!((now.id.as_str(), now.playing), ("a", true));
        assert!((now.progress() - 0.25).abs() < 1e-9);
        assert_eq!(now_of(&json!({"time": 3, "playing": true})), None);
        // Set, not started: nothing marked.
        assert_eq!(
            now_of(&json!({"time": 0, "playing": false, "item": {"id": "a"}})),
            None
        );
        assert_eq!(clock(151.4), "2:31");
    }
}
