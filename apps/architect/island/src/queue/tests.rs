//! The setlist view's contract, held against the host's own reading of it.
//!
//! [`host`] is architect's `apps/web/src/ext_host.rs` (`said`,
//! `ask_refused`, `field`, `required`, `seconds`, `vault_path`,
//! `section_of`, `mark_of`, `item_of`, `source_of`, `intent`'s
//! `PLAYHEAD_SET` and `PLAYHEAD_ITEM`) as of `feat/607-favourites` 8760f6c9
//! (`web/ext-note-embeds` caaea842 under it), reduced to what it decides:
//! taken or refused, and why. Its own accept/refuse cases
//! (`a_source_is_what_the_contract_bounds_or_refused_whole`,
//! `asks_are_the_playheads_intents_and_rate_limited`) are ported first, so
//! the model is held to the host before the view is held to the model.
//!
//! Its media path rule and its time bound are Architect main's
//! `architect_playhead::model` (`vault_path`, `refused_char`,
//! `MAX_SECONDS`; FastTrackStudios/architect `features/playhead`), which
//! the host re-exports; that model's own refuse cases
//! (`media_and_art_are_vault_paths_never_urls`) are ported with it.

use std::fmt::Write as _;

use super::*;

/// No asks.
const NONE: Vec<Ask> = Vec::new();

pub mod host {
    use std::collections::BTreeSet;

    use serde_json::Value;

    pub const MAX_MESSAGE: usize = 64 * 1024;
    pub const QUEUE_MAX: usize = 500;
    pub const SECTIONS_MAX: usize = 200;
    pub const MARKS_MAX: usize = 2000;
    /// The latest time anything of a source is (a day).
    pub const MAX_SECONDS: f64 = 86_400.0;
    pub const INTENTS: &[&str] = &[
        "PLAYHEAD_SEEK",
        "PLAYHEAD_PLAY",
        "PLAYHEAD_PAUSE",
        "PLAYHEAD_NEXT",
        "PLAYHEAD_PREVIOUS",
        "PLAYHEAD_SET",
        "PLAYHEAD_ITEM",
    ];

