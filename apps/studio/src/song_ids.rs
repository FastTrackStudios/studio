//! A song's folder from its id (the UUID in its session file's header:
//! `<Title>.session`, or an older `.fts-session`),
//! wherever the folder is: a setlist names songs by id, so it opens on
//! every device and from an external drive. Looked for, in order:
//!
//! 1. under the setlist's own folder, and in `Songs/` beside that folder
//!    (`<drive>/Setlists/x.setlist` finds `<drive>/Songs/*`), so a drive
//!    carried to another computer opens with its songs;
//! 2. in the home's `Songs/` (`crate::storage`);
//! 3. in Drive's index, when Drive is there (`crate::drive_sets`, async);
//! 4. an older line's path, as it was written.
//!
//! What a scan finds is kept in memory (id → folder), checked before use
//! and found again when stale: it is a cache, never the truth.

// A browser has no disk: what reads and writes it is the native build's.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How deep a scan looks below where it starts (`Songs/<song>/<Arr>/…`).
const DEPTH: usize = 4;

struct Cache {
    found: HashMap<String, PathBuf>,
    scanned: HashSet<PathBuf>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The folder of the song with session id `id`, for the setlist file at
/// `setlist` (a path on disk), searching the disk (steps 1 and 2).
pub fn resolve(id: &str, setlist: &Path) -> Option<PathBuf> {
    let roots = roots(setlist);
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(|| Cache {
        found: HashMap::new(),
        scanned: HashSet::new(),
    });
    if let Some(p) = cache.found.get(id)
        && holds(p, id)
    {
        return Some(p.clone());
    }
    // Each root once, then again only when the id is still missing (a
    // folder copied in since).
    for again in [false, true] {
        for r in &roots {
            if !again && cache.scanned.contains(r) {
                continue;
            }
            scan(r, DEPTH, &mut cache.found);
            cache.scanned.insert(r.clone());
            if let Some(p) = cache.found.get(id)
                && holds(p, id)
            {
                return Some(p.clone());
            }
        }
    }
    None
}

/// Where to look for a setlist's songs, in order.
fn roots(setlist: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(dir) = setlist.parent() {
        v.push(dir.to_path_buf());
        if let Some(up) = dir.parent() {
            v.push(up.join("Songs"));
        }
    }
    if let Some(home) = crate::storage::home() {
        v.push(home.join("Songs"));
    }
    let mut seen = HashSet::new();
    v.retain(|p| p.is_dir() && seen.insert(p.clone()));
    v
}

/// Whether `folder` still has the session with this id.
fn holds(folder: &Path, id: &str) -> bool {
    sessions(folder).any(|f| session_id(&f).as_deref() == Some(id))
}

/// The session files in a folder (`.session`, `.fts-session`; only those
/// that are Architect documents have an id).
pub fn sessions(folder: &Path) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_session(p) && p.is_file())
}

fn is_session(p: &Path) -> bool {
    p.extension()
        .is_some_and(|x| x == "fts-session" || x == "session")
}

/// A session file's id, from its first bytes.
pub fn session_id(file: &Path) -> Option<String> {
    use std::io::Read as _;
    let mut head = vec![0; 4096];
    let n = std::fs::File::open(file).ok()?.read(&mut head).ok()?;
    id(&head[..n])
}

/// The id in an Architect document's header (`ARCHDOC\0`, a `u32`
/// format, then framed records: a `u32` length, the payload, a CRC): the
/// meta record (kind 1) `id` → a UUID. Only the first records are read.
pub fn id(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 12 || &bytes[..8] != b"ARCHDOC\0" {
        return None;
    }
    let u32_at = |at: usize| -> Option<usize> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    let mut at = 12;
    for _ in 0..4 {
        let len = u32_at(at)?;
        let p = bytes.get(at + 4..at + 4 + len)?;
        if p.first() == Some(&1) {
            let field = |from: usize| -> Option<(&[u8], usize)> {
                let n = u32::from_le_bytes(p.get(from..from + 4)?.try_into().ok()?) as usize;
                Some((p.get(from + 4..from + 4 + n)?, from + 4 + n))
            };
            if let Some((k, next)) = field(1)
                && k == b"id"
                && let Some((v, _)) = field(next)
                && let Ok(v) = std::str::from_utf8(v)
                && !v.is_empty()
            {
                return Some(v.to_string());
            }
        }
        at += 4 + len + 4;
    }
    None
}

