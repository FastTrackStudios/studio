//! A song's arrangement, read from its one file (`<Title>.session`): the
//! engine project's tracks (folders and their order), each track's items
//! as clips, the song's sections and its tempo — what the arrangement
//! page draws.
//!
//! Only reading. The engine project is the store's `main` branch, a Loro
//! document in the shape `session::sync::SessionDoc` writes (Studio's
//! `features/session`, `sync/doc.rs`): a `tracks` tree (each node's meta
//! holds `guid`, `name`, `color`), an `items` map (guid to `track`,
//! `position`, `length`, `take.name`, `take.midi`, `color`, `muted`), a
//! `regions` map and `meta.tempo` (`at:bpm:beats/unit` per point). The
//! sections are the session part's when it has them (the same
//! [`crate::session_file::timeline`] the player reads), else the engine's
//! regions. Pure Rust over the bytes, so a browser could read it too; the
//! engine opens and plays the song itself.

use std::collections::HashMap;

use loro::{LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId};

use crate::session_file::{self, Section};

/// A song's arrangement: its tracks with their clips, its sections, how
/// long it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrangement {
    /// The tracks in the project's order, folders before what they hold.
    pub tracks: Vec<Lane>,
    pub sections: Vec<Section>,
    /// Named points (seconds, name), in order.
    pub markers: Vec<(f64, String)>,
    /// Seconds: the end of the last clip, section or marker.
    pub len: f64,
    /// The song's first tempo and meter.
    pub bpm: f64,
    pub beats_per_bar: u32,
}

/// A track of the project, as a lane of the arrangement.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lane {
    pub name: String,
    /// How deep in folders (0 at the top).
    pub depth: usize,
    /// Whether other tracks are in it.
    pub folder: bool,
    /// `#rrggbb`, when the track has a colour.
    pub color: Option<String>,
    /// In order of their start.
    pub clips: Vec<Clip>,
    /// Whether it or a track in it has a clip (a lane with nothing in it
    /// is folded away).
    pub has_media: bool,
}

/// An item on a track.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    /// Seconds from the song's start.
    pub start: f64,
    pub len: f64,
    pub name: String,
    /// `#rrggbb`, its own or its track's.
    pub color: Option<String>,
    pub midi: bool,
    pub muted: bool,
}

impl Arrangement {
    /// The lanes worth drawing: those with media, or all of them when none has.
    pub fn shown(&self) -> Vec<&Lane> {
        if self.tracks.iter().any(|t| t.has_media) {
            self.tracks.iter().filter(|t| t.has_media).collect()
        } else {
            self.tracks.iter().collect()
        }
    }

    /// The bar lines to draw: (seconds, bar number from 1), every `every`
    /// bars, from the song's first tempo.
    pub fn bars(&self, every: u32) -> Vec<(f64, u32)> {
        let bar = 60.0 / self.bpm.max(20.0) * f64::from(self.beats_per_bar.max(1));
        let every = every.max(1);
        let mut out = Vec::new();
        let mut n = 0u32;
        while f64::from(n) * bar <= self.len && out.len() < 2000 {
            if n % every == 0 {
                out.push((f64::from(n) * bar, n + 1));
            }
            n += 1;
        }
        out
    }
}

/// A song's file read as its arrangement. `bpm` stands in when the
/// project says no tempo.
pub fn read(bytes: &[u8], bpm: f64) -> Result<Arrangement, String> {
    if !session_file::is_store(bytes) {
        // The old pair's session document: its sections only (the engine
        // project is the `.RPP` or store beside it).
        let doc = session_file::main_doc(bytes)?;
        return Ok(finish(Arrangement {
            sections: session_file::timeline(&doc, bpm).sections,
            bpm,
            beats_per_bar: 4,
            ..Default::default()
        }));
    }
    let file = session_format::read::ProjectFile::read(bytes).map_err(|e| e.to_string())?;
    let engine = file
        .doc(session_format::read::MAIN)
        .map_err(|e| e.to_string())?;
    let mut out = of_engine(&engine, bpm);
    if file.has_part(session_format::read::SESSION, session_format::read::MAIN)
        && let Ok(part) = file.part(session_format::read::SESSION, session_format::read::MAIN)
    {
        let t = session_file::timeline(&part, out.bpm);
        if !t.sections.is_empty() {
            out.sections = t.sections;
        }
        if !t.markers.is_empty() {
            out.markers = t.markers;
        }
    }
    Ok(finish(out))
}

