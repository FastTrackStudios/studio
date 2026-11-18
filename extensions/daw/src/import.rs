//! A project into a session, and a session back into what the engine
//! plays.
//!
//! The DAW's data is extension data (`session:docs/design/session.md`
//! §4.1), owner [`daw_view::ENGINE`]:
//!
//! - a track is a lane `{kind: "daw.track", name, order, parent}`, and its
//!   mixer state the map entry `track:<lane>` `{volume, pan, mute, solo,
//!   color}` (volume as a fader position, [`daw_view::fader`]);
//! - a clip is an item `{kind: "daw.item", at, end}` in seconds, its
//!   payload a map `{name, source, offset, gain, mute, color, fade_in,
//!   fade_out}`, `source` relative to the register `media_root`;
//! - tempo, markers and regions are the session's own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use daw_view::{ENGINE, ITEM_KIND, TRACK_KIND, fader};
use session::time::TimeSignature;
use session::{
    Entry, ExtensionId, Item, ItemId, Lane, LaneId, MarkId, Marker, Region, Session, TempoMap,
    Time, Value,
};

use crate::rpp;

/// What to take from a project.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// One song of a set: from the marker of this name to the next marker
    /// named `SONG END` (or the next song's marker), moved to 0.
    pub song: Option<String>,
    /// Where the sources are now; a source's path is found under it by the
    /// longest tail of its path that exists there (a project moved from
    /// another machine).
    pub media_root: PathBuf,
}

/// The engine's id as the session knows it.
#[must_use]
pub fn owner() -> ExtensionId {
    ExtensionId::from(ENGINE)
}

/// `path` found under `root`: itself if it is there, else the longest
/// tail of it that is.
fn relative_to(root: &Path, path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    (0..parts.len()).find_map(|skip| {
        let tail = parts[skip..].join("/");
        root.join(&tail).is_file().then_some(tail)
    })
}

/// The span of song `name` in `p`.
fn song_span(p: &rpp::Project, name: &str) -> Result<(f64, f64), String> {
    let start = p
        .markers
        .iter()
        .find(|m| m.end.is_none() && m.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("no marker named {name:?}"))?
        .at;
    let end = p
        .markers
        .iter()
        .filter(|m| m.end.is_none() && m.at > start + 1e-6 && m.name == "SONG END")
        .map(|m| m.at)
        .fold(f64::INFINITY, f64::min);
    if !end.is_finite() {
        return Err(format!("{name:?} has no SONG END after it"));
    }
    Ok((start, end))
}

/// The tempo at `at` (its last change at or before it).
fn tempo_at(p: &rpp::Project, at: f64) -> (f64, (u32, u32)) {
    let mut bpm = p.bpm;
    let mut meter = p.meter;
    for (s, b, m) in &p.tempo {
        if *s <= at + 1e-9 {
            bpm = *b;
            if let Some(m) = m {
                meter = *m;
            }
        }
    }
    (bpm, meter)
}

fn color_value(c: Option<u32>) -> Value {
    c.map_or(Value::Null, |c| Value::Int(i64::from(c)))
}

