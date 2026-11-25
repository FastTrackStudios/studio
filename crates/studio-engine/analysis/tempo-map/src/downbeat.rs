//! Which beat is bar 1's downbeat, and what meter the song is in.
//!
//! Without a score bar 1 is a guess, and it has been a beat late: so the
//! answer here is a list of candidates with what speaks for each (the
//! backbeat's phase, the kick and bass on the downbeat, the bars where
//! the arrangement changes, the crashes, where the music starts) and what
//! is heard there, for the owner to confirm by ear.

use crate::features::{Features, Stem};
use crate::meter::{Layout, Meter};
use crate::phase::{self, Phase};

/// A beat that may be bar 1's downbeat.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Candidate {
    /// The beat's index.
    pub beat: usize,
    pub time: f64,
    /// 0 to 1; the list is sorted by it.
    pub score: f64,
    /// What the score is made of, each 0 to 1.
    pub backbeat: f64,
    pub low: f64,
    pub changes: f64,
    pub crashes: f64,
    pub start: f64,
    /// The stems sounding in the bar that starts there.
    pub heard: Vec<String>,
    /// The beats with music before it (a pickup, or bars before bar 1).
    pub beats_before: usize,
}

/// One number per beat for `stem`: its peak on the beat.
fn on_beat(stem: Option<&Stem>, beats: &[f64]) -> Vec<f64> {
    stem.filter(|s| s.audible()).map_or_else(
        || vec![0.0; beats.len()],
        |s| {
            phase::beat_peaks(s, beats)
                .into_iter()
                .map(f64::from)
                .collect()
        },
    )
}

/// `values` as z-scores (zeros when they do not vary).
fn standard(values: &[f64]) -> Vec<f64> {
    let n = values.len().max(1) as f64;
    let mean = values.iter().sum::<f64>() / n;
    let sd = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt();
    if sd < 1e-12 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|v| (v - mean) / sd).collect()
}

/// `values` stretched to 0..1 (all 0.5 when they are equal).
fn spread(values: &[f64]) -> Vec<f64> {
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if hi - lo < 1e-9 {
        return vec![0.5; values.len()];
    }
    values.iter().map(|v| (v - lo) / (hi - lo)).collect()
}

/// Each stem's level over each beat, dB (floored at −80), and the sum of
/// the linear levels: how much music there is on the beat.
fn levels(features: &Features, beats: &[f64]) -> (Vec<Vec<f64>>, Vec<f64>) {
    let stems: Vec<&Stem> = features
        .stems
        .values()
        .filter(|s| s.name != "Mix" && s.name != "Drums" && s.audible())
        .collect();
    let mut by_stem = Vec::with_capacity(stems.len());
    let mut total = vec![0.0_f64; beats.len()];
    for stem in stems {
        let mut row = Vec::with_capacity(beats.len());
        for (i, t) in beats.iter().enumerate() {
            let next = beats.get(i + 1).copied().unwrap_or(t + 0.5);
            let rms = stem.rms_between(*t, next);
            total[i] += rms;
            row.push((20.0 * (rms + 1e-9).log10()).max(-80.0));
        }
        by_stem.push(row);
    }
    (by_stem, total)
}

/// How much the arrangement changes at each beat: the stems' levels over
/// the two bars after it against the two before.
fn change(by_stem: &[Vec<f64>], beats: usize, pulses: usize) -> Vec<f64> {
    let reach = 2 * pulses.max(1);
    let mean = |row: &[f64], a: usize, b: usize| {
        if b > a {
            row[a..b].iter().sum::<f64>() / (b - a) as f64
        } else {
            -80.0
        }
    };
    (0..beats)
        .map(|i| {
            by_stem
                .iter()
                .map(|row| {
                    let before = mean(row, i.saturating_sub(reach), i);
                    let after = mean(row, i, (i + reach).min(beats));
                    (after - before).abs().min(30.0)
                })
                .sum::<f64>()
        })
        .collect()
}