/// The engine project's tracks, clips, regions and tempo.
pub fn of_engine(doc: &LoroDoc, bpm: f64) -> Arrangement {
    let meta = map(&doc.get_map("meta").get_deep_value());
    let tempo = meta
        .get("tempo")
        .and_then(string)
        .and_then(|t| first_tempo(&t));
    let (bpm, beats_per_bar) = tempo.unwrap_or((bpm, 4));

    let mut tracks = Vec::new();
    let mut by_guid: HashMap<String, usize> = HashMap::new();
    let tree = doc.get_tree("tracks");
    let canonical = canonical_nodes(&tree);
    walk(
        &tree,
        &canonical,
        TreeParentId::Root,
        0,
        &mut tracks,
        &mut by_guid,
    );

    if let LoroValue::Map(items) = doc.get_map("items").get_deep_value() {
        for v in items.values() {
            let f = map(v);
            let Some(&lane) = f
                .get("track")
                .and_then(string)
                .and_then(|g| by_guid.get(&g))
            else {
                continue;
            };
            let len = num(f.get("length")).unwrap_or(0.0);
            if len <= 0.0 {
                continue;
            }
            let track_color = tracks[lane].color.clone();
            tracks[lane].clips.push(Clip {
                start: num(f.get("position")).unwrap_or(0.0),
                len,
                name: f.get("take.name").and_then(string).unwrap_or_default(),
                color: color(f.get("color")).or(track_color),
                midi: matches!(f.get("take.midi"), Some(LoroValue::Bool(true))),
                muted: matches!(f.get("muted"), Some(LoroValue::Bool(true))),
            });
        }
    }
    for t in &mut tracks {
        t.clips.sort_by(|a, b| a.start.total_cmp(&b.start));
    }
    // A folder has media when a track in it has: from the end up, each
    // track's media lifts to the folder above it.
    for i in (0..tracks.len()).rev() {
        if !tracks[i].clips.is_empty() {
            tracks[i].has_media = true;
        }
        if tracks[i].has_media
            && let Some(parent) = (0..i)
                .rev()
                .find(|&j| tracks[j].depth + 1 == tracks[i].depth)
        {
            tracks[parent].has_media = true;
        }
    }

    let mut sections = Vec::new();
    if let LoroValue::Map(regions) = doc.get_map("regions").get_deep_value() {
        for v in regions.values() {
            let f = map(v);
            let (Some(start), Some(end)) = (num(f.get("start")), num(f.get("end"))) else {
                continue;
            };
            if end > start {
                sections.push(Section {
                    name: f.get("name").and_then(string).unwrap_or_default(),
                    start,
                    end,
                });
            }
        }
    }
    sections.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut markers = Vec::new();
    if let LoroValue::Map(ms) = doc.get_map("markers").get_deep_value() {
        for v in ms.values() {
            let f = map(v);
            if let Some(at) = num(f.get("at")) {
                markers.push((at, f.get("name").and_then(string).unwrap_or_default()));
            }
        }
    }
    markers.sort_by(|a, b| a.0.total_cmp(&b.0));
    Arrangement {
        tracks,
        sections,
        markers,
        len: 0.0,
        bpm,
        beats_per_bar,
    }
}

/// Its length, from everything in it.
fn finish(mut a: Arrangement) -> Arrangement {
    let clips = a
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter())
        .map(|c| c.start + c.len);
    let sections = a.sections.iter().map(|s| s.end);
    let markers = a.markers.iter().map(|m| m.0);
    a.len = clips
        .chain(sections)
        .chain(markers)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    a
}