fn scan(dir: &Path, depth: usize, found: &mut HashMap<String, PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            // A song's media and samples hold no sessions.
            let name = e.file_name();
            let skip = matches!(name.to_str(), Some("Media" | "Samples" | ".git"));
            if depth > 0 && !skip {
                scan(&p, depth - 1, found);
            }
        } else if is_session(&p)
            && let Some(id) = session_id(&p)
        {
            found.insert(id, dir.to_path_buf());
        }
    }
}

/// The id of the session in the home's `Songs/` named `title` (its
/// `<Title>.fts-session`), for a song Studio writes into a set.
pub fn by_title(title: &str) -> Option<String> {
    let songs = crate::storage::home()?.join("Songs");
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(|| Cache {
        found: HashMap::new(),
        scanned: HashSet::new(),
    });
    if songs.is_dir() && cache.scanned.insert(songs.clone()) {
        scan(&songs, DEPTH, &mut cache.found);
    }
    cache.found.iter().find_map(|(id, folder)| {
        sessions(folder)
            .any(|f| {
                f.file_stem()
                    .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(title))
                    && session_id(&f).as_deref() == Some(id.as_str())
            })
            .then(|| id.clone())
    })
}

/// The folder a song line names on disk: an id found by a scan, an older
/// path line as written (relative to the setlist), or none (`= Title`, an
/// id found nowhere here).
pub fn folder_of(line: &str, setlist: &Path) -> Option<PathBuf> {
    match crate::setfile::song(line) {
        crate::setfile::Song::Id { id, hint } => resolve(id, setlist).or_else(|| {
            // An id line whose hint is a path (written while moving in).
            hint.contains('/')
                .then(|| PathBuf::from(crate::setfile::folder(&setlist.to_string_lossy(), hint)))
                .filter(|p| holds(p, id))
        }),
        crate::setfile::Song::Path(p) => Some(PathBuf::from(crate::setfile::folder(
            &setlist.to_string_lossy(),
            p,
        ))),
        crate::setfile::Song::Title(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session file with just its header (`ARCHDOC`, the id record).
    pub(crate) fn session(id: &str) -> Vec<u8> {
        let mut payload = vec![1u8];
        for s in ["id", id] {
            payload.extend_from_slice(&(s.len() as u32).to_le_bytes());
            payload.extend_from_slice(s.as_bytes());
        }
        let mut out = b"ARCHDOC\0".to_vec();
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("studio-song-ids-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const A: &str = "45b794d1-d6ba-4d66-ab2d-6be35df2f6bc";
    const B: &str = "df5b7031-b10a-44db-ac67-45af7bdcb8d6";

    #[test]
    fn a_setlist_on_an_external_drive_finds_the_songs_beside_it() {
        // /Volumes/Gig/Setlists/Show.setlist, /Volumes/Gig/Songs/…
        let drive = temp("ext");
        let song = drive.join("Songs").join("Drive To LA - JT");
        std::fs::create_dir_all(&song).unwrap();
        std::fs::write(song.join("Drive To LA.session"), session(A)).unwrap();
        // One kept under the set's own folder too.
        let own = drive.join("Setlists").join("Show").join("extra");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("Intro.fts-session"), session(B)).unwrap();
        let set = drive.join("Setlists").join("Show.setlist");
        std::fs::write(&set, format!("# id: {A}\n{A}  Drive To LA\n{B}  Intro\n")).unwrap();

        assert_eq!(
            folder_of(&format!("{A}  Drive To LA"), &set),
            Some(song.clone())
        );
        assert_eq!(folder_of(&format!("{B}  Intro"), &set), Some(own));
        assert_eq!(
            folder_of("00000000-0000-0000-0000-000000000000  Gone", &set),
            None
        );
        assert_eq!(folder_of("= Amazing Grace", &set), None);

        // Moved: found again, not the stale folder.
        let moved = drive.join("Songs").join("dtla");
        std::fs::rename(&song, &moved).unwrap();
        assert_eq!(resolve(A, &set), Some(moved));
        let _ = std::fs::remove_dir_all(&drive);
    }

    #[test]
    fn older_path_lines_still_work() {
        let set = Path::new("/media/Gig/Setlists/Show.setlist");
        assert_eq!(
            folder_of("../Songs/know-me", set),
            Some(PathBuf::from("/media/Gig/Songs/know-me"))
        );
        assert_eq!(
            folder_of("/run/media/vaults/recital-songs/drive-to-la/", set),
            Some(PathBuf::from("/run/media/vaults/recital-songs/drive-to-la"))
        );
    }

    #[test]
    fn the_header_id_is_read() {
        assert_eq!(id(&session(A)).as_deref(), Some(A));
        assert_eq!(id(b"not a session"), None);
    }
}