    pub const fn ask_refused(ready: bool, hidden: bool) -> Option<&'static str> {
        if !ready {
            Some("not ready")
        } else if hidden {
            Some("the page is hidden")
        } else {
            None
        }
    }

    fn field(v: &Value, k: &str, max: usize) -> Result<Option<String>, String> {
        match v.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) if s.chars().count() <= max => {
                Ok(Some(s.clone()).filter(|s| !s.is_empty()))
            }
            Some(_) => Err(format!("{k} is at most {max} characters")),
        }
    }

    fn required(v: &Value, k: &str, max: usize) -> Result<String, String> {
        field(v, k, max)?.ok_or_else(|| format!("{k} is required"))
    }

    fn seconds(v: &Value, k: &str) -> Result<Option<f64>, String> {
        match v.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(n) => n
                .as_f64()
                .filter(|d| d.is_finite() && (0.0..=MAX_SECONDS).contains(d))
                .map(Some)
                .ok_or_else(|| format!("{k} is not a time in seconds")),
        }
    }

    pub fn vault_path(p: &str) -> bool {
        !p.is_empty()
            && p.len() <= 1024
            && !p.chars().any(refused_char)
            && p.split('/').all(|s| {
                !s.is_empty() && s != "." && s != ".." && !s.ends_with('.') && !s.ends_with(' ')
            })
    }

    fn refused_char(c: char) -> bool {
        matches!(c, ':' | '\\' | '?' | '#' | '%')
            || c.is_control()
            || matches!(
                c,
                '\u{FF0F}'
                    | '\u{2215}'
                    | '\u{2044}'
                    | '\u{29F8}'
                    | '\u{FF0E}'
                    | '\u{2024}'
                    | '\u{FE52}'
            )
            || matches!(
                c,
                '\u{00AD}'
                    | '\u{061C}'
                    | '\u{180E}'
                    | '\u{200B}'..='\u{200F}'
                    | '\u{202A}'..='\u{202E}'
                    | '\u{2060}'..='\u{2064}'
                    | '\u{2066}'..='\u{206F}'
                    | '\u{FEFF}'
                    | '\u{FFF9}'..='\u{FFFB}'
            )
    }

    fn color_ok(c: &str) -> bool {
        c.len() == 7
            && c.strip_prefix('#')
                .is_some_and(|h| h.chars().all(|c| c.is_ascii_hexdigit()))
    }

    fn section_of(v: &Value) -> Result<(), String> {
        let start = seconds(v, "start")?.ok_or("a section's start is required")?;
        let end = seconds(v, "end")?.ok_or("a section's end is required")?;
        let color = field(v, "color", 7)?;
        if end < start {
            return Err("a section ends before it starts".to_owned());
        }
        if color.as_deref().is_some_and(|c| !color_ok(c)) {
            return Err("a section's color is #rrggbb".to_owned());
        }
        required(v, "name", 40).map(|_| ())
    }

    fn mark_of(v: &Value) -> Result<(), String> {
        seconds(v, "at")?.ok_or("a mark's at is required")?;
        required(v, "text", 300)?;
        field(v, "kind", 32).map(|_| ())
    }

    fn list_of(
        v: &Value,
        k: &str,
        max: usize,
        one: fn(&Value) -> Result<(), String>,
    ) -> Result<(), String> {
        match v.get(k) {
            None | Some(Value::Null) => Ok(()),
            Some(Value::Array(list)) if list.len() <= max => list.iter().try_for_each(one),
            Some(Value::Array(_)) => Err(format!("{k}: at most {max}")),
            Some(_) => Err(format!("{k} is a list")),
        }
    }

    fn item_of(i: &Value) -> Result<String, String> {
        let id = field(i, "id", 256)?.ok_or("an item's id is 1 to 256 characters")?;
        let media = field(i, "media", 1024)?;
        let art = field(i, "art", 1024)?;
        if media.as_deref().is_some_and(|m| !vault_path(m)) {
            return Err("media must be a vault path".to_owned());
        }
        if art.as_deref().is_some_and(|a| !vault_path(a)) {
            return Err("art must be a vault path".to_owned());
        }
        let duration = match i.get("duration") {
            None | Some(Value::Null) => None,
            Some(d) => Some(
                d.as_f64()
                    .filter(|d| d.is_finite() && *d > 0.0)
                    .ok_or("duration is a length in seconds")?,
            ),
        };
        if media.is_none() && duration.is_none() {
            return Err("an item without media needs a duration".to_owned());
        }
        required(i, "title", 200)?;
        field(i, "subtitle", 200)?;
        list_of(i, "sections", SECTIONS_MAX, section_of)?;
        list_of(i, "marks", MARKS_MAX, mark_of)?;
        // The host's tempo (#660): bpm 20 to 400, a downbeat at 0 or
        // more, 1 to 16 beats a bar.
        if let Some(t) = i.get("tempo").filter(|t| !t.is_null()) {
            let bpm = t.get("bpm").and_then(Value::as_f64).unwrap_or(f64::NAN);
            let at = t.get("at").map_or(Some(0.0), Value::as_f64).unwrap_or(-1.0);
            let meter = t.get("meter").map_or(Some(4), Value::as_u64).unwrap_or(0);
            if !(20.0..=400.0).contains(&bpm)
                || !at.is_finite()
                || !(0.0..=MAX_SECONDS).contains(&at)
                || !(1..=16).contains(&meter)
            {
                return Err("tempo is not one".to_owned());
            }
        }
        Ok(id)
    }

    /// The queue's ids, or why the whole source is refused.
    pub fn source_of(args: &Value) -> Result<Vec<String>, String> {
        field(args, "name", 64)?;
        let items = args
            .get("queue")
            .and_then(Value::as_array)
            .ok_or("queue is required")?;
        if items.len() > QUEUE_MAX {
            return Err(format!("queue: at most {QUEUE_MAX} items"));
        }
        let mut ids = BTreeSet::new();
        let mut queue = Vec::with_capacity(items.len());
        for i in items {
            let id = item_of(i)?;
            if !ids.insert(id.clone()) {
                return Err(format!("item {id} is in the queue twice"));
            }
            queue.push(id);
        }
        if let Some(n) = args.get("index").filter(|n| !n.is_null()) {
            n.as_u64().ok_or("index is a place in the queue")?;
        }
        seconds(args, "time")?;
        // At most two of the bar's slots, each once (#660).
        if let Some(slots) = args.get("slots").filter(|s| !s.is_null()) {
            let list: Vec<&str> = slots
                .as_array()
                .ok_or("slots is a list")?
                .iter()
                .map(|s| {
                    s.as_str()
                        .filter(|s| matches!(*s, "click" | "open"))
                        .ok_or("slots: click or open")
                })
                .collect::<Result<_, _>>()?;
            if list.len() > 2 || (list.len() == 2 && list[0] == list[1]) {
                return Err("slots: at most 2, each once".to_owned());
            }
        }
        Ok(queue)
    }

    /// An ask read from a frame's message: dropped unread past the limit.
    pub fn said(json: &str) -> Option<(String, Value)> {
        if json.len() > MAX_MESSAGE {
            return None;
        }
        let v: Value = serde_json::from_str(json).ok()?;
        if v.get("t").and_then(Value::as_str) != Some("ask") {
            return None;
        }
        let a = v.get("action").and_then(Value::as_str)?;
        (a.len() <= 64).then(|| (a.to_owned(), v.get("args").cloned().unwrap_or(Value::Null)))
    }

    /// The ask as the host takes it from a frame of `caller`: `Ok` with the
    /// queue's ids for a `SET`, the item's id for an `ITEM`, else empty.
    pub fn intent(action: &str, args: &Value, caller: &str) -> Result<Vec<String>, String> {
        if !INTENTS.contains(&action) {
            return Err(format!("{action} is not something a view may ask"));
        }
        match args.get("source") {
            None | Some(Value::Null) => {}
            Some(Value::String(s)) if s == caller => {}
            Some(_) => return Err("not your source".to_owned()),
        }
        match action {
            "PLAYHEAD_SET" => source_of(args),
            "PLAYHEAD_ITEM" => match (field(args, "id", 256)?, args.get("index")) {
                (Some(id), _) => Ok(vec![id]),
                (None, Some(n)) => n
                    .as_u64()
                    .map(|n| vec![n.to_string()])
                    .ok_or_else(|| "index is a place in the queue".to_owned()),
                (None, None) => Err("an id or an index is required".to_owned()),
            },
            _ => Ok(Vec::new()),
        }
    }

    /// What the host does with `action`/`args` posted by a frame of
    /// `caller`: `None` dropped unread, else the intent's answer.
    pub fn takes(action: &str, args: &Value, caller: &str) -> Option<Result<Vec<String>, String>> {
        let (a, args) = said(&super::message(action, args))?;
        Some(intent(&a, &args, caller))
    }

    /// The queue's ids a `SET` from this extension sets, or none.
    pub fn set_ids(args: &Value) -> Option<Vec<String>> {
        takes("PLAYHEAD_SET", args, super::SOURCE)?.ok()
    }
}

