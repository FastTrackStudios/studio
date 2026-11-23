//! Setlists as Drive documents.
//!
//! **A set** is a Loro document of its own, `<org>/studio:setlist:<id>`,
//! in the org that owns it:
//!
//! - `meta`: the schema version (`drive_spine::schema`), the org and the
//!   set's id (what an upgrade step reads: a step sees only the document).
//! - `set`: what it is ([`Info`]): its title, kind (a service, a
//!   rehearsal, a show, or a template to start sets from), date, start,
//!   call and place, the recurring event it is a meeting of, who leads
//!   it, the team group that plays it, how many songs it wants, its notes
//!   and what it was started from.
//! - `entries`: the running order, a **movable list** of entry maps
//!   ([`Entry`]): two people moving songs at once both land and the list
//!   stays one order; an edit to an entry and a move of it both land (the
//!   entry is one map wherever it moves). An entry is a song (`owner/id`,
//!   so a song shared from another org can be in it, with its face as it
//!   was when added, so an org the set is shared with reads it without the
//!   song's library), a named section that starts a run of the set
//!   ("Top Set", an act: Studio's `## name` lines), or an item that is not
//!   a song (a reading, an announcement, a prayer, a video). Each carries
//!   its own overrides (key, arrangement, leader, tempo), notes and how it
//!   goes into the next ([`Entry::transition`]).
//! - `band`: who plays it, from the org's team, each with the parts they
//!   play in this set ([`Seat`]).
//! - `rehearsals`: its rehearsals ([`Rehearsal`]), each also an event in
//!   Drive's calendar.
//! - `shared_with`: the other orgs it is shared with (a joint service).
//!
//! **The org's sets** are listed in `<org>/studio:sets` (the index): each
//! set's face ([`Face`]: what a list shows without opening every set),
//! kept current by whoever edits the set, and the org's recurring events
//! ([`Series`]): the days they meet, their room and times, how many songs
//! a set. A meeting's set has a fixed id (`<series>_<date>`,
//! [`meeting_id`]), so two people planning the same Sunday open one set.
//!
//! **What a song is played as** is looked up in order: the entry's
//! override, then the org's use of the song (phase 1's `uses`), then the
//! song itself; for the arrangement, the org's default after its use
//! ([`resolve`]).
//!
//! **Sharing** a set: the set's `shared_with` is the truth; a Drive
//! project in the owner's registry (`studio-set-<id>`, [`set_project`])
//! holds the set's document and the sessions of the owner's songs in it,
//! shared with those orgs ([`mirror_set_project`]), which is what Drive's
//! node checks. The other org finds it through Drive's `/shared`
//! ([`shared_sets`]).
//!
//! What this keeps of Studio's setlists (`features/setlists`): a setlist
//! is songs of any library in running order with an arrangement each
//! (`SongRef`), in named sets (`## name` lines; here section entries), a
//! date, notes per song (`notes`, here per entry) and who plays what
//! (`parts`, here the band); the key labels and how a set's key moves them
//! are `crate::keys`, ported whole.

use loro::{LoroDoc, LoroMap, LoroMovableList, LoroValue, ValueOrContainer};

use crate::data::{self, Day};
use crate::orgs::{self, Settings, Use};

/// The org's sets and recurring events.
pub const INDEX_DOC: &str = "studio:sets";

/// A set's document.
pub fn setlist_doc(id: &str) -> String {
    format!("studio:setlist:{id}")
}

/// The prototype's live document of a set before setlists were documents
/// (its keys and arrangements by the song's place).
pub fn legacy_doc(id: &str) -> String {
    format!("studio:set:{id}")
}

/// The Drive project, in the owner's registry, that shares a set.
pub fn set_project(id: &str) -> String {
    format!("studio-set-{id}")
}

/// A recurring event's meeting: its set's id.
pub fn meeting_id(series: &str, day: Day) -> String {
    format!("{series}_{}", day.iso())
}

// ── what a set is ───────────────────────────────────────────────────────

/// What kind of set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    /// A service: a church's Sunday, a youth night.
    #[default]
    Service,
    Rehearsal,
    /// A show, a recital, a concert.
    Show,
    /// Not played: a set to start others from.
    Template,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Service, Kind::Rehearsal, Kind::Show, Kind::Template];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Service => "service",
            Kind::Rehearsal => "rehearsal",
            Kind::Show => "show",
            Kind::Template => "template",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Service => "Service",
            Kind::Rehearsal => "Rehearsal",
            Kind::Show => "Show",
            Kind::Template => "Template",
        }
    }

    pub fn parse(s: &str) -> Kind {
        Kind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .unwrap_or_default()
    }
}

/// A set's own fields.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Info {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    /// `YYYY-MM-DD`; empty for a template.
    pub date: String,
    /// When it starts (`10:30`), when the band is called, where.
    pub starts: String,
    pub call: String,
    pub place: String,
    /// The recurring event it is a meeting of (its id), if any.
    pub series: String,
    /// Who leads it (a person's id in the org's team).
    pub leader: Option<String>,
    /// The team group that plays it (`worship`, `youth`), if any.
    pub group: String,
    /// How many songs it wants (0: no count).
    pub size: usize,
    pub notes: String,
    /// The set or template it was started from (its id).
    pub from: String,
    pub hue: u32,
}

impl Info {
    pub fn day(&self) -> Option<Day> {
        parse_day(&self.date)
    }
}

/// `YYYY-MM-DD` as a day.
pub fn parse_day(s: &str) -> Option<Day> {
    let mut it = s.split('-');
    let y = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| Day::ymd(y, m, d))
}

/// What an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ItemKind {
    #[default]
    Song,
    /// Starts a named run of the set (a Top Set, an act, an encore).
    Section,
    Reading,
    Announcement,
    Prayer,
    Video,
    /// Anything else (a welcome, an offering, a game).
    Other,
}

impl ItemKind {
    pub const ITEMS: [ItemKind; 5] = [
        ItemKind::Reading,
        ItemKind::Announcement,
        ItemKind::Prayer,
        ItemKind::Video,
        ItemKind::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Song => "song",
            ItemKind::Section => "section",
            ItemKind::Reading => "reading",
            ItemKind::Announcement => "announcement",
            ItemKind::Prayer => "prayer",
            ItemKind::Video => "video",
            ItemKind::Other => "other",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Song => "Song",
            ItemKind::Section => "Section",
            ItemKind::Reading => "Reading",
            ItemKind::Announcement => "Announcement",
            ItemKind::Prayer => "Prayer",
            ItemKind::Video => "Video",
            ItemKind::Other => "Item",
        }
    }

    pub fn parse(s: &str) -> ItemKind {
        [ItemKind::Song, ItemKind::Section]
            .into_iter()
            .chain(ItemKind::ITEMS)
            .find(|k| k.as_str() == s)
            .unwrap_or_default()
    }
}

/// A song across orgs: its owner and its id there.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SongRef {
    pub owner: String,
    pub id: String,
}

impl SongRef {
    pub fn new(owner: &str, id: &str) -> Self {
        SongRef {
            owner: owner.into(),
            id: id.into(),
        }
    }

    /// `owner/id`.
    pub fn text(&self) -> String {
        orgs::song_ref(&self.owner, &self.id)
    }

    pub fn parse(s: &str) -> Option<SongRef> {
        let (owner, id) = s.split_once('/')?;
        (!owner.is_empty() && !id.is_empty()).then(|| SongRef::new(owner, id))
    }
}

/// What an entry changes from the defaults; None: as the org plays it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Overrides {
    pub key: Option<String>,
    pub arrangement: Option<String>,
    /// Who leads it in this set (a person's id).
    pub leader: Option<String>,
    pub tempo: Option<u32>,
}

/// A place in the running order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Entry {
    /// Stable across moves and devices.
    pub id: String,
    pub kind: ItemKind,
    /// The song, for a song.
    pub song: Option<SongRef>,
    /// The song's face when it was added (an item's or section's name).
    pub title: String,
    pub artist: Option<String>,
    pub key: Option<String>,
    pub tempo: Option<u32>,
    /// How long it runs, in seconds, when known.
    pub length: Option<u32>,
    pub over: Overrides,
    pub notes: String,
    /// How it goes into the next: a segue, a pad under the prayer, a stop.
    pub transition: String,
}

impl Entry {
    pub fn is_song(&self) -> bool {
        self.kind == ItemKind::Song
    }

    /// A song entry from a library's face.
    pub fn of_song(owner: &str, e: &crate::songs::Entry) -> Entry {
        Entry {
            kind: ItemKind::Song,
            song: Some(SongRef::new(owner, &e.id)),
            title: e.title.clone(),
            artist: e.artist.clone(),
            key: e.key.clone(),
            tempo: e.tempo,
            ..Entry::default()
        }
    }

    pub fn item(kind: ItemKind, title: &str) -> Entry {
        Entry {
            kind,
            title: title.into(),
            ..Entry::default()
        }
    }
}

