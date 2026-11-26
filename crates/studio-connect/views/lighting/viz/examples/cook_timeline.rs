//! A song's whole show, played headless from the top as its transport
//! would play it — cues taking at their positions, hits firing on their
//! beats — and recorded off the wire frame by frame against the song's
//! audio: frame k is what the desk sends at second k/`FPS` of the audio
//! file, through the warp markers of the venue's audio plan. Nothing is
//! rendered and nothing runs in real time; each frame is computed.
//!
//! The output is `cook_song`'s shape with `"timeline": true`: each cue's
//! frames are exactly the frames from its start to the next cue's, so
//! the myDMX Super Scene (`ignition-daslight`'s `mobile_rig`) plays them
//! as they are rather than as a loop.
//!
//! Positions: the song's `sections` laid end to end from beat 0, a
//! section of 16.75 bars ending on a bar of three beats, so a position's
//! bar is the real bar (the same count the song's triggers are written
//! in). Audio second → song beat: the warp markers, song beat = clip beat
//! + `beat_offset`.
//!
//! ```sh
//! cd features/lighting
//! cargo run --release -p ignition-viz --example cook_timeline -- data/venues/room138-cbu \
//!     data/songs/jt-recital/out-of-the-black.json \
//!     data/venues/room138-cbu/daslight/out-of-the-black.audio.json 159 /tmp/timeline.json
//! ```

#![allow(
    clippy::print_stdout,
    reason = "a command-line tool; prints what it recorded"
)]

use std::fmt::Write as _;
use std::path::Path;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use ignition_core::cue::CueList;
use ignition_core::library::{Library, Pools};
use ignition_core::music::{Bars, Section, SongMap, TempoMap, TimeSignature};
use ignition_core::{Class, Show};
use ignition_viz::DmxUniverses;
use ignition_viz::playback::{LoadOptions, Playback, tick_playback};
use ignition_viz::spawn::{DmxRes, VenueRes};
use ignition_viz::venue::Venue;

const FPS: u16 = 25;
const BEATS_PER_BAR: f64 = 4.0;

/// Where every bar starts, in song beats: each section's whole bars, then
/// its part bar (16.75 bars ends on a bar of three beats).
fn bar_starts(document: &serde_json::Value) -> Vec<f64> {
    let mut starts = Vec::new();
    let mut beat = 0.0_f64;
    for s in document
        .get("sections")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let bars = s
            .get("bars")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let whole = bars.floor();
        let mut n = 0.0_f64;
        while n < whole {
            starts.push(beat);
            beat += BEATS_PER_BAR;
            n += 1.0;
        }
        let part = (bars - whole) * BEATS_PER_BAR;
        if part > 1e-6 {
            starts.push(beat);
            beat += part;
        }
    }
    starts.push(beat);
    starts
}

/// A song beat as a position in the real bars.
fn position(starts: &[f64], beat: f64) -> Bars {
    let index = starts.iter().rposition(|s| *s <= beat + 1e-9).unwrap_or(0);
    let from = starts.get(index).copied().unwrap_or(0.0);
    let bar = u32::try_from(index).unwrap_or(0).saturating_add(1);
    Bars::new(bar, beat - from + 1.0)
}

/// The sections at their real bars, so a cue's section-relative position
/// resolves to the bar its triggers are written in.
fn song_map(document: &serde_json::Value, starts: &[f64], bpm: f64) -> SongMap {
    let mut sections = Vec::new();
    let mut beat = 0.0_f64;
    for s in document
        .get("sections")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let bars = s
            .get("bars")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        sections.push(Section {
            name: s
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            start: position(starts, beat),
            bars,
        });
        beat = bars.mul_add(BEATS_PER_BAR, beat);
    }
    SongMap {
        name: document
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        tempo: TempoMap::constant(bpm, TimeSignature::default()),
        sections,
    }
}

