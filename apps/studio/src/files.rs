//! A song's files: where they are, what each one is, and whether a set is
//! ready to play.
//!
//! A song's files are a **Drive project** in the song's org
//! (`<org>/drive:space`, project `studio-song-<id>`, kind
//! `MusicRecording`). Each linked file or folder is one of its **parts**:
//! a location (a folder a computer indexes) and a path inside it, or the
//! Drive tree (location [`TREE`]) for files uploaded here. Linking writes
//! the registry only: the file stays where it is, on whichever computer
//! has it, and nothing is ever copied, moved or deleted. Unlinking takes
//! the part out of the project and leaves the file alone.
//!
//! The part's `label` is the kind's name (`Stems`, `Click`), so Drive's
//! own project page and the orgs the song is shared with group them the
//! same way. What Drive's registry has no room for is Studio's, in
//! `<org>/studio:files:<id>` (a document of the same project, so the orgs
//! it is shared with may open it):
//!
//! - `links`: part id → `{ kind, space, dir }`: the kind the person chose
//!   (else it is recognised), the space of a location that is another
//!   org's (a person's own computers are their personal org's), and
//!   whether the part is a folder;
//! - `kinds`: a file inside a linked folder (its target) → its kind, when
//!   someone changed it;
//! - `aliases`: other names the song goes by, for finding its files.
//!
//! The project's `docs` are the song's session and this document, and its
//! `shared_with` follows the song's ([`ensure_project`]): sharing a song
//! shares its files.

// Finding and uploading run in the web build (the node's indexes).
#![cfg_attr(not(feature = "drive"), allow(dead_code))]

use std::collections::BTreeSet;

use loro::{LoroDoc, LoroMap, LoroValue, ValueOrContainer};

use drive_files::projects::{self, Part};

/// A part's location when it is in the org's Drive tree (an upload).
pub const TREE: &str = "drive";
/// Studio's own folder on this computer (`crate::storage::root`): a hit's
/// path is in it, or absolute.
pub const DISK: &str = "studio-disk";

/// The Drive project that holds a song's files.
pub fn song_project(id: &str) -> String {
    format!("studio-song-{id}")
}

/// Studio's document about a song's files.
pub fn files_doc(id: &str) -> String {
    format!("studio:files:{id}")
}

/// The Drive tree folder a song's uploads go into.
pub fn upload_folder(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '-'
            } else {
                c
            }
        })
        .collect();
    format!("Studio/Songs/{}", clean.trim())
}

// ── kinds ───────────────────────────────────────────────────────────────

/// What a file is to a song.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FileKind {
    ChartPdf,
    Keyflow,
    Lyrics,
    Click,
    Guide,
    Backing,
    Stems,
    Multitrack,
    Reference,
    Midi,
    Session,
    Video,
    Other,
}

impl FileKind {
    /// In the order a song's Files section shows them.
    pub const ALL: [FileKind; 13] = [
        FileKind::ChartPdf,
        FileKind::Keyflow,
        FileKind::Lyrics,
        FileKind::Click,
        FileKind::Guide,
        FileKind::Backing,
        FileKind::Stems,
        FileKind::Multitrack,
        FileKind::Reference,
        FileKind::Midi,
        FileKind::Session,
        FileKind::Video,
        FileKind::Other,
    ];

    pub fn id(self) -> &'static str {
        match self {
            FileKind::ChartPdf => "chart-pdf",
            FileKind::Keyflow => "keyflow",
            FileKind::Lyrics => "lyrics",
            FileKind::Click => "click",
            FileKind::Guide => "guide",
            FileKind::Backing => "backing",
            FileKind::Stems => "stems",
            FileKind::Multitrack => "multitrack",
            FileKind::Reference => "reference",
            FileKind::Midi => "midi",
            FileKind::Session => "session",
            FileKind::Video => "video",
            FileKind::Other => "other",
        }
    }

    /// What people call it (a part's label).
    pub fn name(self) -> &'static str {
        match self {
            FileKind::ChartPdf => "Chart PDF",
            FileKind::Keyflow => "Keyflow chart",
            FileKind::Lyrics => "Lyrics",
            FileKind::Click => "Click",
            FileKind::Guide => "Guide",
            FileKind::Backing => "Backing track",
            FileKind::Stems => "Stems",
            FileKind::Multitrack => "Multitrack",
            FileKind::Reference => "Reference mix",
            FileKind::Midi => "MIDI",
            FileKind::Session => "Session",
            FileKind::Video => "Video",
            FileKind::Other => "Other",
        }
    }

    /// By id or by name (a part's label), any case.
    pub fn parse(s: &str) -> Option<FileKind> {
        let s = s.trim();
        FileKind::ALL
            .into_iter()
            .find(|k| k.id().eq_ignore_ascii_case(s) || k.name().eq_ignore_ascii_case(s))
    }
}