/// Someone in a set's band, and what they play in it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Seat {
    pub person: String,
    /// Their name when seated (an org the set is shared with reads it
    /// without the owner's team).
    pub name: String,
    pub parts: Vec<String>,
}

/// A rehearsal for a set.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Rehearsal {
    pub id: String,
    pub date: String,
    pub time: String,
    pub room: String,
}

// ── reading and writing maps ────────────────────────────────────────────

fn s(m: &LoroMap, k: &str) -> String {
    match m.get(k) {
        Some(ValueOrContainer::Value(LoroValue::String(v))) => v.to_string(),
        _ => String::new(),
    }
}

fn n(m: &LoroMap, k: &str) -> i64 {
    match m.get(k) {
        Some(ValueOrContainer::Value(LoroValue::I64(v))) => v,
        Some(ValueOrContainer::Value(LoroValue::Double(v))) => v as i64,
        _ => 0,
    }
}

fn opt(v: String) -> Option<String> {
    (!v.is_empty()).then_some(v)
}

fn num(v: i64) -> Option<u32> {
    u32::try_from(v).ok().filter(|x| *x > 0)
}

fn list(m: &LoroMap, k: &str) -> Vec<String> {
    s(m, k)
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect()
}

fn child(v: Option<ValueOrContainer>) -> Option<LoroMap> {
    match v {
        Some(ValueOrContainer::Container(c)) => c.into_map().ok(),
        _ => None,
    }
}

/// Write a string only when it differs (each write is an op everyone gets).
fn put_s(m: &LoroMap, k: &str, v: &str) -> loro::LoroResult<()> {
    if s(m, k) != v || m.get(k).is_none() {
        m.insert(k, v)?;
    }
    Ok(())
}

fn put_n(m: &LoroMap, k: &str, v: i64) -> loro::LoroResult<()> {
    if n(m, k) != v || m.get(k).is_none() {
        m.insert(k, v)?;
    }
    Ok(())
}

fn keys_of(m: &LoroMap) -> Vec<String> {
    let mut out: Vec<String> = m.keys().map(|k| k.to_string()).collect();
    out.sort();
    out
}

// ── a set's document ────────────────────────────────────────────────────

/// The set's id, as stamped in its `meta`.
pub fn stamped_set(doc: &LoroDoc) -> String {
    s(&doc.get_map("meta"), "set")
}

pub fn stamp_set(doc: &LoroDoc, org: &str, id: &str) {
    orgs::stamp_org(doc, org);
    if stamped_set(doc) != id {
        let _ = doc.get_map("meta").insert("set", id);
    }
}

pub fn info(doc: &LoroDoc) -> Info {
    let m = doc.get_map("set");
    let id = match s(&m, "id") {
        x if x.is_empty() => stamped_set(doc),
        x => x,
    };
    Info {
        id,
        title: s(&m, "title"),
        kind: Kind::parse(&s(&m, "kind")),
        date: s(&m, "date"),
        starts: s(&m, "starts"),
        call: s(&m, "call"),
        place: s(&m, "place"),
        series: s(&m, "series"),
        leader: opt(s(&m, "leader")),
        group: s(&m, "group"),
        size: usize::try_from(n(&m, "size")).unwrap_or(0),
        notes: s(&m, "notes"),
        from: s(&m, "from"),
        hue: u32::try_from(n(&m, "hue")).unwrap_or(0),
    }
}

pub fn put_info(doc: &LoroDoc, i: &Info) -> loro::LoroResult<()> {
    let m = doc.get_map("set");
    put_s(&m, "id", &i.id)?;
    put_s(&m, "title", &i.title)?;
    put_s(&m, "kind", i.kind.as_str())?;
    put_s(&m, "date", &i.date)?;
    put_s(&m, "starts", &i.starts)?;
    put_s(&m, "call", &i.call)?;
    put_s(&m, "place", &i.place)?;
    put_s(&m, "series", &i.series)?;
    put_s(&m, "leader", i.leader.as_deref().unwrap_or(""))?;
    put_s(&m, "group", &i.group)?;
    put_n(&m, "size", i.size as i64)?;
    put_s(&m, "notes", &i.notes)?;
    put_s(&m, "from", &i.from)?;
    put_n(&m, "hue", i64::from(i.hue))?;
    Ok(())
}

fn entries_list(doc: &LoroDoc) -> LoroMovableList {
    doc.get_movable_list("entries")
}

fn read_entry(m: &LoroMap) -> Entry {
    Entry {
        id: s(m, "id"),
        kind: ItemKind::parse(&s(m, "kind")),
        song: SongRef::parse(&s(m, "song")),
        title: s(m, "title"),
        artist: opt(s(m, "artist")),
        key: opt(s(m, "key")),
        tempo: num(n(m, "tempo")),
        length: num(n(m, "length")),
        over: Overrides {
            key: opt(s(m, "play_key")),
            arrangement: opt(s(m, "arrangement")),
            leader: opt(s(m, "leader")),
            tempo: num(n(m, "play_tempo")),
        },
        notes: s(m, "notes"),
        transition: s(m, "transition"),
    }
}

fn write_entry(m: &LoroMap, e: &Entry) -> loro::LoroResult<()> {
    put_s(m, "id", &e.id)?;
    put_s(m, "kind", e.kind.as_str())?;
    put_s(
        m,
        "song",
        &e.song.as_ref().map(SongRef::text).unwrap_or_default(),
    )?;
    put_s(m, "title", &e.title)?;
    put_s(m, "artist", e.artist.as_deref().unwrap_or(""))?;
    put_s(m, "key", e.key.as_deref().unwrap_or(""))?;
    put_n(m, "tempo", i64::from(e.tempo.unwrap_or(0)))?;
    put_n(m, "length", i64::from(e.length.unwrap_or(0)))?;
    put_s(m, "play_key", e.over.key.as_deref().unwrap_or(""))?;
    put_s(
        m,
        "arrangement",
        e.over.arrangement.as_deref().unwrap_or(""),
    )?;
    put_s(m, "leader", e.over.leader.as_deref().unwrap_or(""))?;
    put_n(m, "play_tempo", i64::from(e.over.tempo.unwrap_or(0)))?;
    put_s(m, "notes", &e.notes)?;
    put_s(m, "transition", &e.transition)?;
    Ok(())
}

/// The running order. An id is read once: two devices writing the same
/// entries at once (a built-in set seeded, a set started from last week's)
/// put each in twice with one id, and the first is the entry.
pub fn entries(doc: &LoroDoc) -> Vec<Entry> {
    let l = entries_list(doc);
    let mut seen = std::collections::HashSet::new();
    (0..l.len())
        .filter_map(|i| child(l.get(i)))
        .map(|m| read_entry(&m))
        .filter(|e| !e.id.is_empty() && seen.insert(e.id.clone()))
        .collect()
}

/// The songs, in order (no sections or items).
pub fn songs_of(doc: &LoroDoc) -> Vec<Entry> {
    entries(doc).into_iter().filter(Entry::is_song).collect()
}

fn find(doc: &LoroDoc, id: &str) -> Option<(usize, LoroMap)> {
    let l = entries_list(doc);
    (0..l.len()).find_map(|i| child(l.get(i)).filter(|m| s(m, "id") == id).map(|m| (i, m)))
}

pub fn entry(doc: &LoroDoc, id: &str) -> Option<Entry> {
    find(doc, id).map(|(_, m)| read_entry(&m))
}

/// A new entry id, unique across devices.
pub fn new_entry_id(doc: &LoroDoc) -> String {
    drive_spine::new_id(doc, "e")
}

/// Put an entry in at `at` (None: the end); its id (a new one if it had none).
pub fn insert_entry(doc: &LoroDoc, at: Option<usize>, e: &Entry) -> loro::LoroResult<String> {
    let l = entries_list(doc);
    let mut e = e.clone();
    if e.id.is_empty() {
        e.id = new_entry_id(doc);
    }
    let at = at.unwrap_or(l.len()).min(l.len());
    let m = l.insert_container(at, LoroMap::new())?;
    write_entry(&m, &e)?;
    Ok(e.id)
}

/// Change an entry where it is (only what differs is written).
pub fn update_entry(doc: &LoroDoc, e: &Entry) -> loro::LoroResult<()> {
    if let Some((_, m)) = find(doc, &e.id) {
        write_entry(&m, e)?;
    }
    Ok(())
}

/// Take an entry out (every copy of its id).
pub fn remove_entry(doc: &LoroDoc, id: &str) -> loro::LoroResult<()> {
    while let Some((i, _)) = find(doc, id) {
        entries_list(doc).delete(i, 1)?;
    }
    Ok(())
}

/// Move an entry to a place in the order (as the list is now).
pub fn move_entry(doc: &LoroDoc, id: &str, to: usize) -> loro::LoroResult<()> {
    let l = entries_list(doc);
    if let Some((from, _)) = find(doc, id) {
        let to = to.min(l.len().saturating_sub(1));
        if from != to {
            l.mov(from, to)?;
        }
    }
    Ok(())
}

