//! How good a map is, and how two maps differ.
//!
//! For one map: how far each beat of its grid sits from the kit's hits,
//! how far the tracked beats sit from the per-bar grid (the number the
//! project script printed), the backbeat's phase bar by bar, a confidence
//! for every bar, and the spans to look at. For two maps: their downbeats
//! bar by bar, and where one has a beat more or fewer than the other.

use crate::features::Features;
use crate::fit::{self, Spread};
use crate::map::TempoMap;
use crate::phase::{self, Phase};
use crate::track::on_a_hit;

/// A grid beat counts as on a hit within this, seconds.
pub const ON_HIT: f64 = 0.035;

/// One bar, measured.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BarReport {
    /// The song bar, from 1.
    pub number: u32,
    pub time: f64,
    pub bpm: f64,
    /// The median distance of its grid beats from the nearest kit hit, ms
    /// (none: no hit near any of them).
    pub hit_ms: Option<f64>,
    /// The share of its grid beats with a kit hit within 35 ms.
    pub on_hits: f64,
    /// The largest distance of its tracked beats from the grid, ms.
    pub grid_ms: Option<f64>,
    /// `+`, `X`, `.` or `?`.
    pub phase: char,
    /// 0 to 1.
    pub confidence: f64,
    pub flags: Vec<String>,
}

/// A stretch of bars to look at.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Flagged {
    pub from_bar: u32,
    pub to_bar: u32,
    pub from_time: f64,
    pub to_time: f64,
    pub why: String,
}

/// A map, measured against the stems it was made from.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Evaluation {
    pub bars: usize,
    pub beats: usize,
    /// The slowest, the median and the fastest bar, bpm.
    pub tempo: (f64, f64, f64),
    /// Grid beats against the nearest kit hit, ms.
    pub beat_error: Spread,
    /// The share of grid beats with a kit hit within 35 ms.
    pub on_hits: f64,
    /// Tracked beats (those on a hit) against the per-bar grid, ms: what
    /// the project script printed as `beats vs per-bar grid`.
    pub grid_error: Option<Spread>,
    /// One character per bar.
    pub phase_line: String,
    /// The share of the bars that say which way they are that are on the
    /// backbeat.
    pub phase_on: f64,
    /// The stem the phase was read from.
    pub backbeat: Option<String>,
    /// The mean of the bars' confidences.
    pub confidence: f64,
    pub flagged: Vec<Flagged>,
    pub per_bar: Vec<BarReport>,
}

/// The beats a map was fitted through, for its grid error.
#[derive(Debug, Clone, Copy)]
pub struct Tracked<'a> {
    pub beats: &'a [f64],
    pub sources: &'a [String],
    /// The index of bar 1's downbeat.
    pub bar1: usize,
}

