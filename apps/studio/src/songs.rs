//! Songs: the library, and each song as a session.
//!
//! A song is a session document (`session_sync::SessionDoc`, the block
//! every view and device agrees on): its tempo map, its sections (regions
//! on the sections lane), markers and chart, the same document a DAW
//! session of the song edits. On the web it is a Drive document
//! (`studio:song:<id>`), live for everyone who has it open; on a device
//! without Drive yet, a document in memory.
//!
//! The library is one more document (`studio:library`): each song's
//! readable face (title, artist, key, tempo, tags, energy), so the list
//! is read without opening every song. It is seeded once from the
//! built-in list.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use session_sync::loro::{LoroDoc, LoroMap, LoroValue, ValueOrContainer};
use session_sync::slug::slugify;
use session_sync::{ORIGIN_LOCAL, RegionState, SessionDoc, SessionModel, TempoPoint};

use crate::data::{self, LIB};

/// The ruler lane a song's sections sit on.
pub const SECTIONS_LANE: u32 = 1;

pub const LIBRARY_DOC: &str = "studio:library";

pub fn song_doc(id: &str) -> String {
    format!("studio:song:{id}")
}

/// A song as the library lists it.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Entry {
    /// Its library id (`slugify` of its title, the library's convention).
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub key: Option<String>,
    pub tempo: Option<u32>,
    pub tags: Vec<String>,
    pub energy: u8,
    pub ready: String,
    /// The other orgs it is shared with (slugs), when its org owns it.
    pub shared_with: Vec<String>,
}

impl Entry {
    pub fn from_lib(x: &data::LibSong) -> Entry {
        Entry {
            id: slugify(x.song.title),
            title: x.song.title.to_string(),
            artist: x.song.artist.map(str::to_string),
            key: x.song.key.map(str::to_string),
            tempo: x.song.tempo,
            tags: x.tags.iter().map(|t| t.to_string()).collect(),
            energy: x.energy,
            ready: x.song.ready.to_string(),
            shared_with: Vec::new(),
        }
    }

    /// The built-in song of the same title, to play it.
    pub fn builtin(&self) -> Option<data::Song> {
        LIB.iter()
            .find(|x| x.song.title == self.title)
            .map(|x| x.song)
    }

    pub fn admitted(&self, f: &crate::state::Filters) -> bool {
        let tags: Vec<&str> = self.tags.iter().map(String::as_str).collect();
        f.admits(&crate::state::Facts {
            title: &self.title,
            artist: self.artist.as_deref(),
            key: self.key.as_deref().map(|k| data::norm_key(k).unwrap_or(k)),
            tempo: self.tempo,
            tags: &tags,
            energy: self.energy,
            ready: &self.ready,
        })
    }
}

// ── the library document ────────────────────────────────────────────────

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

/// Every song in a library document, by title.
pub fn entries(doc: &LoroDoc) -> Vec<Entry> {
    let songs = doc.get_map("songs");
    let mut out: Vec<Entry> = songs
        .keys()
        .filter_map(|k| match songs.get(&k) {
            Some(ValueOrContainer::Container(c)) => c.into_map().ok(),
            _ => None,
        })
        .map(|m| Entry {
            id: s(&m, "id"),
            title: s(&m, "title"),
            artist: opt(s(&m, "artist")),
            key: opt(s(&m, "key")),
            tempo: u32::try_from(n(&m, "tempo")).ok().filter(|t| *t > 0),
            tags: s(&m, "tags")
                .split(',')
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect(),
            energy: u8::try_from(n(&m, "energy")).unwrap_or(0),
            ready: s(&m, "ready"),
            shared_with: shared_of(&m),
        })
        .filter(|e| !e.id.is_empty())
        .collect();
    out.sort_by_key(|e| e.title.to_lowercase());
    out
}

