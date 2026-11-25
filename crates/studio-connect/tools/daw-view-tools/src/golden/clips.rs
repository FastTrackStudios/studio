//! The golden session's clips: audio drawn from synthesized waveforms (a
//! kick, a sustained tone, noise, silence, a clipped signal), MIDI (drum
//! hits and a melody), clips in every colour, selected, muted, locked,
//! faded, crossfaded, overlapping, empty, on take lanes, and running past
//! both edges of a shot.
//!
//! A clip's notes are written once, in quarter notes ([`Hit`]): the
//! arrangement's preview and the editor's take are both read from them.

use std::collections::{HashMap, HashSet};

use daw_proto::Item;
use daw_proto::item::FadeShape;
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_ui::studio::project::Project;
use daw_view_kit::midi::{Note, Wave};

use super::tracks::COLOURS;

/// The drum groove's item, which the editor opens on its kit's lanes.
pub const DRUM_ITEM: &str = "Drums/Drum MIDI/groove";
/// The melody's item, which the editor opens on a piano roll.
pub const MELODY_ITEM: &str = "Keys/Melody/melody";

/// The editor's resolution: ticks to a quarter note.
const PPQ: f64 = 960.0;
/// Seconds to a quarter note at the song's opening tempo, where the MIDI
/// sits.
const QUARTER: f64 = 0.5;
/// Seconds a waveform point stands for.
const STEP: f64 = 0.01;

/// What an audio clip sounds like.
#[derive(Clone, Copy, Debug)]
enum Sound {
    /// A kick on every beat: a transient, then a decaying low tone.
    Kick,
    /// A held tone, breathing slowly.
    Tone,
    /// Noise at a steady level.
    Noise,
    /// Nothing: a clip with a waveform of silence.
    Silence,
    /// A signal driven flat against full scale.
    Clipped,
}

/// What a clip holds.
#[derive(Clone, Copy, Debug)]
enum Body {
    Audio(Sound),
    /// MIDI: its notes are the groove's, the melody's or the riff's, by
    /// the clip ([`hits_of`]); none is an empty MIDI clip.
    Midi,
    /// An item with no source at all.
    Empty,
}

/// One note, in quarter notes from its clip's start.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub at: f64,
    pub len: f64,
    pub pitch: u8,
    pub velocity: u8,
    pub selected: bool,
}

const fn hit(at: f64, len: f64, pitch: u8, velocity: u8) -> Hit {
    Hit {
        at,
        len,
        pitch,
        velocity,
        selected: false,
    }
}

const fn chosen(at: f64, len: f64, pitch: u8, velocity: u8) -> Hit {
    Hit {
        at,
        len,
        pitch,
        velocity,
        selected: true,
    }
}

/// A clip: on which track, where, how long, holding what, and its states.
struct Clip {
    track: &'static str,
    name: &'static str,
    at: f64,
    len: f64,
    body: Body,
    set: fn(&mut Item),
}

const fn clip(track: &'static str, name: &'static str, at: f64, len: f64, body: Body) -> Clip {
    Clip {
        track,
        name,
        at,
        len,
        body,
        set: |_| {},
    }
}

const fn clip_with(
    track: &'static str,
    name: &'static str,
    (at, len): (f64, f64),
    body: Body,
    set: fn(&mut Item),
) -> Clip {
    Clip {
        track,
        name,
        at,
        len,
        body,
        set,
    }
}

use Body::{Audio, Empty, Midi};
use Sound::{Clipped, Kick, Noise, Silence, Tone};

/// A short phrase for the arrangement's MIDI clips.
const RIFF: &[Hit] = &[
    hit(0.0, 1.0, 60, 100),
    hit(1.0, 0.5, 63, 80),
    hit(1.5, 0.5, 65, 90),
    hit(2.0, 2.0, 67, 110),
    hit(4.0, 1.0, 70, 70),
    hit(5.0, 1.0, 72, 120),
    hit(6.0, 2.0, 67, 60),
];

/// The long names, on a track and on a clip.
const LONG_TRACK: &str = "States/A track name far too long for any panel to show whole";

