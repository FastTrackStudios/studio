//! A song's show, cue by cue, played headless on a venue and recorded off
//! the wire: for each cue, the player put at the end of its fade and left
//! running `SECONDS` at the song's tempo, universe 1's 512 bytes `FPS`
//! times a second. Each cue's frames are what the desk sends while that
//! cue holds, tracked state and all, so another controller can play the
//! song cue for cue (the myDMX showfile, `ignition-daslight`'s
//! `mobile_rig` example, which times them against the song's audio).
//!
//! The song's relative positions ("4 bars into the chorus") resolve
//! against its own `sections` list, laid end to end in 4/4 from beat 0;
//! each cue's `beat` in the output is where it is written to fall, in
//! beats from the top of the first section (`written_beat`).
//!
//! ```sh
//! cd features/lighting   # the cook finds the profile under data/profiles
//! cargo run -p ignition-viz --example cook_song -- data/venues/room138-cbu \
//!     data/songs/jt-recital/out-of-the-black.json 158.35 /tmp/song.json
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
use ignition_core::music::{Bars, Section, SongMap, TempoMap, TimeSignature};
use ignition_viz::DmxUniverses;
use ignition_viz::playback::{LoadOptions, Playback, tick_playback};
use ignition_viz::spawn::{DmxRes, VenueRes};
use ignition_viz::venue::Venue;

const FPS: u16 = 10;
const SECONDS: u16 = 32;
const BEATS_PER_BAR: f64 = 4.0;

/// The song's arrangement from its own `sections` list: each section
/// starts where the one before it ends, counted in 4/4 from bar 1.
fn song_map(document: &serde_json::Value, bpm: f64) -> SongMap {
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
        let name = s
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        sections.push(Section {
            name: name.to_owned(),
            start: position_of(beat),
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

/// Beats from the top (0 = bar 1 beat 1) as a 4/4 position.
fn position_of(beats: f64) -> Bars {
    let bar = (beats / BEATS_PER_BAR).floor();
    let whole: u32 = format!("{bar:.0}").parse().unwrap_or(0);
    Bars::new(
        whole.saturating_add(1),
        bar.mul_add(-BEATS_PER_BAR, beats) + 1.0,
    )
}

/// Where a cue is written to fall, in beats from the top: its section's
/// start (the sections laid end to end, a part bar included) plus its
/// bars and beat. Ignition's own resolution starts a section on the bar
/// it begins in, which after a section of 16.75 bars is three beats
/// early against the audio; this is the song's own count.
fn written_beat(document: &serde_json::Value, name: &str) -> Option<f64> {
    let sections = document.get("sections")?.as_array()?;
    let at = document
        .get("cues")?
        .as_array()?
        .iter()
        .find(|c| c.get("name").and_then(serde_json::Value::as_str) == Some(name))?
        .get("at")?;
    let section = at.get("section")?.as_str()?;
    let ordinal = at
        .get("ordinal")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let ordinal = usize::try_from(ordinal).ok()?;
    let mut start = 0.0_f64;
    let mut starts = Vec::new();
    for s in sections {
        let bars = s
            .get("bars")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        starts.push((s.get("name").and_then(serde_json::Value::as_str), start));
        start = bars.mul_add(BEATS_PER_BAR, start);
    }
    let (_, from) = starts
        .into_iter()
        .filter(|(n, _)| *n == Some(section))
        .nth(ordinal)?;
    let bars = at
        .get("bars")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let beat = at
        .get("beat")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(1.0);
    Some(bars.mul_add(BEATS_PER_BAR, from) + beat - 1.0)
}

/// A 4/4 position as beats from the top.
fn beats_of(position: Bars) -> f64 {
    (f64::from(position.bar) - 1.0).mul_add(BEATS_PER_BAR, position.beat - 1.0)
}

/// One cue held from the end of its fade, and its frames.
fn record(
    venue_dir: &str,
    cuelist: &Path,
    song: &SongMap,
    bpm: f32,
    cue: usize,
) -> anyhow::Result<Vec<Vec<u8>>> {
    let venue = Venue::load(venue_dir)?;
    let mut playback = Playback::load(
        &venue,
        LoadOptions {
            cuelist: Some(cuelist),
            jump_to_cue: Some(cue),
            song_bpm: Some(bpm),
            song: Some(song),
            ..LoadOptions::default()
        },
    )?;
    // The load seeds the Song master only for its own cook; the running
    // effects read this one.
    playback.speeds.insert("Song".to_owned(), bpm);
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(VenueRes(venue));
    world.insert_resource(DmxRes(DmxUniverses::new()));
    world.insert_resource(playback);
    let mut frames = Vec::new();
    for i in 0..FPS.saturating_mul(SECONDS) {
        world.resource_mut::<Playback>().frozen_at = Some(f32::from(i) / f32::from(FPS));
        world
            .run_system_once(tick_playback)
            .map_err(|e| anyhow::anyhow!("tick: {e}"))?;
        let snapshot = world.resource::<DmxRes>().0.snapshot();
        frames.push(
            snapshot
                .get(&1)
                .map_or_else(|| vec![0; 512], |u| u.to_vec()),
        );
    }
    Ok(frames)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(venue_dir), Some(song_path), Some(bpm), Some(out)) =
        (args.get(1), args.get(2), args.get(3), args.get(4))
    else {
        anyhow::bail!("usage: cook_song <venue dir> <song.json> <bpm> <out.json>");
    };
    let bpm: f32 = bpm.parse()?;
    let text = std::fs::read_to_string(song_path)?;
    let document: serde_json::Value = serde_json::from_str(&text)?;
    let song = song_map(&document, f64::from(bpm));
    // The order the player holds them in: resolved, then by position.
    let mut list: CueList = serde_json::from_str(&text)?;
    let unresolved = list.resolve_positions(&song);
    anyhow::ensure!(
        unresolved.is_empty(),
        "cues that do not resolve against the song's sections: {unresolved:?}"
    );

    let mut json = format!(
        "{{\"fps\":{FPS},\"bpm\":{bpm},\"song\":{},\"cues\":[",
        serde_json::to_string(&song.name)?
    );
    for (i, cue) in list.cues.iter().enumerate() {
        let beat = written_beat(&document, &cue.name)
            .or_else(|| cue.position().map(beats_of))
            .unwrap_or(0.0);
        let frames = record(venue_dir, Path::new(song_path), &song, bpm, i)?;
        if i > 0 {
            json.push(',');
        }
        let body: Vec<String> = frames
            .iter()
            .map(|f| {
                format!(
                    "[{}]",
                    f.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
                )
            })
            .collect();
        write!(
            json,
            "{{\"name\":{},\"beat\":{beat},\"fade_secs\":{},\"frames\":[{}]}}",
            serde_json::to_string(&cue.name)?,
            cue.fade_secs,
            body.join(",")
        )?;
        println!("{i:2} beat {beat:6.1}  {}", cue.name);
    }
    json.push_str("]}");
    std::fs::write(out, json)?;
    Ok(())
}
