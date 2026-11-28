//! Records: what is made in the studio, an album or a single. A record is
//! a setlist file of kind studio (`# kind: studio`), its songs the
//! record's tracks in album order. It is found where `docs/storage.md`
//! keeps production work, `<home>/Projects/<Name>/<Name>.setlist`, and in
//! `<home>/Setlists/` (where a record's file was kept before), never
//! listed as a setlist too.
//!
//! A track is a song line of the file: a session by its id (found the
//! way a setlist's songs are, `crate::song_ids`), a session folder by its
//! path (an older line, kept until the song's folder is in Drive), or
//! `= Title`, a song with no session yet.

// A browser reads the files Drive found; the disk is the native build's.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use crate::files::Hit;
use crate::setfile::{self, Kind};

/// A record, from its file.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// Its file's own id (`# id:`), else its name: what its page is at.
    pub id: String,
    pub title: String,
    /// The org it belongs to (`# org:`, else the org Studio is in).
    pub org: String,
    /// The file, where it is.
    pub file: Hit,
    pub tracks: Vec<Track>,
}

/// One track of a record, in album order.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub title: String,
    /// The part of the record it is in (the comment above it), if any.
    pub part: Option<String>,
    pub state: TrackState,
}

/// What a track has, so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackState {
    /// It has a session (named by its id, or its folder holds one).
    Session,
    /// `= Title`: no session yet.
    NoSession,
    /// Its line names a folder that is not on this computer.
    Missing,
}

impl TrackState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::NoSession => "No session yet",
            Self::Missing => "Not on this computer",
        }
    }
    pub fn slug(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::NoSession => "no-session",
            Self::Missing => "missing",
        }
    }
}

impl Record {
    /// How many of its tracks have a session.
    pub fn with_sessions(&self) -> usize {
        self.tracks
            .iter()
            .filter(|t| t.state == TrackState::Session)
            .count()
    }
}

/// A file's record, when it is one (`# kind: studio`): its tracks as
/// the file lists them. `has_session` says whether a path line's folder
/// holds a session (on this computer; `None`: not looked at, so taken as
/// one).
pub fn read(
    file: &Hit,
    text: &str,
    here: &str,
    has_session: impl Fn(&str) -> Option<bool>,
) -> Option<Record> {
    let parsed = setfile::parse(text);
    if parsed.kind != Kind::Studio {
        return None;
    }
    let title = setfile::name_of(&file.name);
    let mut tracks = Vec::new();
    for act in &parsed.acts {
        for line in &act.songs {
            let (title, state) = match setfile::song(line) {
                setfile::Song::Title(t) => (t.to_string(), TrackState::NoSession),
                setfile::Song::Id { id, hint } => (
                    if hint.is_empty() { id } else { hint }.to_string(),
                    TrackState::Session,
                ),
                setfile::Song::Path(p) => {
                    let folder = setfile::folder(&file.path, p);
                    let name = folder.rsplit('/').next().unwrap_or(&folder).trim();
                    let state = match has_session(&folder) {
                        Some(false) => TrackState::Missing,
                        _ => TrackState::Session,
                    };
                    (name.to_string(), state)
                }
            };
            tracks.push(Track {
                title,
                part: act.name.clone(),
                state,
            });
        }
    }
    Some(Record {
        id: parsed.id.clone().unwrap_or_else(|| title.clone()),
        title,
        org: parsed.org.unwrap_or_else(|| here.to_string()),
        file: Hit {
            space: setfile::org_of(text, here),
            ..file.clone()
        },
        tracks,
    })
}

/// Whether a file's text is a record's (`# kind: studio`), not a set's.
pub fn is_record(text: &str) -> bool {
    setfile::parse(text).kind == Kind::Studio
}