/// The orgs a song's face says it is shared with (a map, so two people
/// sharing it with two orgs at once both land).
fn shared_of(m: &LoroMap) -> Vec<String> {
    let mut out: Vec<String> = match m.get("shared_with") {
        Some(ValueOrContainer::Container(c)) => c
            .into_map()
            .map(|w| w.keys().map(|k| k.to_string()).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    out.sort();
    out
}

/// Share a song's face with an org, or stop.
pub fn set_shared(
    doc: &LoroDoc,
    id: &str,
    org: &str,
    on: bool,
) -> session_sync::loro::LoroResult<()> {
    let m = doc.get_map("songs").ensure_mergeable_map(id)?;
    let with = m.ensure_mergeable_map("shared_with")?;
    if on {
        with.insert(org, true)?;
    } else if with.get(org).is_some() {
        with.delete(org)?;
    }
    Ok(())
}

/// Add or update a song's face.
pub fn put_entry(doc: &LoroDoc, e: &Entry) -> session_sync::loro::LoroResult<()> {
    let m = doc.get_map("songs").ensure_mergeable_map(&e.id)?;
    m.insert("id", e.id.as_str())?;
    m.insert("title", e.title.as_str())?;
    m.insert("artist", e.artist.as_deref().unwrap_or(""))?;
    m.insert("key", e.key.as_deref().unwrap_or(""))?;
    m.insert("tempo", i64::from(e.tempo.unwrap_or(0)))?;
    m.insert("tags", e.tags.join(","))?;
    m.insert("energy", i64::from(e.energy))?;
    m.insert("ready", e.ready.as_str())?;
    let had = shared_of(&m);
    for o in had.iter().filter(|o| !e.shared_with.contains(o)) {
        set_shared(doc, &e.id, o, false)?;
    }
    for o in e.shared_with.iter().filter(|o| !had.contains(o)) {
        set_shared(doc, &e.id, o, true)?;
    }
    Ok(())
}

/// The built-in songs, into an empty library (each by its id, so two
/// devices seeding at once agree).
pub fn seed_library(doc: &LoroDoc) {
    if doc.get_map("songs").is_empty() {
        for x in LIB.iter() {
            let _ = put_entry(doc, &Entry::from_lib(x));
        }
    }
}

// ── a song's session ────────────────────────────────────────────────────

/// One section of a song, as the form shows it.
#[derive(Clone, PartialEq, Debug)]
pub struct Section {
    /// Its region's guid.
    pub id: String,
    pub name: String,
    pub bars: u32,
    pub start_bar: u32,
}

/// A fresh id for a region.
pub fn new_id() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let t = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    format!("{t:x}{:x}", N.fetch_add(1, Ordering::Relaxed))
}

/// Seconds in one bar at the song's first tempo.
pub fn bar_secs(m: &SessionModel) -> f64 {
    let t = m
        .tempo
        .first()
        .map_or((120.0, 4), |t| (t.bpm, t.beats_per_bar.max(1)));
    60.0 / t.0.max(1.0) * f64::from(t.1)
}

pub fn bpm(m: &SessionModel) -> f64 {
    m.tempo.first().map_or(120.0, |t| t.bpm)
}

pub fn meter(m: &SessionModel) -> (u32, u32) {
    m.tempo
        .first()
        .map_or((4, 4), |t| (t.beats_per_bar, t.beat_unit))
}

/// The song's sections, in order.
pub fn sections(m: &SessionModel) -> Vec<Section> {
    let bar = bar_secs(m);
    let mut list: Vec<(&String, &RegionState)> = m
        .regions
        .iter()
        .filter(|(_, r)| r.lane == Some(SECTIONS_LANE))
        .collect();
    list.sort_by(|a, b| a.1.start.total_cmp(&b.1.start));
    list.into_iter()
        .map(|(id, r)| Section {
            id: id.clone(),
            name: r.name.clone(),
            bars: ((r.end - r.start) / bar).round().max(1.0) as u32,
            start_bar: (r.start / bar).round().max(0.0) as u32,
        })
        .collect()
}

/// Lay sections end to end from the top, at the song's tempo.
pub fn lay_out(m: &mut SessionModel, list: &[(String, String, u32)]) {
    let bar = bar_secs(m);
    m.regions.retain(|_, r| r.lane != Some(SECTIONS_LANE));
    let mut at = 0u32;
    for (id, name, bars) in list {
        m.regions.insert(
            id.clone(),
            RegionState {
                start: f64::from(at) * bar,
                end: f64::from(at + bars) * bar,
                name: name.clone(),
                color: None,
                lane: Some(SECTIONS_LANE),
            },
        );
        at += bars;
    }
}

/// Change the tempo or meter, keeping every section's length in bars.
pub fn set_tempo(m: &mut SessionModel, bpm: f64, beats_per_bar: u32, beat_unit: u32) {
    let list: Vec<(String, String, u32)> = sections(m)
        .into_iter()
        .map(|s| (s.id, s.name, s.bars))
        .collect();
    m.tempo = vec![TempoPoint {
        at: 0.0,
        bpm,
        beats_per_bar,
        beat_unit,
    }];
    lay_out(m, &list);
}

/// A section's colour by its kind (every Verse alike).
pub fn section_hue(name: &str) -> u32 {
    let kind: String = name
        .chars()
        .take_while(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase();
    match kind.as_str() {
        "intro" => 200,
        "verse" => 150,
        "prechorus" | "pre" => 95,
        "chorus" => 340,
        "bridge" => 40,
        "tag" | "outro" | "ending" => 260,
        "instrumental" | "interlude" | "solo" => 180,
        _ => data::str_hue(&kind),
    }
}

/// A song's first session: its tempo, a common form and a chart to start from.
pub fn seed_song(e: &Entry) -> SessionModel {
    let mut m = SessionModel::default();
    m.tempo = vec![TempoPoint {
        at: 0.0,
        bpm: f64::from(e.tempo.unwrap_or(120)),
        beats_per_bar: 4,
        beat_unit: 4,
    }];
    let form = [
        ("Intro", 4),
        ("Verse 1", 8),
        ("Chorus", 8),
        ("Verse 2", 8),
        ("Chorus", 8),
        ("Bridge", 8),
        ("Chorus", 8),
        ("Outro", 4),
    ];
    let list: Vec<(String, String, u32)> = form
        .iter()
        .map(|(name, bars)| (new_id(), name.to_string(), *bars))
        .collect();
    lay_out(&mut m, &list);
    m.chart = chart_skeleton(e, &sections(&m));
    m
}

fn chart_skeleton(e: &Entry, list: &[Section]) -> String {
    let mut c = format!("title: {}\n", e.title);
    if let Some(a) = &e.artist {
        c.push_str(&format!("artist: {a}\n"));
    }
    if let Some(k) = &e.key {
        c.push_str(&format!("key: {k}\n"));
    }
    if let Some(t) = e.tempo {
        c.push_str(&format!("tempo: {t}\n"));
    }
    let mut seen = std::collections::HashSet::new();
    for s in list {
        if seen.insert(s.name.clone()) {
            c.push_str(&format!("\n{} ({} bars)\n", s.name, s.bars));
        }
    }
    c
}

/// A section's short label for the form bar: `V1`, `C`, `PC`, `B`.
pub fn short(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !w.is_empty())
        .collect();
    let num: String = words
        .iter()
        .filter(|w| w.chars().all(|c| c.is_ascii_digit()))
        .copied()
        .collect();
    let letters: String = words
        .iter()
        .filter(|w| !w.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|w| w.chars().next())
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_uppercase)
        .collect();
    match letters.as_str() {
        "I" if name.to_lowercase().starts_with("inst") => format!("Inst{num}"),
        "" => name.chars().take(3).collect(),
        _ => format!("{letters}{num}"),
    }
}