/// The host's own cases, on the model.
#[test]
fn the_model_takes_and_refuses_what_the_host_does() {
    let one = |extra: Value| {
        let mut i = json!({"id": "a", "title": "A", "duration": 2});
        for (k, v) in extra.as_object().unwrap() {
            i[k] = v.clone();
        }
        host::source_of(&json!({"name": "N", "queue": [i]}))
    };
    let set = |queue: Value| host::source_of(&json!({"name": "N", "queue": queue}));
    assert_eq!(
        host::source_of(
            &json!({"name": "N", "index": 1, "time": 4.5, "play": true, "queue": [
                {"id": "a", "title": "A", "subtitle": "S", "media": "Media/a.ogg", "art": "Art/a b.jpg",
                 "sections": [{"name": "Verse 1", "start": 0, "end": 20.5, "color": "#2563eb"}],
                 "marks": [{"at": 1.5, "text": "Amazing grace", "kind": "line"}]},
                {"id": "b", "title": "B", "duration": 3},
            ]})
        ),
        Ok(vec!["a".to_owned(), "b".to_owned()])
    );
    assert_eq!(set(json!([])), Ok(Vec::new()));
    let at = |n: usize| "x".repeat(n);
    for (k, max) in [("id", 256), ("title", 200), ("subtitle", 200)] {
        assert!(one(json!({ k: at(max) })).is_ok(), "{k} of {max}");
        assert!(
            one(json!({ k: at(max + 1) })).is_err(),
            "{k} of {}",
            max + 1
        );
    }
    assert!(host::source_of(&json!({"name": at(64), "queue": []})).is_ok());
    assert!(host::source_of(&json!({"name": at(65), "queue": []})).is_err());
    for bad in [
        json!({"sections": [{"name": at(41), "start": 0, "end": 1}]}),
        json!({"sections": [{"name": "S", "start": 2, "end": 1}]}),
        json!({"sections": [{"name": "S", "start": 0, "end": 1, "color": "red"}]}),
        json!({"marks": [{"at": 0, "text": at(301)}]}),
        json!({"marks": [{"at": -1, "text": "t"}]}),
        json!({"art": "https://evil.test/a.jpg"}),
        json!({"media": "https://x/a.ogg"}),
        json!({"media": "../a.ogg"}),
        json!({"media": "/a.ogg"}),
        json!({"media": "Songs/%2e%2e/%2e%2e/secret.ogg"}),
        json!({"media": "a.ogg?x"}),
        json!({"media": "a.ogg#t=1"}),
        json!({"media": "a\u{1}.ogg"}),
        json!({"media": "Songs//a.ogg"}),
        json!({"media": "Songs/./a.ogg"}),
        // 1024 bytes at most: 600 two-byte characters are 1200.
        json!({"media": "é".repeat(600)}),
        json!({"duration": 0}),
        json!({"id": "é".repeat(257)}),
        json!({"title": ""}),
    ] {
        assert!(one(bad.clone()).is_err(), "{bad}");
    }
    assert!(one(json!({"id": "é".repeat(256)})).is_ok());
    assert!(one(json!({"media": "Songs/A song (live).m4a"})).is_ok());
    for bad in [
        json!([{"title": "no id", "duration": 1}]),
        json!([{"id": "a", "duration": 1}]),
        json!([{"id": "a", "title": "A"}]),
        json!([{"id": "a", "title": "A", "duration": 1}, {"id": "a", "title": "B", "duration": 1}]),
    ] {
        assert!(set(bad.clone()).is_err(), "{bad}");
    }
    let many = |n: usize| -> Value {
        (0..n)
            .map(|n| json!({"id": n.to_string(), "title": "t", "duration": 1}))
            .collect()
    };
    assert!(set(many(host::QUEUE_MAX)).is_ok());
    assert!(set(many(host::QUEUE_MAX + 1)).is_err());
    // A full queue of short items fits one message.
    assert!(host::set_ids(&json!({"name": "N", "queue": many(host::QUEUE_MAX)})).is_some());
    assert_eq!(
        host::intent("PLAYHEAD_PLAY", &json!({"source": "other.ext"}), SOURCE),
        Err("not your source".to_owned())
    );
    assert!(host::intent("PLAYHEAD_ITEM", &json!({}), SOURCE).is_err());
    assert!(host::intent("VAULT_READ", &json!({"path": "a"}), SOURCE).is_err());
    let big = json!({"name": "N", "queue": [{"id": "a", "title": "t", "subtitle": "x".repeat(200), "duration": 1}]});
    assert!(host::takes(PLAYHEAD_SET, &big, SOURCE).is_some());
    let past = json!({"pad": "x".repeat(host::MAX_MESSAGE), "name": "N", "queue": []});
    assert!(host::takes(PLAYHEAD_SET, &past, SOURCE).is_none());
    assert_eq!(host::ask_refused(false, false), Some("not ready"));
    assert_eq!(host::ask_refused(true, true), Some("the page is hidden"));
    assert_eq!(host::ask_refused(true, false), None);
}

/// Architect main's model's own cases (`architect_playhead::model`), on
/// the host model: its media path rule and its day.
#[test]
fn the_model_refuses_what_architects_playhead_model_does() {
    let one = |extra: Value| {
        let mut i = json!({"id": "a", "title": "A", "duration": 2});
        for (k, v) in extra.as_object().unwrap() {
            i[k] = v.clone();
        }
        host::source_of(&json!({"name": "N", "queue": [i]}))
    };
    for bad in [
        // Architect main's model (#659 re-check N3): look-alikes of `/`
        // and `.`, a segment ending in a dot or a space, invisible
        // characters; and its time bound, a day.
        json!({"media": "Recordings\u{FF0F}..\u{FF0F}secret.m4a"}),
        json!({"media": "Recordings/\u{FF0E}\u{FF0E}/secret.m4a"}),
        json!({"media": "Recordings\u{2215}a.m4a"}),
        json!({"media": "Recordings\u{2044}a.m4a"}),
        json!({"media": "Recordings/a\u{2024}m4a"}),
        json!({"media": "Recordings./a.m4a"}),
        json!({"media": "Recordings/a.m4a."}),
        json!({"media": "Recordings /a.m4a"}),
        json!({"media": "Recordings/a.m4a "}),
        json!({"media": "Recordings/\u{200B}a.m4a"}),
        json!({"media": "Recordings/a\u{202E}a4m.mp3"}),
        json!({"media": "\u{FEFF}Recordings/a.m4a"}),
        json!({"media": "Recordings/a\u{00AD}b.m4a"}),
        json!({"art": "Songs/Harry Connick Jr./cover.jpg"}),
        json!({"media": "a".repeat(1025)}),
        json!({"sections": [{"name": "S", "start": 0, "end": 86_400.5}]}),
        json!({"marks": [{"at": 1e9, "text": "t"}]}),
        json!({"tempo": {"bpm": 120, "at": 1e9, "meter": 4}}),
    ] {
        assert!(one(bad.clone()).is_err(), "{bad}");
    }
    assert!(one(json!({"media": "a".repeat(1024)})).is_ok());
    assert!(one(json!({"media": "Songs/Merry & Bright/01 – Joy.flac"})).is_ok());
    assert!(one(json!({"tempo": {"bpm": 120, "at": 86_400, "meter": 4}})).is_ok());
    assert!(host::source_of(&json!({"queue": [], "time": 86_400.5})).is_err());
}

const NOTE: &str = "Intro words.\n\n```studio-setlist downloads\n\
    1 | Joy to the World | Libraries/C/Joy/Joy.md | D · 120 bpm | Libraries/C/Joy/Media/joy.ogg | Isaac Watts | 151.0 | D |\n\
    ## Tree Lighting\n\
    2 | Silent Night | Libraries/C/Silent/Silent.md#Choir | E · 60 bpm | | Franz Gruber | 200.0 | E | F\n\
    3 | Joy to the World | Libraries/C/Joy/Joy.md | | Libraries/C/Joy/Media/joy.ogg | | | |\n\
    ```\n";

fn note_at(path: &str, body: &str) -> Value {
    json!({"note": {"path": path, "title": "", "type": "setlist",
                    "properties": {"title": "Christmas Eve"}, "body": body}})
}

