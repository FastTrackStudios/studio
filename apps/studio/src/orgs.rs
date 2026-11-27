//! Organizations: who Studio works for, the people in each, and the songs
//! they share.
//!
//! The orgs are the auth server's (Drive's `/orgs`): Studio keeps no list
//! of its own, only how to show the ones it knows (a name and a colour).
//! Everything an org keeps is a Drive document in that org:
//!
//! - `<org>/studio:org`: the org's music team. `settings` (the name Studio
//!   shows, the default arrangement, where it rehearses), `groups` (a band,
//!   a worship team, a studio's students) and `people`: each person's
//!   name, role, the parts they play, the groups they are in and the ones
//!   they lead, and the issuer account that is them, when there is one.
//! - `<org>/studio:library`: the songs the org owns (`songs`, each with
//!   `shared_with`), and `uses`: how this org plays a song, its own or one
//!   shared with it (`<owner>/<id>` → its key, its arrangements, who
//!   leads it).
//! - `<owner>/studio:shared:<to>`: the faces of the songs `owner` shares
//!   with `to`, so `to` lists them without reading the owner's whole
//!   library.
//! - in `<owner>/drive:space` (Drive's registry), one project per org
//!   shared with (`studio-shared-<to>`): the digest and the songs'
//!   sessions as its documents, `shared_with` that org. That project is
//!   what Drive's node checks: a member of `to` may open those documents
//!   and nothing else of the owner's.
//!
//! The library's `shared_with` is the truth; the digest and the project
//! are mirrored from it ([`mirror_digest`], [`mirror_project`]) by
//! whoever has the owner's library open.

use loro::{LoroDoc, LoroMap, LoroValue, ValueOrContainer};

use crate::songs::{self, Entry};

/// The org's music team document.
pub const ORG_DOC: &str = "studio:org";

/// The digest of what `owner` shares with `to` (in the owner's org).
pub fn digest_doc(to: &str) -> String {
    format!("studio:shared:{to}")
}

/// The Drive project, in the owner's registry, that shares with `to`.
pub fn share_project(to: &str) -> String {
    format!("studio-shared-{to}")
}

/// An id in another org: `org/id`.
pub fn qualified(org: &str, id: &str) -> String {
    if org.is_empty() {
        id.to_string()
    } else {
        format!("{org}/{id}")
    }
}

// ── the orgs Studio knows how to show ───────────────────────────────────

/// How Studio shows an org of the issuer's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Known {
    pub slug: &'static str,
    pub name: &'static str,
    pub hue: u32,
    /// What kind of place it is, which seeds its groups.
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Personal,
    Church,
    School,
    Lessons,
    Business,
}

pub const KNOWN: [Known; 8] = [
    Known {
        slug: "codywright",
        name: "Cody Wright",
        hue: 210,
        kind: Kind::Personal,
    },
    Known {
        slug: "crossroads-church",
        name: "Crossroads Church",
        hue: 45,
        kind: Kind::Church,
    },
    Known {
        slug: "days-to-praise",
        name: "Days to Praise",
        hue: 320,
        kind: Kind::Church,
    },
    Known {
        slug: "rockstars-of-tomorrow",
        name: "Rockstars of Tomorrow",
        hue: 110,
        kind: Kind::Lessons,
    },
    Known {
        slug: "cbu",
        name: "CBU",
        hue: 55,
        kind: Kind::School,
    },
    Known {
        slug: "fasttrackstudio",
        name: "FastTrackStudio",
        hue: 175,
        kind: Kind::Business,
    },
    Known {
        slug: "fasttrackaudio",
        name: "FastTrack Audio",
        hue: 265,
        kind: Kind::Business,
    },
    Known {
        slug: "tombrooksmusic",
        name: "Tom Brooks Music",
        hue: 25,
        kind: Kind::Business,
    },
];

pub fn known(slug: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.slug == slug)
}