const AUDIO: &[&str] = &[
    "wav", "wave", "mp3", "flac", "aif", "aiff", "aifc", "m4a", "ogg", "opus", "caf", "w64",
    "rf64", "bwf", "wv", "alac",
];
const VIDEO: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi", "mts"];
const SESSIONS: &[&str] = &[
    "rpp",
    "als",
    "ptx",
    "ptf",
    "pts",
    "logicx",
    "cpr",
    "npr",
    "song",
    "flp",
    "dawproject",
    "bwproject",
];

/// The extension of a name, lower case (`""` when it has none).
pub fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.contains(' ') => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub fn is_audio(name: &str) -> bool {
    AUDIO.contains(&extension(name).as_str())
}

/// Lower-case words of a name, split at anything not a letter or digit.
fn words(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn has_any(ws: &[String], list: &[&str]) -> bool {
    ws.iter().any(|w| list.contains(&w.as_str()))
}

fn has_phrase(ws: &[String], phrase: &[&str]) -> bool {
    ws.windows(phrase.len())
        .any(|w| w.iter().zip(phrase).all(|(a, b)| a == b))
}

/// What a folder's name says it holds, if anything.
fn folder_says(name: &str) -> Option<FileKind> {
    let ws = words(name);
    let n = name.to_lowercase();
    if extension(name) == "logicx" {
        return Some(FileKind::Session);
    }
    if has_any(&ws, &["stem", "stems"]) {
        Some(FileKind::Stems)
    } else if n.contains("multitrack")
        || n.contains("multi-track")
        || has_any(&ws, &["multis", "mt", "mts", "tracks"])
        || has_phrase(&ws, &["audio", "files"])
    {
        Some(FileKind::Multitrack)
    } else if has_any(&ws, &["click", "clicks"]) {
        Some(FileKind::Click)
    } else if has_any(&ws, &["guide", "guides", "cues"]) {
        Some(FileKind::Guide)
    } else if has_any(&ws, &["chart", "charts"]) {
        Some(FileKind::ChartPdf)
    } else if has_any(&ws, &["lyrics", "lrc"]) {
        Some(FileKind::Lyrics)
    } else if has_any(&ws, &["midi"]) {
        Some(FileKind::Midi)
    } else if has_any(&ws, &["video", "videos", "footage"]) {
        Some(FileKind::Video)
    } else if has_any(
        &ws,
        &[
            "mix", "mixes", "bounce", "bounces", "bounced", "masters", "master",
        ],
    ) {
        Some(FileKind::Reference)
    } else if has_any(&ws, &["session", "sessions", "reaper", "ableton", "logic"])
        || has_phrase(&ws, &["pro", "tools"])
    {
        Some(FileKind::Session)
    } else {
        None
    }
}

/// Words that name one microphone or source of a recording session.
const MICS: &[&str] = &[
    "kick",
    "snare",
    "hat",
    "hh",
    "hihat",
    "tom",
    "toms",
    "oh",
    "ohl",
    "ohr",
    "overhead",
    "overheads",
    "room",
    "rooms",
    "top",
    "bottom",
    "btm",
    "di",
    "amp",
    "mic",
    "ride",
    "crash",
    "floor",
    "sub",
];
/// Words that name a group of a mix (a stem).
const GROUPS: &[&str] = &[
    "drums",
    "bass",
    "keys",
    "piano",
    "guitars",
    "guitar",
    "gtr",
    "gtrs",
    "eg",
    "ag",
    "vocals",
    "vocal",
    "vox",
    "bgv",
    "bgvs",
    "choir",
    "strings",
    "pad",
    "pads",
    "synth",
    "synths",
    "perc",
    "percussion",
    "organ",
    "horns",
    "brass",
    "loops",
];

/// An audio file's kind, from its name and the folders above it.
fn audio_kind(ws: &[String], folders: &[&str]) -> FileKind {
    if has_any(ws, &["click", "clk", "metronome", "met"]) {
        return FileKind::Click;
    }
    if has_any(ws, &["guide", "cue", "cues"]) {
        return FileKind::Guide;
    }
    if has_any(
        ws,
        &[
            "backing",
            "bt",
            "playback",
            "instrumental",
            "karaoke",
            "minus",
            "accompaniment",
        ],
    ) {
        return FileKind::Backing;
    }
    // The nearest folder that says what it holds.
    if let Some(k) = folders
        .iter()
        .rev()
        .filter_map(|f| folder_says(f))
        .find(|k| {
            matches!(
                k,
                FileKind::Stems | FileKind::Multitrack | FileKind::Reference
            )
        })
    {
        return k;
    }
    if has_any(ws, &["stem", "stems"]) {
        return FileKind::Stems;
    }
    if has_any(
        ws,
        &[
            "mix",
            "master",
            "mastered",
            "ref",
            "reference",
            "demo",
            "original",
            "rough",
        ],
    ) {
        return FileKind::Reference;
    }
    if has_any(ws, MICS) {
        return FileKind::Multitrack;
    }
    if has_any(ws, GROUPS) {
        return FileKind::Stems;
    }
    FileKind::Reference
}

/// What a file or folder is, from its path (in its location or the Drive
/// tree: the folders above it count) and whether it is a folder. Drive's
/// kinds by extension first, then names.
pub fn recognise(path: &str, dir: bool) -> FileKind {
    let path = path.trim_matches('/');
    let (folders, name) = match path.rsplit_once('/') {
        Some((up, name)) => (up.split('/').collect::<Vec<_>>(), name),
        None => (Vec::new(), path),
    };
    if dir {
        return folder_says(name).unwrap_or(FileKind::Other);
    }
    let ext = extension(name);
    let stem = name.strip_suffix(&format!(".{ext}")).unwrap_or(name);
    let ws = words(stem);
    match ext.as_str() {
        "kf" | "musicxml" | "mxl" => FileKind::Keyflow,
        "lrc" => FileKind::Lyrics,
        "mid" | "midi" => FileKind::Midi,
        "pdf" => FileKind::ChartPdf,
        "txt" | "md" if has_any(&ws, &["lyrics", "lyric"]) => FileKind::Lyrics,
        e if SESSIONS.contains(&e) => FileKind::Session,
        e if VIDEO.contains(&e) => FileKind::Video,
        e if AUDIO.contains(&e) => audio_kind(&ws, &folders),
        _ => FileKind::Other,
    }
}

/// A folder's kind from what is in it (names, whether each is a folder),
/// when its own name says nothing: a DAW project makes it a session, many
/// recordings a multitrack, a few stems.
pub fn recognise_folder(name: &str, children: &[(&str, bool)]) -> FileKind {
    if let Some(k) = folder_says(name) {
        return k;
    }
    if children
        .iter()
        .any(|(n, _)| SESSIONS.contains(&extension(n).as_str()))
    {
        return FileKind::Session;
    }
    let audio: Vec<&str> = children
        .iter()
        .filter(|(n, d)| !d && is_audio(n))
        .map(|(n, _)| *n)
        .collect();
    let mics = audio.iter().filter(|n| has_any(&words(n), MICS)).count();
    match audio.len() {
        0 => FileKind::Other,
        n if n >= 10 || mics * 2 > n => FileKind::Multitrack,
        1 => recognise(audio[0], false),
        _ => FileKind::Stems,
    }
}

// ── links: the project's parts and Studio's notes on them ───────────────

/// A linked file or folder of a song.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// The Drive part's id.
    pub part: String,
    pub kind: FileKind,
    /// The space (org) of its location: the song's own unless it is on
    /// another org's computers.
    pub space: String,
    /// A location id, or [`TREE`].
    pub location: String,
    pub path: String,
    pub dir: bool,
    /// Kept on the node (Drive's "available").
    pub available: bool,
}

