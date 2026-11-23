//! A made-up song shaped like Unbreakable, for the arrangement's tests and
//! timings: about 160 tracks in eight families, the Drums folder shut over
//! 19 mics of 33 fixed lanes each (EDIT plays, a COMP, a take a lane), the
//! EDIT lane cut at every hit, every audio item's peaks read.
//!
//! ```ignore
//! let song = test_song::unbreakable(1300);
//! let (project, rows) = song.planner().plan(&song.raw);
//! ```

use std::sync::Arc;

use daw_proto::{Duration, Item, PositionInSeconds, Track};
use daw_ui::studio::project::Project;

/// How long the song is, seconds.
pub const LENGTH: f64 = 240.0;

/// How many fixed lanes a drum mic has: EDIT, COMP and a take a lane.
pub const DRUM_LANES: u32 = 33;

/// The song, as the window reads it back, and its peaks.
pub struct Song {
    pub raw: Project,
    pub previews: crate::midi::Previews,
    /// The Drums folder's guid.
    pub drums: String,
    /// The drum mics, each with every lane.
    pub mics: Vec<String>,
}

impl Song {
    /// The rows' planner, as the window holds it: no scene, no kinds.
    #[must_use]
    pub fn planner(&self) -> crate::song_model::Planner {
        crate::song_model::Planner {
            raw: Arc::new(self.raw.clone()),
            scene: None,
            kinds: Arc::new(crate::plan::Kinds::default()),
        }
    }

    /// Every folder open but Drums, as a person working on the kit has it,
    /// and Drums showing every lane: what the window draws.
    pub fn open_as_edited(&self) {
        for folder in self.raw.tracks.iter().filter(|t| t.is_folder) {
            if folder.guid != self.drums {
                crate::folds::toggle(&folder.guid, false);
            }
        }
        let lanes: Vec<String> = std::iter::once(self.drums.clone())
            .chain(self.mics.iter().cloned())
            .collect();
        let _ = crate::lanes::set_expanded(&lanes, Some(true));
    }
}

/// The song with `slices` slices on each drum mic's EDIT lane.
#[must_use]
pub fn unbreakable(slices: usize) -> Song {
    let mut tree = Tree::default();
    let mut mics = Vec::new();
    let mut drums = String::new();
    let families: [(&str, &[(&str, &[&str])]); 8] = [
        ("Guide", &[("", &["Click", "Cues", "Count"])]),
        (
            "Drums",
            &[
                ("Kick", &["In", "Out", "Sub", "Trig"]),
                ("Snare", &["Top", "Bottom", "Trig", "Sample"]),
                ("Toms", &["Tom 1", "Tom 2", "Tom 3"]),
                ("Cymbals", &["OH L", "OH R", "Hat", "Ride"]),
                ("Rooms", &["Room L", "Room R", "Mono", "Crush"]),
            ],
        ),
        ("Bass", &[("", &["DI", "Amp", "Sub"])]),
        (
            "Guitars",
            &[
                ("Rhythm L", &["SM57", "R121", "DI", "Room"]),
                ("Rhythm R", &["SM57", "R121", "DI", "Room"]),
                ("Lead", &["SM57", "R121", "DI", "Room"]),
                ("Clean", &["SM57", "R121", "DI", "Room"]),
            ],
        ),
        ("Keys", &[("", &NAMES[..12])]),
        ("Vocals", &[("", &NAMES[..24])]),
        ("Synths", &[("", &NAMES[..30])]),
        ("Mix", &[("", &NAMES[..36])]),
    ];
    for (family, groups) in families {
        let folder = tree.open(family);
        if family == "Drums" {
            drums.clone_from(&folder);
        }
        for (group, names) in groups {
            if !group.is_empty() {
                tree.open(group);
            }
            for name in *names {
                if family == "Drums" {
                    mics.push(tree.drum_mic(name, slices));
                } else {
                    tree.track(name);
                }
            }
            if !group.is_empty() {
                tree.close();
            }
        }
        tree.close();
    }
    let previews = crate::midi::Previews::default();
    previews.put_waves(tree.items.iter().map(|item| {
        (
            item.guid.clone(),
            wave_of(item.length.as_seconds(), item.index),
        )
    }));
    Song {
        raw: tree.project(),
        previews,
        drums,
        mics,
    }
}

/// Generic track names, enough for the biggest family.
const NAMES: [&str; 36] = [
    "A1", "A2", "A3", "A4", "A5", "A6", "B1", "B2", "B3", "B4", "B5", "B6", "C1", "C2", "C3", "C4",
    "C5", "C6", "D1", "D2", "D3", "D4", "D5", "D6", "E1", "E2", "E3", "E4", "E5", "E6", "F1", "F2",
    "F3", "F4", "F5", "F6",
];

/// A track list built in project order, folders opened and closed as
/// REAPER writes them (`folder_depth` +1 on the folder, −1 on its last
/// child) with every parent named.
#[derive(Default)]
struct Tree {
    tracks: Vec<Track>,
    items: Vec<Item>,
    names: std::collections::HashMap<String, String>,
    /// The folders open around the next track, outermost first.
    parents: Vec<String>,
}

