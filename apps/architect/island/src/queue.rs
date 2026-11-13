//! The setlist view's contract with the app that hosts it, in one place.
//!
//! The props it reads and the asks it makes, against Architect's
//! now-playing contract `architect.playhead` v0.1
//! (FastTrackStudios/architect#589) and the frame host that carries it
//! (architect#656, #659: `apps/web/src/ext_host.rs`): the setlist is the
//! extension's *source*, its songs the source's *queue*, and the Now
//! Playing bar, which is Architect's, plays it. A revision of that
//! contract is a change to this file alone (`docs/architect-distribution.md`,
//! "The setlist view", writes the shapes down).
//!
//! **Props in** (data only; the frame has no network and no credential):
//!
//! - `note`, the setlist's note as the app gives the view that claims its
//!   type with `reads = ["documents"]`: a member gets `{path, title, type,
//!   properties, body}` (its `%%comments%%` removed); the list is read from
//!   the body's ```` ```studio-setlist ```` fence
//!   (`setlists::setlist::Line`, the share page's own reader), the title
//!   from `title` or `properties.title`. A share link's guest gets `{title,
//!   type, properties}` only, no body and no path (architect#648 open): the
//!   view then says the songs are not shared ([`Setlist::withheld`]);
//! - the playhead's state, flat, as #589 gives a frame: `source` (whose
//!   state it is), `playing`, `time`, `duration`, `item` (`{id, …}`),
//!   `queue` and the rest. Only a state whose `source` is this extension
//!   ([`SOURCE`]) is read as this view's ([`now_of`], [`queue_of`]): a
//!   frame's props are another extension's active source until its own
//!   exists.
//!
//! **Asks out** (`architectPage.ask(action, args)`), on its own source
//! only (the host says whose), through one [`Outbox`]:
//!
//! - [`PLAYHEAD_SET`] `{name, queue: [{id, title, subtitle, duration,
//!   media}]}`: the songs the host will take, in running order
//!   ([`Setlist::source`]: the host's own rules, item by item, and the
//!   whole ask under its message limit). Sent from the view's clock, after
//!   `mount` has returned (the host refuses an ask before the frame is
//!   ready, or while the page is hidden: one not seen in the props is sent
//!   again, [`RETRIES`] times), once the list has stopped changing for
//!   [`SETTLE_MS`]; never empty (an empty queue would remove the
//!   extension's source), never with `play` (it never takes the bar).
//! - [`PLAYHEAD_ITEM`] `{id, play: true}`: a row pressed plays its song;
//!   when the source's queue is not this view's list (another setlist note
//!   set it since, or the host refused the last `SET`), its `SET` first.
//! - [`PLAYHEAD_PLAY`] / [`PLAYHEAD_PAUSE`] (no args): the playing row
//!   pressed again pauses, a paused one plays on.
//!
//! One extension has one source: every setlist note's view of this
//! extension shares it, the last one pressed (or edited) holds it. Item ids
//! are scoped by the setlist note's path ([`Setlist::scope`]), so a view
//! never marks or plays another note's rows.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};
use setlists::row::{row_key, signed, tile_hue};
use setlists::setlist::{Line, SETLIST_FENCE, section_line};

/// This extension's id, whose source the view sets: the island's
/// (`apps/architect/island/extension.toml`, `studio_architect::island::ID`).
pub const SOURCE: &str = "studio.transport";

/// The view's id in the bundle and the manifest (`mount(view)`): one
/// setlist as a playlist. Not `setlist`, which is `studio-setlists`' view
/// of every setlist (`features/setlists/extension.toml`).
pub const VIEW: &str = "setlist.playlist";

/// Sets (replaces) the extension's source: its name and its queue.
pub const PLAYHEAD_SET: &str = "PLAYHEAD_SET";
/// Makes one item of the queue current, by its id (and plays it).
pub const PLAYHEAD_ITEM: &str = "PLAYHEAD_ITEM";
/// Plays on.
pub const PLAYHEAD_PLAY: &str = "PLAYHEAD_PLAY";
/// Pauses.
pub const PLAYHEAD_PAUSE: &str = "PLAYHEAD_PAUSE";

/// The most rows drawn: a setlist longer than this is cut.
pub const MAX_ITEMS: usize = 200;