/// The records in Studio's own folder: `Projects/<Name>/*.setlist` and
/// any record kept in `Setlists/`, each once, by title.
pub fn on_disk(here: &str) -> Vec<Record> {
    use crate::files_live::disk;
    let mut files: Vec<Hit> = disk::list(here, "Setlists");
    for p in disk::list(here, "Projects").into_iter().filter(|h| h.dir) {
        files.extend(disk::list(here, &p.path));
    }
    let mut out: Vec<Record> = files
        .into_iter()
        .filter(|h| !h.dir && h.name.ends_with(".setlist"))
        .filter_map(|h| {
            let text = disk::bytes(&disk::url(&h.path))?.ok()?.1;
            let text = String::from_utf8_lossy(&text).into_owned();
            read(&h, &text, here, folder_has_session)
        })
        .collect();
    // A record moved into Projects/ is the same record as its old file.
    out.sort_by(|a, b| {
        let projects = |r: &Record| !r.file.path.starts_with("Projects/");
        a.title.cmp(&b.title).then(projects(a).cmp(&projects(b)))
    });
    out.dedup_by(|a, b| a.title == b.title);
    out
}

/// Whether a song folder on this computer holds a session.
fn folder_has_session(folder: &str) -> Option<bool> {
    let dir = crate::storage::resolve(folder)?;
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Some(false);
    };
    Some(rd.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        name.ends_with(".session") || name.ends_with(".fts-session")
    }))
}

/// Drive's setlist files split into the setlists and the records, by
/// what each one's text says (`texts`: each file's, in the same order;
/// one not read is a setlist).
pub fn split_drive(files: &[Hit], texts: &[Option<String>], here: &str) -> (Vec<Hit>, Vec<Record>) {
    let mut sets = Vec::new();
    let mut records = Vec::new();
    for (h, text) in files.iter().zip(texts) {
        match text.as_deref().and_then(|t| read(h, t, here, |_| None)) {
            Some(r) => records.push(r),
            None => sets.push(h.clone()),
        }
    }
    (sets, records)
}

/// The records on disk first, then Drive's, each once (by title).
pub fn merge(on_disk: &[Record], from_drive: &[Record]) -> Vec<Record> {
    let mut out = on_disk.to_vec();
    for r in from_drive {
        if !out.iter().any(|o| o.title == r.title) {
            out.push(r.clone());
        }
    }
    out.sort_by(|a, b| a.title.cmp(&b.title));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(path: &str) -> Hit {
        Hit {
            space: "codywright".into(),
            location: crate::files::DISK.into(),
            name: path.rsplit('/').next().unwrap_or(path).into(),
            path: path.into(),
            ..Default::default()
        }
    }

    const ALBUM: &str = "# id: 3f0c6a2e-8b1d-4c57-9e2a-7d4b1f6c9a10\n# kind: studio\n# An album.\n\n= Intro\n= Second\n/somewhere/Third\n";

    #[test]
    fn a_studio_file_is_a_record_with_its_tracks_in_order() {
        let r = read(
            &hit("Projects/Album/Album.setlist"),
            ALBUM,
            "codywright",
            |f| Some(f.ends_with("Third")),
        )
        .expect("a record");
        assert_eq!(r.title, "Album");
        assert_eq!(r.id, "3f0c6a2e-8b1d-4c57-9e2a-7d4b1f6c9a10");
        assert_eq!(r.org, "codywright");
        let t: Vec<(&str, TrackState)> = r
            .tracks
            .iter()
            .map(|t| (t.title.as_str(), t.state))
            .collect();
        assert_eq!(
            t,
            vec![
                ("Intro", TrackState::NoSession),
                ("Second", TrackState::NoSession),
                ("Third", TrackState::Session),
            ]
        );
        assert_eq!(r.with_sessions(), 1);
    }

    #[test]
    fn a_set_is_not_a_record() {
        assert!(
            read(
                &hit("Setlists/Sunday.setlist"),
                "= Amazing Grace\n",
                "x",
                |_| None
            )
            .is_none()
        );
        assert!(!is_record("= Amazing Grace\n"));
        assert!(is_record(ALBUM));
    }

    #[test]
    fn drive_s_files_split_into_sets_and_records() {
        let files = [
            hit("Setlists/Sunday.setlist"),
            hit("Setlists/Album.setlist"),
        ];
        let texts = [
            Some("= Amazing Grace\n".to_string()),
            Some(ALBUM.to_string()),
        ];
        let (sets, records) = split_drive(&files, &texts, "codywright");
        assert_eq!(sets.len(), 1);
        assert_eq!(sets[0].name, "Sunday.setlist");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].tracks.len(), 3);
    }
}
