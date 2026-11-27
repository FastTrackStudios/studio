//! A song's session file (`<Song>.fts-session`), read for its sections.
//!
//! The session primitive's file is an Architect document file: a header
//! (`ARCHDOC\0`, a `u32` format), then framed records (`u32` length, the
//! payload, a `u32` CRC-32). Its `main` branch is a Loro document: the
//! newest snapshot, then the steps after it. The session's shape inside
//! (FastTrackStudios/session, `session::doc`): `regions` and `markers`
//! maps whose times are `["s", seconds]` or `["qn", quarter notes]`.
//!
//! Only reading, and only what the player shows: the regions (the song's
//! sections) and the markers. The engine opens the song itself.
//!
//! A song converted to one file (`<Song>.session`, a SQLite store) keeps
//! the same document as its store's `session` part: [`main_doc`] reads
//! either, told apart by the bytes' header, never the file's name
//! (`session_format::read`, pure Rust, so the browser reads it too). A
//! store must be whole (its log folded in), which a closed song is.

use loro::{LoroDoc, LoroValue};

const MAGIC: &[u8; 8] = b"ARCHDOC\0";
const HEADER_LEN: usize = 12;
const STEP: u8 = 5;
const SNAPSHOT: u8 = 6;
const MAIN: &str = "main";

/// A stretch of the song with a name (a region of the session).
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    /// Seconds from the song's start.
    pub start: f64,
    pub end: f64,
}

/// What the player shows of a session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {
    /// In order of their start.
    pub sections: Vec<Section>,
    /// Named points (seconds, name), in order.
    pub markers: Vec<(f64, String)>,
}

impl Timeline {
    /// The section playing at `t` seconds.
    pub fn at(&self, t: f64) -> Option<&Section> {
        self.sections.iter().rfind(|s| s.start <= t && t < s.end)
    }
}

/// A session file's timeline, read from its bytes. `bpm` turns quarter
/// notes into seconds when the session keeps its times in them.
pub fn read(bytes: &[u8], bpm: f64) -> Result<Timeline, String> {
    let doc = main_doc(bytes)?;
    Ok(timeline(&doc, bpm))
}

/// The song's id in a one-file song's bytes (`meta.id`); `None` for
/// anything else (the old pair's id is in its `.fts-session` header).
pub fn store_id(bytes: &[u8]) -> Option<String> {
    if session_format::sniff(bytes) != session_format::FileFormat::Store {
        return None;
    }
    session_format::read::ProjectFile::read(bytes)
        .ok()?
        .song_id()
        .map(str::to_string)
}

/// Whether `bytes` are a song's one file (a store), not the old pair's
/// session document.
pub fn is_store(bytes: &[u8]) -> bool {
    session_format::sniff(bytes) == session_format::FileFormat::Store
}