thread_local! {
    /// What this person's organizations are called: the issuer's names,
    /// read once Studio is up, and those made or joined here this run
    /// (every page that names an org reads them from here).
    static NAMED: std::cell::RefCell<std::collections::HashMap<String, String>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Remember what an organization is called.
#[cfg_attr(not(feature = "drive"), allow(dead_code))]
pub fn remember_name(slug: &str, name: &str) {
    NAMED.with(|n| n.borrow_mut().insert(slug.to_string(), name.to_string()));
}

/// An org's name: the issuer's (or the one it was given when it was made
/// here), else Studio's for the ones it knows, else its slug in words.
pub fn org_name(slug: &str) -> String {
    if let Some(name) = NAMED.with(|n| n.borrow().get(slug).cloned()) {
        return name;
    }
    match known(slug) {
        Some(k) => k.name.to_string(),
        None => slug
            .split('-')
            .map(crate::look::cap)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

pub fn org_hue(slug: &str) -> u32 {
    known(slug).map_or_else(|| crate::look::str_hue(slug), |k| k.hue)
}

#[cfg_attr(not(feature = "drive"), allow(dead_code))]
/// The orgs in Studio's order (the known ones first, as listed).
pub fn ordered(mut orgs: Vec<String>) -> Vec<String> {
    orgs.sort_by_key(|o| {
        (
            KNOWN.iter().position(|k| k.slug == o).unwrap_or(usize::MAX),
            o.clone(),
        )
    });
    orgs.dedup();
    orgs
}

// ── reading and writing maps ────────────────────────────────────────────

fn s(m: &LoroMap, k: &str) -> String {
    match m.get(k) {
        Some(ValueOrContainer::Value(LoroValue::String(v))) => v.to_string(),
        _ => String::new(),
    }
}

fn list(m: &LoroMap, k: &str) -> Vec<String> {
    s(m, k)
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect()
}

fn opt(v: String) -> Option<String> {
    (!v.is_empty()).then_some(v)
}

fn child(m: &LoroMap, k: &str) -> Option<LoroMap> {
    match m.get(k) {
        Some(ValueOrContainer::Container(c)) => c.into_map().ok(),
        _ => None,
    }
}

fn children(m: &LoroMap) -> Vec<LoroMap> {
    m.keys().filter_map(|k| child(m, &k)).collect()
}

/// The org a document belongs to, as stamped in its `meta` (what an
/// upgrade step reads: a step sees only the document).
pub fn stamped_org(doc: &LoroDoc) -> String {
    s(&doc.get_map("meta"), "org")
}

pub fn stamp_org(doc: &LoroDoc, org: &str) {
    if stamped_org(doc) != org {
        let _ = doc.get_map("meta").insert("org", org);
    }
}

// ── people ──────────────────────────────────────────────────────────────

/// What someone is in an org.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Role {
    /// Leads: a music director, a worship leader, a bandleader.
    Leader,
    #[default]
    Member,
    Teacher,
    Student,
    /// Sound, lights, recording.
    Tech,
    Guest,
}

impl Role {
    pub const ALL: [Role; 6] = [
        Role::Leader,
        Role::Member,
        Role::Teacher,
        Role::Student,
        Role::Tech,
        Role::Guest,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Leader => "leader",
            Role::Member => "member",
            Role::Teacher => "teacher",
            Role::Student => "student",
            Role::Tech => "tech",
            Role::Guest => "guest",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Leader => "Leader",
            Role::Member => "Member",
            Role::Teacher => "Teacher",
            Role::Student => "Student",
            Role::Tech => "Tech",
            Role::Guest => "Guest",
        }
    }

    pub fn parse(s: &str) -> Role {
        Role::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .unwrap_or_default()
    }
}

/// The parts people play, in a band's order.
pub const PARTS: [&str; 14] = [
    "Lead vocals",
    "Vocals",
    "Acoustic guitar",
    "Electric guitar",
    "Bass",
    "Keys",
    "Drums",
    "Percussion",
    "Strings",
    "Horns",
    "Tracks",
    "Sound",
    "Lights",
    "Video",
];

/// A group in an org: a band, a worship team, a teacher's students.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Group {
    pub id: String,
    pub name: String,
    /// `band`, `worship team`, `students`, `staff`.
    pub kind: String,
}

/// Someone in an org's music work.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Person {
    pub id: String,
    pub name: String,
    /// Their account at the issuer (its email), when they have one: how
    /// a signed-in person finds themselves.
    pub account: Option<String>,
    pub hue: u32,
    pub role: Role,
    /// What they play (one of [`PARTS`], or anything).
    pub parts: Vec<String>,
    /// The groups they are in (ids).
    pub groups: Vec<String>,
    /// The groups they lead (ids).
    pub leads: Vec<String>,
}

impl Person {
    pub fn initials(&self) -> String {
        self.name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect()
    }

    /// Whether they lead anything (a group, or the org as its leader).
    pub fn leading(&self) -> bool {
        self.role == Role::Leader || !self.leads.is_empty()
    }
}