/// Each track's node, by guid: the smallest `TreeID` when two replicas
/// made one each (as `SessionDoc` reads it).
fn canonical_nodes(tree: &LoroTree) -> HashMap<String, TreeID> {
    let mut out: HashMap<String, TreeID> = HashMap::new();
    for node in tree.get_nodes(false) {
        let Ok(meta) = tree.get_meta(node.id) else {
            continue;
        };
        let Some(guid) = map(&meta.get_deep_value()).get("guid").and_then(string) else {
            continue;
        };
        let id = node.id;
        out.entry(guid)
            .and_modify(|have| {
                if (id.peer, id.counter) < (have.peer, have.counter) {
                    *have = id;
                }
            })
            .or_insert(id);
    }
    out
}

fn walk(
    tree: &LoroTree,
    canonical: &HashMap<String, TreeID>,
    parent: TreeParentId,
    depth: usize,
    out: &mut Vec<Lane>,
    by_guid: &mut HashMap<String, usize>,
) {
    for id in tree.children(parent).unwrap_or_default() {
        let Ok(meta) = tree.get_meta(id) else {
            continue;
        };
        let f = map(&meta.get_deep_value());
        let Some(guid) = f.get("guid").and_then(string) else {
            continue;
        };
        if canonical.get(&guid) != Some(&id) {
            continue; // a second node for one track
        }
        let at = out.len();
        by_guid.insert(guid, at);
        out.push(Lane {
            name: f.get("name").and_then(string).unwrap_or_default(),
            depth,
            folder: false,
            color: color(f.get("color")),
            clips: Vec::new(),
            has_media: false,
        });
        walk(
            tree,
            canonical,
            TreeParentId::Node(id),
            depth + 1,
            out,
            by_guid,
        );
        out[at].folder = out.len() > at + 1;
    }
}

/// The first tempo point: (bpm, beats per bar).
fn first_tempo(text: &str) -> Option<(f64, u32)> {
    let point = text.split_whitespace().next()?;
    let mut parts = point.split(':');
    let _at = parts.next()?;
    let bpm: f64 = parts.next()?.parse().ok()?;
    let beats = parts
        .next()
        .and_then(|m| m.split_once('/'))
        .and_then(|(b, _)| b.parse().ok())
        .unwrap_or(4);
    (bpm > 0.0).then_some((bpm, beats))
}