/// Every clip of the session but the palette's.
fn clips() -> Vec<Clip> {
    vec![
        // The kit: the Signal fixture's tracks carry the kick.
        clip("Drums/Kick/Sum/In", "Kick In", 0.0, 32.0, Audio(Kick)),
        clip("Drums/Kick/Sum/Out", "Kick Out", 0.0, 32.0, Audio(Kick)),
        clip("Drums/Kick/Sum/Trig", "Trig", 0.0, 32.0, Audio(Kick)),
        clip("Drums/Kick/Sum/Sample", "Sample", 0.0, 32.0, Audio(Kick)),
        clip("Drums/Kick/Sub", "Sub", 0.0, 32.0, Audio(Tone)),
        clip("Drums/Kick/Verb/Room", "Room", 0.0, 32.0, Audio(Noise)),
        clip("Drums/Snare", "Snare", 8.0, 40.0, Audio(Noise)),
        clip("Drums/Hats", "Hats", 0.0, 32.0, Audio(Noise)),
        clip("Drums/Drum MIDI", "Groove", 0.0, 16.0, Midi),
        // The keys, shut: their clips show on the folder's row.
        clip("Keys/Melody", "Melody", 8.0, 16.0, Midi),
        clip("Keys/Pad", "Pad", 0.0, 40.0, Audio(Tone)),
        // One clip on every state track, so each row has something on it.
        clip("States/Muted", "Muted", 2.0, 12.0, Audio(Tone)),
        clip("States/Soloed", "Soloed", 2.0, 12.0, Audio(Tone)),
        clip("States/Armed", "Armed", 2.0, 12.0, Audio(Noise)),
        clip("States/Monitor on", "Monitored", 2.0, 12.0, Audio(Tone)),
        clip("States/Monitor auto", "Auto", 2.0, 12.0, Audio(Tone)),
        clip("States/Polarity", "Flipped", 2.0, 12.0, Audio(Kick)),
        clip("States/Selected", "Selected", 2.0, 12.0, Audio(Tone)),
        clip(
            "States/Selected too",
            "Selected too",
            2.0,
            12.0,
            Audio(Tone),
        ),
        clip("States/Write mode", "Written", 2.0, 12.0, Audio(Tone)),
        clip("States/No colour", "No colour", 2.0, 12.0, Audio(Tone)),
        clip(
            LONG_TRACK,
            "A clip name far too long for its clip to show whole",
            2.0,
            12.0,
            Audio(Tone),
        ),
        clip("States/Short", "Short", 2.0, 12.0, Audio(Kick)),
        clip("States/Tall", "Tall", 2.0, 12.0, Audio(Kick)),
        // The waveforms, one sound a row.
        clip("Clips/Kick hits", "Kick", 4.0, 16.0, Audio(Kick)),
        clip("Clips/Tone", "Tone", 4.0, 26.0, Audio(Tone)),
        clip("Clips/Noise", "Noise", 6.0, 20.0, Audio(Noise)),
        clip("Clips/Silence", "Silence", 8.0, 12.0, Audio(Silence)),
        clip("Clips/Clipping", "Clipped", 4.0, 20.0, Audio(Clipped)),
        // The edits: every state a clip can be in.
        clip("Clips/Edits", "Starts before", 1.0, 5.0, Empty),
        clip_with("Clips/Edits", "Selected", (6.0, 4.0), Audio(Tone), |i| {
            i.selected = true;
        }),
        clip_with("Clips/Edits", "Muted", (10.0, 4.0), Audio(Tone), |i| {
            i.muted = true;
        }),
        clip_with("Clips/Edits", "Locked", (14.0, 4.0), Audio(Tone), |i| {
            i.locked = true;
        }),
        clip_with("Clips/Edits", "Faded", (18.0, 6.0), Audio(Tone), |i| {
            i.fade_in_length = Duration::from_seconds(1.5);
            i.fade_out_length = Duration::from_seconds(2.0);
            i.fade_in_shape = FadeShape::FastStart;
            i.fade_out_shape = FadeShape::FastEndSteep;
        }),
        clip_with("Clips/Edits", "Cross A", (24.0, 5.0), Audio(Noise), |i| {
            i.fade_out_length = Duration::from_seconds(2.0);
        }),
        clip_with("Clips/Edits", "Cross B", (27.0, 5.0), Audio(Kick), |i| {
            i.fade_in_length = Duration::from_seconds(2.0);
            i.take_count = 3;
            i.active_take_index = 1;
        }),
        clip("Clips/Edits", "Under", 32.0, 6.0, Audio(Tone)),
        clip("Clips/Edits", "Over", 34.0, 6.0, Audio(Kick)),
        clip("Clips/Edits", "Runs past the end", 40.0, 40.0, Audio(Tone)),
        // The takes: three lanes, the middle one playing.
        clip_with("Clips/Takes", "Take 1", (4.0, 26.0), Audio(Tone), |i| {
            i.fixed_lane = Some(0);
        }),
        clip_with("Clips/Takes", "Take 2", (4.0, 26.0), Audio(Kick), |i| {
            i.fixed_lane = Some(1);
        }),
        clip_with("Clips/Takes", "Take 3", (4.0, 26.0), Audio(Noise), |i| {
            i.fixed_lane = Some(2);
        }),
        // MIDI clips: a phrase, an empty one, drums.
        clip("Clips/MIDI clips", "Riff", 4.0, 8.0, Midi),
        clip("Clips/MIDI clips", "Empty MIDI", 12.0, 4.0, Midi),
        clip("Clips/MIDI clips", "Hits", 16.0, 8.0, Midi),
        // Something under the automation and on the meters.
        clip("Automation", "Under the lanes", 0.0, 32.0, Audio(Tone)),
        clip("Meters/Quiet", "Quiet", 0.0, 32.0, Audio(Tone)),
        clip("Meters/Hot", "Hot", 0.0, 32.0, Audio(Kick)),
        clip("Meters/Over", "Over", 0.0, 32.0, Audio(Noise)),
        clip("Meters/Clipped", "Clipped", 0.0, 32.0, Audio(Clipped)),
    ]
}

