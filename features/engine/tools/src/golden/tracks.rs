//! The golden session's tracks: folders three deep and more, one track per
//! state, every colour of the palette, a track with none, long names and
//! short and tall rows.

use daw_proto::{InputMonitoringMode, RecordInput, Track};

/// Sixteen colours a hue apart, `0xRRGGBB`, for the tracks and the clip
/// palette: whatever palette a pack sets, sixteen colours this far apart
/// land on as many of its entries as it has.
pub const COLOURS: [u32; 16] = [
    0x00e0_4848,
    0x00e8_7838,
    0x00e8_a838,
    0x00d8_d040,
    0x00a8_d040,
    0x0060_c850,
    0x0040_c088,
    0x0038_c0c0,
    0x0040_a0e0,
    0x0048_70e0,
    0x0060_58d8,
    0x0088_50d8,
    0x00b0_50d0,
    0x00d0_50b0,
    0x00d8_5080,
    0x0090_9098,
];

/// The name the panel has to cut short.
pub const LONG_NAME: &str = "A track name far too long for any panel to show whole";

/// One track of the tree: its depth, its name, and what is particular
/// about it.
struct Spec {
    depth: u32,
    name: &'static str,
    set: fn(&mut Track),
}

const fn at(depth: u32, name: &'static str) -> Spec {
    Spec {
        depth,
        name,
        set: |_| {},
    }
}

const fn with(depth: u32, name: &'static str, set: fn(&mut Track)) -> Spec {
    Spec { depth, name, set }
}

/// The tree, top to bottom: each track's depth under the folders above it.
///
/// The kick's folder is the Signal fixture's (`mixer_view::signal::
/// fixtures::kick_kit`), whose chains are keyed by the tracks' paths —
/// which is what a golden track's guid is.
const TREE: &[Spec] = &[
    with(0, "Drums", |t| t.color = Some(COLOURS[0])),
    with(1, "Kick", |t| {
        t.fx_count = 1;
        t.width = Some(170);
    }),
    with(2, "Sum", |t| t.fx_count = 1),
    with(3, "In", |t| t.fx_count = 1),
    with(3, "Out", |t| t.fx_count = 1),
    with(3, "Trig", |t| t.fx_count = 1),
    with(3, "Sample", |t| t.fx_count = 1),
    with(2, "Sub", |t| t.fx_count = 1),
    with(2, "Verb", |t| t.fx_count = 1),
    with(3, "Room", |t| t.fx_count = 1),
    with(1, "Snare", |t| t.color = Some(COLOURS[1])),
    with(1, "Hats", |t| t.color = Some(COLOURS[2])),
    with(1, "Drum MIDI", |t| {
        t.color = Some(COLOURS[3]);
        t.record_input = RecordInput::Midi {
            device_id: None,
            channel: Some(10),
        };
    }),
    with(0, "Keys", |t| t.color = Some(COLOURS[8])),
    with(1, "Melody", |t| {
        t.color = Some(COLOURS[9]);
        t.record_input = RecordInput::Midi {
            device_id: None,
            channel: None,
        };
    }),
    with(1, "Pad", |t| t.color = Some(COLOURS[10])),
    with(0, "States", |t| t.color = Some(COLOURS[5])),
    with(1, "Muted", |t| t.muted = true),
    with(1, "Soloed", |t| t.soloed = true),
    with(1, "Armed", |t| {
        t.armed = true;
        t.record_input = RecordInput::Audio { channel: 0 };
    }),
    with(1, "Monitor on", |t| {
        t.armed = true;
        t.input_monitor = InputMonitoringMode::Normal;
    }),
    with(1, "Monitor auto", |t| {
        t.armed = true;
        t.input_monitor = InputMonitoringMode::NotWhenPlaying;
    }),
    with(1, "Polarity", |t| t.phase_inverted = true),
    with(1, "Selected", |t| t.selected = true),
    with(1, "Selected too", |t| t.selected = true),
    with(1, "Write mode", |t| {
        t.automation_mode = daw_proto::primitives::AutomationMode::Write;
    }),
    with(1, "No colour", |t| t.color = None),
    with(1, LONG_NAME, |_| {}),
    with(1, "Short", |t| t.height = Some(24)),
    with(1, "Tall", |t| t.height = Some(180)),
    with(0, "Clips", |t| t.color = Some(COLOURS[6])),
    at(1, "Kick hits"),
    at(1, "Tone"),
    at(1, "Noise"),
    at(1, "Silence"),
    at(1, "Clipping"),
    with(1, "Palette", |t| t.height = Some(56)),
    with(1, "Edits", |t| t.height = Some(64)),
    with(1, "Takes", |t| {
        t.lane_count = 3;
        t.lane_play_mask = 0b010;
        t.lane_names = vec!["Take 1".into(), "Take 2".into(), "Take 3".into()];
        t.lane_display = daw_proto::LaneDisplay::Big;
        t.height = Some(120);
    }),
    at(1, "MIDI clips"),
    with(0, "Automation", |t| {
        t.color = Some(COLOURS[11]);
        t.automation_mode = daw_proto::primitives::AutomationMode::Read;
    }),
    with(0, "Meters", |t| t.color = Some(COLOURS[12])),
    with(1, "Quiet", |t| {
        t.volume = 0.25;
        t.pan = -1.0;
    }),
    with(1, "Hot", |t| {
        t.volume = 1.0;
        t.pan = 0.0;
    }),
    with(1, "Over", |t| {
        t.volume = 1.6;
        t.pan = 1.0;
    }),
    with(1, "Clipped", |t| {
        t.volume = 2.0;
        t.pan = 0.35;
    }),
    with(0, "Drum Bus", |t| {
        t.color = Some(COLOURS[13]);
        t.receive_count = 2;
        t.send_count = 1;
        t.fx_count = 2;
        t.grouping.volume_lead = 1;
    }),
    with(0, "Reverb", |t| {
        t.color = Some(COLOURS[14]);
        t.receive_count = 3;
        t.fx_count = 1;
        t.parent_send = false;
        t.grouping.volume_follow = 1;
    }),
    with(0, "Master", |t| {
        t.color = Some(COLOURS[15]);
        t.fx_count = 2;
        t.receive_count = 4;
    }),
];