fn note() -> Value {
    note_at("Setlists/Eve.md", NOTE)
}

/// The fence of a setlist of `n` songs as Studio writes them, with
/// realistic paths.
fn long_fence(n: usize) -> String {
    let mut body = String::from("```studio-setlist\n");
    for i in 1..=n {
        let _ = writeln!(
            body,
            "{i} | Song {i} | Libraries/Christmas/Song {i}/Song {i}.md | D · 120 bpm | \
             Libraries/Christmas/Song {i}/Media/Song {i} (live mix).m4a | Some Artist | 200.0 | D |"
        );
    }
    body.push_str("```\n");
    body
}

fn item(id: &str) -> Item {
    Item {
        id: id.to_owned(),
        number: 1,
        title: "T".to_owned(),
        length: Some(60.0),
        recording: true,
        ..Item::default()
    }
}

fn list_of(items: Vec<Item>) -> Setlist {
    Setlist {
        scope: "S.md".to_owned(),
        title: "S".to_owned(),
        sections: vec![Section { name: None, items }],
        withheld: false,
    }
}

/// `list`'s source, and that the host takes exactly it.
fn taken(list: &Setlist) -> Source {
    let s = list.source(MESSAGE_MAX);
    assert!(s.bytes <= host::MAX_MESSAGE, "{}", s.bytes);
    assert_eq!(s.bytes, message(PLAYHEAD_SET, &s.args).len());
    assert_eq!(
        host::takes(PLAYHEAD_SET, &s.args, SOURCE),
        Some(Ok(s.ids.clone())),
        "the host refuses {}",
        s.args
    );
    s
}

#[test]
fn a_setlist_note_is_read_into_sets_and_rows() {
    let s = setlist_of(&note()).unwrap();
    assert_eq!(
        (s.title.as_str(), s.scope.as_str()),
        ("Christmas Eve", "Setlists/Eve.md")
    );
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
        (
            "Setlists/Eve.md|Libraries/C/Joy/Joy.md",
            1,
            "Isaac Watts",
            "D"
        )
    );
    assert!(joy.recording);
    assert_eq!(joy.length, Some(151.0));
    let silent = &s.sections[1].items[0];
    assert!(!silent.recording);
    assert_eq!(
        silent.id,
        "Setlists/Eve.md|Libraries/C/Silent/Silent.md#Choir"
    );
    assert_eq!(
        (silent.key.as_str(), silent.moved.as_deref()),
        ("F", Some("+1"))
    );
    assert_eq!(
        s.sections[1].items[1].id,
        "Setlists/Eve.md|Libraries/C/Joy/Joy.md@3"
    );
}

/// The fixture (`fixtures/Carol Night.md`): two sets, a song transposed, one
/// with no recording, one played twice.
#[test]
fn the_fixture_reads_and_queues_as_the_host_takes_it() {
    let body = include_str!("../../fixtures/Carol Night.md");
    let s = setlist_of(&note_at("Setlists/Carol Night.md", body)).unwrap();
    let names: Vec<_> = s.sections.iter().map(|s| s.name.as_deref()).collect();
    assert_eq!(names, [Some("Opening Set"), Some("Tree Lighting")]);
    assert_eq!(s.items().count(), 7);
    let src = taken(&s);
    let numbers: Vec<usize> = s
        .items()
        .filter(|i| src.ids.contains(&i.id))
        .map(|i| i.number)
        .collect();
    assert_eq!(numbers, [1, 2, 3, 5, 6, 7]);
    assert!(src.ids[5].ends_with("Joy to the World.md@7"));
}

#[test]
fn the_same_song_three_times_is_three_ids_the_host_takes() {
    // Review probe p1: the same song, the same row number twice.
    let body = "```studio-setlist\n\
        1 | A | A.md | | | | 60 | |\n\
        2 | A | A.md | | | | 60 | |\n\
        2 | A | A.md | | | | 60 | |\n```\n";
    let s = setlist_of(&note_at("S.md", body)).unwrap();
    let ids: Vec<&str> = s.items().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["S.md|A.md", "S.md|A.md@2", "S.md|A.md@3"]);
    assert_eq!(taken(&s).ids.len(), 3);
}

#[test]
fn every_source_is_one_the_host_takes_whole() {
    let mut no_title = item("no-title");
    no_title.title.clear();
    let mut long_media = item("long-media");
    long_media.media = Some(format!("{}.ogg", "a".repeat(1021)));
    let mut long_media_only = long_media.clone();
    long_media_only.id = "long-media-only".to_owned();
    long_media_only.length = None;
    let mut zero = item("zero");
    zero.length = Some(0.0001);
    let mut huge = item("huge");
    huge.length = Some(LENGTH_MAX);
    let mut wide_media = item("wide-media");
    wide_media.media = Some("é".repeat(600));
    let wide = item(&"é".repeat(256));
    let too_wide = item(&"é".repeat(257));
    let twice = item("no-title");
    let mut nothing = item("nothing");
    nothing.length = None;
    let mut long_title = item("long-title");
    long_title.title = "t".repeat(500);
    long_title.artist = "a".repeat(500);
    let list = list_of(vec![
        no_title,
        long_media,
        long_media_only,
        wide_media,
        zero,
        huge,
        wide,
        too_wide,
        twice,
        nothing,
        long_title,
    ]);
    let src = taken(&list);
    assert_eq!(
        src.ids,
        [
            "no-title",
            "long-media",
            "wide-media",
            "huge",
            &"é".repeat(256),
            "long-title"
        ]
    );
    // An empty title is said by its number.
    assert_eq!(src.args["queue"][0]["title"], "Song 1");
    // A path past 1024 bytes is left out (in characters or not); its
    // length plays.
    assert_eq!(src.args["queue"][1].get("media"), None);
    assert_eq!(src.args["queue"][2].get("media"), None);
}

#[test]
fn two_hundred_songs_fit_one_ask_and_a_longer_one_is_cut_from_the_tail() {
    // Review probe p3: 200 songs with realistic paths.
    let s = setlist_of(&note_at("Setlists/Big Night.md", &long_fence(200))).unwrap();
    assert_eq!(s.items().count(), MAX_ITEMS);
    let big = taken(&s);
    assert_eq!(big.ids.len(), 200);
    // Paths near the limit: past 64 KiB, the tail is cut and the rest
    // still taken.
    let mut long = s;
    for i in long.sections.iter_mut().flat_map(|s| s.items.iter_mut()) {
        i.media = Some(format!("{}/{}.m4a", "M".repeat(900), i.number));
    }
    let cut = taken(&long);
    assert!(
        cut.ids.len() > 40 && cut.ids.len() < 200,
        "{}",
        cut.ids.len()
    );
    let first: Vec<String> = long
        .items()
        .take(cut.ids.len())
        .map(|i| i.id.clone())
        .collect();
    assert_eq!(cut.ids, first);
    let mut out = Outbox::default();
    out.set_list(long.clone(), 0.0);
    assert!(!out.queued(&long.items().last().unwrap().id));
}