/// The palette row: one two-second clip in each colour.
fn palette_clips() -> impl Iterator<Item = (Clip, u32)> {
    COLOURS.iter().zip(0_u32..).map(|(colour, i)| {
        let at = f64::from(i).mul_add(2.0, 4.0);
        (
            clip("Clips/Palette", "Colour", at, 2.0, Audio(Tone)),
            *colour,
        )
    })
}

/// The session's clips, built.
pub struct Clips {
    pub items: Vec<Item>,
    pub names: HashMap<String, String>,
    pub midi: HashSet<String>,
    pub notes: Vec<(String, Vec<Note>)>,
    pub waves: Vec<(String, Wave)>,
}

impl Clips {
    /// The items by their track's guid, each track's in the order a lane
    /// draws them (by position, then lane, then guid: a total order).
    #[must_use]
    pub fn items_by_track(&self) -> HashMap<String, Vec<Item>> {
        let mut out: HashMap<String, Vec<Item>> = HashMap::new();
        for item in &self.items {
            out.entry(item.track_guid.clone())
                .or_default()
                .push(item.clone());
        }
        for lane in out.values_mut() {
            lane.sort_by(|a, b| {
                a.position
                    .as_seconds()
                    .total_cmp(&b.position.as_seconds())
                    .then(a.fixed_lane.cmp(&b.fixed_lane))
                    .then(a.guid.cmp(&b.guid))
            });
        }
        out
    }
}

/// Every clip, its notes and its waveform, on the tracks given.
#[must_use]
pub fn all(tracks: &[daw_proto::Track]) -> Clips {
    let on_a_track = |clip: &Clip| tracks.iter().any(|t| t.guid == clip.track);
    let coloured = palette_clips().map(|(c, colour)| (c, Some(colour)));
    let plain = clips().into_iter().map(|c| (c, None));
    let mut out = Clips {
        items: Vec::new(),
        names: HashMap::new(),
        midi: HashSet::new(),
        notes: Vec::new(),
        waves: Vec::new(),
    };
    for (n, (clip, colour)) in plain.chain(coloured).enumerate() {
        if on_a_track(&clip) {
            add(&mut out, &clip, n, colour);
        }
    }
    out
}

/// One clip into the session: its item, its name, and what it draws from.
fn add(out: &mut Clips, clip: &Clip, n: usize, colour: Option<u32>) {
    let guid = guid_of(clip);
    let mut item = Item {
        guid: guid.clone(),
        track_guid: clip.track.to_owned(),
        index: u32::try_from(n).unwrap_or(0),
        position: PositionInSeconds::from_seconds(clip.at),
        length: Duration::from_seconds(clip.len),
        color: colour,
        take_count: 1,
        volume: 1.0,
        ..Item::default()
    };
    (clip.set)(&mut item);
    out.names.insert(guid.clone(), clip.name.to_owned());
    match clip.body {
        Audio(sound) => out.waves.push((guid, wave(sound, clip.len))),
        Midi => {
            out.midi.insert(guid.clone());
            let notes = preview(&hits_of(&guid), clip.len);
            out.notes.push((guid, notes));
        }
        Empty => {}
    }
    out.items.push(item);
}

/// A clip's guid: the named items the editor opens, else its track's path,
/// its name and its start.
fn guid_of(clip: &Clip) -> String {
    match (clip.track, clip.name) {
        ("Drums/Drum MIDI", "Groove") => DRUM_ITEM.to_owned(),
        ("Keys/Melody", "Melody") => MELODY_ITEM.to_owned(),
        (track, name) => format!("{track}/{name}@{:.1}", clip.at),
    }
}