/// A position as song beats.
fn beat_of(starts: &[f64], at: Bars) -> f64 {
    let index = usize::try_from(at.bar.saturating_sub(1)).unwrap_or(0);
    starts.get(index).copied().unwrap_or(0.0) + at.beat - 1.0
}

/// The audio plan: its length and, per audio second, the song beat.
struct Plan {
    seconds: f64,
    beat_offset: f64,
    warp: Vec<(f64, f64)>,
}

impl Plan {
    fn read(path: &Path) -> anyhow::Result<Self> {
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        let warp: Vec<(f64, f64)> = json
            .get("warp")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let p = p.as_array()?;
                Some((p.first()?.as_f64()?, p.get(1)?.as_f64()?))
            })
            .collect();
        anyhow::ensure!(warp.len() >= 2, "the audio plan needs two warp markers");
        Ok(Self {
            seconds: json
                .get("seconds")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0),
            beat_offset: json
                .get("beat_offset")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0),
            warp,
        })
    }

    /// The song beat playing at an audio second (straight lines between
    /// markers, the end segments carried on).
    fn beat_at(&self, seconds: f64) -> f64 {
        let pairs: Vec<_> = self.warp.windows(2).collect();
        let pick = pairs
            .iter()
            .find(|w| w.get(1).is_some_and(|b| seconds <= b.0))
            .or_else(|| pairs.last())
            .copied();
        let Some([(s0, b0), (s1, b1)]) = pick.map(|w| {
            [
                w.first().copied().unwrap_or_default(),
                w.get(1).copied().unwrap_or_default(),
            ]
        }) else {
            return self.beat_offset;
        };
        let t = if (s1 - s0).abs() < 1e-9 {
            0.0
        } else {
            (seconds - s0) / (s1 - s0)
        };
        (b1 - b0).mul_add(t, b0) + self.beat_offset
    }
}