/// Move an entry up (`-1`) or down (`1`).
pub fn move_by(doc: &LoroDoc, id: &str, by: i32) -> loro::LoroResult<()> {
    if let Some((from, _)) = find(doc, id) {
        let to = from as i64 + i64::from(by);
        if to >= 0 && (to as usize) < entries_list(doc).len() {
            move_entry(doc, id, to as usize)?;
        }
    }
    Ok(())
}

/// Move an entry to just before another (`before` None: the end): what a
/// drag onto a row does.
pub fn move_before(doc: &LoroDoc, id: &str, before: Option<&str>) -> loro::LoroResult<()> {
    let Some((from, _)) = find(doc, id) else {
        return Ok(());
    };
    let len = entries_list(doc).len();
    let target = match before.and_then(|b| find(doc, b)) {
        Some((i, _)) if i > from => i - 1,
        Some((i, _)) => i,
        None => len - 1,
    };
    move_entry(doc, id, target)
}

#[cfg_attr(not(test), allow(dead_code))]
/// The set in its named runs: each section entry (None for the songs
/// before the first) and what follows it.
pub fn sections(list: &[Entry]) -> Vec<(Option<Entry>, Vec<Entry>)> {
    let mut out: Vec<(Option<Entry>, Vec<Entry>)> = Vec::new();
    for e in list {
        if e.kind == ItemKind::Section {
            out.push((Some(e.clone()), Vec::new()));
        } else {
            match out.last_mut() {
                Some(last) => last.1.push(e.clone()),
                None => out.push((None, vec![e.clone()])),
            }
        }
    }
    out
}

// ── the band ────────────────────────────────────────────────────────────

pub fn band(doc: &LoroDoc) -> Vec<Seat> {
    let b = doc.get_map("band");
    let mut out: Vec<Seat> = keys_of(&b)
        .into_iter()
        .filter_map(|k| child(b.get(&k)).map(|m| (k, m)))
        .map(|(k, m)| Seat {
            name: match s(&m, "name") {
                n if n.is_empty() => k.clone(),
                n => n,
            },
            person: k,
            parts: list(&m, "parts"),
        })
        .collect();
    out.sort_by(|a, b| {
        let at = |s: &Seat| {
            s.parts
                .iter()
                .filter_map(|p| orgs::PARTS.iter().position(|x| x == p))
                .min()
                .unwrap_or(usize::MAX)
        };
        (at(a), &a.person).cmp(&(at(b), &b.person))
    });
    out
}

/// Seat someone in the band, with their parts in this set.
pub fn put_seat(doc: &LoroDoc, seat: &Seat) -> loro::LoroResult<()> {
    let m = doc.get_map("band").ensure_mergeable_map(&seat.person)?;
    put_s(&m, "name", &seat.name)?;
    put_s(&m, "parts", &seat.parts.join(","))
}

pub fn remove_seat(doc: &LoroDoc, person: &str) -> loro::LoroResult<()> {
    let b = doc.get_map("band");
    if b.get(person).is_some() {
        b.delete(person)?;
    }
    Ok(())
}

/// The band from a team group: everyone in it, with the parts they play.
pub fn band_from_group(people: &[orgs::Person], group: &str) -> Vec<Seat> {
    people
        .iter()
        .filter(|p| p.groups.iter().any(|g| g == group))
        .map(|p| Seat {
            person: p.id.clone(),
            name: p.name.clone(),
            parts: p.parts.clone(),
        })
        .collect()
}

// ── rehearsals ──────────────────────────────────────────────────────────

pub fn rehearsals(doc: &LoroDoc) -> Vec<Rehearsal> {
    let r = doc.get_map("rehearsals");
    let mut out: Vec<Rehearsal> = keys_of(&r)
        .into_iter()
        .filter_map(|k| child(r.get(&k)))
        .map(|m| Rehearsal {
            id: s(&m, "id"),
            date: s(&m, "date"),
            time: s(&m, "time"),
            room: s(&m, "room"),
        })
        .collect();
    out.sort_by(|a, b| (&a.date, &a.time).cmp(&(&b.date, &b.time)));
    out
}

pub fn put_rehearsal(doc: &LoroDoc, r: &Rehearsal) -> loro::LoroResult<String> {
    let id = if r.id.is_empty() {
        drive_spine::new_id(doc, "r")
    } else {
        r.id.clone()
    };
    let m = doc.get_map("rehearsals").ensure_mergeable_map(&id)?;
    put_s(&m, "id", &id)?;
    put_s(&m, "date", &r.date)?;
    put_s(&m, "time", &r.time)?;
    put_s(&m, "room", &r.room)?;
    Ok(id)
}

// ── what a song is played as ────────────────────────────────────────────

/// What an entry is played as, after the lookups.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Played {
    /// The key: the entry's, else the org's use, else the song's.
    pub key: Option<String>,
    /// The song's own key, to say how far it moved.
    pub own_key: Option<String>,
    pub arrangement: String,
    /// Who leads it: the entry's, else the org's use, else the set's leader.
    pub leader: Option<String>,
    pub tempo: Option<u32>,
}

/// Look an entry up: the entry, then the org's use, then the song (`song`:
/// the library's face now, when the org can read it; else the entry's).
pub fn resolve(
    e: &Entry,
    song: Option<&crate::songs::Entry>,
    u: &Use,
    st: &Settings,
    set: &Info,
) -> Played {
    let own_key = song.and_then(|x| x.key.clone()).or_else(|| e.key.clone());
    let own_tempo = song.and_then(|x| x.tempo).or(e.tempo);
    Played {
        key: e
            .over
            .key
            .clone()
            .or_else(|| u.key.clone())
            .or_else(|| own_key.clone()),
        own_key,
        arrangement: e
            .over
            .arrangement
            .clone()
            .unwrap_or_else(|| orgs::arrangement_here(u, st)),
        leader: e
            .over
            .leader
            .clone()
            .or_else(|| u.leader.clone())
            .or_else(|| set.leader.clone()),
        tempo: e.over.tempo.or(own_tempo),
    }
}

/// What one person sees of a set: their parts, and per song its key and,
/// for a guitar, where the capo goes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MyView {
    pub parts: Vec<String>,
    /// Per song entry: its id, the key, the capo hint (fret, shape), and
    /// whether they lead it.
    pub songs: Vec<(String, Option<String>, Option<(u8, String)>, bool)>,
}

/// A person's view of a set (`played`: each song entry's id and what it is
/// played as).
pub fn my_view(person: &str, band: &[Seat], played: &[(String, Played)]) -> Option<MyView> {
    let seat = band.iter().find(|s| s.person == person)?;
    let guitar = seat.parts.iter().any(|p| p.contains("guitar"));
    Some(MyView {
        parts: seat.parts.clone(),
        songs: played
            .iter()
            .map(|(id, p)| {
                let capo = if guitar {
                    p.key.as_deref().and_then(crate::keys::capo)
                } else {
                    None
                };
                (
                    id.clone(),
                    p.key.clone(),
                    capo.map(|(f, s)| (f, s.to_string())),
                    p.leader.as_deref() == Some(person),
                )
            })
            .collect(),
    })
}

// ── the index: faces and recurring events ───────────────────────────────

/// A set as the lists show it, without opening it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Face {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    pub date: String,
    pub starts: String,
    pub call: String,
    pub place: String,
    pub series: String,
    pub songs: usize,
    pub size: usize,
    /// The first songs' titles.
    pub first: Vec<String>,
    /// Who plays it (people's ids).
    pub band: Vec<String>,
    pub shared_with: Vec<String>,
    /// Its rehearsals (date, time, room; no ids).
    pub rehearsals: Vec<Rehearsal>,
    pub hue: u32,
}

impl Face {
    pub fn day(&self) -> Option<Day> {
        parse_day(&self.date)
    }

    /// From today on (a template, never).
    pub fn upcoming(&self) -> bool {
        self.day().is_some_and(|d| d.from_today() >= 0)
    }
}

/// The face of a set's document.
pub fn face_of(doc: &LoroDoc) -> Face {
    let i = info(doc);
    let songs = songs_of(doc);
    Face {
        id: i.id,
        title: i.title,
        kind: i.kind,
        date: i.date,
        starts: i.starts,
        call: i.call,
        place: i.place,
        series: i.series,
        songs: songs.len(),
        size: i.size,
        first: songs.iter().take(3).map(|e| e.title.clone()).collect(),
        band: band(doc).into_iter().map(|s| s.person).collect(),
        shared_with: shared_with(doc),
        rehearsals: rehearsals(doc)
            .into_iter()
            .map(|r| Rehearsal {
                id: String::new(),
                ..r
            })
            .collect(),
        hue: i.hue,
    }
}

