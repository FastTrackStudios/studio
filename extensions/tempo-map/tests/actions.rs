//! The extension's actions on a song made here: analysed, bar 1 set, a
//! meter fixed by hand, the map and the hits read, two maps compared; and
//! its manifest held to what it registers.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::panic,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::assert_is_empty,
    reason = "a test"
)]

use std::path::Path;

use architect_extensions::linked::{Builder, Extension as _};
use serde_json::{Value as Json, json};
use studio_tempo_map::store::Store;
use studio_tempo_map::{TempoMap, ops};

const RATE: u32 = 44_100;

/// A burst of noise `seconds` long that dies away: `bright` leaves the
/// lows out (a snare, a hat), else it is a thud.
fn burst(seed: &mut u32, seconds: f64, level: f32, bright: bool) -> Vec<f32> {
    let len = (seconds * f64::from(RATE)) as usize;
    let mut low = 0.0_f32;
    (0..len)
        .map(|i| {
            *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (*seed >> 8) as f32 / 8_388_608.0 - 1.0;
            low += 0.05 * (noise - low);
            let fade = (1.0 - i as f32 / len as f32).powi(3);
            level * fade * if bright { noise - low } else { 6.0 * low }
        })
        .collect()
}

fn write(folder: &Path, name: &str, len: usize, hits: &[(f64, Vec<f32>)]) {
    let mut samples = vec![0.0_f32; len];
    for (at, sound) in hits {
        let start = (at * f64::from(RATE)) as usize;
        for (i, s) in sound.iter().enumerate() {
            if let Some(slot) = samples.get_mut(start + i) {
                *slot += s;
            }
        }
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(folder.join(format!("Song ({name}).wav")), spec).unwrap();
    for s in samples {
        writer
            .write_sample((s.clamp(-1.0, 1.0) * 32_000.0) as i16)
            .unwrap();
    }
    writer.finalize().unwrap();
}

/// Sixteen bars of 4/4 at 120 from one second in: the kick on 1 and 3,
/// the snare on 2 and 4, hats on every beat. The bar lines' times.
fn song(folder: &Path) -> Vec<f64> {
    let mut seed = 11_u32;
    let beats: Vec<f64> = (0..64).map(|i| 1.0 + 0.5 * f64::from(i)).collect();
    let len = ((beats[63] + 2.0) * f64::from(RATE)) as usize;
    let mut kicks = Vec::new();
    let mut snares = Vec::new();
    let mut hats = Vec::new();
    for (i, t) in beats.iter().enumerate() {
        if i % 2 == 0 {
            kicks.push((*t, burst(&mut seed, 0.12, 0.8, false)));
        } else {
            snares.push((*t, burst(&mut seed, 0.1, 0.6, true)));
        }
        hats.push((*t, burst(&mut seed, 0.02, 0.2, true)));
    }
    let all: Vec<(f64, Vec<f32>)> = kicks.iter().chain(&snares).chain(&hats).cloned().collect();
    write(folder, "Kick", len, &kicks);
    write(folder, "Snare", len, &snares);
    write(folder, "Hi-Hat", len, &hats);
    write(folder, "Mix", len, &all);
    (0..=16).map(|b| 1.0 + 2.0 * f64::from(b)).collect()
}

#[test]
fn every_declared_action_is_registered_under_the_extension_s_prefix() {
    let manifest: toml::Table = TempoMap.manifest().parse().unwrap();
    let declared: Vec<String> = manifest["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(declared.len() >= 10);
    let mut registered = Builder::of(&TempoMap).action_ids();
    registered.sort();
    let mut sorted = declared.clone();
    sorted.sort();
    assert_eq!(registered, sorted);
    for id in &declared {
        assert!(id.starts_with("STUDIO_TEMPO_MAP_"), "{id}");
    }
    // Every schema is JSON.
    for action in manifest["actions"].as_array().unwrap() {
        if let Some(params) = action.get("params").and_then(|p| p.as_str()) {
            serde_json::from_str::<Json>(params)
                .unwrap_or_else(|e| panic!("{}: {e}", action["id"]));
        }
    }
}

#[test]
fn a_song_goes_from_stems_to_a_map_and_its_hits() {
    let stems = tempfile::tempdir().unwrap();
    let kept = tempfile::tempdir().unwrap();
    let truth = song(stems.path());
    let store = Store::at(kept.path().to_owned());

    let said = ops::analyse(
        &store,
        "the-song",
        &json!({ "stems": stems.path().display().to_string(), "bpm": 120 }),
    )
    .unwrap();
    assert_eq!(said["summary"]["meter"], "4/4");
    assert!(
        said["summary"]["beats"].as_u64().unwrap() >= 60,
        "{}",
        said["summary"]
    );
    assert!(kept.path().join("the-song.json").is_file());

    // The best candidate is the first downbeat with music.
    let candidates = ops::candidates(&store, "the-song").unwrap();
    let best = &candidates["candidates"][0];
    assert!(
        (best["time"].as_f64().unwrap() - truth[0]).abs() < 0.03,
        "{best}"
    );
    let set = ops::set_bar1(&store, "the-song", &json!({ "candidate": 1 })).unwrap();
    assert_eq!(set["bar1"], best["beat"]);

    let map = ops::map(&store, "the-song").unwrap();
    let bars = map["map"]["bars"].as_array().unwrap();
    assert!(bars.len() >= 15, "{} bars", bars.len());
    for (bar, want) in bars.iter().zip(&truth) {
        assert!((bar["time"].as_f64().unwrap() - want).abs() < 0.02, "{bar}");
        assert!((bar["bpm"].as_f64().unwrap() - 120.0).abs() < 1.0, "{bar}");
    }
    // The points the engine gets: the count-in at 0 with its meter, then
    // a point per bar.
    let (points, bar1) = ops::points_for_engine(&store, "the-song", 0.0).unwrap();
    let points = points.as_array().unwrap();
    assert_eq!(points[0]["at"], 0.0);
    assert_eq!(points[0]["numerator"], 4);
    assert!((bar1 - truth[0]).abs() < 0.02);
    assert!(points[1].get("numerator").is_none());
    // Moved with the audio: the count-in stays at the session's start.
    let (moved, _) = ops::points_for_engine(&store, "the-song", 2.0).unwrap();
    assert_eq!(moved[0]["at"], 0.0);
    assert!((moved[1]["at"].as_f64().unwrap() - truth[0] - 2.0).abs() < 0.02);

    // The kick's hits fall on beats 1 and 3, the snare's on 2 and 4.
    let (answer, tracks) = ops::hits(&store, "the-song", &json!({})).unwrap();
    for (piece, on) in [("Kick", [1.0, 3.0]), ("Snare", [2.0, 4.0])] {
        let hits = answer["hits"][piece].as_array().unwrap();
        assert_eq!(hits.len(), 32, "{piece}");
        for h in hits {
            let beat = h["beat"].as_f64().unwrap();
            assert!(on.iter().any(|b| (beat - b).abs() < 0.1), "{piece}: {h}");
        }
    }
    let kick = tracks.iter().find(|t| t.piece == "Kick").unwrap();
    assert_eq!((kick.track.as_str(), kick.at.len()), ("Kick", 32));
    let (one, _) = ops::hits(
        &store,
        "the-song",
        &json!({ "piece": "snare", "from": 5.0, "to": 9.0 }),
    )
    .unwrap();
    assert_eq!(one["hits"]["Snare"].as_array().unwrap().len(), 4);
    assert!(one["hits"].get("Kick").is_none());

    let measured = ops::evaluate(&store, "the-song", &json!({})).unwrap();
    assert!(measured["on_hits"].as_f64().unwrap() > 0.95, "{measured}");
    assert!(!measured["phase_line"].as_str().unwrap().contains('X'));

    // A second analysis kept under another name, read as 3/4 by hand: the
    // two maps differ from bar 2 on.
    ops::analyse(
        &store,
        "in-three",
        &json!({ "stems": stems.path().display().to_string(), "bpm": 120 }),
    )
    .unwrap();
    ops::edit(&store, "in-three", &json!({ "meter": "3/4" })).unwrap();
    ops::set_bar1(&store, "in-three", &json!({ "time": truth[0] })).unwrap();
    let diff = ops::compare(&store, &json!({ "a": "the-song", "b": "in-three" })).unwrap();
    assert!(diff["differing"].as_u64().unwrap() >= 10, "{diff}");
    assert!(!diff["meters_differ"].as_array().unwrap().is_empty());
    let same = ops::compare(
        &store,
        &json!({ "a": "the-song", "b": kept.path().join("the-song.json").display().to_string() }),
    )
    .unwrap();
    assert_eq!(same["differing"], 0);

    let state = ops::state(&store, "a build");
    assert_eq!(state["build"], "a build");
    assert_eq!(state["songs"].as_array().unwrap().len(), 2);
}

#[test]
fn what_is_not_there_is_said() {
    let kept = tempfile::tempdir().unwrap();
    let store = Store::at(kept.path().to_owned());
    let none = ops::map(&store, "nothing").unwrap_err();
    assert!(none.contains("STUDIO_TEMPO_MAP_ANALYSE"), "{none}");
    let no_stems = ops::analyse(&store, "x", &json!({})).unwrap_err();
    assert!(no_stems.contains("stems"), "{no_stems}");
    let bad = ops::analyse(
        &store,
        "x",
        &json!({ "stems": kept.path().display().to_string(), "meter": "seven" }),
    )
    .unwrap_err();
    assert!(bad.contains("not a meter"), "{bad}");
}