/// An org's settings for its music work.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Settings {
    /// The name Studio shows (empty: the org's own).
    pub name: String,
    /// The arrangement a song is played in when nobody chose one.
    pub arrangement: String,
    /// Where it rehearses.
    pub room: String,
}

pub fn people(doc: &LoroDoc) -> Vec<Person> {
    let mut out: Vec<Person> = children(&doc.get_map("people"))
        .iter()
        .map(|m| Person {
            id: s(m, "id"),
            name: s(m, "name"),
            account: opt(s(m, "account")),
            hue: match m.get("hue") {
                Some(ValueOrContainer::Value(LoroValue::I64(v))) => v as u32,
                _ => 0,
            },
            role: Role::parse(&s(m, "role")),
            parts: list(m, "parts"),
            groups: list(m, "groups"),
            leads: list(m, "leads"),
        })
        .filter(|p| !p.id.is_empty())
        .collect();
    // Leaders first, then by name.
    out.sort_by_key(|p| (!p.leading(), p.name.to_lowercase()));
    out
}

pub fn person(doc: &LoroDoc, id: &str) -> Option<Person> {
    people(doc).into_iter().find(|p| p.id == id)
}

pub fn put_person(doc: &LoroDoc, p: &Person) -> loro::LoroResult<()> {
    let m = doc.get_map("people").ensure_mergeable_map(&p.id)?;
    m.insert("id", p.id.as_str())?;
    m.insert("name", p.name.as_str())?;
    m.insert("account", p.account.as_deref().unwrap_or(""))?;
    m.insert("hue", i64::from(p.hue))?;
    m.insert("role", p.role.as_str())?;
    m.insert("parts", p.parts.join(","))?;
    m.insert("groups", p.groups.join(","))?;
    m.insert("leads", p.leads.join(","))?;
    Ok(())
}

pub fn remove_person(doc: &LoroDoc, id: &str) -> loro::LoroResult<()> {
    let people = doc.get_map("people");
    if people.get(id).is_some() {
        people.delete(id)?;
    }
    Ok(())
}

/// The person a signed-in account is, in this org.
pub fn person_for(doc: &LoroDoc, account: &str) -> Option<Person> {
    if account.is_empty() {
        return None;
    }
    people(doc)
        .into_iter()
        .find(|p| p.account.as_deref() == Some(account))
}

/// Make `id` the account's person here (and no one else).
pub fn link_account(doc: &LoroDoc, id: &str, account: &str) -> loro::LoroResult<()> {
    for mut p in people(doc) {
        let mine = p.id == id;
        let was = p.account.as_deref() == Some(account);
        if mine && !was {
            p.account = Some(account.to_string());
            put_person(doc, &p)?;
        } else if !mine && was {
            p.account = None;
            put_person(doc, &p)?;
        }
    }
    Ok(())
}

pub fn groups(doc: &LoroDoc) -> Vec<Group> {
    let mut out: Vec<Group> = children(&doc.get_map("groups"))
        .iter()
        .map(|m| Group {
            id: s(m, "id"),
            name: s(m, "name"),
            kind: s(m, "kind"),
        })
        .filter(|g| !g.id.is_empty())
        .collect();
    out.sort_by_key(|g| g.name.to_lowercase());
    out
}

pub fn put_group(doc: &LoroDoc, g: &Group) -> loro::LoroResult<()> {
    let m = doc.get_map("groups").ensure_mergeable_map(&g.id)?;
    m.insert("id", g.id.as_str())?;
    m.insert("name", g.name.as_str())?;
    m.insert("kind", g.kind.as_str())?;
    Ok(())
}

/// Who leads a group.
pub fn leaders(doc: &LoroDoc, group: &str) -> Vec<Person> {
    people(doc)
        .into_iter()
        .filter(|p| p.leads.iter().any(|g| g == group))
        .collect()
}

/// Who plays a part (in a group, when one is given).
pub fn playing(doc: &LoroDoc, part: &str, group: Option<&str>) -> Vec<Person> {
    people(doc)
        .into_iter()
        .filter(|p| p.parts.iter().any(|x| x == part))
        .filter(|p| group.is_none_or(|g| p.groups.iter().any(|x| x == g)))
        .collect()
}

pub fn settings(doc: &LoroDoc) -> Settings {
    let m = doc.get_map("settings");
    Settings {
        name: s(&m, "name"),
        arrangement: s(&m, "arrangement"),
        room: s(&m, "room"),
    }
}