impl Link {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// The name `drive-local` and Drive's MCP tools take.
    pub fn target(&self) -> String {
        target(&self.location, &self.path)
    }

    pub fn in_tree(&self) -> bool {
        self.location == TREE
    }

    /// Whether a file of a location is this link, or inside it.
    pub fn covers(&self, space: &str, location: &str, path: &str) -> bool {
        self.space == space && self.location == location && {
            let p = Part {
                id: String::new(),
                label: String::new(),
                location: location.to_string(),
                path: self.path.clone(),
                available: false,
                ..Default::default()
            };
            p.contains(path.trim_matches('/'))
        }
    }
}

/// `drive:PATH` for the Drive tree, else `LOCATION:PATH`.
pub fn target(location: &str, path: &str) -> String {
    format!("{location}:{}", path.trim_matches('/'))
}

/// What to link.
#[derive(Clone, Debug, PartialEq)]
pub struct NewLink {
    pub space: String,
    pub location: String,
    pub path: String,
    pub dir: bool,
    /// None: recognised from the path.
    pub kind: Option<FileKind>,
}

fn s(m: &LoroMap, k: &str) -> String {
    match m.get(k) {
        Some(ValueOrContainer::Value(LoroValue::String(v))) => v.to_string(),
        _ => String::new(),
    }
}

fn b(m: &LoroMap, k: &str) -> bool {
    matches!(
        m.get(k),
        Some(ValueOrContainer::Value(LoroValue::Bool(true)))
    )
}

fn child(m: &LoroMap, k: &str) -> Option<LoroMap> {
    match m.get(k) {
        Some(ValueOrContainer::Container(c)) => c.into_map().ok(),
        _ => None,
    }
}

/// The song's project as Drive has it, if it exists.
pub fn project(registry: &LoroDoc, song: &str) -> Option<projects::Project> {
    let id = song_project(song);
    projects::projects(registry)
        .into_iter()
        .find(|p| p.id == id)
}