/// mm:ss
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

// ── hooks: a document on Drive (web), or in memory ──────────────────────

/// Someone else with a song open.
#[derive(Clone, PartialEq, Debug)]
pub struct Here {
    pub name: String,
    pub hue: u32,
    /// The section they are on.
    pub section: Option<String>,
}

#[cfg(feature = "web")]
pub(crate) mod backing {
    use super::*;
    use drive_dioxus::{Doc, use_doc};

    #[derive(Clone, PartialEq)]
    pub struct Backing(pub Doc);

    pub fn use_backing(id: String) -> Backing {
        Backing(use_doc(id))
    }

    impl Backing {
        pub fn loro(&self) -> LoroDoc {
            self.0.loro()
        }
        pub fn peek(&self) -> LoroDoc {
            self.0.peek()
        }
        pub fn edit<R>(&self, f: impl FnOnce(&LoroDoc) -> R) -> R {
            self.0.edit(f)
        }
        pub fn ready(&self) -> bool {
            self.0.synced()
        }
        pub fn others(&self) -> Vec<Here> {
            self.0
                .others()
                .into_iter()
                .map(|p| Here {
                    name: p.name,
                    hue: p.hue,
                    section: p
                        .data
                        .get("section")
                        .and_then(|s| s.as_str())
                        .map(str::to_string),
                })
                .collect()
        }
        pub fn say_section(&self, section: Option<&str>) {
            self.0
                .set_presence(&serde_json::json!({ "section": section }));
        }
    }