pub fn put_settings(doc: &LoroDoc, st: &Settings) -> loro::LoroResult<()> {
    let m = doc.get_map("settings");
    m.insert("name", st.name.as_str())?;
    m.insert("arrangement", st.arrangement.as_str())?;
    m.insert("room", st.room.as_str())?;
    Ok(())
}

/// A person's id from their name (two people of one name get a suffix).
pub fn person_id(doc: &LoroDoc, name: &str) -> String {
    let base = session_sync::slug::slugify(name);
    let base = if base.is_empty() {
        "person".into()
    } else {
        base
    };
    let taken = |id: &str| doc.get_map("people").get(id).is_some();
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken(id))
        .expect("a free id")
}

/// The people, groups and rooms the prototype made up for an org's team
/// (version 1 seeded them, none with an account).
const SEEDED_PEOPLE: [&str; 9] = [
    "cody-wright",
    "jake",
    "mia",
    "sam",
    "riley",
    "jordan",
    "ava",
    "liam",
    "dr-hale",
];
const SEEDED_GROUPS: [&str; 4] = ["worship", "youth", "recital", "christmas"];
const SEEDED_ROOMS: [&str; 3] = ["Worship Center", "Studio A", "Recital Hall"];

/// The org document's version 1 seeded the prototype's made-up team; it
/// seeds nothing now: an org's people are the ones added to it.
fn seed_team(_: &LoroDoc) {}

/// Version 2: the made-up team version 1 seeded taken out again (each
/// seeded person not linked to an account, a seeded group nobody left is
/// in, a seeded room). Everyone and everything added since stays.
fn team_v2(doc: &LoroDoc) {
    for p in people(doc) {
        if SEEDED_PEOPLE.contains(&p.id.as_str()) && p.account.is_none() {
            let _ = remove_person(doc, &p.id);
        }
    }
    let left = people(doc);
    let groups = doc.get_map("groups");
    for g in groups_of(doc) {
        let used = left
            .iter()
            .any(|p| p.groups.contains(&g) || p.leads.contains(&g));
        if SEEDED_GROUPS.contains(&g.as_str()) && !used {
            let _ = groups.delete(&g);
        }
    }
    let mut st = settings(doc);
    if SEEDED_ROOMS.contains(&st.room.as_str()) {
        st.room.clear();
        let _ = put_settings(doc, &st);
    }
}

/// The ids of the org's groups.
fn groups_of(doc: &LoroDoc) -> Vec<String> {
    doc.get_map("groups")
        .keys()
        .map(|k| k.to_string())
        .collect()
}

/// The org document's shape versions: version 1 seeded the prototype's
/// team (nothing now), version 2 takes it out again.
pub const ORG_STEPS: &[drive_spine::schema::Step] = &[seed_team, team_v2];

// ── how an org plays a song ─────────────────────────────────────────────

/// How this org plays a song (its own, or one shared with it).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Use {
    /// Its key here (None: the song's own).
    pub key: Option<String>,
    /// The arrangement it is played in here (None: the org's default).
    pub arrangement: Option<String>,
    /// The arrangements this org has of it.
    pub arrangements: Vec<String>,
    /// Who leads it here (a person's id).
    pub leader: Option<String>,
}

/// A song's id across orgs: `owner/id`.
pub fn song_ref(owner: &str, id: &str) -> String {
    format!("{owner}/{id}")
}

pub fn use_of(lib: &LoroDoc, owner: &str, id: &str) -> Use {
    let Some(m) = child(&lib.get_map("uses"), &song_ref(owner, id)) else {
        return Use::default();
    };
    Use {
        key: opt(s(&m, "key")),
        arrangement: opt(s(&m, "arrangement")),
        arrangements: list(&m, "arrangements"),
        leader: opt(s(&m, "leader")),
    }
}

pub fn put_use(lib: &LoroDoc, owner: &str, id: &str, u: &Use) -> loro::LoroResult<()> {
    let m = lib
        .get_map("uses")
        .ensure_mergeable_map(&song_ref(owner, id))?;
    m.insert("key", u.key.as_deref().unwrap_or(""))?;
    m.insert("arrangement", u.arrangement.as_deref().unwrap_or(""))?;
    m.insert("arrangements", u.arrangements.join(","))?;
    m.insert("leader", u.leader.as_deref().unwrap_or(""))?;
    Ok(())
}

/// The key an org plays a song in: its own, else the song's.
pub fn key_here(u: &Use, e: &Entry) -> Option<String> {
    u.key.clone().or_else(|| e.key.clone())
}