/// Sets taken out of the org's list (their setlist file was deleted by
/// hand): the index's `removed` map, set id → when. Their documents are
/// kept; only the list no longer shows them, and nothing writes their
/// file again.
pub fn removed_sets(index: &LoroDoc) -> Vec<String> {
    keys_of(&index.get_map("removed"))
}

/// Take a set out of the org's list (see `removed_sets`).
pub fn remove_set(index: &LoroDoc, id: &str, when: &str) {
    let _ = put_s(&index.get_map("removed"), id, when);
}

pub fn faces(index: &LoroDoc) -> Vec<Face> {
    let m = index.get_map("sets");
    let gone = removed_sets(index);
    let mut out: Vec<Face> = keys_of(&m)
        .into_iter()
        .filter(|k| !gone.contains(k))
        .filter_map(|k| child(m.get(&k)))
        .map(|m| Face {
            id: s(&m, "id"),
            title: s(&m, "title"),
            kind: Kind::parse(&s(&m, "kind")),
            date: s(&m, "date"),
            starts: s(&m, "starts"),
            call: s(&m, "call"),
            place: s(&m, "place"),
            series: s(&m, "series"),
            songs: usize::try_from(n(&m, "songs")).unwrap_or(0),
            size: usize::try_from(n(&m, "size")).unwrap_or(0),
            first: s(&m, "first")
                .split('\n')
                .filter(|x| !x.is_empty())
                .map(str::to_string)
                .collect(),
            band: list(&m, "band"),
            shared_with: list(&m, "shared_with"),
            rehearsals: s(&m, "rehearsals")
                .split(';')
                .filter_map(|r| {
                    let mut it = r.split('|');
                    Some(Rehearsal {
                        id: String::new(),
                        date: it.next()?.to_string(),
                        time: it.next()?.to_string(),
                        room: it.next()?.to_string(),
                    })
                })
                .collect(),
            hue: u32::try_from(n(&m, "hue")).unwrap_or(0),
        })
        .filter(|f| !f.id.is_empty())
        .collect();
    out.sort_by(|a, b| (&a.date, &a.title).cmp(&(&b.date, &b.title)));
    out
}

pub fn face(index: &LoroDoc, id: &str) -> Option<Face> {
    faces(index).into_iter().find(|f| f.id == id)
}

/// Bring a set's face in the index up to date; whether it changed.
pub fn put_face(index: &LoroDoc, f: &Face) -> bool {
    if face(index, &f.id).as_ref() == Some(f) {
        return false;
    }
    let Ok(m) = index.get_map("sets").ensure_mergeable_map(&f.id) else {
        return false;
    };
    let _ = put_s(&m, "id", &f.id);
    let _ = put_s(&m, "title", &f.title);
    let _ = put_s(&m, "kind", f.kind.as_str());
    let _ = put_s(&m, "date", &f.date);
    let _ = put_s(&m, "starts", &f.starts);
    let _ = put_s(&m, "call", &f.call);
    let _ = put_s(&m, "place", &f.place);
    let _ = put_s(&m, "series", &f.series);
    let _ = put_n(&m, "songs", f.songs as i64);
    let _ = put_n(&m, "size", f.size as i64);
    let _ = put_s(&m, "first", &f.first.join("\n"));
    let _ = put_s(&m, "band", &f.band.join(","));
    let _ = put_s(&m, "shared_with", &f.shared_with.join(","));
    let reh: Vec<String> = f
        .rehearsals
        .iter()
        .map(|r| format!("{}|{}|{}", r.date, r.time, r.room))
        .collect();
    let _ = put_s(&m, "rehearsals", &reh.join(";"));
    let _ = put_n(&m, "hue", i64::from(f.hue));
    true
}

/// A recurring event: the days it meets, where, how many songs a set.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Series {
    pub id: String,
    pub name: String,
    /// Weekdays, 0 for Sunday.
    pub days: Vec<usize>,
    pub hue: u32,
    pub size: usize,
    pub room: String,
    pub starts: String,
    pub call: String,
    /// What one of its days is called, where it is not just the weekday.
    pub note: Option<(usize, String)>,
    /// The team group that plays it.
    pub group: String,
}

impl Series {
    /// "Every Sunday and Wednesday night".
    pub fn every(&self) -> String {
        let days: Vec<String> = self.days.iter().map(|d| self.day_name(*d)).collect();
        format!("Every {}", days.join(" and "))
    }

    fn day_name(&self, d: usize) -> String {
        match &self.note {
            Some((n, name)) if *n == d => name.clone(),
            _ => data::DAY[d % 7].to_string(),
        }
    }

    /// A meeting's name: "Main, Wednesday night", or the event's.
    pub fn name_on(&self, d: Day) -> String {
        match &self.note {
            Some((n, name)) if *n == d.weekday() => format!("{}, {name}", self.name),
            _ => self.name.clone(),
        }
    }

    /// The days it meets from `from` to `to`, both included.
    pub fn dates(&self, from: Day, to: Day) -> Vec<Day> {
        (from.0..=to.0)
            .map(Day)
            .filter(|d| self.days.contains(&d.weekday()))
            .collect()
    }

    /// The next day it meets, from `from` on.
    pub fn next_from(&self, from: Day) -> Option<Day> {
        self.dates(from, from.plus(6)).into_iter().next()
    }

    /// A new meeting's set: its fields from the event.
    pub fn meeting(&self, d: Day) -> Info {
        Info {
            id: meeting_id(&self.id, d),
            title: self.name_on(d),
            kind: Kind::Service,
            date: d.iso(),
            starts: self.starts.clone(),
            call: self.call.clone(),
            place: self.room.clone(),
            series: self.id.clone(),
            group: self.group.clone(),
            size: self.size,
            hue: self.hue,
            ..Info::default()
        }
    }
}

pub fn series_list(index: &LoroDoc) -> Vec<Series> {
    let m = index.get_map("series");
    let mut out: Vec<Series> = keys_of(&m)
        .into_iter()
        .filter_map(|k| child(m.get(&k)))
        .map(|m| Series {
            id: s(&m, "id"),
            name: s(&m, "name"),
            days: list(&m, "days")
                .iter()
                .filter_map(|d| d.parse().ok())
                .collect(),
            hue: u32::try_from(n(&m, "hue")).unwrap_or(0),
            size: usize::try_from(n(&m, "size")).unwrap_or(0),
            room: s(&m, "room"),
            starts: s(&m, "starts"),
            call: s(&m, "call"),
            note: s(&m, "note")
                .split_once(':')
                .and_then(|(d, name)| Some((d.parse().ok()?, name.to_string()))),
            group: s(&m, "group"),
        })
        .filter(|x| !x.id.is_empty())
        .collect();
    out.sort_by_key(|x| (*x.days.first().unwrap_or(&7), x.name.clone()));
    out
}

pub fn put_series(index: &LoroDoc, x: &Series) -> loro::LoroResult<()> {
    let m = index.get_map("series").ensure_mergeable_map(&x.id)?;
    put_s(&m, "id", &x.id)?;
    put_s(&m, "name", &x.name)?;
    let days: Vec<String> = x.days.iter().map(usize::to_string).collect();
    put_s(&m, "days", &days.join(","))?;
    put_n(&m, "hue", i64::from(x.hue))?;
    put_n(&m, "size", x.size as i64)?;
    put_s(&m, "room", &x.room)?;
    put_s(&m, "starts", &x.starts)?;
    put_s(&m, "call", &x.call)?;
    put_s(
        &m,
        "note",
        &x.note
            .as_ref()
            .map(|(d, n)| format!("{d}:{n}"))
            .unwrap_or_default(),
    )?;
    put_s(&m, "group", &x.group)?;
    Ok(())
}

/// A recurring event's sets, by date.
pub fn meetings<'a>(faces: &'a [Face], series: &str) -> Vec<&'a Face> {
    let mut v: Vec<&Face> = faces.iter().filter(|f| f.series == series).collect();
    v.sort_by(|a, b| a.date.cmp(&b.date));
    v
}

/// The latest set of a recurring event before a day: what "from last
/// week's" copies.
pub fn previous<'a>(faces: &'a [Face], series: &str, before: Day) -> Option<&'a Face> {
    meetings(faces, series)
        .into_iter()
        .filter(|f| f.day().is_some_and(|d| d < before) && f.songs > 0)
        .next_back()
}

// ── starting a set from another ─────────────────────────────────────────

/// Start a set from another (last week's, or a template): its running
/// order (keeping each entry's overrides, notes and transitions) and its
/// band, under `info`. An empty set only: a set that already has entries
/// is left as it is. Each copy's id is the new set's and the old entry's,
/// so two devices starting the same set at once read as one.
pub fn start_from(src: &LoroDoc, dst: &LoroDoc, info: &Info) -> loro::LoroResult<bool> {
    let mut i = info.clone();
    i.from = self::info(src).id;
    put_info(dst, &i)?;
    if !entries(dst).is_empty() {
        return Ok(false);
    }
    for e in entries(src) {
        insert_entry(
            dst,
            None,
            &Entry {
                id: format!("{}~{}", i.id, e.id),
                ..e
            },
        )?;
    }
    if band(dst).is_empty() {
        for seat in band(src) {
            put_seat(dst, &seat)?;
        }
    }
    Ok(true)
}