/// Measures `map` against `features`; with the beats it was fitted
/// through, their distance from its grid too.
#[must_use]
pub fn evaluate(map: &TempoMap, features: &Features, tracked: Option<Tracked<'_>>) -> Evaluation {
    let grid = map.grid();
    if grid.is_empty() {
        return Evaluation::default();
    }
    let hits: Vec<f64> = features
        .kit_hits()
        .into_iter()
        .filter(|h| !h.loose)
        .map(|h| h.time)
        .collect();
    let times: Vec<f64> = grid.iter().map(|g| g.time).collect();
    // Each grid beat's distance to the nearest hit, within a third of a
    // beat.
    let errors: Vec<Option<f64>> = grid
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let gap = times
                .get(i + 1)
                .map_or_else(|| map.end - g.time, |next| next - g.time);
            let reach = (gap * 0.33).min(0.12);
            let at = hits.partition_point(|h| *h < g.time);
            let near = [at.checked_sub(1), Some(at)]
                .into_iter()
                .flatten()
                .filter_map(|k| hits.get(k))
                .map(|h| h - g.time)
                .min_by(|a, b| a.abs().total_cmp(&b.abs()))?;
            (near.abs() <= reach).then_some(near * 1000.0)
        })
        .collect();

    let stem = phase::backbeat_stem(features, &times);
    let peaks = stem.map(|s| phase::beat_peaks(s, &times));
    let threshold = peaks
        .as_ref()
        .map_or(0.0, |p| 0.25 * crate::onset::percentile(p, 95.0) as f32);

    // The tracked beats against the grid, bar by bar.
    let mut grid_by_bar: Vec<Vec<f64>> = vec![Vec::new(); map.bars.len()];
    if let Some(t) = tracked {
        let mut at = t.bar1;
        for (b, bar) in map.bars.iter().enumerate() {
            let pulses = bar.meter.pulses().max(1);
            let length = map.bar_length(b);
            for q in 0..pulses {
                let Some(beat) = t.beats.get(at + q) else {
                    break;
                };
                if t.sources.get(at + q).is_none_or(|s| on_a_hit(s)) {
                    let on_grid = bar.time + length * q as f64 / pulses as f64;
                    grid_by_bar[b].push((beat - on_grid) * 1000.0);
                }
            }
            at += pulses;
        }
    }

    let bpms: Vec<f64> = map.bars.iter().map(|b| b.bpm).collect();
    let mut per_bar = Vec::with_capacity(map.bars.len());
    let mut first = 0;
    for (b, bar) in map.bars.iter().enumerate() {
        let pulses = bar.meter.pulses().max(1);
        let own = &errors[first..(first + pulses).min(errors.len())];
        let found: Vec<f64> = own.iter().flatten().map(|e| e.abs()).collect();
        let on_hits = own
            .iter()
            .flatten()
            .filter(|e| e.abs() <= ON_HIT * 1000.0)
            .count() as f64
            / pulses as f64;
        let hit_ms = (!found.is_empty()).then(|| fit::median(&found));
        let phase = peaks.as_ref().map_or(Phase::Silent, |p| {
            phase::bar_phase(
                &p[first..(first + pulses).min(p.len())],
                bar.meter,
                threshold,
            )
        });
        let around: Vec<f64> = (b.saturating_sub(2)..(b + 3).min(bpms.len()))
            .filter(|k| *k != b)
            .map(|k| bpms[k])
            .collect();
        let jump = if around.is_empty() {
            0.0
        } else {
            (bar.bpm - fit::median(&around)).abs() / bar.bpm.max(1e-9)
        };
        let grid_ms = grid_by_bar[b]
            .iter()
            .map(|e| e.abs())
            .fold(None, |m: Option<f64>, e| Some(m.map_or(e, |m| m.max(e))));
        let phase_part = match phase {
            Phase::On => 1.0,
            Phase::Silent => 0.6,
            Phase::Unclear => 0.5,
            Phase::Turned(_) => 0.1,
        };
        let confidence = 0.35 * on_hits
            + 0.25 * hit_ms.map_or(0.0, |ms| (-ms / 20.0).exp())
            + 0.25 * phase_part
            + 0.15 * (-jump / 0.04).exp();
        let mut flags = Vec::new();
        if matches!(phase, Phase::Turned(_)) {
            flags.push("backbeat turned".to_owned());
        }
        if found.is_empty() {
            flags.push("no hits".to_owned());
        } else if hit_ms.is_some_and(|ms| ms > 25.0) {
            flags.push("off the hits".to_owned());
        }
        if jump > 0.04 {
            flags.push("tempo jump".to_owned());
        }
        if grid_ms.is_some_and(|ms| ms > 40.0) {
            flags.push("beats off the grid".to_owned());
        }
        per_bar.push(BarReport {
            number: bar.number,
            time: bar.time,
            bpm: bar.bpm,
            hit_ms,
            on_hits,
            grid_ms,
            phase: phase.mark(),
            confidence,
            flags,
        });
        first += pulses;
    }

    let all: Vec<f64> = errors.iter().flatten().copied().collect();
    let on = all.iter().filter(|e| e.abs() <= ON_HIT * 1000.0).count();
    let tracked_errors: Vec<f64> = grid_by_bar.iter().flatten().copied().collect();
    let decided: Vec<char> = per_bar
        .iter()
        .map(|b| b.phase)
        .filter(|c| matches!(c, '+' | 'X'))
        .collect();
    Evaluation {
        bars: map.bars.len(),
        beats: grid.len(),
        tempo: map.tempo_range(),
        beat_error: Spread::of(&all),
        on_hits: on as f64 / grid.len() as f64,
        grid_error: tracked.map(|_| Spread::of(&tracked_errors)),
        phase_line: per_bar.iter().map(|b| b.phase).collect(),
        phase_on: if decided.is_empty() {
            0.0
        } else {
            decided.iter().filter(|c| **c == '+').count() as f64 / decided.len() as f64
        },
        backbeat: stem.map(|s| s.name.clone()),
        confidence: per_bar.iter().map(|b| b.confidence).sum::<f64>() / per_bar.len().max(1) as f64,
        flagged: flagged(map, &per_bar),
        per_bar,
    }
}