/// The project's `docs` for a song: its session and Studio's notes.
pub fn project_docs(song: &str) -> Vec<String> {
    let mut v = vec![crate::songs::song_doc(song), files_doc(song)];
    v.sort();
    v
}

/// Make the song's project, or bring it up to date: its name (the song's
/// title), its documents, and the orgs it is shared with (the song's).
/// Whether it changed anything (each change is sent, so only when needed).
pub fn ensure_project(registry: &LoroDoc, song: &str, title: &str, shared_with: &[String]) -> bool {
    let id = song_project(song);
    let current = project(registry, song);
    let want_docs = project_docs(song);
    let mut want_shared: Vec<String> = shared_with.to_vec();
    want_shared.sort();
    want_shared.dedup();
    if let Some(p) = &current
        && p.name == title
        && p.docs == want_docs
        && p.shared_with == want_shared
    {
        return false;
    }
    let Ok(m) = registry.get_map("projects").ensure_mergeable_map(&id) else {
        return false;
    };
    if current.is_none() {
        let _ = m.insert("id", id.as_str());
        let _ = m.insert("kind", "MusicRecording");
        let _ = m.insert("created", drive_files::now_ms());
    }
    if current.as_ref().is_none_or(|p| p.name != title) {
        let _ = m.insert("name", title);
    }
    let had_docs = current.as_ref().map(|p| p.docs.clone()).unwrap_or_default();
    for d in had_docs.iter().filter(|d| !want_docs.contains(d)) {
        let _ = projects::link_doc(registry, &id, d, false);
    }
    for d in want_docs.iter().filter(|d| !had_docs.contains(d)) {
        let _ = projects::link_doc(registry, &id, d, true);
    }
    let had = current.map(|p| p.shared_with).unwrap_or_default();
    for o in had.iter().filter(|o| !want_shared.contains(o)) {
        let _ = projects::share_with_org(registry, &id, o, false);
    }
    for o in want_shared.iter().filter(|o| !had.contains(o)) {
        let _ = projects::share_with_org(registry, &id, o, true);
    }
    true
}

/// Bring every song's project that exists up to date with the library
/// (titles and sharing). Songs without files have no project and get
/// none. Whether anything changed.
pub fn mirror_projects(registry: &LoroDoc, songs: &[crate::songs::Entry]) -> bool {
    let mut changed = false;
    for e in songs {
        if project(registry, &e.id).is_some() {
            changed |= ensure_project(registry, &e.id, &e.title, &e.shared_with);
        }
    }
    changed
}

/// Whether [`mirror_projects`] would change anything.
pub fn projects_differ(registry: &LoroDoc, songs: &[crate::songs::Entry]) -> bool {
    songs.iter().any(|e| {
        project(registry, &e.id).is_some_and(|p| {
            let mut want = e.shared_with.clone();
            want.sort();
            want.dedup();
            p.name != e.title || p.shared_with != want || p.docs != project_docs(&e.id)
        })
    })
}

/// Link a file or folder to a song where it is: one more part of the
/// song's project (made if it is not there yet), and Studio's note of its
/// kind. Writes the two documents only. The part's id.
pub fn link(
    registry: &LoroDoc,
    meta: &LoroDoc,
    org: &str,
    song: &crate::songs::Entry,
    l: &NewLink,
) -> Option<String> {
    let path = l.path.trim_matches('/').to_string();
    // Already linked: the same part.
    if let Some(have) = links(registry, meta, &song.id, org)
        .into_iter()
        .find(|x| x.space == l.space && x.location == l.location && x.path == path)
    {
        return Some(have.part);
    }
    ensure_project(registry, &song.id, &song.title, &song.shared_with);
    let kind = l.kind.unwrap_or_else(|| recognise(&path, l.dir));
    let part = projects::add_part(
        registry,
        &song_project(&song.id),
        kind.name(),
        &l.location,
        &path,
    )
    .ok()?;
    let m = meta.get_map("links").ensure_mergeable_map(&part).ok()?;
    let _ = m.insert("kind", kind.id());
    let _ = m.insert("dir", l.dir);
    // The space only when it is not the song's own.
    let _ = m.insert("space", if l.space == org { "" } else { l.space.as_str() });
    Some(part)
}

/// Take a part out of the song (the file stays where it is).
pub fn unlink(registry: &LoroDoc, meta: &LoroDoc, song: &str, part: &str) {
    let _ = projects::remove_part(registry, &song_project(song), part);
    let links = meta.get_map("links");
    if links.get(part).is_some() {
        let _ = links.delete(part);
    }
}

/// Change what a linked part is.
pub fn set_link_kind(registry: &LoroDoc, meta: &LoroDoc, song: &str, part: &str, kind: FileKind) {
    if let Ok(m) = meta.get_map("links").ensure_mergeable_map(part) {
        let _ = m.insert("kind", kind.id());
    }
    // The label Drive shows follows it.
    if let Ok(parts) = registry
        .get_map("projects")
        .ensure_mergeable_map(&song_project(song))
        .and_then(|p| p.ensure_mergeable_map("parts"))
        && let Some(p) = child(&parts, part)
    {
        let _ = p.insert("label", kind.name());
    }
}