/// The folders shown shut: the keys, so a folded folder shows its
/// children's clips on its own row.
const SHUT: &[&str] = &["Keys"];

/// Every track of the session, in project order.
#[must_use]
pub fn all() -> Vec<Track> {
    let paths = paths(TREE);
    TREE.iter()
        .enumerate()
        .map(|(i, spec)| {
            let next = TREE.get(i.saturating_add(1)).map_or(0, |n| n.depth);
            track(spec, i, &paths, next)
        })
        .collect()
}

/// The folders to show shut, by guid.
#[must_use]
pub fn shut(tracks: &[Track]) -> Vec<String> {
    tracks
        .iter()
        .filter(|t| SHUT.contains(&t.name.as_str()))
        .map(|t| t.guid.clone())
        .collect()
}

/// Each spec's path: its folders' names and its own, `/`-joined.
fn paths(tree: &[Spec]) -> Vec<String> {
    let mut above: Vec<&str> = Vec::new();
    tree.iter()
        .map(|spec| {
            above.truncate(usize::try_from(spec.depth).unwrap_or(0));
            above.push(spec.name);
            above.join("/")
        })
        .collect()
}

/// One track from its spec: its guid is its path, a folder is a track
/// whose next is deeper, and the folder depth says how many folders it
/// opens or closes (REAPER's `I_FOLDERDEPTH`).
fn track(spec: &Spec, index: usize, paths: &[String], next_depth: u32) -> Track {
    let path = paths.get(index).cloned().unwrap_or_default();
    let parent = path.rsplit_once('/').map(|(p, _)| p.to_owned());
    let is_folder = next_depth > spec.depth;
    let mut t = Track {
        is_folder,
        folder_depth: i32::try_from(next_depth)
            .unwrap_or(0)
            .saturating_sub(i32::try_from(spec.depth).unwrap_or(0)),
        parent_guid: parent,
        color: COLOURS
            .get(index.checked_rem(COLOURS.len()).unwrap_or(0))
            .copied(),
        volume: fader_at(index),
        pan: 0.0,
        parent_send: true,
        visible_in_tcp: true,
        visible_in_mixer: true,
        ..Track::new(
            path,
            u32::try_from(index).unwrap_or(0),
            spec.name.to_owned(),
        )
    };
    (spec.set)(&mut t);
    t
}

/// A fader position per track, so the strips show the range: from about
/// -18 dB up to +4 dB and round again.
fn fader_at(index: usize) -> f64 {
    const STEPS: [f64; 6] = [0.12, 0.35, 0.6, 1.0, 1.4, 0.8];
    STEPS
        .get(index.checked_rem(STEPS.len()).unwrap_or(0))
        .copied()
        .unwrap_or(1.0)
}