impl Tree {
    /// A folder, the tracks after it inside it until [`Self::close`].
    fn open(&mut self, name: &str) -> String {
        let guid = self.push(name, true);
        self.parents.push(guid.clone());
        guid
    }

    /// The innermost folder ends at the last track pushed.
    fn close(&mut self) {
        self.parents.pop();
        if let Some(last) = self.tracks.last_mut() {
            last.folder_depth -= 1;
        }
    }

    fn push(&mut self, name: &str, folder: bool) -> String {
        let index = u32::try_from(self.tracks.len()).unwrap_or(u32::MAX);
        let guid = format!("{{TRACK-{index:04}}}");
        self.tracks.push(Track {
            parent_guid: self.parents.last().cloned(),
            folder_depth: i32::from(folder),
            is_folder: folder,
            color: Some(0x0040_8000 + index * 97),
            ..Track::new(guid.clone(), index, name.to_owned())
        });
        guid
    }

    /// A plain track: a dozen items across the song.
    fn track(&mut self, name: &str) {
        let guid = self.push(name, false);
        for k in 0..12 {
            let at = f64::from(k) * 20.0;
            self.item(&guid, (at, 18.0), None, &format!("{name}-{k:02}.wav"));
        }
    }

    /// A drum mic: every lane, the EDIT lane in `slices` slices.
    fn drum_mic(&mut self, name: &str, slices: usize) -> String {
        let guid = self.push(name, false);
        if let Some(track) = self.tracks.last_mut() {
            track.lane_count = DRUM_LANES;
            track.lane_play_mask = 1;
            track.lane_names = std::iter::once("EDIT".to_owned())
                .chain(std::iter::once("COMP".to_owned()))
                .chain((1..DRUM_LANES - 1).map(|k| format!("C{k}")))
                .collect();
        }
        let file = |take: u32| format!("65-{name}-260907_1321-{take:02}.wav");
        let width = LENGTH / crate::num::coord(slices.max(1));
        for k in 0..slices {
            let take = u32::try_from(k % 4).unwrap_or(0) + 1;
            self.item(
                &guid,
                (crate::num::coord(k) * width, width),
                Some(0),
                &file(take),
            );
        }
        for k in 0..40 {
            let take = u32::try_from(k % 4).unwrap_or(0) + 1;
            self.item(&guid, (f64::from(k) * 6.0, 6.0), Some(1), &file(take));
        }
        for lane in 2..DRUM_LANES {
            self.item(&guid, (0.0, LENGTH), Some(lane), &file(lane - 1));
        }
        guid
    }

    fn item(&mut self, track: &str, (at, length): (f64, f64), lane: Option<u32>, name: &str) {
        let index = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        let guid = format!("{{ITEM-{index:06}}}");
        self.names.insert(guid.clone(), name.to_owned());
        self.items.push(Item {
            guid,
            track_guid: track.to_owned(),
            index,
            position: PositionInSeconds::from_seconds(at),
            length: Duration::from_seconds(length),
            fixed_lane: lane,
            ..Item::default()
        });
    }

    fn project(self) -> Project {
        let mut items: std::collections::HashMap<String, Vec<Item>> =
            std::collections::HashMap::new();
        let item_count = self.items.len();
        for item in self.items {
            items.entry(item.track_guid.clone()).or_default().push(item);
        }
        for lane in items.values_mut() {
            daw_ui::studio::project::sort_lane(lane);
        }
        Project {
            tracks: self.tracks,
            items,
            bpm: 120.0,
            tempo: vec![daw_ui::studio::project::TempoChange {
                at: 0.0,
                bpm: 120.0,
                beats_per_bar: 4,
                beat_unit: 4,
            }],
            length_secs: LENGTH,
            item_count,
            names: self.names,
            ..Project::default()
        }
    }
}

/// A waveform `length` seconds long, its shape set by `seed`: a point
/// every hundredth of a second (a twentieth for a whole take).
#[must_use]
pub fn wave_of(length: f64, seed: u32) -> crate::midi::Wave {
    let step = if length > 30.0 { 0.05 } else { 0.01 };
    let count = crate::num::index((length / step).ceil()).max(1);
    let mut state = u64::from(seed).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let points = (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            #[expect(clippy::cast_precision_loss, reason = "a made-up peak")]
            let peak = (state % 1000) as f32 / 1000.0;
            (peak, -peak)
        })
        .collect();
    crate::midi::Wave { step, points }
}

/// The items of `track` in `raw` on lane `lane`, by position.
#[must_use]
pub fn on_lane<'a>(raw: &'a Project, track: &str, lane: u32) -> Vec<&'a Item> {
    raw.items
        .get(track)
        .into_iter()
        .flatten()
        .filter(|item| item.fixed_lane == Some(lane))
        .collect()
}