/// Change what one file inside a linked folder is (its target).
pub fn set_file_kind(meta: &LoroDoc, target: &str, kind: Option<FileKind>) {
    let m = meta.get_map("kinds");
    match kind {
        Some(k) => {
            let _ = m.insert(target, k.id());
        }
        None if m.get(target).is_some() => {
            let _ = m.delete(target);
        }
        None => {}
    }
}

/// The kind someone gave a file inside a linked folder.
pub fn file_kind(meta: &LoroDoc, target: &str) -> Option<FileKind> {
    match meta.get_map("kinds").get(target) {
        Some(ValueOrContainer::Value(LoroValue::String(v))) => FileKind::parse(&v),
        _ => None,
    }
}

/// Other names the song goes by (for finding its files).
pub fn aliases(meta: &LoroDoc) -> Vec<String> {
    let m = meta.get_map("aliases");
    let mut v: Vec<String> = m.keys().map(|k| k.to_string()).collect();
    v.sort();
    v
}

pub fn set_alias(meta: &LoroDoc, alias: &str, on: bool) {
    let alias = alias.trim();
    if alias.is_empty() {
        return;
    }
    let m = meta.get_map("aliases");
    if on {
        let _ = m.insert(alias, true);
    } else if m.get(alias).is_some() {
        let _ = m.delete(alias);
    }
}

/// A song's links: its project's parts, with Studio's notes on them. For
/// a project another org shares, pass its parts and the owner's notes.
pub fn links_of(parts: &[Part], meta: &LoroDoc, space: &str) -> Vec<Link> {
    let notes = meta.get_map("links");
    let mut out: Vec<Link> = parts
        .iter()
        .map(|p| {
            let n = child(&notes, &p.id);
            let kind = n
                .as_ref()
                .and_then(|n| FileKind::parse(&s(n, "kind")))
                .or_else(|| FileKind::parse(&p.label))
                .unwrap_or_else(|| recognise(&p.path, false));
            let sp = n.as_ref().map(|n| s(n, "space")).unwrap_or_default();
            Link {
                part: p.id.clone(),
                kind,
                space: if sp.is_empty() { space.to_string() } else { sp },
                location: p.location.clone(),
                path: p.path.clone(),
                dir: n.as_ref().is_some_and(|n| b(n, "dir")),
                available: p.available,
            }
        })
        .collect();
    out.sort_by(|a, b| (a.kind, a.name().to_lowercase()).cmp(&(b.kind, b.name().to_lowercase())));
    out
}

/// A song's links, from its org's registry and Studio's notes (`org`:
/// the song's org, the space its parts are in unless noted otherwise).
pub fn links(registry: &LoroDoc, meta: &LoroDoc, song: &str, org: &str) -> Vec<Link> {
    let parts = project(registry, song).map(|p| p.parts).unwrap_or_default();
    links_of(&parts, meta, org)
}

/// Keep a song's parts on the node (or stop): Drive's "available", which
/// the computer that has the files carries out.
pub fn set_available(registry: &LoroDoc, song: &str, parts: &[String], on: bool) {
    for p in parts {
        let _ = projects::set_part_available(registry, &song_project(song), p, on);
    }
}

// ── finding a song's files ──────────────────────────────────────────────

/// One file or folder an index has (the node's `/search` and `/list`
/// answers, plus the space they were asked in).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Hit {
    pub space: String,
    pub location: String,
    pub location_name: String,
    pub device: String,
    pub device_name: String,
    pub online: bool,
    pub path: String,
    pub name: String,
    pub dir: bool,
    pub size: i64,
    pub mtime: i64,
    pub sampled: String,
    /// The full hash of the copy the node keeps, when it keeps one.
    pub available: Option<String>,
    /// How many locations hold the same content.
    pub copies: i64,
}

impl Hit {
    /// A Drive tree file as a hit.
    pub fn tree(space: &str, path: &str, size: i64, hash: &str) -> Hit {
        Hit {
            space: space.into(),
            location: TREE.into(),
            location_name: "Drive".into(),
            device_name: "Drive".into(),
            online: true,
            path: path.trim_matches('/').into(),
            name: path.rsplit('/').next().unwrap_or(path).into(),
            size,
            available: (!hash.is_empty()).then(|| hash.to_string()),
            copies: 1,
            ..Default::default()
        }
    }

    pub fn target(&self) -> String {
        target(&self.location, &self.path)
    }
}

/// Text folded for matching: lower case, letters and digits, single spaces.
pub fn fold(s: &str) -> String {
    words(s).join(" ")
}

