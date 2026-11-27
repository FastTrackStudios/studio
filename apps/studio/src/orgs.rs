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
            .map(crate::data::cap)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

pub fn org_hue(slug: &str) -> u32 {
    known(slug).map_or_else(|| crate::data::str_hue(slug), |k| k.hue)
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

/// An org's first team: who the prototype already had, by the kind of org.
fn seed_team(doc: &LoroDoc) {
    let org = stamped_org(doc);
    let kind = known(&org).map(|k| k.kind);
    let g = |id: &str, name: &str, kind: &str| Group {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
    };
    let p = |name: &str, hue: u32, role: Role, parts: &[&str], groups: &[&str], leads: &[&str]| {
        Person {
            id: session_sync::slug::slugify(name),
            name: name.into(),
            account: None,
            hue,
            role,
            parts: parts.iter().map(|x| x.to_string()).collect(),
            groups: groups.iter().map(|x| x.to_string()).collect(),
            leads: leads.iter().map(|x| x.to_string()).collect(),
        }
    };
    let (groups, people, settings): (Vec<Group>, Vec<Person>, Settings) = match kind {
        Some(Kind::Church) if org == "crossroads-church" => (
            vec![
                g("worship", "Worship team", "worship team"),
                g("youth", "Youth band", "band"),
            ],
            vec![
                p(
                    "Cody Wright",
                    30,
                    Role::Leader,
                    &["Electric guitar", "Tracks"],
                    &["worship", "youth"],
                    &["youth"],
                ),
                p(
                    "Jake",
                    200,
                    Role::Leader,
                    &["Lead vocals", "Acoustic guitar"],
                    &["worship", "youth"],
                    &["worship"],
                ),
                p(
                    "Mia",
                    320,
                    Role::Member,
                    &["Vocals", "Keys"],
                    &["worship", "youth"],
                    &[],
                ),
                p(
                    "Sam",
                    140,
                    Role::Member,
                    &["Drums"],
                    &["worship", "youth"],
                    &[],
                ),
                p(
                    "Riley",
                    260,
                    Role::Tech,
                    &["Sound", "Lights"],
                    &["worship"],
                    &[],
                ),
            ],
            Settings {
                name: String::new(),
                arrangement: "Normal".into(),
                room: "Worship Center".into(),
            },
        ),
        Some(Kind::Church) => (
            vec![g("worship", "Worship team", "worship team")],
            vec![p(
                "Cody Wright",
                30,
                Role::Leader,
                &["Electric guitar", "Vocals"],
                &["worship"],
                &["worship"],
            )],
            Settings {
                arrangement: "Normal".into(),
                ..Settings::default()
            },
        ),
        Some(Kind::Lessons) => (
            vec![g("recital", "Recital band", "students")],
            vec![
                p(
                    "Cody Wright",
                    30,
                    Role::Teacher,
                    &["Electric guitar", "Bass"],
                    &["recital"],
                    &["recital"],
                ),
                p(
                    "Jordan",
                    15,
                    Role::Student,
                    &["Electric guitar", "Lead vocals"],
                    &["recital"],
                    &[],
                ),
                p("Ava", 300, Role::Student, &["Drums"], &["recital"], &[]),
                p("Liam", 180, Role::Student, &["Bass"], &["recital"], &[]),
            ],
            Settings {
                arrangement: "Normal".into(),
                room: "Studio A".into(),
                ..Settings::default()
            },
        ),
        Some(Kind::School) => (
            vec![g("christmas", "Christmas band", "band")],
            vec![
                p(
                    "Cody Wright",
                    30,
                    Role::Member,
                    &["Electric guitar"],
                    &["christmas"],
                    &[],
                ),
                p(
                    "Dr. Hale",
                    90,
                    Role::Leader,
                    &["Keys"],
                    &["christmas"],
                    &["christmas"],
                ),
            ],
            Settings {
                arrangement: "Normal".into(),
                room: "Recital Hall".into(),
                ..Settings::default()
            },
        ),
        _ => (
            Vec::new(),
            vec![p(
                "Cody Wright",
                30,
                Role::Leader,
                &["Electric guitar", "Bass", "Keys"],
                &[],
                &[],
            )],
            Settings {
                arrangement: "Normal".into(),
                ..Settings::default()
            },
        ),
    };
    if doc.get_map("people").is_empty() {
        for x in &groups {
            let _ = put_group(doc, x);
        }
        for x in &people {
            let _ = put_person(doc, x);
        }
        let _ = put_settings(doc, &settings);
    }
}

/// The org document's shape versions: version 1 is the seeded team.
pub const ORG_STEPS: &[drive_spine::schema::Step] = &[seed_team];

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

/// Which org a built-in song belongs to, by its tags: the Christmas show
/// is CBU's, the recital Rockstars of Tomorrow's, the worship rotation
/// Crossroads Church's.
pub fn home_of(tags: &[String]) -> Option<&'static str> {
    let has = |t: &str| tags.iter().any(|x| x == t);
    if has("christmas") {
        Some("cbu")
    } else if has("recital") || has("originals") || has("covers") {
        Some("rockstars-of-tomorrow")
    } else if has("worship") || has("contemporary") || has("hymns") {
        Some("crossroads-church")
    } else {
        None
    }
}

/// What each home org shares to start with.
fn first_shares(org: &str, e: &Entry) -> Vec<&'static str> {
    let has = |t: &str| e.tags.iter().any(|x| x == t);
    match org {
        // The hymns go to the other worship team.
        "crossroads-church" if has("hymns") => vec!["days-to-praise"],
        // The Christmas show's carols and worship songs go to the church.
        "cbu" if has("hymns") || has("worship") => vec!["crossroads-church"],
        // The students' originals go to the teacher's own org.
        "rockstars-of-tomorrow" if has("originals") => vec!["codywright"],
        _ => Vec::new(),
    }
}