/// The document of the file's `main` branch: the old pair's session
/// document, or the `session` part of a one-file song.
pub fn main_doc(bytes: &[u8]) -> Result<LoroDoc, String> {
    if is_store(bytes) {
        let file = session_format::read::ProjectFile::read(bytes).map_err(|e| e.to_string())?;
        // An engine store of a folder not converted yet holds no session:
        // its `.fts-session` beside it does.
        if !file.has_part(session_format::read::SESSION, MAIN) {
            return Err("this store holds no session (its folder is not converted)".into());
        }
        return file
            .part(session_format::read::SESSION, MAIN)
            .map_err(|e| e.to_string());
    }
    if bytes.len() < HEADER_LEN || &bytes[..8] != MAGIC {
        return Err("not a session file".into());
    }
    let mut snapshot: Option<(u64, &[u8])> = None;
    let mut steps: Vec<(u64, &[u8])> = Vec::new();
    let mut at = HEADER_LEN;
    while at + 4 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap_or_default()) as usize;
        let Some(end) = at.checked_add(4 + len + 4).filter(|e| *e <= bytes.len()) else {
            break; // a torn write: everything before it stands
        };
        let payload = &bytes[at + 4..at + 4 + len];
        let mut r = Reader { b: payload, at: 0 };
        match r.u8() {
            Some(STEP) => {
                let (Some(seq), Some(branch), Some(_label), Some(_at), Some(_front), Some(data)) =
                    (r.u64(), r.str(), r.str(), r.i64(), r.bytes(), r.bytes())
                else {
                    break;
                };
                if branch == MAIN && !data.is_empty() {
                    steps.push((seq, data));
                }
            }
            Some(SNAPSHOT) => {
                let (Some(branch), Some(upto), Some(_at), Some(data)) =
                    (r.str(), r.u64(), r.i64(), r.bytes())
                else {
                    break;
                };
                if branch == MAIN && snapshot.is_none_or(|(u, _)| upto >= u) {
                    snapshot = Some((upto, data));
                }
            }
            Some(_) => {}
            None => break,
        }
        at = end;
    }
    let doc = LoroDoc::new();
    let upto = match snapshot {
        Some((upto, data)) => {
            doc.import(data).map_err(|e| e.to_string())?;
            Some(upto)
        }
        None => None,
    };
    steps.sort_by_key(|(seq, _)| *seq);
    for (seq, data) in steps {
        if upto.is_some_and(|u| seq <= u) {
            continue;
        }
        // A step the snapshot already holds imports as a no-op.
        let _ = doc.import(data);
    }
    Ok(doc)
}

/// The sections and markers of a session document.
pub fn timeline(doc: &LoroDoc, bpm: f64) -> Timeline {
    let secs = |v: Option<&LoroValue>| -> Option<f64> {
        let LoroValue::List(l) = v? else { return None };
        let (LoroValue::String(base), n) = (l.first()?, l.get(1)?) else {
            return None;
        };
        let n = match n {
            LoroValue::Double(x) => *x,
            LoroValue::I64(x) => *x as f64,
            _ => return None,
        };
        match base.as_str() {
            "s" => Some(n),
            "qn" if bpm > 0.0 => Some(n * 60.0 / bpm),
            _ => None,
        }
    };
    let name = |v: Option<&LoroValue>| match v {
        Some(LoroValue::String(s)) => s.to_string(),
        _ => String::new(),
    };
    let mut out = Timeline::default();
    if let LoroValue::Map(regions) = doc.get_map("regions").get_deep_value() {
        for v in regions.values() {
            let LoroValue::Map(m) = v else { continue };
            if let (Some(start), Some(end)) = (secs(m.get("start")), secs(m.get("end"))) {
                out.sections.push(Section {
                    name: name(m.get("name")),
                    start,
                    end,
                });
            }
        }
    }
    if let LoroValue::Map(markers) = doc.get_map("markers").get_deep_value() {
        for v in markers.values() {
            let LoroValue::Map(m) = v else { continue };
            if let Some(at) = secs(m.get("at")) {
                out.markers.push((at, name(m.get("name"))));
            }
        }
    }
    out.sections.sort_by(|a, b| a.start.total_cmp(&b.start));
    // The song's own region (it holds every other one) is not a section.
    let whole = |w: &Section, all: &[Section]| {
        all.len() > 1
            && all
                .iter()
                .all(|s| std::ptr::eq(s, w) || (w.start <= s.start && s.end <= w.end))
    };
    let all = out.sections.clone();
    out.sections
        .retain(|w| !all.iter().any(|a| a == w && whole(a, &all)));
    out.markers.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// A record's fields: integers little-endian, strings and byte strings a
/// `u32` length then the bytes.
struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u64(&mut self) -> Option<u64> {
        self.take(8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or_default()))
    }
    fn i64(&mut self) -> Option<i64> {
        self.take(8)
            .map(|b| i64::from_le_bytes(b.try_into().unwrap_or_default()))
    }
    fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize;
        self.take(n)
    }
    fn str(&mut self) -> Option<&'a str> {
        std::str::from_utf8(self.bytes()?).ok()
    }
}