/// The arrangement an org plays a song in: its choice, else its default.
pub fn arrangement_here(u: &Use, st: &Settings) -> String {
    u.arrangement
        .clone()
        .or_else(|| opt(st.arrangement.clone()))
        .unwrap_or_else(|| "Normal".into())
}

// ── sharing ─────────────────────────────────────────────────────────────

/// Share a song this org owns with another org, or stop. The library is
/// the truth; [`mirror_digest`] and [`mirror_project`] follow it.
pub fn share_song(lib: &LoroDoc, id: &str, to: &str, on: bool) -> loro::LoroResult<()> {
    if lib.get_map("songs").get(id).is_none() {
        return Ok(());
    }
    songs::set_shared(lib, id, to, on)?;
    // Every org ever shared with, so taking the last song back empties its digest.
    lib.get_map("share_targets").insert(to, true)?;
    Ok(())
}

/// The orgs this library has shared with (now or before).
pub fn share_targets(lib: &LoroDoc) -> Vec<String> {
    let mut out: Vec<String> = lib
        .get_map("share_targets")
        .keys()
        .map(|k| k.to_string())
        .collect();
    out.sort();
    out
}

/// The songs a library shares with `to`.
pub fn shared_with(lib: &LoroDoc, to: &str) -> Vec<Entry> {
    songs::entries(lib)
        .into_iter()
        .filter(|e| e.shared_with.iter().any(|o| o == to))
        .collect()
}

/// Bring the digest of what this library shares with `to` up to date:
/// the shared songs' faces, nothing else. Whether it changed anything.
pub fn mirror_digest(lib: &LoroDoc, to: &str, digest: &LoroDoc) -> bool {
    let want = shared_with(lib, to);
    let have = songs::entries(digest);
    let mut changed = false;
    let map = digest.get_map("songs");
    for e in &have {
        if !want.iter().any(|w| w.id == e.id) {
            let _ = map.delete(&e.id);
            changed = true;
        }
    }
    for e in &want {
        if have.iter().find(|h| h.id == e.id) != Some(e) {
            let _ = songs::put_entry(digest, e);
            changed = true;
        }
    }
    changed
}

/// Whether the Drive project that shares with `to` needs [`mirror_project`].
pub fn project_differs(lib: &LoroDoc, to: &str, registry: &LoroDoc) -> bool {
    let (docs, shared) = project_wants(lib, to);
    let current = drive_files::projects::projects(registry)
        .into_iter()
        .find(|p| p.id == share_project(to));
    match current {
        None => shared,
        Some(p) => p.docs != docs || p.shared_with != shared_list(to, shared),
    }
}

/// The documents the sharing project should hold, and whether it is shared.
fn project_wants(lib: &LoroDoc, to: &str) -> (Vec<String>, bool) {
    let want = shared_with(lib, to);
    let mut docs: Vec<String> = want.iter().map(|e| songs::song_doc(&e.id)).collect();
    docs.push(digest_doc(to));
    docs.sort();
    (docs, !want.is_empty())
}

fn shared_list(to: &str, shared: bool) -> Vec<String> {
    if shared {
        vec![to.to_string()]
    } else {
        Vec::new()
    }
}

/// Bring the Drive project that shares with `to` up to date, in the
/// owner's registry: the digest and the shared songs' sessions as its
/// documents, shared with `to` while anything is. Whether it changed.
pub fn mirror_project(lib: &LoroDoc, to: &str, registry: &LoroDoc) -> bool {
    use drive_files::projects;
    if !project_differs(lib, to, registry) {
        return false;
    }
    let (docs, shared) = project_wants(lib, to);
    let id = share_project(to);
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
    let _ = m.insert("name", format!("Studio songs shared with {}", org_name(to)));
    let had = current.map(|p| p.docs).unwrap_or_default();
    for d in had.iter().filter(|d| !docs.contains(d)) {
        let _ = projects::link_doc(registry, &id, d, false);
    }
    for d in docs.iter().filter(|d| !had.contains(d)) {
        let _ = projects::link_doc(registry, &id, d, true);
    }
    let _ = projects::share_with_org(registry, &id, to, shared);
    true
}