// ── sharing ─────────────────────────────────────────────────────────────

pub fn shared_with(doc: &LoroDoc) -> Vec<String> {
    keys_of(&doc.get_map("shared_with"))
}

pub fn share_set(doc: &LoroDoc, org: &str, on: bool) -> loro::LoroResult<()> {
    let m = doc.get_map("shared_with");
    if on {
        if m.get(org).is_none() {
            m.insert(org, true)?;
        }
    } else if m.get(org).is_some() {
        m.delete(org)?;
    }
    Ok(())
}

/// The documents a set's sharing project holds: the set's, and the
/// sessions of its songs that its org owns (a song from a third org is
/// that org's to share; the entry's face still reads).
fn set_project_wants(doc: &LoroDoc) -> (Vec<String>, Vec<String>) {
    let org = orgs::stamped_org(doc);
    let id = info(doc).id;
    let mut docs: Vec<String> = songs_of(doc)
        .into_iter()
        .filter_map(|e| e.song)
        .filter(|r| r.owner == org)
        .map(|r| crate::songs::song_doc(&r.id))
        .collect();
    docs.push(setlist_doc(&id));
    docs.sort();
    docs.dedup();
    (docs, shared_with(doc))
}

/// Whether the set's sharing project needs [`mirror_set_project`].
pub fn set_project_differs(doc: &LoroDoc, registry: &LoroDoc) -> bool {
    let (docs, shared) = set_project_wants(doc);
    let id = set_project(&info(doc).id);
    match drive_files::projects::projects(registry)
        .into_iter()
        .find(|p| p.id == id)
    {
        None => !shared.is_empty(),
        Some(p) => {
            let mut have = p.shared_with.clone();
            have.sort();
            p.docs != docs || have != shared
        }
    }
}

/// Bring the set's sharing project up to date in the owner's registry;
/// whether it changed.
pub fn mirror_set_project(doc: &LoroDoc, registry: &LoroDoc) -> bool {
    use drive_files::projects;
    if !set_project_differs(doc, registry) {
        return false;
    }
    let (docs, shared) = set_project_wants(doc);
    let i = info(doc);
    let id = set_project(&i.id);
    let current = projects::projects(registry)
        .into_iter()
        .find(|p| p.id == id);
    let Ok(m) = registry.get_map("projects").ensure_mergeable_map(&id) else {
        return false;
    };
    if current.is_none() {
        let _ = m.insert("id", id.as_str());
        let _ = m.insert("kind", "Collection");
        let _ = m.insert(
            "created",
            web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64),
        );
    }
    let _ = m.insert("name", format!("Studio set: {}", i.title));
    let (had_docs, had_orgs) = current.map(|p| (p.docs, p.shared_with)).unwrap_or_default();
    for d in had_docs.iter().filter(|d| !docs.contains(d)) {
        let _ = projects::link_doc(registry, &id, d, false);
    }
    for d in docs.iter().filter(|d| !had_docs.contains(d)) {
        let _ = projects::link_doc(registry, &id, d, true);
    }
    for o in had_orgs.iter().filter(|o| !shared.contains(o)) {
        let _ = projects::share_with_org(registry, &id, o, false);
    }
    for o in shared.iter().filter(|o| !had_orgs.contains(o)) {
        let _ = projects::share_with_org(registry, &id, o, true);
    }
    true
}