    /// Write a new document now, outside any component (a set just made):
    /// opened on Drive, edited, committed and sent.
    pub fn create(id: &str, f: impl FnOnce(&LoroDoc)) -> LoroDoc {
        let Some(ctx) = try_consume_context::<drive_dioxus::DriveCtx>() else {
            let d = LoroDoc::new();
            f(&d);
            return d;
        };
        let id = ctx.qualify(id);
        let doc = ctx.drive.open(&id);
        f(&doc);
        ctx.drive.commit(&id);
        doc
    }
}

#[cfg(not(feature = "web"))]
pub(crate) mod backing {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        /// The documents of this run, by id (no Drive on this device yet).
        static DOCS: RefCell<HashMap<String, LoroDoc>> = RefCell::new(HashMap::new());
    }

    #[derive(Clone, PartialEq)]
    pub struct Backing {
        doc: LoroDocCell,
        rev: Signal<u64>,
    }

    #[derive(Clone)]
    struct LoroDocCell(LoroDoc);

    impl PartialEq for LoroDocCell {
        fn eq(&self, _: &Self) -> bool {
            true
        }
    }

    pub fn use_backing(id: String) -> Backing {
        let rev = use_signal(|| 0u64);
        let doc = use_hook(|| DOCS.with(|d| d.borrow_mut().entry(id).or_default().clone()));
        Backing {
            doc: LoroDocCell(doc),
            rev,
        }
    }

    impl Backing {
        pub fn loro(&self) -> LoroDoc {
            let _ = (self.rev)();
            self.doc.0.clone()
        }
        pub fn peek(&self) -> LoroDoc {
            self.doc.0.clone()
        }
        pub fn edit<R>(&self, f: impl FnOnce(&LoroDoc) -> R) -> R {
            let r = f(&self.doc.0);
            self.doc.0.commit();
            let mut rev = self.rev;
            rev += 1;
            r
        }
        pub fn ready(&self) -> bool {
            true
        }
        pub fn others(&self) -> Vec<Here> {
            Vec::new()
        }
        pub fn say_section(&self, _section: Option<&str>) {}
    }

    /// Write a new document now, outside any component (a set just made).
    pub fn create(id: &str, f: impl FnOnce(&LoroDoc)) -> LoroDoc {
        DOCS.with(|d| {
            let doc = d.borrow_mut().entry(id.to_string()).or_default().clone();
            f(&doc);
            doc.commit();
            doc
        })
    }

    /// A document of this run, if anything opened it.
    pub fn peek(id: &str) -> Option<LoroDoc> {
        DOCS.with(|d| d.borrow().get(id).cloned())
    }
}

/// A song document's shape versions: version 1 is a session-sync session
/// (the seed writes it; the step only stamps the version).
const SONG_STEPS: &[drive_spine::schema::Step] = &[|_| {}];