/// The ways a title is written: as it is, without a parenthesis
/// (`Goodness of God (RK version)`), without a leading "The".
fn title_forms(title: &str) -> Vec<String> {
    let mut v = vec![fold(title)];
    if let Some((before, _)) = title.split_once('(') {
        v.push(fold(before));
    }
    if let Some((before, _)) = title.split_once(" - ") {
        v.push(fold(before));
    }
    let f = fold(title);
    if let Some(rest) = f.strip_prefix("the ") {
        v.push(rest.to_string());
    }
    v.retain(|x| !x.is_empty());
    v.dedup();
    v
}

/// What to ask the node's name index, for a song: its title (and its
/// shorter forms) and its aliases. The index matches every word of a
/// query in a name, so short queries find more.
pub fn queries(title: &str, aliases: &[String]) -> Vec<String> {
    let mut v: Vec<String> = title_forms(title);
    for a in aliases {
        v.extend(title_forms(a));
    }
    // Leave out words too common to narrow anything.
    v = v
        .into_iter()
        .map(|q| {
            let ws: Vec<&str> = q.split(' ').collect();
            if ws.len() > 1 {
                ws.into_iter()
                    .filter(|w| !matches!(*w, "the" | "a" | "of" | "and" | "is"))
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                q
            }
        })
        .filter(|q| !q.is_empty())
        .collect();
    let mut seen = BTreeSet::new();
    v.retain(|q| seen.insert(q.clone()));
    v
}

/// A match proposed for linking.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub hit: Hit,
    /// The same file or folder in other places (mirrors, backups): one
    /// proposal, its other copies here.
    pub also: Vec<Hit>,
    pub kind: FileKind,
    pub score: i32,
    /// Why it was proposed, in a few words.
    pub why: String,
}

const STALE: &[&str] = &[
    "copy of", "backup", "backups", "bak", "old", "archive", "trash", "autosave", "z inbox",
    "inbox", "unused",
];

fn score(hit: &Hit, forms: &[String], artist: &str) -> (i32, String) {
    let name = fold(&hit.name);
    let stem = fold(
        hit.name
            .rsplit_once('.')
            .map_or(hit.name.as_str(), |(s, _)| s),
    );
    let path = fold(&hit.path);
    let a = fold(artist);
    let mut best = (0, String::new());
    for f in forms {
        let fw: Vec<&str> = f.split(' ').collect();
        let (s, why) = if stem == *f || name == *f {
            (100, "named after the song")
        } else if !a.is_empty() && stem.starts_with(f.as_str()) && stem.contains(&a) {
            (100, "named after the song and its artist")
        } else if stem.starts_with(f.as_str()) || name.starts_with(f.as_str()) {
            (80, "its name starts with the title")
        } else if name.contains(f.as_str()) {
            (60, "its name has the title")
        } else if fw.iter().all(|w| name.split(' ').any(|n| n == *w)) {
            (40, "its name has the title's words")
        } else if path.contains(f.as_str()) {
            (25, "in a folder named after the song")
        } else {
            (0, "")
        };
        if s > best.0 {
            best = (s, why.to_string());
        }
    }
    if best.0 == 0 {
        return best;
    }
    let mut s = best.0;
    if !a.is_empty() && path.contains(&a) {
        s += 15;
        best.1.push_str(", with the artist");
    }
    if hit.dir {
        s += 10;
    }
    for st in STALE {
        if path
            .split(' ')
            .collect::<Vec<_>>()
            .windows(st.split(' ').count())
            .any(|w| w.join(" ") == *st)
        {
            s -= 20;
            break;
        }
    }
    let depth = hit.path.matches('/').count() as i32;
    if depth > 4 {
        s -= (depth - 4) * 3;
    }
    if recognise(&hit.path, hit.dir) != FileKind::Other {
        s += 5;
    }
    (s, best.1)
}

/// Whether `path` is inside folder `dir` (both in one location).
fn inside(dir: &str, path: &str) -> bool {
    path.len() > dir.len() && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'/'
}