#[cfg_attr(not(feature = "drive"), allow(dead_code))]
/// The sets other orgs share with `to`, from Drive's projects shared with
/// it (`/shared`: (owner org, project id) pairs): (owner, set id).
pub fn shared_sets(to: &str, shared: &[(String, String)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = shared
        .iter()
        .filter(|(org, _)| org != to)
        .filter_map(|(org, p)| {
            p.strip_prefix("studio-set-")
                .map(|id| (org.clone(), id.to_string()))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

// ── plain text ──────────────────────────────────────────────────────────

/// A set as plain text, to print, paste or send: its title and date, the
/// band, then each section and entry with its key, arrangement, leader,
/// notes and transition (`played` and `names` give what each song is
/// played as and people's names).
pub fn plain_text(
    info: &Info,
    list: &[Entry],
    played: &dyn Fn(&Entry) -> Played,
    band: &[Seat],
    name: &dyn Fn(&str) -> String,
) -> String {
    let mut out = info.title.clone();
    if let Some(d) = info.day() {
        out.push_str(&format!(
            " · {}, {} {} {}",
            data::DAY[d.weekday()],
            data::MONTH[d.month0()],
            d.mday(),
            d.year()
        ));
    }
    out.push('\n');
    let place: Vec<&str> = [info.place.as_str()]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect();
    let mut when = place.join("");
    if !info.call.is_empty() {
        when.push_str(&format!(" · call {}", info.call));
    }
    if !info.starts.is_empty() {
        when.push_str(&format!(" · starts {}", info.starts));
    }
    if !when.is_empty() {
        out.push_str(when.trim_start_matches(" · "));
        out.push('\n');
    }
    if !band.is_empty() {
        let b: Vec<String> = band
            .iter()
            .map(|s| format!("{} ({})", name(&s.person), s.parts.join(", ")))
            .collect();
        out.push_str(&format!("Band: {}\n", b.join("; ")));
    }
    if !info.notes.is_empty() {
        out.push_str(&format!("{}\n", info.notes));
    }
    let mut k = 0;
    for e in list {
        match e.kind {
            ItemKind::Section => out.push_str(&format!("\n{}\n", e.title.to_uppercase())),
            ItemKind::Song => {
                k += 1;
                let p = played(e);
                let mut line = format!("{k}. {}", e.title);
                if let Some(key) = &p.key {
                    line.push_str(&format!(" · {key}"));
                    if let Some(own) = p.own_key.as_ref().filter(|o| *o != key) {
                        line.push_str(&format!(" (from {own})"));
                    }
                }
                if let Some(t) = p.tempo {
                    line.push_str(&format!(" · {t} bpm"));
                }
                if p.arrangement != "Normal" {
                    line.push_str(&format!(" · {}", p.arrangement));
                }
                if let Some(l) = &p.leader {
                    line.push_str(&format!(" · {}", name(l)));
                }
                out.push_str(&line);
                out.push('\n');
            }
            kind => out.push_str(&format!("   [{}] {}\n", kind.label(), e.title)),
        }
        if !e.notes.is_empty() {
            out.push_str(&format!("   {}\n", e.notes));
        }
        if !e.transition.is_empty() {
            out.push_str(&format!("   → {}\n", e.transition));
        }
    }
    out
}

// ── the built-in sets, and the versions ─────────────────────────────────

/// The org a built-in set belongs to: the weekly sets Crossroads
/// Church's, the recital Rockstars of Tomorrow's, the Christmas show CBU's.
pub fn builtin_home(id: &str) -> Option<&'static str> {
    if data::occ(id).is_some() {
        Some("crossroads-church")
    } else {
        match id {
            "jt" => Some("rockstars-of-tomorrow"),
            "mb" => Some("cbu"),
            _ => None,
        }
    }
}

/// A built-in song, as an entry: owned by its home org (phase 1's
/// `home_of`), else by the set's.
fn builtin_entry(org: &str, x: &data::Song) -> Entry {
    let lib = data::LIB.iter().find(|l| l.song.title == x.title);
    let tags: Vec<String> = lib
        .map(|l| l.tags.iter().map(|t| t.to_string()).collect())
        .unwrap_or_default();
    let owner = orgs::home_of(&tags).unwrap_or(org);
    Entry {
        kind: ItemKind::Song,
        song: Some(SongRef::new(owner, &session_sync::slug::slugify(x.title))),
        title: x.title.into(),
        artist: x.artist.map(str::to_string),
        key: x.key.map(str::to_string),
        tempo: x.tempo,
        length: x.len,
        ..Entry::default()
    }
}

/// A person of the seeded teams, by the prototype's first name.
fn person_of(name: &str) -> String {
    match name {
        "Cody" => "cody-wright".into(),
        n => session_sync::slug::slugify(n),
    }
}

/// The band a built-in set starts with (the seeded teams' people).
fn builtin_band(id: &str) -> Vec<Seat> {
    let seat = |p: &str, parts: &[&str]| Seat {
        person: p.into(),
        name: match p {
            "cody-wright" => "Cody Wright".into(),
            "dr-hale" => "Dr. Hale".into(),
            p => data::cap(p),
        },
        parts: parts.iter().map(|x| x.to_string()).collect(),
    };
    let mine = data::mine(id).is_some();
    match id {
        "jt" => vec![
            seat("cody-wright", &["Electric guitar", "Bass"]),
            seat("jordan", &["Electric guitar", "Lead vocals"]),
            seat("ava", &["Drums"]),
            seat("liam", &["Bass"]),
        ],
        "mb" => vec![
            seat("cody-wright", &["Electric guitar"]),
            seat("dr-hale", &["Keys"]),
        ],
        _ => {
            let mut v = vec![
                seat("jake", &["Lead vocals", "Acoustic guitar"]),
                seat("mia", &["Vocals", "Keys"]),
                seat("sam", &["Drums"]),
            ];
            if mine {
                v.push(seat("cody-wright", &["Electric guitar"]));
            } else {
                v.push(seat("riley", &["Sound", "Lights"]));
            }
            v
        }
    }
}

fn builtin_series(x: &data::Series) -> Series {
    let pl = x.place();
    Series {
        id: x.id.into(),
        name: x.name.into(),
        days: x.days.to_vec(),
        hue: x.hue,
        size: x.size,
        room: pl.map_or(String::new(), |p| p.room.into()),
        starts: pl.map_or(String::new(), |p| p.starts.into()),
        call: pl.map_or(String::new(), |p| p.call.into()),
        note: x.note.map(|(d, n)| (d, n.to_string())),
        group: if x.id == "main" { "worship" } else { "youth" }.into(),
    }
}

/// Write a built-in set into an empty document; whether it was one.
fn write_builtin(doc: &LoroDoc, org: &str, id: &str) -> bool {
    if builtin_home(id) != Some(org) || !entries(doc).is_empty() {
        return false;
    }
    if let Some(o) = data::occ(id) {
        let sr = builtin_series(o.series);
        let mut i = sr.meeting(o.date);
        i.title = o.name();
        let _ = put_info(doc, &i);
        for (name, from, to) in data::sections_of(o) {
            let mut sec = Entry::item(ItemKind::Section, name);
            sec.id = format!("b-{}", session_sync::slug::slugify(name));
            let _ = insert_entry(doc, None, &sec);
            for (k, x) in o.songs.iter().enumerate().take(to).skip(from) {
                let mut e = builtin_entry(org, x);
                e.id = format!("b{k}");
                let (lead, _) = data::leads_of(o, k, x);
                e.over.leader = lead.map(person_of);
                let arr = data::arr_default(o, k);
                if arr != "Normal" {
                    e.over.arrangement = Some(arr.into());
                }
                e.over.key = data::key_default(id, k).map(str::to_string);
                let _ = insert_entry(doc, None, &e);
            }
        }
        for seat in builtin_band(id) {
            let _ = put_seat(doc, &seat);
        }
        for r in data::first_rehearsals().iter().filter(|r| r.set == id) {
            let _ = put_rehearsal(
                doc,
                &Rehearsal {
                    id: format!("r-{}", r.date.iso()),
                    date: r.date.iso(),
                    time: r.time.clone(),
                    room: r.room.clone(),
                },
            );
        }
        return true;
    }
    let Some(st) = data::show(id) else {
        return false;
    };
    let date = match id {
        "jt" => Day::ymd(2026, 5, 15),
        _ => Day::ymd(2026, 12, 12),
    };
    let _ = put_info(
        doc,
        &Info {
            id: id.into(),
            title: st.title.into(),
            kind: Kind::Show,
            date: date.iso(),
            place: if id == "jt" {
                "Studio A"
            } else {
                "Recital Hall"
            }
            .into(),
            starts: "7:00".into(),
            call: "5:30".into(),
            leader: Some(if id == "jt" { "cody-wright" } else { "dr-hale" }.into()),
            group: if id == "jt" { "recital" } else { "christmas" }.into(),
            hue: st.hue,
            ..Info::default()
        },
    );
    let mut k = 0;
    for (act, songs) in &st.acts {
        let mut sec = Entry::item(ItemKind::Section, act);
        sec.id = format!("b-{}", session_sync::slug::slugify(act));
        let _ = insert_entry(doc, None, &sec);
        for x in songs {
            let mut e = builtin_entry(org, x);
            e.id = format!("b{k}");
            k += 1;
            let _ = insert_entry(doc, None, &e);
        }
    }
    for seat in builtin_band(id) {
        let _ = put_seat(doc, &seat);
    }
    true
}

/// The ids of an org's built-in sets.
pub fn builtin_ids(org: &str) -> Vec<String> {
    let mut v: Vec<String> = data::OCC
        .iter()
        .map(|o| o.id.clone())
        .chain(["jt".to_string(), "mb".to_string()])
        .filter(|id| builtin_home(id) == Some(org))
        .collect();
    v.sort();
    v
}

/// A set's version 1: a built-in set of its org (the prototype's weekly
/// sets and shows), written in. A new set is already version 1.
pub fn setlist_v1(doc: &LoroDoc) {
    let (org, id) = (orgs::stamped_org(doc), stamped_set(doc));
    write_builtin(doc, &org, &id);
}

/// A set's shape versions (`drive_spine::schema`).
pub const SETLIST_STEPS: &[drive_spine::schema::Step] = &[setlist_v1];

/// The index's version 1: the org's recurring events and the faces of its
/// built-in sets (each set's own document is written when it is first
/// opened, by [`setlist_v1`]; the face here is the same set's).
///
/// Crossroads Church gets its recurring events only (Main, JHM Sunday,
/// HSM, JHM, CYA, each with its room and times): its sets are planned in
/// Studio as they come, not the prototype's sample weeks.
pub fn index_v1(index: &LoroDoc) {
    let org = orgs::stamped_org(index);
    if org == "crossroads-church" {
        for x in data::SERIES.iter() {
            let _ = put_series(index, &builtin_series(x));
        }
        return;
    }
    for id in builtin_ids(&org) {
        let tmp = LoroDoc::new();
        stamp_set(&tmp, &org, &id);
        if write_builtin(&tmp, &org, &id) {
            put_face(index, &face_of(&tmp));
        }
    }
}

pub const INDEX_STEPS: &[drive_spine::schema::Step] = &[index_v1];

/// Carry the prototype's live keys and arrangements (`studio:set:<id>`:
/// `songs`, by the song's place) onto a set's song entries, once. How
/// many it moved.
pub fn import_legacy(doc: &LoroDoc, legacy: &LoroDoc) -> usize {
    let meta = doc.get_map("meta");
    if matches!(
        meta.get("legacy"),
        Some(ValueOrContainer::Value(LoroValue::Bool(true)))
    ) {
        return 0;
    }
    let old = legacy.get_map("songs");
    let songs = songs_of(doc);
    let mut moved = 0;
    for k in old.keys() {
        let (Ok(i), Some(m)) = (k.parse::<usize>(), child(old.get(&k))) else {
            continue;
        };
        let Some(mut e) = songs.get(i).cloned() else {
            continue;
        };
        let (key, arr) = (opt(s(&m, "key")), opt(s(&m, "arr")));
        if key.is_some() {
            e.over.key = key;
        }
        if let Some(a) = arr {
            e.over.arrangement = (a != "Normal").then_some(a);
        }
        let _ = update_entry(doc, &e);
        moved += 1;
    }
    let _ = meta.insert("legacy", true);
    moved
}

/// What the calendar says of a dated set (`base`: where the app is served).
pub fn calendar_event(i: &Info, base: &str) -> Option<drive_spine::Event> {
    if i.kind == Kind::Template || i.day().is_none() {
        return None;
    }
    Some(drive_spine::Event {
        id: setlist_doc(&i.id),
        title: format!("{} · {}", i.title, i.kind.label().to_lowercase()),
        app: "studio".into(),
        link: format!("{base}/set/{}", i.id),
        date: i.date.clone(),
        time: i.starts.clone(),
        place: i.place.clone(),
        ..Default::default()
    })
}

pub fn rehearsal_event(i: &Info, r: &Rehearsal, base: &str) -> drive_spine::Event {
    drive_spine::Event {
        id: format!("studio:reh:{}:{}", i.id, r.id),
        title: format!("Rehearsal · {}", i.title),
        app: "studio".into(),
        link: format!("{base}/set/{}", i.id),
        date: r.date.clone(),
        time: r.time.clone(),
        place: r.room.clone(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drive_spine::schema;
    use loro::ExportMode;

    fn sync(a: &LoroDoc, b: &LoroDoc) {
        a.import(&b.export(ExportMode::Snapshot).unwrap()).unwrap();
        b.import(&a.export(ExportMode::Snapshot).unwrap()).unwrap();
    }

    fn song(title: &str, key: &str) -> Entry {
        Entry {
            kind: ItemKind::Song,
            song: Some(SongRef::new(
                "crossroads-church",
                &session_sync::slug::slugify(title),
            )),
            title: title.into(),
            key: Some(key.into()),
            ..Entry::default()
        }
    }

    fn titles(d: &LoroDoc) -> Vec<String> {
        entries(d).into_iter().map(|e| e.title).collect()
    }

    fn a_set() -> LoroDoc {
        let d = LoroDoc::new();
        stamp_set(&d, "crossroads-church", "s1");
        put_info(
            &d,
            &Info {
                id: "s1".into(),
                title: "Sunday".into(),
                date: "2026-11-29".into(),
                ..Info::default()
            },
        )
        .unwrap();
        for (t, k) in [("One", "G"), ("Two", "A"), ("Three", "D"), ("Four", "E")] {
            insert_entry(&d, None, &song(t, k)).unwrap();
        }
        d
    }

    #[test]
    fn reorders_from_two_devices_merge_into_one_order() {
        let a = a_set();
        let b = LoroDoc::new();
        b.import(&a.export(ExportMode::Snapshot).unwrap()).unwrap();
        let id = |d: &LoroDoc, t: &str| entries(d).into_iter().find(|e| e.title == t).unwrap().id;
        // A moves Four to the top while B moves One to the end, and B
        // changes Two's key: all of it lands, the same on both.
        move_entry(&a, &id(&a, "Four"), 0).unwrap();
        move_before(&b, &id(&b, "One"), None).unwrap();
        let mut two = entry(&b, &id(&b, "Two")).unwrap();
        two.over.key = Some("Bb".into());
        update_entry(&b, &two).unwrap();
        a.commit();
        b.commit();
        sync(&a, &b);
        assert_eq!(titles(&a), titles(&b));
        assert_eq!(titles(&a), vec!["Four", "Two", "Three", "One"]);
        assert_eq!(
            entry(&a, &id(&a, "Two")).unwrap().over.key.as_deref(),
            Some("Bb")
        );
        // Both moving the same entry: it lands once, in one place.
        move_entry(&a, &id(&a, "Three"), 0).unwrap();
        move_entry(&b, &id(&b, "Three"), 3).unwrap();
        sync(&a, &b);
        assert_eq!(titles(&a), titles(&b));
        assert_eq!(titles(&a).iter().filter(|t| *t == "Three").count(), 1);
        assert_eq!(entries(&a).len(), 4);
    }

    #[test]
    fn a_drag_onto_a_row_puts_it_before_that_row() {
        let d = a_set();
        let id = |t: &str| entries(&d).into_iter().find(|e| e.title == t).unwrap().id;
        move_before(&d, &id("One"), Some(&id("Three"))).unwrap();
        assert_eq!(titles(&d), vec!["Two", "One", "Three", "Four"]);
        move_before(&d, &id("Four"), Some(&id("Two"))).unwrap();
        assert_eq!(titles(&d), vec!["Four", "Two", "One", "Three"]);
        move_by(&d, &id("Four"), -1).unwrap();
        assert_eq!(titles(&d)[0], "Four");
        move_by(&d, &id("Four"), 1).unwrap();
        assert_eq!(titles(&d), vec!["Two", "Four", "One", "Three"]);
        remove_entry(&d, &id("Four")).unwrap();
        assert_eq!(titles(&d), vec!["Two", "One", "Three"]);
    }

    #[test]
    fn overrides_win_over_the_orgs_use_over_the_song() {
        let set = Info {
            leader: Some("jake".into()),
            ..Info::default()
        };
        let st = Settings {
            arrangement: "Normal".into(),
            ..Settings::default()
        };
        let face = crate::songs::Entry {
            id: "one".into(),
            title: "One".into(),
            key: Some("G".into()),
            tempo: Some(72),
            ..crate::songs::Entry::default()
        };
        let mut e = song("One", "G");
        // Nothing chosen: the song's key, the org's default, the set's leader.
        let p = resolve(&e, Some(&face), &Use::default(), &st, &set);
        assert_eq!(
            (
                p.key.as_deref(),
                p.arrangement.as_str(),
                p.leader.as_deref(),
                p.tempo
            ),
            (Some("G"), "Normal", Some("jake"), Some(72))
        );
        // The org plays it in A, acoustic, led by Mia.
        let u = Use {
            key: Some("A".into()),
            arrangement: Some("Acoustic".into()),
            leader: Some("mia".into()),
            ..Use::default()
        };
        let p = resolve(&e, Some(&face), &u, &st, &set);
        assert_eq!(
            (
                p.key.as_deref(),
                p.arrangement.as_str(),
                p.leader.as_deref()
            ),
            (Some("A"), "Acoustic", Some("mia"))
        );
        // This set moves it to Bb, shortened, led by Sam, faster.
        e.over = Overrides {
            key: Some("Bb".into()),
            arrangement: Some("Shortened".into()),
            leader: Some("sam".into()),
            tempo: Some(80),
        };
        let p = resolve(&e, Some(&face), &u, &st, &set);
        assert_eq!(
            (
                p.key.as_deref(),
                p.arrangement.as_str(),
                p.leader.as_deref(),
                p.tempo
            ),
            (Some("Bb"), "Shortened", Some("sam"), Some(80))
        );
        assert_eq!(p.own_key.as_deref(), Some("G"));
        // A song the org cannot read: the face on the entry.
        e.over = Overrides::default();
        let p = resolve(&e, None, &Use::default(), &st, &set);
        assert_eq!(p.key.as_deref(), Some("G"));
    }

    #[test]
    fn a_guitarist_sees_their_parts_and_the_capo() {
        let band = vec![
            Seat {
                person: "cody-wright".into(),
                name: "Cody Wright".into(),
                parts: vec!["Electric guitar".into()],
            },
            Seat {
                person: "sam".into(),
                name: "Sam".into(),
                parts: vec!["Drums".into()],
            },
        ];
        let played = vec![
            (
                "e1".to_string(),
                Played {
                    key: Some("Ab".into()),
                    leader: Some("cody-wright".into()),
                    ..Played::default()
                },
            ),
            (
                "e2".to_string(),
                Played {
                    key: Some("G".into()),
                    ..Played::default()
                },
            ),
        ];
        let v = my_view("cody-wright", &band, &played).unwrap();
        assert_eq!(v.parts, vec!["Electric guitar"]);
        assert_eq!(v.songs[0].2, Some((1, "G".to_string())));
        assert!(v.songs[0].3);
        assert_eq!(v.songs[1].2, None);
        // The drummer gets no capo; someone not in the band, no view.
        assert_eq!(my_view("sam", &band, &played).unwrap().songs[0].2, None);
        assert!(my_view("mia", &band, &played).is_none());
    }

    #[test]
    fn sections_split_the_running_order() {
        let d = a_set();
        insert_entry(&d, Some(0), &Entry::item(ItemKind::Section, "Top Set")).unwrap();
        insert_entry(&d, Some(3), &Entry::item(ItemKind::Section, "Bottom Set")).unwrap();
        insert_entry(&d, None, &Entry::item(ItemKind::Prayer, "Closing prayer")).unwrap();
        let secs = sections(&entries(&d));
        let names: Vec<_> = secs
            .iter()
            .map(|(s, l)| (s.as_ref().map(|e| e.title.clone()), l.len()))
            .collect();
        assert_eq!(
            names,
            vec![
                (Some("Top Set".to_string()), 2),
                (Some("Bottom Set".to_string()), 3)
            ]
        );
        assert_eq!(songs_of(&d).len(), 4);
        assert_eq!(face_of(&d).songs, 4);
    }

    #[test]
    fn a_set_starts_from_last_weeks_or_a_template() {
        let last = a_set();
        let mut first = entries(&last)[0].clone();
        first.over.key = Some("A".into());
        first.notes = "Start soft".into();
        update_entry(&last, &first).unwrap();
        put_seat(
            &last,
            &Seat {
                person: "sam".into(),
                name: "Sam".into(),
                parts: vec!["Drums".into()],
            },
        )
        .unwrap();
        let sr = Series {
            id: "main".into(),
            name: "Main".into(),
            days: vec![0],
            size: 5,
            ..Series::default()
        };
        let next = sr.next_from(Day::ymd(2026, 11, 30)).unwrap();
        assert_eq!(next, Day::ymd(2026, 12, 6));
        let info = sr.meeting(next);
        assert_eq!(info.id, "main_2026-12-06");
        // Two devices start next week's at once: one copy lands, not two.
        let a = LoroDoc::new();
        let b = LoroDoc::new();
        assert!(start_from(&last, &a, &info).unwrap());
        assert!(start_from(&last, &b, &info).unwrap());
        sync(&a, &b);
        // Both lists land (Loro keeps both inserts), but each copy has the
        // same id on both devices, so the set reads once, the same on both.
        assert_eq!(entries(&a).len(), 4);
        assert_eq!(titles(&a), titles(&b));
        // Taking one out takes both copies.
        let gone = entries(&a)[0].id.clone();
        remove_entry(&a, &gone).unwrap();
        sync(&a, &b);
        assert_eq!(entries(&b).len(), 3);
        // Started on one device: the order, the overrides and notes, the band.
        let c = LoroDoc::new();
        start_from(&last, &c, &info).unwrap();
        let e = &entries(&c)[0];
        assert_eq!(e.over.key.as_deref(), Some("A"));
        assert_eq!(e.notes, "Start soft");
        assert_ne!(e.id, first.id);
        assert_eq!(band(&c)[0].person, "sam");
        assert_eq!(info_of(&c).from, "s1");
        assert_eq!(info_of(&c).size, 5);
        // A set that has songs is left alone.
        assert!(!start_from(&last, &c, &info).unwrap());
        assert_eq!(entries(&c).len(), 4);
        // previous(): the latest set before, with songs.
        let faces = vec![
            Face {
                id: "a".into(),
                series: "main".into(),
                date: "2026-11-22".into(),
                songs: 4,
                ..Face::default()
            },
            Face {
                id: "b".into(),
                series: "main".into(),
                date: "2026-11-29".into(),
                songs: 0,
                ..Face::default()
            },
        ];
        assert_eq!(previous(&faces, "main", next).unwrap().id, "a");
    }

    fn info_of(d: &LoroDoc) -> Info {
        info(d)
    }

    #[test]
    fn a_shared_set_opens_to_the_other_org() {
        let d = a_set();
        // A song from another org: its face reads, its session stays that org's.
        insert_entry(
            &d,
            None,
            &Entry {
                song: Some(SongRef::new("cbu", "silent-night")),
                ..song("Silent Night", "A")
            },
        )
        .unwrap();
        let registry = LoroDoc::new();
        assert!(!set_project_differs(&d, &registry));
        share_set(&d, "days-to-praise", true).unwrap();
        assert!(mirror_set_project(&d, &registry));
        assert!(!mirror_set_project(&d, &registry));
        let p = drive_files::projects::projects(&registry)
            .into_iter()
            .find(|p| p.id == set_project("s1"))
            .unwrap();
        assert_eq!(p.shared_with, vec!["days-to-praise"]);
        assert!(p.docs.contains(&setlist_doc("s1")));
        assert!(p.docs.contains(&crate::songs::song_doc("one")));
        assert!(!p.docs.contains(&crate::songs::song_doc("silent-night")));
        assert_eq!(face_of(&d).shared_with, vec!["days-to-praise"]);
        // The other org finds it through Drive's shared projects.
        let found = shared_sets(
            "days-to-praise",
            &[
                ("crossroads-church".into(), set_project("s1")),
                (
                    "crossroads-church".into(),
                    "studio-shared-days-to-praise".into(),
                ),
            ],
        );
        assert_eq!(found, vec![("crossroads-church".into(), "s1".into())]);
        // Taking it back closes the project.
        share_set(&d, "days-to-praise", false).unwrap();
        assert!(mirror_set_project(&d, &registry));
        let p = drive_files::projects::projects(&registry)
            .into_iter()
            .find(|p| p.id == set_project("s1"))
            .unwrap();
        assert!(p.shared_with.is_empty());
    }

    #[test]
    fn the_built_in_sets_become_documents() {
        // The index: Crossroads' recurring events, and no sample sets.
        let index = LoroDoc::new();
        orgs::stamp_org(&index, "crossroads-church");
        assert_eq!(
            schema::upgrade(&index, INDEX_STEPS),
            schema::Upgraded::From(0)
        );
        assert_eq!(series_list(&index).len(), 5);
        assert!(faces(&index).is_empty());
        // A show's index has its built-in set.
        let rot = LoroDoc::new();
        orgs::stamp_org(&rot, "rockstars-of-tomorrow");
        schema::upgrade(&rot, INDEX_STEPS);
        assert!(face(&rot, "jt").is_some());
        let tmp = LoroDoc::new();
        stamp_set(&tmp, "crossroads-church", "cya_2026-11-19");
        assert!(write_builtin(&tmp, "crossroads-church", "cya_2026-11-19"));
        let cya = face_of(&tmp);
        assert_eq!((cya.songs, cya.size), (4, 4));
        assert!(cya.band.contains(&"cody-wright".to_string()));

        // A set's document, when opened: the same set as its face.
        let d = LoroDoc::new();
        stamp_set(&d, "crossroads-church", "cya_2026-11-19");
        schema::upgrade(&d, SETLIST_STEPS);
        assert_eq!(face_of(&d), cya);
        let secs = sections(&entries(&d));
        assert_eq!(secs[0].0.as_ref().unwrap().title, "Top Set");
        assert_eq!(secs[0].1.len(), 2);
        let s = songs_of(&d);
        assert_eq!(s[0].over.leader.as_deref(), Some("mia"));
        assert_eq!(s[1].over.arrangement.as_deref(), Some("Big Intro"));
        // Its songs point at their home org's library.
        assert!(
            s.iter()
                .all(|e| e.song.as_ref().unwrap().owner == "crossroads-church")
        );
        // Two devices seeding it at once read as one set.
        let d2 = LoroDoc::new();
        stamp_set(&d2, "crossroads-church", "cya_2026-11-19");
        schema::upgrade(&d2, SETLIST_STEPS);
        sync(&d, &d2);
        assert_eq!(face_of(&d), cya);
        // Running it again changes nothing.
        let before = entries(&d);
        assert_eq!(
            schema::upgrade(&d, SETLIST_STEPS),
            schema::Upgraded::Current
        );
        setlist_v1(&d);
        assert_eq!(entries(&d), before);

        // The recital in Rockstars of Tomorrow, in its acts.
        let jt = LoroDoc::new();
        stamp_set(&jt, "rockstars-of-tomorrow", "jt");
        schema::upgrade(&jt, SETLIST_STEPS);
        assert_eq!(info(&jt).kind, Kind::Show);
        assert_eq!(songs_of(&jt).len(), 16);
        assert_eq!(sections(&entries(&jt)).len(), 3);
        // The same set in another org's document stays empty.
        let other = LoroDoc::new();
        stamp_set(&other, "cbu", "jt");
        schema::upgrade(&other, SETLIST_STEPS);
        assert!(entries(&other).is_empty());

        // A rehearsal carried over.
        let jhms = LoroDoc::new();
        stamp_set(&jhms, "crossroads-church", "jhms_2026-11-22");
        schema::upgrade(&jhms, SETLIST_STEPS);
        assert_eq!(rehearsals(&jhms)[0].room, "Room 210");
    }

    #[test]
    fn the_live_keys_carry_over_once() {
        let d = LoroDoc::new();
        stamp_set(&d, "crossroads-church", "main_2026-11-22");
        schema::upgrade(&d, SETLIST_STEPS);
        // The old live document: song 2's key and song 3's arrangement.
        let old = LoroDoc::new();
        let m = old.get_map("songs").ensure_mergeable_map("2").unwrap();
        m.insert("key", "F").unwrap();
        let m = old.get_map("songs").ensure_mergeable_map("3").unwrap();
        m.insert("arr", "Normal").unwrap();
        assert_eq!(import_legacy(&d, &old), 2);
        let s = songs_of(&d);
        assert_eq!(s[2].over.key.as_deref(), Some("F"));
        assert_eq!(s[3].over.arrangement, None);
        // Once only: a later change to the old document does nothing.
        let m = old.get_map("songs").ensure_mergeable_map("0").unwrap();
        m.insert("key", "B").unwrap();
        assert_eq!(import_legacy(&d, &old), 0);
        assert_eq!(songs_of(&d)[0].over.key.as_deref(), Some("C"));
    }

    #[test]
    fn plain_text_and_the_calendar() {
        let d = a_set();
        insert_entry(&d, Some(0), &Entry::item(ItemKind::Section, "Top Set")).unwrap();
        let mut two = songs_of(&d)[1].clone();
        two.over.key = Some("Bb".into());
        two.transition = "Segue into Three".into();
        update_entry(&d, &two).unwrap();
        insert_entry(&d, None, &Entry::item(ItemKind::Reading, "Psalm 23")).unwrap();
        let i = info(&d);
        let text = plain_text(
            &i,
            &entries(&d),
            &|e| resolve(e, None, &Use::default(), &Settings::default(), &i),
            &[Seat {
                person: "sam".into(),
                name: "Sam".into(),
                parts: vec!["Drums".into()],
            }],
            &|p| p.to_uppercase(),
        );
        assert!(text.starts_with("Sunday · Sunday, November 29 2026\n"));
        assert!(text.contains("Band: SAM (Drums)"));
        assert!(text.contains("TOP SET"));
        assert!(text.contains("2. Two · Bb (from A)"));
        assert!(text.contains("→ Segue into Three"));
        assert!(text.contains("[Reading] Psalm 23"));
        let ev = calendar_event(&i, "/studio").unwrap();
        assert_eq!(ev.id, "studio:setlist:s1");
        assert_eq!(ev.link, "/studio/set/s1");
        assert_eq!(ev.date, "2026-11-29");
        let t = Info {
            kind: Kind::Template,
            ..i
        };
        assert!(calendar_event(&t, "/studio").is_none());
    }
}