/// `p` as a session named `name`. Returns it and what was left out.
///
/// # Errors
///
/// The song asked for is not in the project.
#[expect(clippy::too_many_lines, reason = "one walk over the project")]
pub fn session_of(
    p: &rpp::Project,
    name: &str,
    options: &Options,
) -> Result<(Session, Report), String> {
    let (from, to) = match &options.song {
        Some(song) => song_span(p, song)?,
        None => (0.0, f64::INFINITY),
    };
    let mut report = Report::default();
    let mut s = Session::new(name);

    // Tempo: what holds at the start, then the changes inside the song.
    let (bpm, meter) = tempo_at(p, from);
    let sig = |(n, d): (u32, u32)| TimeSignature::new(n, d);
    let points = p
        .tempo
        .iter()
        .filter(|(at, ..)| *at > from + 1e-9 && *at < to)
        .map(|(at, b, m)| TempoMap::point(at - from, *b, m.map(sig)))
        .collect();
    s.tempo = TempoMap::new(bpm, sig(meter), points);

    for m in &p.markers {
        let inside = m.at >= from - 1e-9 && m.at < to;
        if !inside || (m.end.is_none() && m.name == "SONG END") {
            continue;
        }
        let id = MarkId::from(format!("mark-{}-{}", u8::from(m.end.is_some()), m.id).as_str());
        match m.end {
            Some(end) => {
                s.regions.insert(
                    id,
                    Region {
                        start: Time::Seconds(m.at - from),
                        end: Time::Seconds(end.min(to) - from),
                        name: m.name.clone(),
                        color: m.color,
                        ruler: None,
                    },
                );
            }
            None => {
                s.markers.insert(
                    id,
                    Marker {
                        at: Time::Seconds(m.at - from),
                        name: m.name.clone(),
                        color: m.color,
                        ruler: None,
                    },
                );
            }
        }
    }

    let owner = owner();
    let mut parents: Vec<LaneId> = Vec::new();
    let mut lanes_kept = 0_u32;
    let mut track_maps: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    for (ti, t) in p.tracks.iter().enumerate() {
        let lane = LaneId::from(format!("track-{ti}").as_str());
        let depth = usize::try_from(t.depth).unwrap_or(0);
        parents.truncate(depth);
        let parent = parents.last().cloned();
        let mut kept = 0;
        for (ii, c) in t.items.iter().enumerate() {
            let (start, end) = (c.position, c.position + c.length);
            if end <= from || start >= to || c.length <= 0.0 {
                continue;
            }
            // Clipped to the song, its source offset moved with its start.
            let (a, b) = (start.max(from), end.min(to));
            // A clip with no audio source (MIDI, a text cue, an empty
            // item) is kept, named: the arrangement shows it, the engine
            // plays nothing of it.
            let source = match &c.file {
                None => {
                    report.no_source += 1;
                    Value::Null
                }
                Some(file) => match relative_to(&options.media_root, file) {
                    Some(source) => Value::Text(source),
                    None => {
                        report.missing.push(file.clone());
                        continue;
                    }
                },
            };
            let payload = Value::Map(BTreeMap::from([
                ("name".to_owned(), Value::Text(c.name.clone())),
                ("source".to_owned(), source),
                ("offset".to_owned(), Value::Float(c.offset + (a - start))),
                ("gain".to_owned(), Value::Float(c.gain)),
                ("mute".to_owned(), Value::Bool(c.mute)),
                ("color".to_owned(), color_value(c.color.or(t.color))),
                (
                    "fade_in".to_owned(),
                    Value::Float(if a > start { 0.0 } else { c.fade_in }),
                ),
                (
                    "fade_out".to_owned(),
                    Value::Float(if b < end { 0.0 } else { c.fade_out }),
                ),
            ]));
            s.items.insert(
                ItemId::from(format!("item-{ti}-{ii}").as_str()),
                Item {
                    owner: owner.clone(),
                    lane: lane.clone(),
                    kind: ITEM_KIND.to_owned(),
                    at: Time::Seconds(a - from),
                    end: Some(Time::Seconds(b - from)),
                    payload,
                },
            );
            kept += 1;
        }
        report.items += kept;
        // A track with nothing in the song is left out, unless it is a
        // folder something kept is in (decided below).
        s.lanes.insert(
            lane.clone(),
            Lane {
                owner: owner.clone(),
                kind: TRACK_KIND.to_owned(),
                name: t.name.clone(),
                order: f64::from(u32::try_from(ti).unwrap_or(u32::MAX)),
                parent,
            },
        );
        track_maps.insert(
            format!("track:{lane}"),
            BTreeMap::from([
                ("volume".to_owned(), Value::Float(fader::position(t.volume))),
                ("pan".to_owned(), Value::Float(t.pan)),
                ("mute".to_owned(), Value::Bool(t.mute)),
                ("solo".to_owned(), Value::Bool(t.solo)),
                ("color".to_owned(), color_value(t.color)),
            ]),
        );
        parents.push(lane);
        let _ = kept;
    }
    // Keep the lanes that hold an item, and the folders above them.
    let mut keep: std::collections::BTreeSet<LaneId> =
        s.items.values().map(|i| i.lane.clone()).collect();
    loop {
        let more: Vec<LaneId> = keep
            .iter()
            .filter_map(|l| s.lanes.get(l)?.parent.clone())
            .filter(|p| !keep.contains(p))
            .collect();
        if more.is_empty() {
            break;
        }
        keep.extend(more);
    }
    s.lanes.retain(|id, _| keep.contains(id));
    track_maps.retain(|k, _| keep.iter().any(|l| *k == format!("track:{l}")));
    lanes_kept += u32::try_from(s.lanes.len()).unwrap_or(u32::MAX);
    report.tracks = lanes_kept;

    let data = s.ext_mut(&owner);
    data.version = 1;
    data.set(
        "media_root",
        Value::Text(options.media_root.to_string_lossy().into_owned()),
    );
    for (name, map) in track_maps {
        data.entries.insert(name, Entry::Map(map));
    }
    Ok((s, report))
}

