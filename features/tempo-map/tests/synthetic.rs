//! The whole path on stems made here: a kick, a snare, hats and a bass
//! rendered to WAV files at known times, analysed, and the map held to
//! the truth. No recording is needed, so this runs anywhere; the meters
//! the scripts could not do (3/4, 6/8, a change of meter) are checked
//! here.
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
    reason = "a test"
)]

use std::path::Path;

use tempo_map::analysis::{self, Analysis, Options};
use tempo_map::eval;
use tempo_map::features::{self, Features};
use tempo_map::map::TempoMap;
use tempo_map::meter::{Meter, MeterChange};
use tempo_map::track::TrackParams;

const RATE: u32 = 44_100;

/// Noise that is the same every run.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

fn add(buffer: &mut [f32], at: f64, sound: &[f32]) {
    let start = (at * f64::from(RATE)) as usize;
    for (i, s) in sound.iter().enumerate() {
        if let Some(slot) = buffer.get_mut(start + i) {
            *slot += s;
        }
    }
}

fn kick() -> Vec<f32> {
    let len = (0.25 * f64::from(RATE)) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            // Faded to nothing: a tail cut short is a click, and a click
            // is a hit.
            let fade = 1.0 - i as f32 / len as f32;
            0.9 * fade
                * (-t / 0.03).exp()
                * (2.0 * std::f32::consts::PI * (55.0 + 80.0 * (-t / 0.01).exp()) * t).sin()
        })
        .collect()
}

fn snare(noise: &mut Noise) -> Vec<f32> {
    (0..(0.1 * f64::from(RATE)) as usize)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            0.7 * (-t / 0.025).exp() * noise.next()
        })
        .collect()
}

fn hat(noise: &mut Noise) -> Vec<f32> {
    let mut last = 0.0;
    (0..(0.02 * f64::from(RATE)) as usize)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let n = noise.next();
            let high = n - last;
            last = n;
            0.25 * (-t / 0.005).exp() * high
        })
        .collect()
}

fn bass(hz: f32, seconds: f64) -> Vec<f32> {
    (0..(seconds * f64::from(RATE)) as usize)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            0.5 * (1.0 - (-t / 0.004).exp())
                * (-t / 0.25).exp()
                * (2.0 * std::f32::consts::PI * hz * t).sin()
        })
        .collect()
}