#[test]
fn a_row_can_be_pressed_only_when_its_song_is_queued() {
    // Review probe p7: a recording the host would refuse, and no length.
    let body = "```studio-setlist\n\
        1 | A | A.md | | Songs/%2e%2e/secret.ogg | | | |\n\
        2 | B | B.md | | Songs/b.ogg | | | |\n```\n";
    let props = note_at("S.md", body);
    let s = setlist_of(&props).unwrap();
    let (a, b) = (
        s.sections[0].items[0].clone(),
        s.sections[0].items[1].clone(),
    );
    assert!(a.recording && a.media.is_none());
    let mut out = Outbox::default();
    out.set_list(s, 0.0);
    assert!(!out.queued(&a.id) && out.queued(&b.id));
    assert_eq!(out.press(&a, &props, 0.0), NONE);
    assert_ne!(out.press(&b, &props, 0.0), NONE);
}

#[test]
fn a_recording_is_a_vault_path_the_host_takes() {
    for ok in ["Songs/a.ogg", "a.ogg", "Libraries/C/Joy (live)/joy.m4a"] {
        assert_eq!(vault_path(ok).as_deref(), Some(ok));
    }
    for bad in [
        "https://x/a.ogg",
        "data:audio/ogg;base64,AA",
        "/a.ogg",
        "..\\a.ogg",
        "../a.ogg",
        "Songs/../a.ogg",
        "Songs/./a.ogg",
        "Songs//a.ogg",
        "Songs/%2e%2e/%2e%2e/secret.ogg",
        "a.ogg?x=1",
        "a.ogg#t=3",
        "a\u{7}.ogg",
        "Songs/",
        "",
        // As Architect main's model refuses them (#659 re-check N3).
        "Songs/Harry Connick Jr./Joy.ogg",
        "Songs/a.ogg.",
        "Songs /a.ogg",
        "Recordings\u{FF0F}..\u{FF0F}secret.m4a",
        "Recordings/\u{FF0E}\u{FF0E}/secret.m4a",
        "Recordings\u{2215}a.m4a",
        "Recordings\u{2044}a.m4a",
        "Recordings\u{29F8}a.m4a",
        "Recordings/a\u{2024}m4a",
        "Recordings/a\u{FE52}m4a",
        "Recordings/\u{200B}a.m4a",
        "Recordings/a\u{202E}a4m.mp3",
        "\u{FEFF}Recordings/a.m4a",
        "Recordings/a\u{00AD}b.m4a",
        "Recordings/a\u{2066}b.m4a",
    ] {
        assert_eq!(vault_path(bad), None, "{bad:?}");
        assert!(!host::vault_path(bad), "the host refuses {bad:?}");
    }
    assert!(vault_path(&"a".repeat(1024)).is_some());
    assert_eq!(vault_path(&"a".repeat(1025)), None);
    assert_eq!(
        vault_path(&"é".repeat(513)),
        None,
        "1024 bytes, not characters"
    );
}

/// One song whose recording the host refuses is left out, and the rest of
/// the setlist plays (review M2: `Songs/Harry Connick Jr./…` refused the
/// whole `SET`).
#[test]
fn a_song_the_host_refuses_is_left_out_and_the_rest_plays() {
    let body = "```studio-setlist\n\
        1 | Joy | Songs/Joy/Joy.md | | Songs/Joy/joy.ogg | | | |\n\
        2 | What Are You Doing | Songs/Harry Connick Jr./What.md | | Songs/Harry Connick Jr./what.ogg | | | |\n\
        3 | Hark | Songs/Hark/Hark.md | | Songs/Hark/hark.ogg | | | |\n\
        ```\n";
    let s = setlist_of(&note_at("S.md", body)).unwrap();
    let source = s.source(MESSAGE_MAX);
    let ids = host::set_ids(&source.args).expect("the host takes the SET");
    assert_eq!(
        ids,
        vec!["S.md|Songs/Joy/Joy.md", "S.md|Songs/Hark/Hark.md"]
    );
}

/// A row's tempo is sent from its downbeat, within a day (review M1).
#[test]
fn a_rows_tempo_starts_at_its_downbeat() {
    let list = list_of(vec![
        Item {
            tempo: Some((72.0, 3)),
            downbeat: Some(0.5),
            ..item("a")
        },
        Item {
            tempo: Some((72.0, 3)),
            downbeat: Some(1e9),
            ..item("b")
        },
    ]);
    let s = taken(&list);
    assert_eq!(
        s.args["queue"][0]["tempo"],
        json!({"bpm": 72.0, "at": 0.5, "meter": 3})
    );
    assert_eq!(s.args["queue"][1]["tempo"]["at"], json!(LENGTH_MAX));
    assert_eq!(
        tempo(120.0, 4, f64::NAN),
        Some(json!({"bpm": 120.0, "at": 0.0, "meter": 4}))
    );
    assert_eq!(tempo(120.0, 4, -3.0).unwrap()["at"], json!(0.0));
    assert_eq!(tempo(9000.0, 4, 0.0), None);
}

#[test]
fn a_length_is_clamped_to_a_day() {
    // Review probe p8.
    let body = "```studio-setlist\n1 | A | A.md | | | | 1e300 | |\n```\n";
    let s = setlist_of(&note_at("S.md", body)).unwrap();
    assert_eq!(s.sections[0].items[0].length, Some(LENGTH_MAX));
    assert_eq!(clock(1e300), "1440:00");
    assert_eq!(clock(151.4), "2:31");
    assert_eq!(taken(&s).args["queue"][0]["duration"], LENGTH_MAX);
}

fn refs(ids: &[String]) -> Vec<&str> {
    ids.iter().map(String::as_str).collect()
}

fn state(source: &str, item: &str, playing: bool, time: f64, queue: &[&str]) -> Value {
    json!({"source": source, "playing": playing, "time": time, "at": 1, "duration": 120,
           "item": {"id": item, "title": "A"},
           "queue": queue.iter().map(|id| json!({"id": id, "title": "x"})).collect::<Vec<_>>()})
}