#[cfg_attr(not(feature = "drive"), allow(dead_code))]
/// The owners sharing Studio songs with `to`, from Drive's projects
/// shared with it (`/shared`: (owner org, project id) pairs).
pub fn owners_sharing(to: &str, shared: &[(String, String)]) -> Vec<String> {
    let mut out: Vec<String> = shared
        .iter()
        .filter(|(org, project)| org != to && *project == share_project(to))
        .map(|(org, _)| org.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

// ── the library's versions ──────────────────────────────────────────────

/// The songs the prototype made up for every library (version 1 seeded
/// them by their titles' slugs, with made-up keys, tempi and tags).
const PROTOTYPE_SONGS: [&str; 40] = [
    "DTLA Cinematic Intro",
    "Drive To LA",
    "For You",
    "Heart Isn't Ready",
    "No One Knows",
    "Hey!",
    "Sleeptalker",
    "Figure It Out",
    "Seven Nation Army",
    "Sad But True",
    "Out Of The Black",
    "She's American",
    "WORST PART OF YOU",
    "Fire",
    "Not Too Much Longer (Live)",
    "Know Me (Extended)",
    "All I Want for Christmas Is You",
    "Feliz Navidad",
    "Joy to the World (Unspeakable Joy)",
    "Last Christmas",
    "Rockin' Around the Christmas Tree",
    "Silent Night",
    "Sleigh Ride",
    "This Christmas",
    "What Christmas Means",
    "Goodness of God",
    "Gratitude",
    "Build My Life",
    "Way Maker",
    "Firm Foundation (He Won't)",
    "Holy Forever",
    "Praise",
    "Trust in God",
    "Jireh",
    "Living Hope",
    "This Is Amazing Grace",
    "See A Victory",
    "King of Kings",
    "Graves Into Gardens",
    "Same God",
];

/// Whether a library song is one the prototype seeded: its id is the
/// slug of one of [`PROTOTYPE_SONGS`] and its title that title (a song
/// someone added has an id of its own, `songs::new_id`).
fn seeded_song(id: &str, title: &str) -> bool {
    PROTOTYPE_SONGS
        .iter()
        .any(|t| *t == title && session_sync::slug::slugify(t) == id)
}

/// Version 1 seeded every library with the prototype's songs, and version
/// 2 moved each to its "home" org; neither does anything now.
fn library_v2(_: &LoroDoc) {}

/// Version 3: the prototype's seeded songs taken out of the library, with
/// this org's uses of them (in any org). Songs people added stay.
pub fn library_v3(lib: &LoroDoc) {
    let mut gone: Vec<String> = Vec::new();
    for e in songs::entries(lib) {
        if seeded_song(&e.id, &e.title) {
            let _ = lib.get_map("songs").delete(&e.id);
            gone.push(e.id);
        }
    }
    let seeded: Vec<String> = PROTOTYPE_SONGS
        .iter()
        .map(|t| session_sync::slug::slugify(t))
        .collect();
    let uses = lib.get_map("uses");
    for k in uses.keys().map(|k| k.to_string()).collect::<Vec<_>>() {
        let id = k.rsplit_once('/').map_or(k.as_str(), |(_, id)| id);
        if seeded.iter().any(|s| s == id) || gone.iter().any(|g| g == id) {
            let _ = uses.delete(&k);
        }
    }
}

/// The library's shape versions (`drive_spine::schema`): 1 and 2 once
/// seeded and moved the prototype's songs (nothing now), 3 takes them out.
pub const LIBRARY_STEPS: &[drive_spine::schema::Step] =
    &[songs::seed_library, library_v2, library_v3];

#[cfg(test)]
mod tests {
    use super::*;
    use drive_spine::schema;

    /// A library with two songs of its own (fixtures, this test's).
    fn lib(org: &str) -> LoroDoc {
        let d = LoroDoc::new();
        stamp_org(&d, org);
        schema::upgrade(&d, LIBRARY_STEPS);
        for (id, title, key) in [("s-one", "Song One", "G"), ("s-two", "Song Two", "A")] {
            songs::put_entry(
                &d,
                &Entry {
                    id: id.into(),
                    title: title.into(),
                    key: Some(key.into()),
                    ..Entry::default()
                },
            )
            .unwrap();
        }
        d
    }

    #[test]
    fn org_names_and_order() {
        assert_eq!(org_name("crossroads-church"), "Crossroads Church");
        assert_eq!(org_name("new-band"), "New Band");
        let o = ordered(vec![
            "tombrooksmusic".into(),
            "zeta".into(),
            "codywright".into(),
        ]);
        assert_eq!(o, vec!["codywright", "tombrooksmusic", "zeta"]);
    }

    #[test]
    fn a_new_org_starts_with_no_made_up_team() {
        let d = LoroDoc::new();
        stamp_org(&d, "crossroads-church");
        schema::upgrade(&d, ORG_STEPS);
        assert!(people(&d).is_empty());
        assert!(groups(&d).is_empty());
        assert_eq!(settings(&d).room, "");
    }

    #[test]
    fn version_2_takes_the_seeded_team_out_and_keeps_the_org_s_own() {
        let d = LoroDoc::new();
        stamp_org(&d, "crossroads-church");
        let _ = d.get_map("meta").insert("version", 1);
        let person = |id: &str, account: Option<&str>, groups: &[&str]| Person {
            id: id.into(),
            name: id.into(),
            account: account.map(str::to_string),
            groups: groups.iter().map(|g| g.to_string()).collect(),
            ..Person::default()
        };
        put_person(&d, &person("jake", None, &["worship"])).unwrap();
        put_person(&d, &person("sam", None, &["youth"])).unwrap();
        put_person(
            &d,
            &person("cody-wright", Some("me@example.com"), &["worship"]),
        )
        .unwrap();
        put_person(&d, &person("ana-ruiz", None, &[])).unwrap();
        for g in ["worship", "youth"] {
            put_group(
                &d,
                &Group {
                    id: g.into(),
                    name: g.into(),
                    kind: "band".into(),
                },
            )
            .unwrap();
        }
        put_settings(
            &d,
            &Settings {
                room: "Worship Center".into(),
                ..Settings::default()
            },
        )
        .unwrap();
        schema::upgrade(&d, ORG_STEPS);
        let ids: Vec<String> = people(&d).into_iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
        assert!(ids.contains(&"cody-wright".to_string()) && ids.contains(&"ana-ruiz".to_string()));
        // `worship` still has someone in it; `youth` had only the made-up Sam.
        let gs: Vec<String> = groups(&d).into_iter().map(|g| g.id).collect();
        assert_eq!(gs, vec!["worship"]);
        assert_eq!(settings(&d).room, "");
    }

    #[test]
    fn people_round_trip_and_link_to_an_account() {
        let d = LoroDoc::new();
        let mut p = Person {
            id: person_id(&d, "Mia Lopez"),
            name: "Mia Lopez".into(),
            hue: 320,
            role: Role::Member,
            parts: vec!["Vocals".into(), "Keys".into()],
            groups: vec!["worship".into()],
            ..Person::default()
        };
        put_person(&d, &p).unwrap();
        assert_eq!(person(&d, "mia-lopez"), Some(p.clone()));
        assert_eq!(person_id(&d, "Mia Lopez"), "mia-lopez-2");
        assert_eq!(p.initials(), "ML");

        let other = Person {
            id: "sam".into(),
            name: "Sam".into(),
            account: Some("me@example.com".into()),
            ..Person::default()
        };
        put_person(&d, &other).unwrap();
        // Linking an account moves it: one person per account.
        link_account(&d, "mia-lopez", "me@example.com").unwrap();
        assert_eq!(person_for(&d, "me@example.com").unwrap().id, "mia-lopez");
        assert_eq!(person(&d, "sam").unwrap().account, None);

        p.role = Role::Leader;
        p.leads = vec!["worship".into()];
        put_person(&d, &p).unwrap();
        assert_eq!(leaders(&d, "worship")[0].id, "mia-lopez");
        remove_person(&d, "sam").unwrap();
        assert_eq!(people(&d).len(), 1);
    }

    #[test]
    fn a_new_library_is_empty_and_version_3_takes_the_prototype_s_songs_out() {
        let d = LoroDoc::new();
        stamp_org(&d, "crossroads-church");
        schema::upgrade(&d, LIBRARY_STEPS);
        assert!(songs::entries(&d).is_empty());

        // A library version 1 seeded: its made-up songs go, the ones
        // added (with ids of their own) stay, and so do their uses.
        let old = LoroDoc::new();
        stamp_org(&old, "crossroads-church");
        let _ = old.get_map("meta").insert("version", 2);
        let seeded = Entry {
            id: "build-my-life".into(),
            title: "Build My Life".into(),
            ..Entry::default()
        };
        let added = Entry {
            id: "a1b2".into(),
            title: "Holy Forever".into(),
            ..Entry::default()
        };
        songs::put_entry(&old, &seeded).unwrap();
        songs::put_entry(&old, &added).unwrap();
        let key = |k: &str| Use {
            key: Some(k.into()),
            ..Use::default()
        };
        put_use(&old, "crossroads-church", "build-my-life", &key("A")).unwrap();
        put_use(&old, "crossroads-church", "a1b2", &key("B")).unwrap();
        assert_eq!(
            schema::upgrade(&old, LIBRARY_STEPS),
            schema::Upgraded::From(2)
        );
        let left: Vec<String> = songs::entries(&old).into_iter().map(|e| e.id).collect();
        assert_eq!(left, vec!["a1b2"]);
        assert_eq!(
            use_of(&old, "crossroads-church", "build-my-life"),
            Use::default()
        );
        assert_eq!(
            use_of(&old, "crossroads-church", "a1b2").key.as_deref(),
            Some("B")
        );
    }

    #[test]
    fn a_song_shared_with_another_org() {
        let lib = lib("rockstars-of-tomorrow");
        let song = songs::entries(&lib).into_iter().next().unwrap();
        share_song(&lib, &song.id, "cbu", true).unwrap();
        assert!(shared_with(&lib, "cbu").iter().any(|e| e.id == song.id));

        // The digest and Drive's project follow the library.
        let digest = LoroDoc::new();
        let registry = LoroDoc::new();
        assert!(mirror_digest(&lib, "cbu", &digest));
        assert!(mirror_project(&lib, "cbu", &registry));
        assert!(!mirror_digest(&lib, "cbu", &digest));
        assert!(!mirror_project(&lib, "cbu", &registry));
        assert_eq!(songs::entries(&digest), shared_with(&lib, "cbu"));
        let p = drive_files::projects::projects(&registry)
            .into_iter()
            .find(|p| p.id == share_project("cbu"))
            .unwrap();
        assert_eq!(p.shared_with, vec!["cbu"]);
        assert!(p.docs.contains(&songs::song_doc(&song.id)));
        assert!(p.docs.contains(&digest_doc("cbu")));

        // CBU finds the owner through Drive's shared projects, and plays it its own way.
        let found = owners_sharing(
            "cbu",
            &[
                ("rockstars-of-tomorrow".into(), share_project("cbu")),
                ("rockstars-of-tomorrow".into(), "some-other-project".into()),
                ("crossroads-church".into(), share_project("days-to-praise")),
            ],
        );
        assert_eq!(found, vec!["rockstars-of-tomorrow"]);
        let cbu = LoroDoc::new();
        let mine = Use {
            key: Some("D".into()),
            arrangement: Some("Acoustic".into()),
            arrangements: vec!["Acoustic".into(), "Normal".into()],
            leader: Some("dr-hale".into()),
        };
        put_use(&cbu, "rockstars-of-tomorrow", &song.id, &mine).unwrap();
        assert_eq!(use_of(&cbu, "rockstars-of-tomorrow", &song.id), mine);
        assert_eq!(key_here(&mine, &song), Some("D".into()));
        // The owner's own key is untouched.
        assert_eq!(use_of(&lib, "rockstars-of-tomorrow", &song.id).key, None);
        assert_eq!(
            arrangement_here(&Use::default(), &Settings::default()),
            "Normal"
        );

        // Taking it back empties the digest and closes the project to CBU.
        for e in shared_with(&lib, "cbu") {
            share_song(&lib, &e.id, "cbu", false).unwrap();
        }
        assert!(mirror_digest(&lib, "cbu", &digest));
        assert!(mirror_project(&lib, "cbu", &registry));
        assert!(songs::entries(&digest).is_empty());
        let p = drive_files::projects::projects(&registry)
            .into_iter()
            .find(|p| p.id == share_project("cbu"))
            .unwrap();
        assert!(p.shared_with.is_empty());
        assert_eq!(p.docs, vec![digest_doc("cbu")]);
    }

    #[test]
    fn two_orgs_share_at_once_and_both_land() {
        let a = lib("crossroads-church");
        let b = LoroDoc::new();
        b.import(&a.export(loro::ExportMode::Snapshot).unwrap())
            .unwrap();
        let id = songs::entries(&a)[0].id.clone();
        share_song(&a, &id, "cbu", true).unwrap();
        share_song(&b, &id, "fasttrackaudio", true).unwrap();
        a.import(&b.export(loro::ExportMode::Snapshot).unwrap())
            .unwrap();
        // Both shares land (the sharing is a map, merged key by key),
        // and the mirror visits both orgs.
        let e = songs::entries(&a).into_iter().find(|e| e.id == id).unwrap();
        assert_eq!(e.shared_with, vec!["cbu", "fasttrackaudio"]);
        let targets = share_targets(&a);
        assert!(targets.contains(&"cbu".to_string()));
        assert!(targets.contains(&"fasttrackaudio".to_string()));
    }
}