/// Propose matches to link, best first: hits that match the song's title
/// or an alias, scored (named after the song, the artist in the path, a
/// whole folder; old copies, backups and deep inbox folders lower). The
/// same thing in several places (a mirror, a second copy) is one proposal
/// with its copies in `also`; something already linked, or inside a
/// linked folder or a better proposed folder, is left out.
pub fn propose(
    title: &str,
    artist: Option<&str>,
    aliases: &[String],
    hits: &[Hit],
    linked: &[Link],
) -> Vec<Proposal> {
    let mut forms = title_forms(title);
    for a in aliases {
        forms.extend(title_forms(a));
    }
    let artist = artist.unwrap_or("");
    let mut scored: Vec<(i32, String, &Hit)> = hits
        .iter()
        .filter(|h| {
            !linked
                .iter()
                .any(|l| l.covers(&h.space, &h.location, &h.path))
        })
        .filter_map(|h| {
            let (s, why) = score(h, &forms, artist);
            (s > 0).then_some((s, why, h))
        })
        .collect();
    // What a matching folder holds comes with it: leave it out.
    let folders: Vec<(String, String, String)> = scored
        .iter()
        .filter(|(_, _, h)| h.dir)
        .map(|(_, _, h)| (h.space.clone(), h.location.clone(), h.path.clone()))
        .collect();
    scored.retain(|(_, _, h)| {
        !folders
            .iter()
            .any(|(s, l, p)| *s == h.space && *l == h.location && inside(p, &h.path))
    });
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.2.online.cmp(&a.2.online))
            .then(a.2.path.len().cmp(&b.2.path.len()))
    });
    let mut out: Vec<Proposal> = Vec::new();
    for (score, why, h) in scored {
        // A copy of one already proposed: the same name, kind of entry
        // and size, and the same last folders.
        let tail = |x: &Hit| {
            let parts: Vec<&str> = x.path.rsplit('/').take(3).collect();
            parts.join("/")
        };
        if let Some(p) = out.iter_mut().find(|p| {
            p.hit.dir == h.dir
                && p.hit.name == h.name
                && (h.dir || p.hit.size == h.size)
                && (tail(&p.hit) == tail(h)
                    || (!h.dir && !h.sampled.is_empty() && p.hit.sampled == h.sampled))
        }) {
            p.also.push(h.clone());
            continue;
        }
        // Inside a folder already proposed.
        if out.iter().any(|p| {
            p.hit.dir
                && p.hit.space == h.space
                && p.hit.location == h.location
                && inside(&p.hit.path, &h.path)
        }) {
            continue;
        }
        out.push(Proposal {
            hit: h.clone(),
            also: Vec::new(),
            kind: recognise(&h.path, h.dir),
            score,
            why,
        });
    }
    out.truncate(40);
    out
}

/// A search typed in the find box, in Drive's search language
/// (docs/tags.md): what to ask the node, and the search to check its
/// answers against.
pub struct Typed {
    /// Words for `/search?q=` (one request per entry).
    pub queries: Vec<String>,
    /// Extensions for `/search?exts=`, from `kind:`.
    pub exts: Vec<String>,
    pub search: drive_spine::tags::query::Search,
    /// Whether it is plain words (rank by the song) or the language.
    pub plain: bool,
}

/// Parse a typed search. `Err` says what is wrong with it.
pub fn typed(q: &str) -> Result<Option<Typed>, String> {
    use drive_spine::tags::query::{Search, kind_extensions};
    let lib = drive_spine::tags::Library::load(&LoroDoc::new());
    let Some(search) = Search::new(q, &lib).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let h = search.hints();
    let kinds = drive_spine::kinds::Kinds::from(Vec::new());
    let exts: Vec<String> = h
        .kinds
        .iter()
        .flat_map(|k| kind_extensions(&kinds, k))
        .map(|e| e.trim_start_matches('.').to_string())
        .collect();
    let plain = search.is_plain();
    let queries = if h.words.is_empty() {
        Vec::new()
    } else {
        // All the words at once (the index wants every word in the name);
        // and each alone when the language joined them some other way.
        let mut v = vec![h.words.join(" ")];
        if !plain {
            v.extend(h.words.iter().cloned());
        }
        v.dedup();
        v
    };
    Ok(Some(Typed {
        queries,
        exts,
        search,
        plain,
    }))
}

impl Typed {
    /// Whether a hit is what the search asks for.
    pub fn matches(&self, h: &Hit) -> bool {
        let kinds = drive_spine::kinds::Kinds::from(Vec::new());
        let k = kinds.of_file(&h.name, h.dir);
        let words = drive_spine::tags::query::kind_words(&kinds, k);
        let on = drive_spine::tags::OnTarget::default();
        self.search.matches(&drive_spine::tags::query::Subject {
            name: &h.name,
            path: &h.path,
            kinds: &words,
            on: &on,
        })
    }
}

// ── availability and readiness ──────────────────────────────────────────

/// One place a file is.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Copy {
    pub device_name: String,
    pub location_name: String,
    pub online: bool,
}

/// What Studio knows of one file of a song, to say whether it can be had.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FileState {
    pub target: String,
    pub name: String,
    pub size: i64,
    pub kind: Option<FileKind>,
    /// Every location holding it (it own included).
    pub copies: Vec<Copy>,
    /// The node keeps a copy (Drive tree files, files kept available).
    pub on_node: bool,
    /// Whether this device has it (`drive-local`); None where nobody can
    /// say (a browser).
    pub here: Option<bool>,
    /// Found in no index at all (moved, renamed, or its location gone).
    pub missing: bool,
}

/// How far a file, a song or a set is from being played, worst first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ready {
    /// Linked, but no index has it any more.
    Missing,
    /// Only on computers that are off.
    Offline,
    /// A computer that has it is on now.
    Reachable,
    /// The node keeps it: reachable whatever is switched off.
    Kept,
    /// On this device (warmed, or its own file).
    Here,
}