/// The most bytes one message from a frame may take as JSON, as #589 v0.1
/// and the host (`ext_host::MAX_MESSAGE`) set it: a `SET` is cut from its
/// tail to fit.
pub const MESSAGE_MAX: usize = 64 * 1024;

/// The contract's limits on what a source says (#589 v0.1, as the host
/// checks them: characters, but `media` in bytes).
const QUEUE_MAX: usize = 500;
const NAME_MAX: usize = 64;
const TEXT_MAX: usize = 200;
const ID_MAX: usize = 256;
const MEDIA_MAX: usize = 1024;
/// The longest a song is taken to be, seconds (a day): a longer length is
/// a typo, drawn and sent as this.
const LENGTH_MAX: f64 = 86_400.0;

/// How long the list must stay as it is before it is sent, ms.
///
/// Typing in the fence sends one `SET`, not one a keystroke (the host's
/// asks are 8 a second, shared by every frame, and a dropped one is not
/// answered).
pub const SETTLE_MS: f64 = 400.0;
/// How long a `SET` has to show in the props before it is taken as
/// refused, ms.
pub const CONFIRM_MS: f64 = 1_500.0;
/// How many times a `SET` not seen in the props is sent again before the
/// view waits for a press.
pub const RETRIES: u8 = 3;

/// One ask: an action and its args.
pub type Ask = (&'static str, Value);

/// One song of the list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Item {
    /// Its id in the queue: the setlist's path, `|`, its note (`#` its
    /// arrangement), `@` its number the second time a song is listed.
    pub id: String,
    /// Its place in the running order, from 1.
    pub number: usize,
    pub title: String,
    /// Who it is by; empty when unknown.
    pub artist: String,
    /// The key as played (`E`, `E*`, `E→F`); empty for none.
    pub key: String,
    /// The key said in words, for a screen reader.
    pub key_said: String,
    /// How far the setlist moves it from its chart's key (`+1`, `−3`).
    pub moved: Option<String>,
    /// Its tile's hue (from its title, as the share page's).
    pub hue: u32,
    /// Seconds (at most a day); none when unknown.
    pub length: Option<f64>,
    /// Whether its row names a recording.
    pub recording: bool,
    /// Its recording, a vault path the host takes, when it has one.
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
    /// The setlist note's path: what its item ids are scoped by.
    pub scope: String,
    pub title: String,
    pub sections: Vec<Section>,
    /// The note came without its body (a share link's guest): its songs
    /// are not the view's to show.
    pub withheld: bool,
}

/// A `PLAYHEAD_SET`'s args and what they queue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Source {
    pub args: Value,
    /// The ids queued, in order.
    pub ids: Vec<String>,
    /// The ask's size as the host measures it (the message's JSON).
    pub bytes: usize,
}

/// The message a frame posts for `ask`, as the host reads it
/// (`{"t":"ask","action":…,"args":…}`).
#[must_use]
pub fn message(action: &str, args: &Value) -> String {
    json!({"t": "ask", "action": action, "args": args}).to_string()
}

impl Setlist {
    /// Every song, in running order.
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.sections.iter().flat_map(|s| s.items.iter())
    }

    /// The [`PLAYHEAD_SET`] ask's args, as the host takes them whole: each
    /// song the host's rules let through (an id of 1 to 256 characters,
    /// unique; a title of 1 to 200; its recording a vault path of at most
    /// 1024, or else its length), at most 500, in running order, cut from
    /// the tail where the message would pass `limit` bytes.
    #[must_use]
    pub fn source(&self, limit: usize) -> Source {
        let name = if self.title.is_empty() {
            "Setlist"
        } else {
            &self.title
        };
        let name = cut(name, NAME_MAX);
        let empty = message(PLAYHEAD_SET, &json!({"name": name, "queue": []})).len();
        let mut bytes = empty;
        let mut seen = BTreeSet::new();
        let mut queue = Vec::new();
        let mut ids = Vec::new();
        for i in self.items() {
            if queue.len() >= QUEUE_MAX {
                break;
            }
            let Some(item) = queued(i) else { continue };
            if seen.contains(&i.id) {
                continue;
            }
            let more = item
                .to_string()
                .len()
                .saturating_add(usize::from(!queue.is_empty()));
            if bytes.saturating_add(more) > limit {
                break;
            }
            bytes = bytes.saturating_add(more);
            seen.insert(i.id.clone());
            ids.push(i.id.clone());
            queue.push(item);
        }
        Source {
            args: json!({"name": name, "queue": queue}),
            ids,
            bytes,
        }
    }
}