/// What an import kept and left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub tracks: u32,
    pub items: u32,
    /// Of `items`, those with no audio source (kept, not played).
    pub no_source: u32,
    /// Sources not found under the media root (their clips left out).
    pub missing: Vec<String>,
}

fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::Text(t) => Some(t.clone()),
        _ => None,
    }
}

#[expect(clippy::cast_precision_loss, reason = "a gain or a colour")]
fn float(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Float(f) => Some(*f),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

fn boolean(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(true)))
}

fn color(v: Option<&Value>) -> Option<u32> {
    match v? {
        Value::Int(i) => u32::try_from(*i).ok(),
        _ => None,
    }
}

/// A `daw.track` lane as the engine sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineTrack {
    pub lane: LaneId,
    pub track: rpp::Track,
    /// Each clip's item id and its source as stored (relative).
    pub clips: Vec<(ItemId, Option<String>)>,
}

/// The tracks of `s` the engine plays, in order (folders depth-first),
/// with their clips; sources made absolute under the media root.
#[must_use]
pub fn engine_tracks(s: &Session) -> (Vec<EngineTrack>, PathBuf) {
    let owner = owner();
    let data = s.ext(&owner);
    let root = data
        .and_then(|d| match d.get("media_root") {
            Some(Entry::Register(v)) => text(Some(v)),
            _ => None,
        })
        .map(PathBuf::from)
        .unwrap_or_default();
    let map_of = |lane: &LaneId| -> BTreeMap<String, Value> {
        match data.and_then(|d| d.get(&format!("track:{lane}"))) {
            Some(Entry::Map(m)) => m.clone(),
            _ => BTreeMap::new(),
        }
    };
    let tracks: Vec<(&LaneId, &Lane)> = s
        .lanes
        .iter()
        .filter(|(_, l)| l.owner == owner && l.kind == TRACK_KIND)
        .collect();
    // Depth-first under each parent, by order.
    let mut out = Vec::new();
    let mut stack: Vec<(Option<LaneId>, u32)> = vec![(None, 0)];
    fn children<'a>(
        tracks: &[(&'a LaneId, &'a Lane)],
        parent: Option<&LaneId>,
    ) -> Vec<(&'a LaneId, &'a Lane)> {
        let mut c: Vec<_> = tracks
            .iter()
            .filter(|(_, l)| l.parent.as_ref() == parent)
            .copied()
            .collect();
        c.sort_by(|a, b| a.1.order.total_cmp(&b.1.order));
        c
    }
    fn walk(
        tracks: &[(&LaneId, &Lane)],
        parent: Option<&LaneId>,
        depth: u32,
        visit: &mut dyn FnMut(&LaneId, &Lane, u32),
    ) {
        for (id, lane) in children(tracks, parent) {
            visit(id, lane, depth);
            if depth < 64 {
                walk(tracks, Some(id), depth + 1, visit);
            }
        }
    }
    stack.clear();
    walk(&tracks, None, 0, &mut |id, lane, depth| {
        let m = map_of(id);
        let mut items: Vec<(&ItemId, &Item)> = s
            .items
            .iter()
            .filter(|(_, i)| i.lane == *id && i.owner == owner && i.kind == ITEM_KIND)
            .collect();
        items.sort_by(|a, b| {
            a.1.at
                .seconds(&s.tempo)
                .total_cmp(&b.1.at.seconds(&s.tempo))
        });
        let mut clips = Vec::new();
        let mut rclips = Vec::new();
        for (iid, item) in items {
            let (a, b) = item.seconds(&s.tempo);
            let pl = match &item.payload {
                Value::Map(m) => m.clone(),
                _ => BTreeMap::new(),
            };
            let source = text(pl.get("source"));
            rclips.push(rpp::Clip {
                position: a,
                length: b.unwrap_or(a) - a,
                name: text(pl.get("name")).unwrap_or_default(),
                offset: float(pl.get("offset")).unwrap_or(0.0),
                gain: float(pl.get("gain")).unwrap_or(1.0),
                mute: boolean(pl.get("mute")),
                color: color(pl.get("color")),
                fade_in: float(pl.get("fade_in")).unwrap_or(0.0),
                fade_out: float(pl.get("fade_out")).unwrap_or(0.0),
                file: source
                    .as_ref()
                    .map(|s| root.join(s).to_string_lossy().into_owned()),
            });
            clips.push((iid.clone(), source));
        }
        out.push(EngineTrack {
            lane: id.clone(),
            track: rpp::Track {
                name: lane.name.clone(),
                color: color(m.get("color")),
                volume: fader::gain(float(m.get("volume")).unwrap_or(fader::UNITY)),
                pan: float(m.get("pan")).unwrap_or(0.0),
                mute: boolean(m.get("mute")),
                solo: boolean(m.get("solo")),
                depth,
                items: rclips,
            },
            clips,
        });
    });
    (out, root)
}