/// The runs of bars that share a flag.
fn flagged(map: &TempoMap, bars: &[BarReport]) -> Vec<Flagged> {
    let mut out: Vec<Flagged> = Vec::new();
    let mut reasons: Vec<&str> = bars
        .iter()
        .flat_map(|b| b.flags.iter().map(String::as_str))
        .collect();
    reasons.sort_unstable();
    reasons.dedup();
    for why in reasons {
        let mut run: Option<(usize, usize)> = None;
        for (i, bar) in bars.iter().enumerate() {
            let has = bar.flags.iter().any(|f| f == why);
            match (has, run) {
                (true, None) => run = Some((i, i)),
                (true, Some((a, _))) => run = Some((a, i)),
                (false, Some((a, b))) => {
                    out.push(span(map, bars, a, b, why));
                    run = None;
                }
                (false, None) => {}
            }
        }
        if let Some((a, b)) = run {
            out.push(span(map, bars, a, b, why));
        }
    }
    out.sort_by(|a, b| a.from_time.total_cmp(&b.from_time));
    out
}

fn span(map: &TempoMap, bars: &[BarReport], a: usize, b: usize, why: &str) -> Flagged {
    Flagged {
        from_bar: bars[a].number,
        to_bar: bars[b].number,
        from_time: bars[a].time,
        to_time: bars[b].time + map.bar_length(b),
        why: why.to_owned(),
    }
}

/// One bar of two maps.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BarDiff {
    pub number: u32,
    /// The downbeat in each map, seconds.
    pub a: f64,
    pub b: f64,
    /// `b` less `a`, ms.
    pub ms: f64,
    /// That difference in whole beats (0 when the bar lines agree).
    pub beats: i32,
    pub bpm_a: f64,
    pub bpm_b: f64,
}

/// Where one map gains or loses beats against the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BeatSlip {
    /// The first bar at the new offset.
    pub bar: u32,
    /// How many beats `b` is behind `a` from there (negative: ahead).
    pub beats: i32,
}

/// Two maps, bar by bar.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Comparison {
    /// The bars both maps have.
    pub bars: usize,
    /// The bars only one has.
    pub only_a: usize,
    pub only_b: usize,
    /// The downbeats' differences, ms.
    pub downbeat: Spread,
    /// The same with whole beats taken out: how far the bar lines are
    /// apart where the maps count the same beats.
    pub residual: Spread,
    /// The median of `b` less `a`, ms, signed: one map late throughout.
    pub offset_ms: f64,
    /// The bars more than `tolerance_ms` apart.
    pub differing: usize,
    pub tolerance_ms: f64,
    /// Where the whole-beat offset changes: an added or dropped beat.
    pub slips: Vec<BeatSlip>,
    /// The bars whose meters differ.
    pub meters_differ: Vec<u32>,
    pub per_bar: Vec<BarDiff>,
}