#[test]
fn only_this_extensions_source_marks_a_row() {
    // Review M1: another extension's source with an id that looks the same.
    let id = "Setlists/Eve.md|Libraries/C/Joy/Joy.md";
    let now = now_of(&state(SOURCE, id, true, 30.0, &[id])).unwrap();
    assert_eq!((now.id.as_str(), now.playing), (id, true));
    assert!((now.progress() - 0.25).abs() < 1e-9);
    assert_eq!(now_of(&state("acme.player", id, true, 30.0, &[id])), None);
    assert_eq!(now_of(&json!({"item": {"id": id}, "playing": true})), None);
    assert_eq!(queue_of(&state("acme.player", id, true, 3.0, &[id])), None);
    // Set, not started: nothing marked.
    assert_eq!(now_of(&state(SOURCE, id, false, 0.0, &[id])), None);
}

#[test]
fn the_list_is_sent_once_it_settles_and_never_empty() {
    let props = note();
    let s = setlist_of(&props).unwrap();
    let mut out = Outbox::default();
    // The first list at once.
    out.set_list(s.clone(), 0.0);
    let asks = out.tick(&Value::Null, 0.0);
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].0, PLAYHEAD_SET);
    // Review M6: typing in the fence, a change every 100 ms for a second.
    let mut sent = Vec::new();
    for step in 0..10 {
        let t = f64::from(step).mul_add(100.0, 2000.0);
        let mut edited = s.clone();
        edited.title = format!("Eve {step}");
        out.set_list(edited, t);
        sent.extend(out.tick(&Value::Null, t));
    }
    assert!(sent.is_empty(), "{sent:?}");
    assert_eq!(out.tick(&Value::Null, 2900.0 + SETTLE_MS - 1.0), NONE);
    let last = out.tick(&Value::Null, 2900.0 + SETTLE_MS);
    assert_eq!(last.len(), 1);
    assert_eq!(last[0].1["name"], "Eve 9");
    // Review M2: a list with nothing to queue sends nothing.
    let mut empty = Outbox::default();
    let none = "```studio-setlist\n1 | A | A.md | | | | | |\n```\n";
    empty.set_list(setlist_of(&note_at("S.md", none)).unwrap(), 0.0);
    assert_eq!(empty.tick(&Value::Null, 0.0), NONE);
    assert_eq!(empty.tick(&Value::Null, 10_000.0), NONE);
}

#[test]
fn a_view_mounted_again_does_not_set_the_list_the_source_already_has() {
    let s = setlist_of(&note()).unwrap();
    let args = s.source(MESSAGE_MAX).args;
    // The source as the host holds it after this list's SET, playing its
    // second song.
    let mut held = args;
    held["source"] = json!(SOURCE);
    held["index"] = json!(1);
    held["playing"] = json!(true);
    let mut out = Outbox::default();
    out.set_list(s.clone(), 0.0);
    assert_eq!(out.tick(&held, 0.0), NONE, "nothing that would move it");
    // The same songs edited (its name): sent.
    let mut renamed = s;
    renamed.title = "Eve".to_owned();
    let mut again = Outbox::default();
    again.set_list(renamed, 0.0);
    assert_eq!(again.tick(&held, 0.0).len(), 1);
}

#[test]
fn a_press_sets_its_own_list_first_when_the_queue_is_anothers() {
    // Review M2: two setlist notes, one source.
    let a = setlist_of(&note_at("Setlists/A.md", NOTE)).unwrap();
    let b = setlist_of(&note_at("Setlists/B.md", NOTE)).unwrap();
    let (ida, idb) = (
        a.sections[0].items[0].clone(),
        b.sections[0].items[0].clone(),
    );
    assert_ne!(ida.id, idb.id);
    let mut out_a = Outbox::default();
    out_a.set_list(a.clone(), 0.0);
    out_a.tick(&Value::Null, 0.0);
    let a_ids = a.source(MESSAGE_MAX).ids;
    let b_ids = b.source(MESSAGE_MAX).ids;
    // B's list holds the source, playing B's first song: A marks nothing,
    // and does not take it back by itself.
    let held_by_b = state(SOURCE, &idb.id, true, 10.0, &refs(&b_ids));
    assert!(now_of(&held_by_b).is_some_and(|n| n.id != ida.id));
    assert_eq!(out_a.tick(&held_by_b, 5_000.0), NONE);
    // A row of A pressed: A's list, then its song.
    let asks = out_a.press(&ida, &held_by_b, 6_000.0);
    assert_eq!(asks.len(), 2);
    assert_eq!(asks[0].0, PLAYHEAD_SET);
    assert_eq!(
        host::takes(asks[0].0, &asks[0].1, SOURCE),
        Some(Ok(a_ids.clone()))
    );
    assert_eq!(asks[1].0, PLAYHEAD_ITEM);
    assert_eq!(
        host::takes(asks[1].0, &asks[1].1, SOURCE),
        Some(Ok(vec![ida.id.clone()]))
    );
    // Once A's list is the queue: a press is the song alone, the playing
    // one pauses, a paused one plays on.
    let held_by_a = state(SOURCE, &ida.id, true, 10.0, &refs(&a_ids));
    assert_eq!(
        out_a.press(&ida, &held_by_a, 7_000.0),
        vec![(PLAYHEAD_PAUSE, Value::Null)]
    );
    let paused = state(SOURCE, &ida.id, false, 10.0, &refs(&a_ids));
    assert_eq!(
        out_a.press(&ida, &paused, 7_000.0),
        vec![(PLAYHEAD_PLAY, Value::Null)]
    );
    let other = a.sections[1].items[1].clone();
    assert_eq!(
        out_a.press(&other, &held_by_a, 7_000.0),
        vec![(PLAYHEAD_ITEM, json!({"id": other.id, "play": true}))]
    );
}