/// Bring a document up to its steps once its node has answered (an empty
/// document is then really empty), stamped with its org first: the steps
/// read it.
pub(crate) fn use_upgrade(
    b: &backing::Backing,
    org: &str,
    steps: &'static [drive_spine::schema::Step],
) {
    let ready = b.ready();
    let b = b.clone();
    let org = org.to_string();
    use_effect(use_reactive!(|ready| {
        if ready && drive_spine::schema::version(&b.peek()) < steps.len() as i64 {
            b.edit(|d| {
                crate::orgs::stamp_org(d, &org);
                drive_spine::schema::upgrade(d, steps)
            });
        }
    }));
}

/// An org's library, live.
#[derive(Clone, PartialEq)]
pub struct Library {
    b: backing::Backing,
    /// Whose library it is.
    pub org: String,
}

pub fn use_library_of(org: &str) -> Library {
    let b = backing::use_backing(crate::orgs::qualified(org, LIBRARY_DOC));
    use_upgrade(&b, org, crate::orgs::LIBRARY_STEPS);
    Library {
        b,
        org: org.to_string(),
    }
}

/// The library of the org Studio is in.
pub fn use_library() -> Library {
    let org = (crate::state::use_app().here)();
    use_library_of(&org)
}

impl Library {
    pub fn entries(&self) -> Vec<Entry> {
        entries(&self.b.loro())
    }

    pub fn ready(&self) -> bool {
        self.b.ready()
    }

    /// The document itself, as it is now (no re-render on change).
    pub fn peek(&self) -> LoroDoc {
        self.b.peek()
    }

    /// The document, re-read when it changes.
    pub fn loro(&self) -> LoroDoc {
        self.b.loro()
    }

    /// How this org plays a song (its own or a shared one).
    pub fn use_of(&self, owner: &str, id: &str) -> crate::orgs::Use {
        crate::orgs::use_of(&self.b.loro(), owner, id)
    }

    pub fn put_use(&self, owner: &str, id: &str, u: &crate::orgs::Use) {
        self.b.edit(|d| {
            let _ = crate::orgs::put_use(d, owner, id, u);
        });
    }

    /// Share one of this org's songs with another org, or stop.
    pub fn share(&self, id: &str, to: &str, on: bool) {
        self.b.edit(|d| {
            let _ = crate::orgs::share_song(d, id, to, on);
        });
    }

    pub fn get(&self, id: &str) -> Option<Entry> {
        self.entries().into_iter().find(|e| e.id == id)
    }

    pub fn put(&self, e: &Entry) {
        self.b.edit(|d| {
            let _ = put_entry(d, e);
        });
    }
}

/// A song's session, live.
#[derive(Clone, PartialEq)]
pub struct Song(backing::Backing);

/// A song's session, in the org that owns it.
pub fn use_song(owner: &str, id: &str, entry: Option<Entry>) -> Song {
    let song = Song(backing::use_backing(crate::orgs::qualified(
        owner,
        &song_doc(id),
    )));
    let ready = song.0.ready();
    let seed = song.clone();
    use_effect(use_reactive!(|ready, entry| {
        if let (true, Some(e)) = (ready, entry) {
            let sd = SessionDoc::from_loro(seed.0.peek());
            if !sd.has_session() {
                seed.0.edit(|d| {
                    let _ = SessionDoc::from_loro(d.clone()).write(&seed_song(&e), ORIGIN_LOCAL);
                });
            }
            if drive_spine::schema::version(&seed.0.peek()) < SONG_STEPS.len() as i64 {
                seed.0.edit(|d| drive_spine::schema::upgrade(d, SONG_STEPS));
            }
        }
    }));
    song
}

impl Song {
    /// The session as it is now (re-read when it changes).
    pub fn model(&self) -> Option<SessionModel> {
        let sd = SessionDoc::from_loro(self.0.loro());
        sd.has_session().then(|| sd.read())
    }