/// The candidates for bar 1's downbeat among `beats`, best first.
#[must_use]
pub fn candidates(features: &Features, beats: &[f64], meter: Meter) -> Vec<Candidate> {
    let pulses = meter.pulses().max(1);
    if beats.len() < 4 * pulses {
        return Vec::new();
    }
    let layout = Layout::new(meter, Vec::new());
    let back: Vec<f32> = phase::backbeat_stem(features, beats)
        .map(|s| phase::beat_peaks(s, beats))
        .unwrap_or_default();
    let kick = standard(&on_beat(features.stem("Kick"), beats));
    let bass: Vec<f64> = features
        .stem("Bass")
        .filter(|s| s.envelope.is_some())
        .map_or_else(
            || vec![0.0; beats.len()],
            |s| {
                beats
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let next = beats.get(i + 1).copied().unwrap_or(t + 0.5);
                        f64::from(s.onset_between(t - 0.04, t + 0.3 * (next - t)))
                    })
                    .collect()
            },
        );
    let bass = standard(&bass);
    let crash = standard(&on_beat(features.stem("Cymbals"), beats));
    let (by_stem, total) = levels(features, beats);
    let changes = change(&by_stem, beats.len(), pulses);

    // Per offset in the bar: how well each cue says "this is beat 1".
    let mut backbeat = vec![0.0_f64; pulses];
    let mut low = vec![0.0_f64; pulses];
    let mut crashes = vec![0.0_f64; pulses];
    let mut turns = vec![0.0_f64; pulses];
    // The beats where the arrangement changes most: one for every four
    // bars or so.
    let mut order: Vec<usize> = (0..beats.len()).collect();
    order.sort_by(|a, b| changes[*b].total_cmp(&changes[*a]));
    let mut top: Vec<usize> = Vec::new();
    for i in order {
        if top.len() >= (beats.len() / (4 * pulses)).max(2) {
            break;
        }
        // One per bar: a change shows on the beats around it too.
        if top.iter().all(|t| t.abs_diff(i) >= pulses) {
            top.push(i);
        }
    }
    for offset in 0..pulses {
        if !back.is_empty() {
            let read = phase::phases(&back, &layout, offset);
            let on = read.iter().filter(|(_, p)| *p == Phase::On).count();
            let said = read
                .iter()
                .filter(|(_, p)| matches!(p, Phase::On | Phase::Turned(_)))
                .count();
            backbeat[offset] = if said == 0 {
                0.5
            } else {
                on as f64 / said as f64
            };
        } else {
            backbeat[offset] = 0.5;
        }
        let own: Vec<usize> = (offset..beats.len()).step_by(pulses).collect();
        let mean =
            |values: &[f64]| own.iter().map(|i| values[*i]).sum::<f64>() / own.len().max(1) as f64;
        low[offset] = mean(&kick) + mean(&bass);
        crashes[offset] = mean(&crash);
        turns[offset] =
            top.iter().filter(|i| *i % pulses == offset).count() as f64 / top.len().max(1) as f64;
    }
    let (low, crashes, turns) = (spread(&low), spread(&crashes), spread(&turns));

    // Where the music starts: the first beat with a tenth of the song's
    // usual level that holds for a bar.
    let mut sorted = total.clone();
    sorted.sort_by(f64::total_cmp);
    let usual = sorted[sorted.len() / 2];
    let loud = |i: usize| total[i] > 0.1 * usual;
    let start = (0..beats.len())
        .find(|i| (*i..(*i + pulses).min(beats.len())).all(loud))
        .unwrap_or(0);

    let mut out = Vec::new();
    for offset in 0..pulses {
        // The downbeats at this offset around the start: the one before
        // it, the first at or after it, and the next (a pickup).
        let first = start + (offset + pulses - start % pulses) % pulses;
        let around = [first.checked_sub(pulses), Some(first), Some(first + pulses)];
        for (k, beat) in around.into_iter().enumerate() {
            let Some(beat) = beat.filter(|b| b + pulses <= beats.len()) else {
                continue;
            };
            // The first downbeat with music is the likeliest; one with
            // less than a bar of music before it is a pickup's.
            let before = (0..beat).filter(|i| loud(*i)).count();
            let placed: f64 = match k {
                1 if before == 0 => 1.0,
                1 => 0.85,
                2 if before < 2 * pulses => 0.6,
                0 => 0.35,
                _ => 0.3,
            };
            let score = 0.35 * backbeat[offset]
                + 0.15 * low[offset]
                + 0.2 * turns[offset]
                + 0.05 * crashes[offset]
                + 0.25 * placed;
            let end = beats
                .get(beat + pulses)
                .copied()
                .unwrap_or(beats[beat] + 2.0);
            let heard: Vec<String> = features
                .stems
                .values()
                .filter(|s| s.name != "Mix" && s.name != "Drums")
                .filter(|s| 20.0 * (s.rms_between(beats[beat], end) + 1e-9).log10() > -50.0)
                .map(|s| s.name.clone())
                .collect();
            out.push(Candidate {
                beat,
                time: beats[beat],
                score,
                backbeat: backbeat[offset],
                low: low[offset],
                changes: turns[offset],
                crashes: crashes[offset],
                start: placed,
                heard,
                beats_before: before,
            });
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.beat.cmp(&b.beat)));
    out
}