impl Ready {
    pub fn label(self) -> &'static str {
        match self {
            Ready::Missing => "Missing",
            Ready::Offline => "Computer off",
            Ready::Reachable => "Reachable",
            Ready::Kept => "Kept on Drive",
            Ready::Here => "On this device",
        }
    }
}

pub fn file_ready(f: &FileState) -> Ready {
    if f.here == Some(true) {
        Ready::Here
    } else if f.missing && !f.on_node {
        Ready::Missing
    } else if f.on_node {
        Ready::Kept
    } else if f.copies.iter().any(|c| c.online) {
        Ready::Reachable
    } else {
        Ready::Offline
    }
}

/// A song's readiness.
#[derive(Clone, Debug, PartialEq)]
pub struct SongReady {
    /// The worst of its files; None when it has no files.
    pub state: Option<Ready>,
    pub files: usize,
    pub bytes: i64,
    /// Files not yet on this device (or, where that cannot be known, not
    /// kept on the node), and their bytes: what warming would bring.
    pub to_warm: usize,
    pub bytes_to_warm: i64,
    /// The computers that must be switched on for what is only there.
    pub needs: Vec<String>,
    /// Kinds a set wants that the song has none of.
    pub lacks: Vec<FileKind>,
}

/// What a set wants of each song: something to follow (a chart), and
/// something to play along to when it plays to tracks.
pub fn wanted(tracks: bool) -> Vec<FileKind> {
    let mut v = vec![FileKind::ChartPdf];
    if tracks {
        v.push(FileKind::Click);
    }
    v
}

/// A song's readiness from its files' states. `want`: kinds the set
/// wants (a chart PDF is satisfied by a keyflow chart, a click by stems
/// or a backing track that carries one).
pub fn song_ready(files: &[FileState], want: &[FileKind]) -> SongReady {
    let state = files.iter().map(file_ready).min();
    let warmable = |f: &&FileState| match f.here {
        Some(h) => !h,
        None => !f.on_node,
    };
    let mut needs: Vec<String> = files
        .iter()
        .filter(|f| file_ready(f) == Ready::Offline)
        .flat_map(|f| f.copies.iter().map(|c| c.device_name.clone()))
        .collect();
    needs.sort();
    needs.dedup();
    let has = |k: FileKind| files.iter().any(|f| f.kind == Some(k));
    let lacks = want
        .iter()
        .copied()
        .filter(|k| match k {
            FileKind::ChartPdf | FileKind::Keyflow => {
                !has(FileKind::ChartPdf) && !has(FileKind::Keyflow)
            }
            FileKind::Click => {
                !has(FileKind::Click) && !has(FileKind::Stems) && !has(FileKind::Backing)
            }
            k => !has(*k),
        })
        .collect();
    SongReady {
        state,
        files: files.len(),
        bytes: files.iter().map(|f| f.size).sum(),
        to_warm: files.iter().filter(warmable).count(),
        bytes_to_warm: files.iter().filter(warmable).map(|f| f.size).sum(),
        needs,
        lacks,
    }
}

/// A set's readiness: its songs'.
#[derive(Clone, Debug, PartialEq)]
pub struct SetReady {
    pub songs: usize,
    /// Songs with no files linked.
    pub without_files: usize,
    /// The worst of the songs that have files.
    pub state: Option<Ready>,
    /// Songs whose every file is at least `Kept` (or `Here`): ready
    /// whatever is switched off.
    pub ready_offline: usize,
    pub to_warm: usize,
    pub bytes_to_warm: i64,
    pub needs: Vec<String>,
}

pub fn set_ready(songs: &[SongReady]) -> SetReady {
    let mut needs: Vec<String> = songs.iter().flat_map(|s| s.needs.clone()).collect();
    needs.sort();
    needs.dedup();
    SetReady {
        songs: songs.len(),
        without_files: songs.iter().filter(|s| s.files == 0).count(),
        state: songs.iter().filter_map(|s| s.state).min(),
        ready_offline: songs
            .iter()
            .filter(|s| s.state.is_some_and(|r| r >= Ready::Kept))
            .count(),
        to_warm: songs.iter().map(|s| s.to_warm).sum(),
        bytes_to_warm: songs.iter().map(|s| s.bytes_to_warm).sum(),
        needs,
    }
}

/// `1 file`, `3 files`.
pub fn count(n: usize) -> String {
    if n == 1 {
        "1 file".into()
    } else {
        format!("{n} files")
    }
}

/// Bytes, as people read them.
pub fn bytes(n: i64) -> String {
    let n = n.max(0) as f64;
    if n >= 1e9 {
        format!("{:.1} GB", n / 1e9)
    } else if n >= 1e6 {
        format!("{:.0} MB", n / 1e6)
    } else if n >= 1e3 {
        format!("{:.0} KB", n / 1e3)
    } else {
        format!("{n} B")
    }
}

#[cfg(test)]
mod tests;