fn write(folder: &Path, name: &str, samples: &[f32]) {
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

/// A bar to render: its beats' times, and which of them the snare is on.
struct BarPlan {
    beats: Vec<f64>,
    snare_on: Vec<usize>,
}

/// Renders `bars` as stems into `folder`: the kick and the bass on each
/// bar's first beat, the snare where the bar says, hats on every beat and
/// halfway to the next. A second of room at the end.
fn render(folder: &Path, bars: &[BarPlan]) {
    let end = bars.last().and_then(|b| b.beats.last()).copied().unwrap() + 2.0;
    let len = (end * f64::from(RATE)) as usize;
    let mut noise = Noise(7);
    let (mut k, mut s, mut h, mut b) = (
        vec![0.0_f32; len],
        vec![0.0_f32; len],
        vec![0.0_f32; len],
        vec![0.0_f32; len],
    );
    let all: Vec<f64> = bars
        .iter()
        .flat_map(|bar| bar.beats.iter().copied())
        .collect();
    for (n, bar) in bars.iter().enumerate() {
        for (i, t) in bar.beats.iter().enumerate() {
            if i == 0 {
                add(&mut k, *t, &kick());
                add(&mut b, *t, &bass(if n % 2 == 0 { 55.0 } else { 73.4 }, 0.6));
            }
            if bar.snare_on.contains(&i) {
                add(&mut s, *t, &snare(&mut noise));
            }
        }
    }
    for (i, t) in all.iter().enumerate() {
        add(&mut h, *t, &hat(&mut noise));
        if let Some(next) = all.get(i + 1) {
            add(&mut h, (t + next) / 2.0, &hat(&mut noise));
        }
    }
    let mix: Vec<f32> = (0..len).map(|i| k[i] + s[i] + h[i] + b[i]).collect();
    write(folder, "Kick", &k);
    write(folder, "Snare", &s);
    write(folder, "Hi-Hat", &h);
    write(folder, "Bass", &b);
    write(folder, "Mix", &mix);
}

/// Bars of `pulses` beats each, the beat `period(bar)` seconds long, from
/// `start`.
fn plan(
    start: f64,
    spec: &[(usize, usize, Vec<usize>)],
    period: impl Fn(usize) -> f64,
) -> (Vec<BarPlan>, Vec<f64>) {
    let mut t = start;
    let mut bars = Vec::new();
    let mut downbeats = Vec::new();
    let mut number = 0;
    for (count, pulses, snare_on) in spec {
        for _ in 0..*count {
            downbeats.push(t);
            let mut beats = Vec::new();
            for _ in 0..*pulses {
                beats.push(t);
                t += period(number);
            }
            bars.push(BarPlan {
                beats,
                snare_on: snare_on.clone(),
            });
            number += 1;
        }
    }
    downbeats.push(t);
    (bars, downbeats)
}

fn analysed(
    folder: &Path,
    bpm: f64,
    meter: Option<Meter>,
    changes: Vec<MeterChange>,
) -> (Features, Analysis) {
    let measured = features::extract(folder).unwrap();
    let found = analysis::analyse(
        &measured,
        &Options {
            track: TrackParams {
                bpm,
                ..TrackParams::default()
            },
            meter,
            changes,
            no_repair: false,
        },
    );
    (measured, found)
}

/// The map's downbeats against the truth: every one within `within`
/// seconds once the constant lateness of an onset's peak is taken out.
fn assert_downbeats(map: &TempoMap, truth: &[f64], within: f64) {
    let got = map.downbeats();
    assert!(
        got.len() + 1 >= truth.len() && got.len() <= truth.len(),
        "{} downbeats for {}",
        got.len(),
        truth.len()
    );
    let errors: Vec<f64> = got.iter().zip(truth).map(|(g, t)| g - t).collect();
    let mut sorted = errors.clone();
    sorted.sort_by(f64::total_cmp);
    let late = sorted[sorted.len() / 2];
    assert!(late.abs() < 0.03, "the map is {late} s off throughout");
    for (i, e) in errors.iter().enumerate() {
        assert!(
            (e - late).abs() < within,
            "bar {}: {:.1} ms off ({:.1} ms the usual)",
            i + 1,
            e * 1000.0,
            late * 1000.0
        );
    }
}

#[test]
fn four_four_with_a_drifting_tempo() {
    let dir = tempfile::tempdir().unwrap();
    // 24 bars from 118 to 124 bpm, the snare on 2 and 4.
    let (bars, truth) = plan(1.0, &[(24, 4, vec![1, 3])], |bar| {
        60.0 / (118.0 + 6.0 * bar as f64 / 23.0)
    });
    render(dir.path(), &bars);
    let (measured, mut found) = analysed(dir.path(), 120.0, None, Vec::new());
    assert_eq!(found.layout.meter, Meter::COMMON, "{:?}", found.meter_guess);
    // Bar 1 is the first beat with music: the best candidate.
    let best = found.candidates[0].clone();
    assert!((best.time - truth[0]).abs() < 0.03, "{best:?}");
    found.set_bar1(best.beat).unwrap();
    let map = found.map();
    assert_downbeats(&map, &truth, 0.006);
    // The tempo follows the drift.
    assert!((map.bars[0].bpm - 118.0).abs() < 0.8, "{}", map.bars[0].bpm);
    assert!(
        (map.bars[22].bpm - 123.7).abs() < 0.8,
        "{}",
        map.bars[22].bpm
    );
    let measured_map = found.evaluate(&measured);
    assert!(
        measured_map
            .phase_line
            .chars()
            .filter(|c| *c == '+')
            .count()
            >= 22,
        "{}",
        measured_map.phase_line
    );
    assert!(
        measured_map.beat_error.median < 5.0,
        "{:?}",
        measured_map.beat_error
    );
    assert!(measured_map.on_hits > 0.95);
    assert!(measured_map.grid_error.is_some_and(|g| g.median < 3.0));
    assert!(measured_map.confidence > 0.8, "{}", measured_map.confidence);
    // The kick's hits are the downbeats.
    let marks = found.hit_marks();
    let kick = &marks["Kick"];
    assert_eq!(
        kick.len(),
        24,
        "{:?}",
        kick.iter()
            .take(6)
            .map(|h| (h.time, h.strength, h.beat))
            .collect::<Vec<_>>()
    );
    assert!(kick.iter().all(|h| (h.beat - 1.0).abs() < 0.1), "{kick:?}");
    assert_eq!(kick[4].bar, 5);
}

#[test]
fn three_four_is_heard_and_mapped_in_threes() {
    let dir = tempfile::tempdir().unwrap();
    // A waltz at 100: the kick on 1, the snare on 2 and 3.
    let (bars, truth) = plan(0.8, &[(28, 3, vec![1, 2])], |_| 0.6);
    render(dir.path(), &bars);
    let (measured, mut found) = analysed(dir.path(), 100.0, None, Vec::new());
    assert_eq!(
        found.layout.meter,
        Meter::new(3, 4),
        "{:?}",
        found.meter_guess
    );
    let best = found.candidates[0].clone();
    assert!(
        (best.time - truth[0]).abs() < 0.03,
        "{:?}",
        found.candidates
    );
    found.set_bar1(best.beat).unwrap();
    let map = found.map();
    assert_downbeats(&map, &truth, 0.006);
    for bar in &map.bars {
        assert_eq!(bar.meter, Meter::new(3, 4));
        assert!(
            (bar.bpm - 100.0).abs() < 0.5,
            "bar {}: {}",
            bar.number,
            bar.bpm
        );
    }
    // The first point sets the meter; no other does.
    let points = map.points();
    assert_eq!(points[0].meter, Some(Meter::new(3, 4)));
    assert_eq!(points.iter().filter(|p| p.meter.is_some()).count(), 1);
    let line = found.evaluate(&measured).phase_line;
    assert!(line.chars().filter(|c| *c == '+').count() >= 26, "{line}");
}

#[test]
fn a_change_of_meter_moves_the_bar_lines() {
    let dir = tempfile::tempdir().unwrap();
    // Eight bars of 4/4, one of 2/4, then eight of 3/4, all at 120.
    let (bars, truth) = plan(
        1.0,
        &[(8, 4, vec![1, 3]), (1, 2, vec![1]), (8, 3, vec![1, 2])],
        |_| 0.5,
    );
    render(dir.path(), &bars);
    let changes = vec![
        MeterChange {
            bar: 9,
            meter: Meter::new(2, 4),
        },
        MeterChange {
            bar: 10,
            meter: Meter::new(3, 4),
        },
    ];
    let (measured, mut found) = analysed(dir.path(), 120.0, Some(Meter::COMMON), changes);
    let bar1 = found.beat_near(truth[0]).unwrap();
    found.set_bar1(bar1).unwrap();
    let map = found.map();
    assert_downbeats(&map, &truth, 0.006);
    let meters: Vec<(f64, Meter)> = map
        .points()
        .iter()
        .filter_map(|p| p.meter.map(|m| (p.time, m)))
        .collect();
    // The count-in and bar 1 share 4/4: three meters are set.
    assert_eq!(meters.len(), 3, "{meters:?}");
    assert_eq!(meters[1].1, Meter::new(2, 4));
    assert!((meters[1].0 - truth[8]).abs() < 0.03);
    assert_eq!(meters[2].1, Meter::new(3, 4));
    assert!((meters[2].0 - truth[9]).abs() < 0.03);
    // Every bar reads on its backbeat, through both changes.
    let line = found.evaluate(&measured).phase_line;
    assert!(!line.contains('X'), "{line}");
    // Without the changes the same beats read turned after bar 9.
    let (_, mut plain) = analysed(dir.path(), 120.0, Some(Meter::COMMON), Vec::new());
    plain.set_bar1(bar1).unwrap();
    let diff = eval::compare(&map, &plain.map(), 25.0);
    assert!(diff.differing >= 6, "{diff:?}");
}

#[test]
fn six_eight_is_tracked_in_two_and_its_tempo_is_in_quarters() {
    let dir = tempfile::tempdir().unwrap();
    // 6/8 at 60 to the dotted quarter: two beats a bar, the snare on 2.
    let (bars, truth) = plan(1.0, &[(20, 2, vec![1])], |_| 1.0);
    render(dir.path(), &bars);
    let (_, mut found) = analysed(dir.path(), 60.0, Some(Meter::new(6, 8)), Vec::new());
    let bar1 = found.beat_near(truth[0]).unwrap();
    found.set_bar1(bar1).unwrap();
    let map = found.map();
    assert_downbeats(&map, &truth, 0.008);
    // A dotted quarter at 60 is a quarter at 90.
    assert!((map.bars[3].bpm - 90.0).abs() < 0.5, "{}", map.bars[3].bpm);
    assert_eq!(map.points()[0].meter, Some(Meter::new(6, 8)));
}

#[test]
fn an_analysis_survives_json() {
    let dir = tempfile::tempdir().unwrap();
    let (bars, _) = plan(1.0, &[(12, 4, vec![1, 3])], |_| 0.5);
    render(dir.path(), &bars);
    let (_, found) = analysed(dir.path(), 120.0, None, Vec::new());
    let text = serde_json::to_string(&found).unwrap();
    let back: Analysis = serde_json::from_str(&text).unwrap();
    assert_eq!(back.beats.len(), found.beats.len());
    assert_eq!(back.map().bars.len(), found.map().bars.len());
    for (a, b) in back.beats.iter().zip(&found.beats) {
        assert!((a - b).abs() < 1e-9);
    }
}