/// What the beats say the meter is.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MeterGuess {
    pub meter: Meter,
    /// How strongly the accents repeat every 2, 3 and 4 beats (−1 to 1).
    pub every_two: f64,
    pub every_three: f64,
    pub every_four: f64,
}

/// The correlation of `values` with themselves `lag` beats on.
fn repeats(values: &[f64], lag: usize) -> f64 {
    if values.len() <= lag + 1 {
        return 0.0;
    }
    let z = standard(values);
    let n = z.len() - lag;
    (0..n).map(|i| z[i] * z[i + lag]).sum::<f64>() / n as f64
}

/// Whether the accents of `beats` repeat in threes or in fours: the kick,
/// the backbeat and the bass, each asked how well it repeats every 2, 3
/// and 4 beats. Three is said only when it clearly beats both.
#[must_use]
pub fn guess_meter(features: &Features, beats: &[f64]) -> MeterGuess {
    let mut cues: Vec<Vec<f64>> = Vec::new();
    for name in ["Kick", "Bass"] {
        let values = on_beat(features.stem(name), beats);
        if values.iter().any(|v| *v > 0.0) {
            cues.push(values);
        }
    }
    if let Some(stem) = phase::backbeat_stem(features, beats) {
        cues.push(on_beat(Some(stem), beats));
    }
    let mean = |lag: usize| {
        if cues.is_empty() {
            0.0
        } else {
            cues.iter().map(|c| repeats(c, lag)).sum::<f64>() / cues.len() as f64
        }
    };
    let (two, three, four) = (mean(2), mean(3), mean(4));
    let meter = if three > two.max(four) + 0.1 {
        Meter::new(3, 4)
    } else {
        Meter::COMMON
    };
    MeterGuess {
        meter,
        every_two: two,
        every_three: three,
        every_four: four,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_in_threes_repeat_in_threes() {
        let waltz: Vec<f64> = (0..90)
            .map(|i| if i % 3 == 0 { 1.0 } else { 0.1 })
            .collect();
        assert!(repeats(&waltz, 3) > 0.9);
        assert!(repeats(&waltz, 4) < 0.0);
        let four: Vec<f64> = (0..96)
            .map(|i| if i % 4 == 0 { 1.0 } else { 0.1 })
            .collect();
        assert!(repeats(&four, 4) > 0.9);
        assert!(repeats(&four, 3) < 0.0);
    }

    #[test]
    fn a_range_spreads_to_unit() {
        assert_eq!(spread(&[2.0, 4.0, 3.0]), vec![0.0, 1.0, 0.5]);
        assert_eq!(spread(&[1.0, 1.0]), vec![0.5, 0.5]);
    }
}