/// Compares `b` to `a` by song bar: a bar differs when its downbeats are
/// more than `tolerance_ms` apart.
#[must_use]
pub fn compare(a: &TempoMap, b: &TempoMap, tolerance_ms: f64) -> Comparison {
    let both = a.bars.len().min(b.bars.len());
    let mut per_bar = Vec::with_capacity(both);
    let mut slips = Vec::new();
    let mut meters_differ = Vec::new();
    let mut offset = 0_i32;
    for i in 0..both {
        let (x, y) = (&a.bars[i], &b.bars[i]);
        let beat = a.bar_length(i) / x.meter.pulses().max(1) as f64;
        let diff = y.time - x.time;
        let beats = if beat > 0.0 {
            (diff / beat).round() as i32
        } else {
            0
        };
        if beats != offset {
            slips.push(BeatSlip {
                bar: x.number,
                beats,
            });
            offset = beats;
        }
        if x.meter != y.meter {
            meters_differ.push(x.number);
        }
        per_bar.push(BarDiff {
            number: x.number,
            a: x.time,
            b: y.time,
            ms: diff * 1000.0,
            beats,
            bpm_a: x.bpm,
            bpm_b: y.bpm,
        });
    }
    let raw: Vec<f64> = per_bar.iter().map(|d| d.ms).collect();
    let residual: Vec<f64> = per_bar
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let beat = a.bar_length(i) / a.bars[i].meter.pulses().max(1) as f64;
            d.ms - f64::from(d.beats) * beat * 1000.0
        })
        .collect();
    Comparison {
        bars: both,
        only_a: a.bars.len() - both,
        only_b: b.bars.len() - both,
        downbeat: Spread::of(&raw),
        residual: Spread::of(&residual),
        offset_ms: fit::median(&raw),
        differing: raw.iter().filter(|ms| ms.abs() > tolerance_ms).count(),
        tolerance_ms,
        slips,
        meters_differ,
        per_bar,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::MapOptions;

    fn map_of(beats: &[f64]) -> TempoMap {
        let sources = vec!["Kick".to_owned(); beats.len()];
        TempoMap::build(beats, &sources, &MapOptions::default())
    }

    #[test]
    fn a_map_agrees_with_itself() {
        let beats: Vec<f64> = (0..65).map(|i| 1.0 + f64::from(i) * 0.5).collect();
        let map = map_of(&beats);
        let same = compare(&map, &map, 20.0);
        assert_eq!((same.bars, same.differing, same.slips.len()), (16, 0, 0));
        assert!(same.downbeat.max < 1e-9);
    }

    #[test]
    fn a_dropped_beat_is_found_at_its_bar() {
        let beats: Vec<f64> = (0..65).map(|i| 1.0 + f64::from(i) * 0.5).collect();
        let a = map_of(&beats);
        // The second map misses beat 30: every bar line after is a beat late.
        let mut short = beats;
        short.remove(30);
        let b = map_of(&short);
        let diff = compare(&a, &b, 20.0);
        assert_eq!(diff.slips.len(), 1, "{:?}", diff.slips);
        assert_eq!(diff.slips[0].beats, 1);
        assert!((7..=9).contains(&diff.slips[0].bar), "{:?}", diff.slips);
        assert!(diff.differing >= 7);
        // With the whole beat taken out the later bars agree.
        assert!(diff.residual.median < 1.0, "{:?}", diff.residual);
    }

    #[test]
    fn a_constant_lateness_is_an_offset_not_a_slip() {
        let beats: Vec<f64> = (0..65).map(|i| 1.0 + f64::from(i) * 0.5).collect();
        let late: Vec<f64> = beats.iter().map(|t| t + 0.012).collect();
        let diff = compare(&map_of(&beats), &map_of(&late), 20.0);
        assert!((diff.offset_ms - 12.0).abs() < 1e-6);
        assert_eq!((diff.differing, diff.slips.len()), (0, 0));
    }
}