#[test]
fn a_set_the_host_does_not_take_is_sent_again_a_few_times() {
    // The host refuses an ask before the frame is ready or while the page
    // is hidden: the props never show it.
    let s = setlist_of(&note()).unwrap();
    let mut out = Outbox::default();
    out.set_list(s.clone(), 0.0);
    let first = out.tick(&Value::Null, 0.0);
    assert_eq!(first.len(), 1);
    assert_eq!(out.tick(&Value::Null, CONFIRM_MS - 1.0), NONE);
    let mut t = 0.0;
    for _ in 0..RETRIES {
        t += CONFIRM_MS;
        assert_eq!(out.tick(&Value::Null, t), first, "at {t}");
    }
    // Then it waits for a press, never asking in a loop.
    for n in 1..10 {
        assert_eq!(
            out.tick(&Value::Null, f64::from(n).mul_add(CONFIRM_MS, t)),
            NONE
        );
    }
    // Taken on a retry: nothing more.
    let mut taken_once = Outbox::default();
    taken_once.set_list(s.clone(), 0.0);
    taken_once.tick(&Value::Null, 0.0);
    taken_once.tick(&Value::Null, CONFIRM_MS);
    let ids = taken_once.ids().to_vec();
    let shown = state(SOURCE, &ids[0], false, 0.0, &refs(&ids));
    assert_eq!(taken_once.tick(&shown, 2.0 * CONFIRM_MS), NONE);
    assert_eq!(taken_once.tick(&Value::Null, 10.0 * CONFIRM_MS), NONE);
    // A press waiting on a refused list is asked again with it.
    let mut pressed = Outbox::default();
    pressed.set_list(s.clone(), 0.0);
    let item0 = s.sections[0].items[0].clone();
    pressed.tick(&Value::Null, 0.0);
    assert_eq!(pressed.press(&item0, &Value::Null, 10.0).len(), 2);
    let retried = pressed.tick(&Value::Null, 10.0 + CONFIRM_MS);
    assert_eq!(retried.len(), 2);
    assert_eq!(
        host::takes(retried[1].0, &retried[1].1, SOURCE),
        Some(Ok(vec![item0.id.clone()]))
    );
}

#[test]
fn a_guest_handed_no_body_is_told_the_songs_are_not_shared() {
    // architect#659: a share link's guest gets title, type and the
    // claim's properties, no body.
    let guest = json!({"note": {"title": "Carol Night", "type": "setlist",
                                "properties": {"date": "2026-12-20"}}});
    let s = setlist_of(&guest).unwrap();
    assert!(s.withheld && s.items().next().is_none());
    assert_eq!(s.title, "Carol Night");
    let mut out = Outbox::default();
    out.set_list(s, 0.0);
    assert_eq!(out.tick(&Value::Null, 0.0), NONE);
    assert!(!setlist_of(&note()).unwrap().withheld);
}

#[test]
fn a_row_says_its_number_title_artist_and_key() {
    let s = setlist_of(&note()).unwrap();
    let silent = &s.sections[1].items[0];
    assert_eq!(
        row_label(silent, false, false, false),
        "2. Silent Night, Franz Gruber, Key of F, transposed up 1 semitone from E, no recording"
    );
    let joy = &s.sections[0].items[0];
    assert_eq!(
        row_label(joy, true, true, true),
        "1. Joy to the World, Isaac Watts, Key of D, playing"
    );
}

#[test]
fn a_tiles_key_reads_on_every_colour() {
    for hue in 0..360 {
        let rgb = tile_rgb(hue);
        assert!(contrast_with_white(rgb) >= 4.5, "hue {hue}: {rgb:?}");
    }
    // Hues that already read keep the share page's colour.
    assert_eq!(tile_rgb(240), hsl(240.0, 0.42, 0.40));
}