/// The notes a MIDI item holds, by its guid: the groove, the melody, half
/// the groove for the arrangement's drum clip, the riff, or none.
fn hits_of(guid: &str) -> Vec<Hit> {
    if guid == DRUM_ITEM {
        groove()
    } else if guid == MELODY_ITEM {
        melody()
    } else if guid.contains("/Hits@") {
        groove().into_iter().filter(|h| h.at < 16.0).collect()
    } else if guid.contains("/Riff@") {
        RIFF.to_vec()
    } else {
        Vec::new()
    }
}

/// The arrangement's preview of `hits` in a clip `len` seconds long:
/// each note as a fraction of the clip.
fn preview(hits: &[Hit], len: f64) -> Vec<Note> {
    let quarters = (len / QUARTER).max(f64::MIN_POSITIVE);
    hits.iter()
        .map(|h| Note {
            at: narrow(h.at / quarters),
            len: narrow((h.len / quarters).max(1e-4)),
            pitch: h.pitch,
            velocity: h.velocity,
        })
        .collect()
}

/// The editor's take of the MIDI item `item`: its notes in ticks, and the
/// melody's controller lanes.
#[must_use]
pub fn take(item: &str, project: &Project) -> super::Take {
    let len = project
        .items
        .values()
        .flatten()
        .find(|i| i.guid == item)
        .map_or(16.0, |i| i.length.as_seconds());
    let drums = item == DRUM_ITEM;
    let snapshot = daw_proto::midi::MidiTakeSnapshot {
        notes: hits_of(item)
            .iter()
            .zip(0_u32..)
            .map(|(h, index)| daw_proto::midi::MidiNote {
                index,
                channel: if drums { 9 } else { 0 },
                pitch: h.pitch,
                velocity: h.velocity,
                start_ppq: h.at * PPQ,
                length_ppq: h.len * PPQ,
                selected: h.selected,
                muted: false,
            })
            .collect(),
        ccs: if drums { Vec::new() } else { controllers(len) },
        ppq: PPQ,
        length_ppq: len / QUARTER * PPQ,
        ..Default::default()
    };
    super::Take {
        item: item.to_owned(),
        snapshot,
    }
}

/// The melody's controllers: the mod wheel rising and falling, the
/// expression pedal swelling, the sustain pedal down and up.
fn controllers(len: f64) -> Vec<daw_proto::midi::MidiCC> {
    const STEPS: u32 = 64;
    let quarters = len / QUARTER;
    let curve = move |controller: u8, shape: fn(f64) -> f64| {
        (0..=STEPS).map(move |k| {
            let f = f64::from(k) / f64::from(STEPS);
            (controller, f * quarters, shape(f))
        })
    };
    let pedal = [(0.0, 1.0), (8.0, 0.0), (8.5, 1.0), (24.0, 0.0)]
        .into_iter()
        .map(|(at, v)| (64_u8, at, v));
    curve(1, |f| (f * std::f64::consts::PI).sin())
        .chain(curve(11, |f| (0.7 * f).mul_add(f, 0.3)))
        .chain(pedal)
        .zip(0_u32..)
        .map(|((controller, at, v), index)| daw_proto::midi::MidiCC {
            index,
            channel: 0,
            controller,
            value: midi_value(v),
            position_ppq: at * PPQ,
            selected: false,
        })
        .collect()
}

/// `v` (0 to 1) as a 7-bit MIDI value.
fn midi_value(v: f64) -> u8 {
    let scaled = (v.clamp(0.0, 1.0) * 127.0).round();
    (0_u8..=127)
        .find(|n| f64::from(*n) >= scaled)
        .unwrap_or(127)
}

/// A sound's waveform over `len` seconds, the same every time.
fn wave(sound: Sound, len: f64) -> Wave {
    let mut noise = Noisy::new(0x9e37_79b9);
    let count = daw_view_kit::num::index((len / STEP).ceil());
    let points = (0..count)
        .map(|k| daw_view_kit::num::coord(k) * STEP)
        .map(|t| {
            let level = narrow(level_at(sound, t, &mut noise));
            (level, -level)
        })
        .collect();
    Wave { step: STEP, points }
}

/// How loud `sound` is `t` seconds into its clip, 0 to 1 (and 1 where it
/// clips).
fn level_at(sound: Sound, t: f64, noise: &mut Noisy) -> f64 {
    match sound {
        // A hit every beat at 120: a transient, then a decay.
        Kick => (-(t % QUARTER) * 14.0).exp().mul_add(0.92, 0.02),
        Tone => 0.18f64.mul_add((t * 0.7).sin(), 0.55),
        Noise => noise.next_unit().mul_add(0.25, 0.3),
        Silence => 0.0,
        Clipped => 1.0,
    }
}