/// Points the song list and its hits at a position, as the visualizer's
/// export does each frame from its schedule.
fn follow(world: &mut World, at: Bars, secs: f32) {
    let mut playback = world.resource_mut::<Playback>();
    let Playback {
        playbacks,
        groups,
        rig,
        palettes,
        speeds,
        profile,
        library,
        bundles,
        looks,
        named_tricks,
        triggers,
        ..
    } = &mut *playback;
    triggers.advance(at, secs);
    if let Some(player) = playbacks.of_class(Class::Song) {
        let show = Show {
            groups,
            palettes,
            rig,
            speeds,
            roles: profile,
            library,
            bundles,
            looks,
            named_tricks,
            ..Show::new(groups, rig)
        };
        player.set_clock(secs);
        player.seek(at, &show);
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(venue_dir), Some(song_path), Some(plan_path), Some(bpm), Some(out)) = (
        args.get(1),
        args.get(2),
        args.get(3),
        args.get(4),
        args.get(5),
    ) else {
        anyhow::bail!("usage: cook_timeline <venue dir> <song.json> <audio.json> <bpm> <out.json>");
    };
    let bpm: f32 = bpm.parse()?;
    let text = std::fs::read_to_string(song_path)?;
    let document: serde_json::Value = serde_json::from_str(&text)?;
    let starts = bar_starts(&document);
    let song = song_map(&document, &starts, f64::from(bpm));
    let plan = Plan::read(Path::new(plan_path))?;
    let mut list: CueList = serde_json::from_str(&text)?;
    let unresolved = list.resolve_positions(&song);
    anyhow::ensure!(
        unresolved.is_empty(),
        "cues that do not resolve: {unresolved:?}"
    );
    let cue_beats: Vec<(String, f64, f32)> = list
        .cues
        .iter()
        .map(|c| {
            (
                c.name.clone(),
                c.position().map_or(0.0, |p| beat_of(&starts, p)),
                c.fade_secs,
            )
        })
        .collect();

    let venue = Venue::load(venue_dir)?;
    let mut playback = Playback::load(
        &venue,
        LoadOptions {
            cuelist: Some(Path::new(song_path)),
            song_bpm: Some(bpm),
            song: Some(&song),
            ..LoadOptions::default()
        },
    )?;
    playback.speeds.insert("Song".to_owned(), bpm);
    // The library layers the desk lays over what it plays: the set's
    // (`data/library/<set>.json`, the set named by the song's folder)
    // then the song's own. They hold the song's palette — the colours its
    // `Primary`, `Tertiary`… slots name — and its own looks and effects;
    // without them every slot resolves to no colour and the show is dark.
    let set_path = Path::new(song_path)
        .parent()
        .and_then(Path::file_name)
        .map(|set| Path::new("data/library").join(set).with_extension("json"));
    let set: Library = set_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let own: Library = document
        .get("library")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    {
        let Playback {
            palettes,
            library,
            looks,
            named_tricks,
            ..
        } = &mut playback;
        let mut pools = Pools {
            palettes,
            effects: library,
            looks,
            tricks: named_tricks,
        };
        set.lay_over(&mut pools);
        own.over(&set).lay_over(&mut pools);
    }
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(VenueRes(venue));
    world.insert_resource(DmxRes(DmxUniverses::new()));
    world.insert_resource(playback);

    // Song seconds for the effects' clock: beats at the song tempo.
    let secs_of = |beat: f64| -> f32 {
        let s = beat * 60.0 / f64::from(bpm);
        format!("{s}").parse().unwrap_or(0.0)
    };
    let total = (plan.seconds * f64::from(FPS)).ceil();
    let frames_n: u32 = format!("{total:.0}").parse().unwrap_or(0);
    // The hits' first sight of the playhead is a locate; start it a
    // frame before the first so the opening hits fire.
    let first = plan.beat_at(0.0);
    follow(
        &mut world,
        position(&starts, (first - 0.01).max(0.0)),
        secs_of(first - 0.01),
    );
    let mut frames: Vec<(f64, Vec<u8>)> = Vec::new();
    for k in 0..frames_n {
        let second = f64::from(k) / f64::from(FPS);
        let beat = plan.beat_at(second);
        let secs = secs_of(beat);
        follow(&mut world, position(&starts, beat), secs);
        world.resource_mut::<Playback>().frozen_at = Some(secs);
        world
            .run_system_once(tick_playback)
            .map_err(|e| anyhow::anyhow!("tick: {e}"))?;
        let snapshot = world.resource::<DmxRes>().0.snapshot();
        frames.push((
            beat,
            snapshot
                .get(&1)
                .map_or_else(|| vec![0; 512], |u| u.to_vec()),
        ));
        if k.checked_rem(u32::from(FPS) * 10) == Some(0) {
            let mut pb = world.resource_mut::<Playback>();
            let cue = pb
                .playbacks
                .of_class(Class::Song)
                .map(|p| (p.current_index(), p.cues().len()));
            println!("{second:6.1} s  beat {beat:6.1}  song cue {cue:?}");
        }
    }

    let mut json = format!(
        "{{\"fps\":{FPS},\"bpm\":{bpm},\"timeline\":true,\"song\":{},\"cues\":[",
        serde_json::to_string(&song.name)?
    );
    for (i, (name, beat, fade)) in cue_beats.iter().enumerate() {
        let next = cue_beats
            .get(i.saturating_add(1))
            .map_or(f64::INFINITY, |c| c.1);
        let body: Vec<String> = frames
            .iter()
            .filter(|(b, _)| *b >= *beat && *b < next)
            .map(|(_, f)| {
                format!(
                    "[{}]",
                    f.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
                )
            })
            .collect();
        if i > 0 {
            json.push(',');
        }
        write!(
            json,
            "{{\"name\":{},\"beat\":{beat},\"fade_secs\":{fade},\"frames\":[{}]}}",
            serde_json::to_string(name)?,
            body.join(",")
        )?;
        println!("{i:2} beat {beat:6.1}  {:4} frames  {name}", body.len());
    }
    json.push_str("]}");
    std::fs::write(out, json)?;
    Ok(())
}