/// The manifest names the view this module mounts, claims setlist notes
/// for it, reads the note, and its favourites open only its own views.
#[test]
fn the_manifest_declares_the_view_its_claim_and_its_favourites() {
    let m: toml::Table = toml::from_str(include_str!("../../extension.toml")).unwrap();
    assert_eq!(m["id"].as_str(), Some(SOURCE));
    let views: Vec<&str> = m["views"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        views,
        ["progress", "transport", "lyrics", VIEW, crate::song::VIEW]
    );
    let embed = &m["embeds"].as_array().unwrap()[0];
    assert_eq!(embed["view"].as_str(), Some(VIEW));
    assert_eq!(embed["claims"]["note_type"][0].as_str(), Some("setlist"));
    // The claim reads the note, as the extension declares it does, and
    // names what a guest gets.
    assert_eq!(embed["reads"][0].as_str(), Some("documents"));
    assert_eq!(embed["properties"][0].as_str(), Some("date"));
    let caps = &m["capabilities"];
    // Its text and its files (architect#673): both, or a view is handed
    // no files.
    let reads: Vec<&str> = caps["reads"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r.as_str())
        .collect();
    assert_eq!(reads, ["documents", "files"]);
    // The song view claims song notes, reading them.
    let song = &m["embeds"].as_array().unwrap()[1];
    assert_eq!(song["view"].as_str(), Some(crate::song::VIEW));
    assert_eq!(song["claims"]["note_type"][0].as_str(), Some("song"));
    assert_eq!(song["reads"][0].as_str(), Some("documents"));
    // Every ask it makes, and the round button's toggle, by the prefix
    // `architect-playhead`'s rule wants (`Declarer::may_target`), up to
    // their effect (`mutating` in its manifest).
    let prefixes: Vec<&str> = caps["actions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a.as_str())
        .collect();
    for a in [
        PLAYHEAD_SET,
        PLAYHEAD_ITEM,
        PLAYHEAD_PLAY,
        PLAYHEAD_PAUSE,
        "PLAYHEAD_TOGGLE",
    ] {
        assert!(prefixes.iter().any(|p| a.starts_with(p)), "{a}");
    }
    assert_eq!(caps["effect"].as_str(), Some("mutating"));
    let favs = m["favourites"].as_array().unwrap();
    assert_eq!(favs.len(), 1);
    assert_eq!(favs[0]["note_type"].as_str(), Some("setlist"));
    for tab in favs[0]["tabs"].as_array().unwrap() {
        let view = tab["target"]["view"].as_str().unwrap();
        assert!(views.contains(&view), "{view} is not its own view");
    }
    let primary = &favs[0]["primary"];
    assert_eq!(
        primary["target"]["action"].as_str(),
        Some("PLAYHEAD_TOGGLE")
    );
    assert_eq!(primary["state"].as_str(), Some("playing"));
    assert_eq!(primary["on"]["label"].as_str(), Some("Pause"));
    // The setlists extension's own `setlist` view is not this one.
    let other: toml::Table = toml::from_str(include_str!(
        "../../../../../features/setlists/extension.toml"
    ))
    .unwrap();
    assert!(other.get("favourites").is_none());
    assert!(
        other["views"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["id"].as_str() != Some(VIEW))
    );
}

#[test]
fn lyrics_are_the_pages_lines_else_the_playing_items_sung_marks() {
    // A share page's own lines win, and carry no `source`.
    let page =
        json!({"lyrics": [{"at": 1.0, "text": "Joy"}, {"at": 2.0, "text": ""}], "title": "Joy"});
    assert_eq!(lyric_lines(&page), vec![(1.0, "Joy".to_owned())]);
    assert_eq!(lyrics_song(&page), None, "a share page keeps its own list");
    // In the app: the item's marks that are sung, not its cues.
    let app = json!({"source": SOURCE, "title": "Joy", "artist": "Watts", "item": {"id": "a", "marks": [
        {"at": 0.5, "text": "Joy to the world", "kind": "line"},
        {"at": 1.0, "text": "Count in", "kind": "cue"},
        {"at": 4.0, "text": "the Lord is come"},
        {"at": -1.0, "text": "before the start"},
    ]}});
    assert_eq!(
        lyric_lines(&app),
        vec![
            (0.5, "Joy to the world".to_owned()),
            (4.0, "the Lord is come".to_owned())
        ]
    );
    // Another source's marks are not this view's.
    let other = json!({"source": "app:notes", "title": "Memo", "item": {"marks": [{"at": 1.0, "text": "x"}]}});
    assert_eq!(lyric_lines(&other), Vec::new());
    assert_eq!(lyrics_song(&other), None);
}

#[test]
fn with_no_timed_lines_the_app_names_the_song_playing() {
    let state = |title: &str, artist: Value, subtitle: Value| json!({"source": SOURCE, "title": title, "artist": artist, "subtitle": subtitle, "item": {"id": "a"}});
    assert_eq!(
        lyrics_song(&state("Silent Night", json!("Franz Gruber"), Value::Null)),
        Some(("Silent Night".to_owned(), "Franz Gruber".to_owned()))
    );
    assert_eq!(
        lyrics_song(&state("Hark", Value::Null, json!("Mendelssohn"))),
        Some(("Hark".to_owned(), "Mendelssohn".to_owned()))
    );
    assert_eq!(
        lyrics_song(&state("", json!("x"), Value::Null)),
        None,
        "nothing current"
    );
}

#[test]
fn a_row_s_summary_gives_its_beat_and_the_source_asks_for_click_and_open() {
    assert_eq!(tempo_of("D · 120 bpm · 4/4"), Some((120.0, 4)));
    assert_eq!(tempo_of("Bb · 60 bpm · 6/8"), Some((60.0, 2)));
    assert_eq!(tempo_of("D · 92 bpm · 3/4"), Some((92.0, 3)));
    assert_eq!(tempo_of("A · 83 bpm"), Some((83.0, 4)));
    assert_eq!(tempo_of("A · 3/4"), None);
    assert_eq!(tempo_of("A · 9000 bpm · 4/4"), None);
    let list = list_of(vec![
        Item {
            tempo: Some((120.0, 4)),
            ..item("a")
        },
        item("b"),
    ]);
    let s = taken(&list);
    assert_eq!(s.args["slots"], json!(["click", "open"]));
    assert_eq!(
        s.args["queue"][0]["tempo"],
        json!({"bpm": 120.0, "at": 0.0, "meter": 4})
    );
    assert!(s.args["queue"][1].get("tempo").is_none());
}

/// A setlist's rows take their cover and timed lines from the files the
/// app hands its note (architect#673): the images and `.lrc` in each
/// song's folder, sent as the queue item's `art` and sung `marks`.
#[test]
fn a_row_takes_its_cover_and_lines_from_the_handed_files() {
    let body = "```studio-setlist\n\
        1 | Joy | Libraries/Carols/Joy/Joy.md | D · 120 bpm · 4/4 | Libraries/Carols/Joy/Media/joy.ogg | Watts | 151.0 | D |\n\
        2 | Hark | Libraries/Carols/Hark/Hark.md | F · 104 bpm · 4/4 | Libraries/Carols/Hark/Media/hark.ogg | | 175.0 | F |\n\
        ```\n";
    let note = json!({"title": "Eve", "body": body, "files": [
        {"path": "Libraries/Carols/Joy/Joy.chart-1.png", "kind": "image", "size": 4, "data": "data:image/png;base64,AA=="},
        {"path": "Libraries/Carols/Joy/cover.jpg", "kind": "image", "size": 4, "data": "data:image/jpeg;base64,AA=="},
        {"path": "Libraries/Carols/Joy/Joy.lrc", "kind": "lyrics", "size": 30, "text": "[00:01.00]Joy to the world\n[00:03.25]The Lord is come"},
        {"path": "Libraries/Carols/Hark/hark.webp", "kind": "image", "size": 4, "data": "data:image/webp;base64,AA=="},
        {"path": "Libraries/Carols/Hark/x.png", "kind": "image", "size": 4, "data": "javascript:alert(1)"},
    ]});
    let list = setlist_of_note(&note).unwrap();
    let items: Vec<&Item> = list.items().collect();
    assert_eq!(
        items[0].art.as_deref(),
        Some("Libraries/Carols/Joy/cover.jpg")
    );
    assert_eq!(
        items[0].lines,
        vec![
            (1.0, "Joy to the world".to_owned()),
            (3.25, "The Lord is come".to_owned())
        ]
    );
    assert_eq!(
        items[1].art.as_deref(),
        Some("Libraries/Carols/Hark/hark.webp")
    );
    assert_eq!(items[1].lines, Vec::new());
    let images = images_of(&files_of(&note));
    assert!(
        !images.contains_key("Libraries/Carols/Hark/x.png"),
        "only an image's data: URL"
    );
    let s = taken(&list);
    assert_eq!(
        s.args["queue"][0]["art"],
        json!("Libraries/Carols/Joy/cover.jpg")
    );
    assert_eq!(
        s.args["queue"][0]["marks"],
        json!([{"at": 1.0, "text": "Joy to the world", "kind": "line"}, {"at": 3.25, "text": "The Lord is come", "kind": "line"}])
    );
    assert!(s.args["queue"][1].get("marks").is_none());
}

#[test]
fn lrc_lines_are_timed_and_tags_left_out() {
    assert_eq!(
        lrc_lines(
            "[ti:Eve]\n[00:02.50]b\n[00:01:00][01:00.00] a <00:01.20>word\n[00:03.00]\nno stamp"
        ),
        vec![
            (1.0, "a word".to_owned()),
            (2.5, "b".to_owned()),
            (60.0, "a word".to_owned())
        ]
    );
}