/// The file the engine opens for the song in `folder`: its one file (a
/// store) when it has one, else the first session file there.
#[cfg(not(target_arch = "wasm32"))]
pub fn song_file(folder: &str) -> Option<std::path::PathBuf> {
    use std::io::Read as _;
    let dir = crate::storage::resolve(folder)?;
    let mut all: Vec<_> = crate::song_ids::sessions(&dir).collect();
    all.sort();
    let store = all.iter().find(|f| {
        let mut head = [0u8; 16];
        std::fs::File::open(f)
            .and_then(|mut h| h.read_exact(&mut head))
            .is_ok()
            && is_store(&head)
    });
    store.cloned().or_else(|| all.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(payload: &[u8]) -> Vec<u8> {
        let mut out = (payload.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(payload);
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    fn put(out: &mut Vec<u8>, b: &[u8]) {
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    }

    fn region(doc: &LoroDoc, id: &str, name: &str, start: LoroValue, end: LoroValue) {
        let m = doc
            .get_map("regions")
            .insert_container(id, loro::LoroMap::new())
            .unwrap();
        m.insert("start", start).unwrap();
        m.insert("end", end).unwrap();
        m.insert("name", name).unwrap();
    }

    fn time(base: &str, n: f64) -> LoroValue {
        LoroValue::List(vec![LoroValue::from(base), LoroValue::Double(n)].into())
    }

    /// A file of a snapshot and a step after it, as a session's document
    /// file writes them.
    fn file() -> Vec<u8> {
        let doc = LoroDoc::new();
        region(&doc, "b", "Chorus", time("s", 30.0), time("s", 60.0));
        region(&doc, "song", "The Song", time("s", 0.0), time("s", 61.0));
        doc.commit();
        let snap = doc.export(loro::ExportMode::Snapshot).unwrap();
        let v = doc.oplog_vv();
        region(&doc, "a", "Verse", time("qn", 0.0), time("qn", 60.0));
        doc.commit();
        let step = doc.export(loro::ExportMode::updates(&v)).unwrap();

        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&1u32.to_le_bytes());
        let mut s = vec![SNAPSHOT];
        put(&mut s, b"main");
        s.extend_from_slice(&1u64.to_le_bytes());
        s.extend_from_slice(&0i64.to_le_bytes());
        put(&mut s, &snap);
        out.extend(frame(&s));
        let mut p = vec![STEP];
        p.extend_from_slice(&2u64.to_le_bytes());
        put(&mut p, b"main");
        put(&mut p, b"edit");
        p.extend_from_slice(&0i64.to_le_bytes());
        put(&mut p, b"");
        put(&mut p, &step);
        out.extend(frame(&p));
        // A torn write at the end is dropped.
        out.extend_from_slice(&[9, 9, 9]);
        out
    }

    #[test]
    fn sections_come_in_order_in_seconds() {
        let t = read(&file(), 120.0).unwrap();
        let names: Vec<_> = t
            .sections
            .iter()
            .map(|s| (s.name.as_str(), s.start, s.end))
            .collect();
        assert_eq!(names, vec![("Verse", 0.0, 30.0), ("Chorus", 30.0, 60.0)]);
        assert_eq!(t.at(45.0).map(|s| s.name.as_str()), Some("Chorus"));
        assert_eq!(t.at(61.0), None);
    }

    #[test]
    fn other_files_are_refused() {
        assert!(read(b"OggS not a session", 120.0).is_err());
    }

    /// A real session file's sections: `STUDIO_SESSION_FILE=… cargo test
    /// -p studio-app -- --ignored real_session_file --nocapture`.
    #[test]
    #[ignore]
    fn real_session_file() {
        let path = std::env::var("STUDIO_SESSION_FILE").expect("STUDIO_SESSION_FILE");
        let t = read(&std::fs::read(path).unwrap(), 120.0).unwrap();
        assert!(!t.sections.is_empty() || !t.markers.is_empty(), "{t:?}");
        for s in &t.sections {
            assert!(s.end > s.start, "{s:?}");
        }
        std::fs::write(
            std::env::temp_dir().join("studio-session-file.txt"),
            format!("{t:#?}"),
        )
        .unwrap();
    }
}