/// What the engine plays of `s`, as a project its loader reads.
#[must_use]
pub fn project_of(s: &Session) -> rpp::Project {
    let (tracks, _) = engine_tracks(s);
    let meter = s.tempo.default_meter();
    rpp::Project {
        bpm: s.tempo.default_bpm(),
        meter: (meter.numerator, meter.denominator),
        tempo: Vec::new(),
        markers: Vec::new(),
        tracks: tracks
            .into_iter()
            .map(|mut t| {
                t.track.items.retain(|c| c.file.is_some());
                t.track
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(root: &Path) -> rpp::Project {
        std::fs::create_dir_all(root.join("Samples")).unwrap();
        std::fs::write(root.join("Samples/a.wav"), b"x").unwrap();
        rpp::Project {
            bpm: 160.0,
            meter: (4, 4),
            tempo: vec![
                (0.0, 160.0, None),
                (100.0, 120.0, None),
                (150.0, 90.0, None),
            ],
            markers: vec![
                rpp::Mark {
                    id: 1,
                    at: 100.0,
                    name: "Song".into(),
                    ..rpp::Mark::default()
                },
                rpp::Mark {
                    id: 2,
                    at: 110.0,
                    name: "Verse".into(),
                    ..rpp::Mark::default()
                },
                rpp::Mark {
                    id: 3,
                    at: 200.0,
                    name: "SONG END".into(),
                    ..rpp::Mark::default()
                },
            ],
            tracks: vec![
                rpp::Track {
                    name: "Band".into(),
                    volume: 1.0,
                    ..rpp::Track::default()
                },
                rpp::Track {
                    name: "Gtr".into(),
                    volume: 0.5,
                    depth: 1,
                    items: vec![
                        rpp::Clip {
                            position: 90.0,
                            length: 20.0,
                            gain: 1.0,
                            file: Some("/Users/jt/Proj/Samples/a.wav".into()),
                            ..rpp::Clip::default()
                        },
                        rpp::Clip {
                            position: 0.0,
                            length: 5.0,
                            gain: 1.0,
                            file: Some("x".into()),
                            ..rpp::Clip::default()
                        },
                    ],
                    ..rpp::Track::default()
                },
                rpp::Track {
                    name: "Empty".into(),
                    volume: 1.0,
                    ..rpp::Track::default()
                },
            ],
        }
    }

    #[test]
    fn a_song_comes_out_of_a_set_moved_to_zero() {
        let dir = std::env::temp_dir().join(format!("daw-import-{}", std::process::id()));
        let p = project(&dir);
        let options = Options {
            song: Some("song".into()),
            media_root: dir.clone(),
        };
        let (s, report) = session_of(&p, "Song", &options).unwrap();
        assert_eq!(
            (report.tracks, report.items),
            (2, 1),
            "the folder stays, the empty track goes"
        );
        assert_eq!(s.tempo.default_bpm(), 120.0);
        let item = s.items.values().next().unwrap();
        assert_eq!(item.seconds(&s.tempo), (0.0, Some(10.0)));
        let names: Vec<&str> = s.markers.values().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names.len(),
            2,
            "the song's start and Verse, not its end: {names:?}"
        );
        assert!(!names.contains(&"SONG END"));

        let (tracks, root) = engine_tracks(&s);
        assert_eq!(root, dir);
        assert_eq!(tracks.len(), 2);
        assert_eq!(
            (tracks[1].track.depth, tracks[1].track.name.as_str()),
            (1, "Gtr")
        );
        let clip = &tracks[1].track.items[0];
        assert_eq!((clip.position, clip.length, clip.offset), (0.0, 10.0, 10.0));
        assert_eq!(
            clip.file.as_deref(),
            Some(dir.join("Samples/a.wav").to_str().unwrap())
        );
        assert!((tracks[1].track.volume - 0.5).abs() < 1e-9);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