/// `i` as the host takes a queue's item, or none when it would refuse it.
fn queued(i: &Item) -> Option<Value> {
    let chars = i.id.chars().count();
    if chars == 0 || chars > ID_MAX {
        return None;
    }
    let media = i.media.as_deref().filter(|m| m.len() <= MEDIA_MAX);
    let duration = i
        .length
        .map(|l| (l * 1000.0).round() / 1000.0)
        .filter(|l| *l > 0.0);
    if media.is_none() && duration.is_none() {
        return None;
    }
    let mut item = Map::new();
    item.insert("id".to_owned(), json!(i.id));
    let title = if i.title.is_empty() {
        format!("Song {}", i.number)
    } else {
        cut(&i.title, TEXT_MAX)
    };
    item.insert("title".to_owned(), json!(title));
    if !i.artist.is_empty() {
        item.insert("subtitle".to_owned(), json!(cut(&i.artist, TEXT_MAX)));
    }
    if let Some(d) = duration {
        item.insert("duration".to_owned(), json!(d));
    }
    if let Some(m) = media {
        item.insert("media".to_owned(), json!(m));
    }
    Some(Value::Object(item))
}

/// `s`, at most `max` characters.
fn cut(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// A recording the app may play: a vault path as the host takes `media`
/// (no scheme, not absolute, no `\`, no `?`, `#` or percent-encoding, no
/// control character, no empty, `.` or `..` segment); anything else is
/// left out rather than failing the whole source.
fn vault_path(p: &str) -> Option<String> {
    let p = p.trim();
    let ok = !p.is_empty()
        && !p.starts_with('/')
        && !p.contains([':', '\\', '?', '#', '%'])
        && !p.chars().any(char::is_control)
        && p.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..");
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

/// Whether the playhead's state in `props` is this extension's source.
fn own(props: &Value) -> bool {
    props.get("source").and_then(Value::as_str) == Some(SOURCE)
}

/// The list in the app's props: read from `note`; none without one.
#[must_use]
pub fn setlist_of(props: &Value) -> Option<Setlist> {
    props.get("note").filter(|n| n.is_object()).map(from_note)
}

/// The list read from a setlist note's fence.
fn from_note(note: &Value) -> Setlist {
    let scope = text(note, "path");
    let title = Some(text(note, "title"))
        .filter(|t| !t.is_empty())
        .or_else(|| {
            note.get("properties")
                .map(|p| text(p, "title"))
                .filter(|t| !t.is_empty())
        })
        .unwrap_or_default();
    let withheld = note.get("body").is_none();
    let body = note.get("body").and_then(Value::as_str).unwrap_or("");
    let mut sections = vec![Section::default()];
    let mut seen = BTreeSet::new();
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
        let song = row
            .arrangement
            .as_ref()
            .map_or_else(|| row.note.clone(), |a| format!("{}#{a}", row.note));
        let id = unique(&mut seen, &format!("{scope}|{song}"), row.number);
        let key = row_key(&row);
        let item = Item {
            id,
            number: row.number,
            artist: row.artist.clone().unwrap_or_default(),
            key: key.as_ref().map(|k| k.label.text()).unwrap_or_default(),
            key_said: key
                .as_ref()
                .map(setlists::row::RowKey::spoken)
                .unwrap_or_default(),
            moved: key.and_then(|k| k.moved).map(|(_, n)| signed(n)),
            hue: tile_hue(&row.label),
            title: row.label,
            length: row
                .length
                .filter(|l| l.is_finite() && *l > 0.0)
                .map(|l| l.min(LENGTH_MAX)),
            media: row.recording.as_deref().and_then(vault_path),
            recording: row.recording.is_some(),
        };
        if let Some(last) = sections.last_mut() {
            last.items.push(item);
        }
    }
    sections.retain(|s| s.name.is_some() || !s.items.is_empty());
    capped(Setlist {
        scope,
        title,
        sections,
        withheld,
    })
}

/// `base`, or `base@<n>` from `number` up, whichever is not in `seen` yet;
/// put in `seen`.
fn unique(seen: &mut BTreeSet<String>, base: &str, number: usize) -> String {
    let mut id = base.to_owned();
    let mut n = number;
    while seen.contains(&id) {
        id = format!("{base}@{n}");
        n = n.saturating_add(1);
    }
    seen.insert(id.clone());
    id
}

/// At most [`MAX_ITEMS`] songs.
fn capped(mut s: Setlist) -> Setlist {
    let mut left = MAX_ITEMS;
    for sec in &mut s.sections {
        sec.items.truncate(left);
        left = left.saturating_sub(sec.items.len());
    }
    s.sections
        .retain(|s| s.name.is_some() || !s.items.is_empty());
    s
}

/// Where this extension's source is, from the playhead's state in the
/// props.
///
/// None when the state is another extension's, when nothing is current,
/// or when the current item has not started (a source just set is at its
/// first item, paused at 0: nothing is marked until something plays).
#[must_use]
pub fn now_of(props: &Value) -> Option<Now> {
    if !own(props) {
        return None;
    }
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

/// The ids this extension's source queues, from the props; none when the
/// state is another extension's.
#[must_use]
pub fn queue_of(props: &Value) -> Option<Vec<String>> {
    if !own(props) {
        return None;
    }
    Some(
        props
            .get("queue")
            .and_then(Value::as_array)
            .map(|q| q.iter().map(|i| text(i, "id")).collect())
            .unwrap_or_default(),
    )
}

/// What was last sent, waiting to show in the props.
#[derive(Debug, Clone, PartialEq)]
struct Sent {
    ids: Vec<String>,
    at: f64,
    /// How many times it was sent again.
    tries: u8,
}

/// The view's asks of its source.
///
/// The list sent once it settles, a `SET` the host did not take sent again
/// (a few times), a row's press with its list first when the source holds
/// another. Time is passed in (ms), so it is tested without a clock.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outbox {
    list: Option<Setlist>,
    /// The list as the host takes it.
    built: Source,
    /// When the list last changed, while it is not sent.
    changed: Option<f64>,
    sent: Option<Sent>,
    /// The item a press asked for, while its `SET` is not confirmed.
    item: Option<String>,
}

impl Outbox {
    /// The list as drawn now: sent once it has not changed for
    /// [`SETTLE_MS`] (the first one at once).
    pub fn set_list(&mut self, list: Setlist, now: f64) {
        if self.list.as_ref() == Some(&list) {
            return;
        }
        self.changed = Some(if self.list.is_none() {
            now - SETTLE_MS
        } else {
            now
        });
        self.built = list.source(MESSAGE_MAX);
        self.list = Some(list);
    }

    /// The ids queued, in order: the rows that can be pressed.
    #[must_use]
    pub fn ids(&self) -> &[String] {
        &self.built.ids
    }

    /// Whether `id` is queued (its row can be pressed).
    #[must_use]
    pub fn queued(&self, id: &str) -> bool {
        self.built.ids.iter().any(|i| i == id)
    }

    /// Whether the source's queue in `props` is this list.
    fn shown(&self, props: &Value) -> bool {
        queue_of(props).is_some_and(|q| q == self.built.ids)
    }

    fn send(&mut self, now: f64, tries: u8) -> Vec<Ask> {
        if self.built.ids.is_empty() {
            self.sent = None;
            return Vec::new();
        }
        self.sent = Some(Sent {
            ids: self.built.ids.clone(),
            at: now,
            tries,
        });
        let mut asks = vec![(PLAYHEAD_SET, self.built.args.clone())];
        if let Some(id) = self.item.clone().filter(|id| self.queued(id)) {
            asks.push((PLAYHEAD_ITEM, json!({"id": id, "play": true})));
        }
        asks
    }

    /// What is due at `now`, the props as they are: the settled list, or
    /// a refused `SET` again.
    pub fn tick(&mut self, props: &Value, now: f64) -> Vec<Ask> {
        if let Some(t) = self.changed {
            if now - t < SETTLE_MS {
                return Vec::new();
            }
            self.changed = None;
            return self.send(now, 0);
        }
        let Some(sent) = self.sent.clone() else {
            return Vec::new();
        };
        if now - sent.at < CONFIRM_MS {
            return Vec::new();
        }
        let shows = queue_of(props);
        if shows.as_ref() == Some(&sent.ids) {
            // Taken.
            self.sent = None;
            self.item = None;
            return Vec::new();
        }
        let prefix = self.list.as_ref().map(|l| format!("{}|", l.scope));
        let another = shows.is_some_and(|q| {
            !q.is_empty()
                && prefix
                    .as_ref()
                    .is_some_and(|p| !q.iter().all(|i| i.starts_with(p)))
        });
        if another {
            // Another setlist's view holds the source now: a press here
            // takes it back.
            self.sent = None;
            self.item = None;
            return Vec::new();
        }
        if sent.tries < RETRIES {
            // Before the frame was ready, while the page was hidden, or
            // dropped by the rate limit: again as it was.
            return self.send(now, sent.tries.saturating_add(1));
        }
        self.sent = None;
        self.item = None;
        Vec::new()
    }

    /// What pressing `item`'s row asks: the playing song pauses, a paused
    /// one plays on, any other plays from its start, its list set first
    /// when the source's queue is not this list; a song not queued asks
    /// nothing.
    pub fn press(&mut self, item: &Item, props: &Value, now: f64) -> Vec<Ask> {
        if !self.queued(&item.id) {
            return Vec::new();
        }
        if self.shown(props) {
            let now = now_of(props);
            return vec![match now {
                Some(n) if n.id == item.id && n.playing => (PLAYHEAD_PAUSE, Value::Null),
                Some(n) if n.id == item.id => (PLAYHEAD_PLAY, Value::Null),
                _ => (PLAYHEAD_ITEM, json!({"id": item.id, "play": true})),
            }];
        }
        self.changed = None;
        self.item = Some(item.id.clone());
        self.send(now, 0)
    }
}

/// `m:ss`.
#[must_use]
pub fn clock(seconds: f64) -> String {
    let s = seconds.clamp(0.0, LENGTH_MAX).floor();
    let m = (s / 60.0).floor();
    format!("{m}:{:02}", m.mul_add(-60.0, s))
}

/// What a row says to a screen reader: its number, title, artist, key,
/// and whether it is playing, paused, or cannot play.
#[must_use]
pub fn row_label(item: &Item, current: bool, playing: bool, playable: bool) -> String {
    let mut label = format!("{}. {}", item.number, item.title);
    if !item.artist.is_empty() {
        label.push_str(", ");
        label.push_str(&item.artist);
    }
    if !item.key_said.is_empty() {
        label.push_str(", ");
        label.push_str(&item.key_said);
    }
    label.push_str(match (playing, current, playable, item.recording) {
        (true, _, _, _) => ", playing",
        (false, true, _, _) => ", paused",
        (_, _, false, false) => ", no recording",
        (_, _, false, true) => ", cannot play here",
        _ => "",
    });
    label
}

/// The tile's colour for `hue`: the share page's (`hsl(hue 42% 40%)`),
/// darkened only as far as white text on it needs for 4.5:1.
#[must_use]
pub fn tile_rgb(hue: u32) -> (u8, u8, u8) {
    let mut lightness = 0.40;
    loop {
        let rgb = hsl(f64::from(hue % 360), 0.42, lightness);
        if contrast_with_white(rgb) >= 4.5 || lightness <= 0.10 {
            return rgb;
        }
        lightness -= 0.01;
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "each channel is rounded within 0..=255"
)]
fn hsl(hue: f64, saturation: f64, lightness: f64) -> (u8, u8, u8) {
    let chroma = (1.0 - 2.0f64.mul_add(lightness, -1.0).abs()) * saturation;
    let second = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let base = lightness - chroma / 2.0;
    let (red, green, blue) = match hue {
        h if h < 60.0 => (chroma, second, 0.0),
        h if h < 120.0 => (second, chroma, 0.0),
        h if h < 180.0 => (0.0, chroma, second),
        h if h < 240.0 => (0.0, second, chroma),
        h if h < 300.0 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let channel = |v: f64| ((v + base) * 255.0).round().clamp(0.0, 255.0) as u8;
    (channel(red), channel(green), channel(blue))
}

fn contrast_with_white((r, g, b): (u8, u8, u8)) -> f64 {
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let y = 0.0722f64.mul_add(lin(b), 0.2126f64.mul_add(lin(r), 0.7152 * lin(g)));
    1.05 / (y + 0.05)
}

#[cfg(test)]
mod tests;