    /// Change the session: read it, change the model, write it back (only
    /// what differs becomes ops, merged field by field with everyone else's).
    pub fn change(&self, f: impl FnOnce(&mut SessionModel)) {
        self.0.edit(|d| {
            let sd = SessionDoc::from_loro(d.clone());
            let before = sd.read();
            let mut m = before.clone();
            f(&mut m);
            if m != before {
                let _ = sd.write(&m, ORIGIN_LOCAL);
            }
        });
    }

    pub fn others(&self) -> Vec<Here> {
        self.0.others()
    }

    pub fn say_section(&self, section: Option<&str>) {
        self.0.say_section(section);
    }

    /// How many peers' worth of edits the document holds (its version).
    pub fn version(&self) -> BTreeMap<String, i32> {
        self.0
            .loro()
            .oplog_vv()
            .iter()
            .map(|(peer, counter)| (format!("{peer:x}").chars().take(4).collect(), *counter))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        Entry {
            id: "build-my-life".into(),
            title: "Build My Life".into(),
            artist: Some("Housefires".into()),
            key: Some("G".into()),
            tempo: Some(68),
            tags: vec!["worship".into()],
            energy: 2,
            ready: String::new(),
            shared_with: Vec::new(),
        }
    }

    fn form(m: &SessionModel) -> Vec<(String, u32, u32)> {
        sections(m)
            .into_iter()
            .map(|s| (s.name, s.start_bar, s.bars))
            .collect()
    }

    #[test]
    fn a_song_round_trips_through_its_session() {
        let m = seed_song(&entry());
        let doc = SessionDoc::new();
        doc.write(&m, ORIGIN_LOCAL).unwrap();
        let back = doc.read();
        assert_eq!(form(&back), form(&m));
        assert_eq!(form(&back)[1], ("Verse 1".into(), 4, 8));
        assert_eq!(bpm(&back), 68.0);
        assert!(back.chart.contains("Build My Life"));
    }

    #[test]
    fn tempo_and_meter_keep_bars() {
        let mut m = seed_song(&entry());
        let before = form(&m);
        set_tempo(&mut m, 136.0, 6, 8);
        assert_eq!(form(&m), before);
        assert_eq!(meter(&m), (6, 8));
        assert!((bar_secs(&m) - 60.0 / 136.0 * 6.0).abs() < 1e-9);
    }

    #[test]
    fn two_people_edit_one_song() {
        let a = SessionDoc::new();
        a.write(&seed_song(&entry()), ORIGIN_LOCAL).unwrap();
        let b = a.fork();
        // A renames the bridge while B makes the outro longer.
        let mut ma = a.read();
        let bridge = sections(&ma)
            .into_iter()
            .find(|s| s.name == "Bridge")
            .unwrap()
            .id;
        ma.regions.get_mut(&bridge).unwrap().name = "Bridge (down)".into();
        a.write(&ma, ORIGIN_LOCAL).unwrap();
        let mut mb = b.read();
        let list: Vec<(String, String, u32)> = sections(&mb)
            .into_iter()
            .map(|s| {
                (
                    s.id,
                    s.name.clone(),
                    if s.name == "Outro" { 8 } else { s.bars },
                )
            })
            .collect();
        lay_out(&mut mb, &list);
        b.write(&mb, ORIGIN_LOCAL).unwrap();
        // Both land, wherever they meet.
        a.import(&b.updates_since(&a.version()).unwrap(), "b")
            .unwrap();
        b.import(&a.updates_since(&b.version()).unwrap(), "a")
            .unwrap();
        for doc in [&a, &b] {
            let f = form(&doc.read());
            assert!(f.iter().any(|s| s.0 == "Bridge (down)"));
            assert_eq!(f.last().unwrap().2, 8);
        }
        assert_eq!(form(&a.read()), form(&b.read()));
    }

    #[test]
    fn short_labels() {
        assert_eq!(short("Verse 2"), "V2");
        assert_eq!(short("Pre-Chorus"), "PC");
        assert_eq!(short("Chorus"), "C");
        assert_eq!(short("Instrumental"), "Inst");
        assert_eq!(short("Bridge (down)"), "B");
    }
}