/// A colour as the engine keeps it, as CSS: REAPER's native `0x01BBGGRR`
/// (the flag says it is set), else `0xRRGGBB`.
pub fn css_color(c: u32) -> Option<String> {
    if c == 0 {
        return None;
    }
    let (r, g, b) = if c & 0x0100_0000 != 0 {
        (c & 0xff, (c >> 8) & 0xff, (c >> 16) & 0xff)
    } else {
        ((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff)
    };
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn color(v: Option<&LoroValue>) -> Option<String> {
    match v? {
        LoroValue::I64(i) => u32::try_from(*i).ok().and_then(css_color),
        _ => None,
    }
}

fn map(v: &LoroValue) -> HashMap<String, LoroValue> {
    match v {
        LoroValue::Map(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => HashMap::new(),
    }
}

fn string(v: &LoroValue) -> Option<String> {
    match v {
        LoroValue::String(s) => Some(s.to_string()),
        _ => None,
    }
}

fn num(v: Option<&LoroValue>) -> Option<f64> {
    match v? {
        LoroValue::Double(d) => Some(*d),
        LoroValue::I64(i) => Some(*i as f64),
        _ => None,
    }
}

/// Natively: the song's one file in its folder (a store), read as its
/// arrangement, with the file it was read from.
#[cfg(not(target_arch = "wasm32"))]
pub fn of_folder(folder: &str, bpm: f64) -> Result<(std::path::PathBuf, Arrangement), String> {
    let file = song_file(folder).ok_or("no session file in the song's folder")?;
    let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
    read(&bytes, bpm).map(|a| (file, a))
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
            && session_file::is_store(&head)
    });
    store.cloned().or_else(|| all.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;
    use loro::LoroMap;

    fn track(tree: &LoroTree, parent: TreeParentId, guid: &str, name: &str) -> TreeID {
        let id = tree.create(parent).unwrap();
        let m = tree.get_meta(id).unwrap();
        m.insert("guid", guid).unwrap();
        m.insert("name", name).unwrap();
        m.insert("color", LoroValue::I64(0x0100_80ff)).unwrap();
        id
    }

    fn item(doc: &LoroDoc, guid: &str, track: &str, at: f64, len: f64, name: &str) {
        let m = doc
            .get_map("items")
            .insert_container(guid, LoroMap::new())
            .unwrap();
        m.insert("track", track).unwrap();
        m.insert("position", at).unwrap();
        m.insert("length", len).unwrap();
        m.insert("take.name", name).unwrap();
    }

    fn song() -> LoroDoc {
        let doc = LoroDoc::new();
        doc.get_map("meta").insert("tempo", "0:150:3/4").unwrap();
        let tree = doc.get_tree("tracks");
        tree.enable_fractional_index(0);
        let drums = track(&tree, TreeParentId::Root, "d", "Drums");
        track(&tree, TreeParentId::Node(drums), "k", "Kick");
        track(&tree, TreeParentId::Root, "e", "Empty");
        track(&tree, TreeParentId::Root, "b", "Bass");
        item(&doc, "i1", "k", 4.0, 2.0, "kick.wav");
        item(&doc, "i2", "b", 0.0, 10.0, "bass.wav");
        item(&doc, "i3", "k", 0.0, 1.0, "kick 1");
        let r = doc
            .get_map("regions")
            .insert_container("r", LoroMap::new())
            .unwrap();
        r.insert("start", 0.0).unwrap();
        r.insert("end", 8.0).unwrap();
        r.insert("name", "Verse").unwrap();
        doc.commit();
        doc
    }

    #[test]
    fn tracks_come_in_order_with_their_folders_and_clips() {
        let a = finish(of_engine(&song(), 120.0));
        let names: Vec<_> = a
            .tracks
            .iter()
            .map(|t| (t.name.as_str(), t.depth, t.folder, t.has_media))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Drums", 0, true, true),
                ("Kick", 1, false, true),
                ("Empty", 0, false, false),
                ("Bass", 0, false, true),
            ]
        );
        let kick: Vec<_> = a.tracks[1].clips.iter().map(|c| c.start).collect();
        assert_eq!(kick, vec![0.0, 4.0]);
        assert_eq!(a.tracks[1].clips[0].color.as_deref(), Some("#ff8000"));
        assert_eq!((a.bpm, a.beats_per_bar), (150.0, 3));
        assert_eq!(a.len, 10.0);
        assert_eq!(a.sections[0].name, "Verse");
        assert_eq!(a.shown().len(), 3, "the empty track is folded away");
    }

    #[test]
    fn bars_follow_the_tempo() {
        let a = Arrangement {
            len: 8.0,
            bpm: 120.0,
            beats_per_bar: 4,
            ..Default::default()
        };
        assert_eq!(
            a.bars(1),
            vec![(0.0, 1), (2.0, 2), (4.0, 3), (6.0, 4), (8.0, 5)]
        );
        assert_eq!(a.bars(2), vec![(0.0, 1), (4.0, 3), (8.0, 5)]);
    }

    #[test]
    fn colours_read_as_css() {
        assert_eq!(css_color(0x0100_00ff).as_deref(), Some("#ff0000"));
        assert_eq!(css_color(0x00ff_0000).as_deref(), Some("#ff0000"));
        assert_eq!(css_color(0), None);
    }

    /// A real song's arrangement: `STUDIO_SONG_FILE=… cargo test -p
    /// studio-app -- --ignored real_song_file --nocapture`.
    #[test]
    #[ignore]
    fn real_song_file() {
        let path = std::env::var("STUDIO_SONG_FILE").expect("STUDIO_SONG_FILE");
        let a = read(&std::fs::read(path).unwrap(), 120.0).unwrap();
        let clips: usize = a.tracks.iter().map(|t| t.clips.len()).sum();
        assert!(!a.tracks.is_empty() || !a.sections.is_empty());
        std::fs::write(
            std::env::temp_dir().join("studio-arrangement.txt"),
            format!(
                "{} tracks ({} shown), {clips} clips, {} sections, {:.1}s at {} bpm\n{:#?}",
                a.tracks.len(),
                a.shown().len(),
                a.sections.len(),
                a.len,
                a.bpm,
                a.tracks.iter().take(12).collect::<Vec<_>>()
            ),
        )
        .unwrap();
    }
}