/// Version 2: a library is one org's. Every org's library was seeded
/// with every built-in song; each built-in song now lives in its home
/// org only (its key here, when this org had changed it, kept as this
/// org's use of the home's song). Songs people added stay where they
/// are. The home orgs start sharing a few songs.
pub fn library_v2(lib: &LoroDoc) {
    let org = stamped_org(lib);
    let builtin: Vec<Entry> = crate::data::LIB.iter().map(Entry::from_lib).collect();
    for e in songs::entries(lib) {
        let Some(seed) = builtin.iter().find(|b| b.id == e.id) else {
            continue;
        };
        match home_of(&seed.tags) {
            Some(home) if home != org => {
                if e.key.is_some() && e.key != seed.key {
                    let mut u = use_of(lib, home, &e.id);
                    u.key = e.key.clone();
                    let _ = put_use(lib, home, &e.id, &u);
                }
                let _ = lib.get_map("songs").delete(&e.id);
            }
            Some(home) => {
                for to in first_shares(home, &e) {
                    let _ = share_song(lib, &e.id, to, true);
                }
            }
            None => {}
        }
    }
}

/// The library's shape versions (`drive_spine::schema`): 1, the built-in
/// songs; 2, one org's songs, their sharing and the org's uses.
pub const LIBRARY_STEPS: &[drive_spine::schema::Step] = &[songs::seed_library, library_v2];

#[cfg(test)]
mod tests {
    use super::*;
    use drive_spine::schema;

    fn lib_v1(org: &str) -> LoroDoc {
        let d = LoroDoc::new();
        stamp_org(&d, org);
        schema::upgrade(&d, &LIBRARY_STEPS[..1]);
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
    fn a_church_starts_with_its_worship_team() {
        let d = LoroDoc::new();
        stamp_org(&d, "crossroads-church");
        schema::upgrade(&d, ORG_STEPS);
        let ps = people(&d);
        assert!(ps.len() >= 4);
        // Leaders first.
        assert!(ps[0].leading());
        let worship: Vec<String> = leaders(&d, "worship").into_iter().map(|p| p.name).collect();
        assert_eq!(worship, vec!["Jake"]);
        let drums: Vec<String> = playing(&d, "Drums", Some("worship"))
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(drums, vec!["Sam"]);
        assert_eq!(settings(&d).room, "Worship Center");
        // Seeding again changes nothing.
        let before = people(&d);
        seed_team(&d);
        assert_eq!(people(&d), before);
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
    fn upgrade_to_v2_keeps_each_song_in_its_home_org() {
        // Crossroads: keeps the worship songs, loses the recital's and the Christmas show's.
        let d = lib_v1("crossroads-church");
        let all = songs::entries(&d).len();
        assert!(all > 20);
        assert_eq!(
            schema::upgrade(&d, LIBRARY_STEPS),
            schema::Upgraded::From(1)
        );
        let kept = songs::entries(&d);
        assert!(!kept.is_empty() && kept.len() < all);
        assert!(
            kept.iter()
                .all(|e| home_of(&e.tags) == Some("crossroads-church"))
        );
        // A hymn is shared with Days to Praise from the start.
        let hymns = shared_with(&d, "days-to-praise");
        assert!(!hymns.is_empty());
        assert!(hymns.iter().all(|e| e.tags.iter().any(|t| t == "hymns")));
        assert_eq!(share_targets(&d), vec!["days-to-praise"]);
    }

    #[test]
    fn every_home_org_starts_sharing_something() {
        for (org, to) in [
            ("crossroads-church", "days-to-praise"),
            ("cbu", "crossroads-church"),
            ("rockstars-of-tomorrow", "codywright"),
        ] {
            let d = lib_v1(org);
            schema::upgrade(&d, LIBRARY_STEPS);
            assert!(
                !shared_with(&d, to).is_empty(),
                "{org} shares nothing with {to}"
            );
        }
    }

    #[test]
    fn upgrade_keeps_added_songs_and_a_changed_key() {
        let d = lib_v1("codywright");
        // Someone added a song, and moved a worship song's key.
        let mine = Entry {
            id: "my-song".into(),
            title: "My Song".into(),
            ..Entry::default()
        };
        songs::put_entry(&d, &mine).unwrap();
        let mut moved = songs::entries(&d)
            .into_iter()
            .find(|e| home_of(&e.tags) == Some("crossroads-church"))
            .unwrap();
        let old_key = moved.key.clone();
        moved.key = Some(
            if old_key.as_deref() == Some("A") {
                "B"
            } else {
                "A"
            }
            .into(),
        );
        songs::put_entry(&d, &moved).unwrap();

        schema::upgrade(&d, LIBRARY_STEPS);
        let left = songs::entries(&d);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, "my-song");
        // The key this org played it in is kept, as its use of the church's song.
        assert_eq!(use_of(&d, "crossroads-church", &moved.id).key, moved.key);
        // Running it again does nothing.
        assert_eq!(
            schema::upgrade(&d, LIBRARY_STEPS),
            schema::Upgraded::Current
        );
    }

    #[test]
    fn a_song_shared_with_another_org() {
        let lib = lib_v1("rockstars-of-tomorrow");
        schema::upgrade(&lib, LIBRARY_STEPS);
        let song = songs::entries(&lib)
            .into_iter()
            .find(|e| !e.tags.iter().any(|t| t == "originals"))
            .unwrap();
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
        let a = lib_v1("crossroads-church");
        schema::upgrade(&a, LIBRARY_STEPS);
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