/// A small fixed noise generator (xorshift), so noise draws the same
/// every time.
struct Noisy(u32);

impl Noisy {
    const fn new(seed: u32) -> Self {
        Self(seed)
    }

    /// The next value, 0 to 1.
    fn next_unit(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        f64::from(x) / f64::from(u32::MAX)
    }
}

/// A fraction, drawn at f32.
fn narrow(v: f64) -> f32 {
    daw_view_kit::num::narrow(v)
}

/// The groove, written out bar by bar: kick, a backbeat with a flam on
/// the fourth and ghost notes, hats (a ride in bars 5 and 6) with accents,
/// an open hat, crashes, a cowbell, a tom fill; bar 2's snares selected.
#[must_use]
pub fn groove() -> Vec<Hit> {
    const KICK: u8 = 36;
    const SNARE: u8 = 38;
    const HAT: u8 = 42;
    const OPEN: u8 = 46;
    const RIDE: u8 = 51;
    const CRASH: u8 = 49;
    const COWBELL: u8 = 56;
    const TOMS: [u8; 3] = [48, 47, 45];
    let mut out = Vec::new();
    for bar in 0..8_u8 {
        let b = f64::from(bar) * 4.0;
        let cymbal = if (4..6).contains(&bar) { RIDE } else { HAT };
        out.extend([
            hit(b, 0.25, KICK, 118),
            hit(b + 2.0, 0.25, KICK, 104),
            hit(b + 2.75, 0.25, KICK, 72),
            hit(b + 1.0, 0.25, SNARE, 112),
            // The flam: a quiet grace a thirty-second before the fourth.
            hit(b + 2.875, 0.125, SNARE, 46),
            hit(b + 3.0, 0.25, SNARE, 120),
            // Ghost notes on the e's.
            hit(b + 1.25, 0.125, SNARE, 28),
            hit(b + 3.75, 0.125, SNARE, 32),
        ]);
        let eighths = if bar == 7 { 4 } else { 8 };
        out.extend((0..eighths).map(|e: u8| {
            let accent = if e.is_multiple_of(2) { 100 } else { 64 };
            hit(f64::from(e).mul_add(0.5, b), 0.25, cymbal, accent)
        }));
        if bar % 2 == 1 {
            out.push(hit(b + 3.5, 0.5, OPEN, 90));
        }
        if bar % 4 == 0 {
            out.push(hit(b, 1.0, CRASH, 124));
        }
        if bar == 3 {
            out.push(hit(b + 1.5, 0.25, COWBELL, 80));
        }
    }
    // A fill across the last two beats, down the toms.
    out.extend(TOMS.iter().cycle().zip(0_u8..8).map(|(tom, k)| {
        hit(
            f64::from(k).mul_add(0.25, 30.0),
            0.25,
            *tom,
            k.saturating_mul(4).saturating_add(90),
        )
    }));
    for h in out
        .iter_mut()
        .filter(|h| (5.0..8.0).contains(&h.at) && h.pitch == SNARE)
    {
        h.selected = true;
    }
    out
}

/// The melody: notes across the range, a chromatic run through every
/// pitch class with the velocity climbing from soft to hard, triplets, a
/// held chord and a note selected.
#[must_use]
pub fn melody() -> Vec<Hit> {
    let mut out = vec![
        hit(0.0, 1.0, 36, 50),
        hit(1.0, 1.0, 48, 70),
        hit(2.0, 1.0, 60, 90),
        hit(3.0, 1.0, 72, 110),
        hit(4.0, 2.0, 84, 127),
        hit(6.0, 2.0, 96, 20),
    ];
    out.extend((0..12_u8).map(|k| {
        hit(
            f64::from(k).mul_add(0.5, 8.0),
            0.5,
            k.saturating_add(60),
            k.saturating_mul(10).saturating_add(10),
        )
    }));
    out.extend((0..6_u8).map(|k| {
        hit(
            14.0 + f64::from(k) / 3.0,
            1.0 / 3.0,
            67_u8.saturating_sub(k),
            80,
        )
    }));
    out.extend((0..3_u8).map(|k| {
        hit(
            f64::from(k).mul_add(2.0 / 3.0, 16.0),
            2.0 / 3.0,
            k.saturating_mul(3).saturating_add(64),
            96,
        )
    }));
    out.extend([55, 59, 62, 66].map(|pitch| chosen(18.0, 4.0, pitch, 88)));
    out.extend([
        hit(22.0, 1.0, 79, 72),
        chosen(23.0, 1.0, 81, 100),
        hit(24.0, 4.0, 76, 60),
    ]);
    out
}
