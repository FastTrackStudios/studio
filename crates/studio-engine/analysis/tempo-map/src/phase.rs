//! Checking the beats before believing them: the backbeat's phase bar by
//! bar, the irregular intervals, and the repair of a beat the tracker
//! added or dropped.
//!
//! The snare sits on 2 and 4. A run of bars where it sits on 1 and 3 means
//! the beats slipped by one at the change: the tracker put one beat too
//! many or too few through a fill, an entrance or a bridge. Whether that
//! is the tracker's doing shows in the time the span takes: if it holds a
//! different number of beats than its length over the tempo around it,
//! the span is re-spaced with the right number; if not, the music itself
//! turned (an odd bar: a meter change to set by hand).
//!
//! Generalised to the meter: the backbeat is where the meter puts it
//! ([`Meter::backbeat`]), and a bar is read by which rotation of that
//! pattern its hits fit.

use crate::features::{Features, KIT, KitHit, Stem};
use crate::fit;
use crate::meter::{Layout, Meter};
use crate::track::{self, TrackParams};

/// What a bar's backbeat says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Phase {
    /// On the backbeat: the bar line is where it should be.
    On,
    /// Off it: the beats are rotated by this many (1 and 3 for 2 and 4).
    Turned(usize),
    /// Nothing loud enough to tell.
    Silent,
    /// Hits, but no clear pattern.
    Unclear,
}

impl Phase {
    /// One character, as the script printed it: `+`, `X`, `.`, `?`.
    #[must_use]
    pub const fn mark(self) -> char {
        match self {
            Self::On => '+',
            Self::Turned(_) => 'X',
            Self::Silent => '.',
            Self::Unclear => '?',
        }
    }
}

/// The peak of `stem` on each beat: from 40 ms before it to three tenths
/// of the way to the next (a hit on the beat, not one after it).
#[must_use]
pub fn beat_peaks(stem: &Stem, beats: &[f64]) -> Vec<f32> {
    beats
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let next = beats.get(i + 1).copied().unwrap_or(t + 0.56);
            stem.peak_between(t - 0.04, t + 0.3 * (next - t))
        })
        .collect()
}

/// The stem that carries the backbeat: the snare when it is audible, else
/// the kit piece whose hits alternate most from beat to beat (brushes
/// come out of a separator as toms or hats).
#[must_use]
pub fn backbeat_stem<'a>(features: &'a Features, beats: &[f64]) -> Option<&'a Stem> {
    if let Some(snare) = features.stem("Snare").filter(|s| s.audible()) {
        return Some(snare);
    }
    let mut best: Option<(f64, &Stem)> = None;
    for name in KIT.iter().filter(|n| **n != "Kick" && **n != "Snare") {
        let Some(stem) = features.stem(name).filter(|s| s.audible()) else {
            continue;
        };
        let peaks = beat_peaks(stem, beats);
        let (mut even, mut odd) = (0.0_f64, 0.0_f64);
        for (i, p) in peaks.iter().enumerate() {
            if i % 2 == 0 {
                even += f64::from(*p);
            } else {
                odd += f64::from(*p);
            }
        }
        let contrast = (even - odd).abs() / (even + odd).max(1e-9);
        if best.is_none_or(|(c, _)| contrast > c) {
            best = Some((contrast, stem));
        }
    }
    best.map(|(_, stem)| stem)
}

/// A beat whose interval is off the tempo around it.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Irregular {
    /// The beat the interval starts at.
    pub beat: usize,
    pub time: f64,
    pub interval: f64,
    /// The median of the sixteen intervals around it.
    pub around: f64,
}

/// The intervals more than `tolerance` (0.04 in the script) off the median
/// of the sixteen around them.
#[must_use]
pub fn irregular(beats: &[f64], tolerance: f64) -> Vec<Irregular> {
    let gaps: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    let mut out = Vec::new();
    for (i, gap) in gaps.iter().enumerate() {
        let around = fit::median(&gaps[i.saturating_sub(8)..(i + 8).min(gaps.len())]);
        if around > 0.0 && (gap - around).abs() / around > tolerance {
            out.push(Irregular {
                beat: i,
                time: beats[i],
                interval: *gap,
                around,
            });
        }
    }
    out
}

/// One bar's phase from its beats' backbeat peaks: `threshold` is what
/// counts as a hit.
#[must_use]
pub fn bar_phase(peaks: &[f32], meter: Meter, threshold: f32) -> Phase {
    let pulses = meter.pulses();
    if peaks.len() < pulses || pulses == 0 {
        return Phase::Silent;
    }
    let peaks = &peaks[..pulses];
    if peaks.iter().copied().fold(0.0_f32, f32::max) < threshold {
        return Phase::Silent;
    }
    let on = meter.backbeat();
    if on.is_empty() {
        return Phase::Unclear;
    }
    if pulses == 4 {
        // The script's rule, as it was.
        let (a, b, c, d) = (peaks[0], peaks[1], peaks[2], peaks[3]);
        return if b > threshold && d > threshold && b + d > a + c {
            Phase::On
        } else if a + c > b + d {
            Phase::Turned(1)
        } else {
            Phase::Unclear
        };
    }
    // The rotation of the backbeat pattern the bar's hits fit best.
    let score = |shift: usize| {
        let (mut hit, mut rest) = (0.0_f32, 0.0_f32);
        for (k, p) in peaks.iter().enumerate() {
            if on.contains(&((k + pulses - shift) % pulses)) {
                hit += p;
            } else {
                rest += p;
            }
        }
        hit / on.len() as f32 - rest / (pulses - on.len()).max(1) as f32
    };
    let mut best = (f32::NEG_INFINITY, 0);
    for shift in 0..pulses {
        let s = score(shift);
        if s > best.0 + 1e-6 {
            best = (s, shift);
        }
    }
    if best.0 <= 0.0 {
        Phase::Unclear
    } else if best.1 == 0 {
        Phase::On
    } else {
        Phase::Turned(best.1)
    }
}

/// The phase of every whole bar from beat `from`, with the bars' first
/// beats (as indices into `peaks`).
#[must_use]
pub fn phases(peaks: &[f32], layout: &Layout, from: usize) -> Vec<(usize, Phase)> {
    let from = from.min(peaks.len());
    let song = &peaks[from..];
    let threshold = 0.25 * crate::onset::percentile(peaks, 95.0) as f32;
    layout
        .bars(song.len())
        .into_iter()
        .filter(|span| span.start + span.meter.pulses() <= song.len())
        .map(|span| {
            let bar = &song[span.start..span.start + span.meter.pulses()];
            (from + span.start, bar_phase(bar, span.meter, threshold))
        })
        .collect()
}

/// The phase line as the script printed it: one character per bar.
#[must_use]
pub fn line(phases: &[(usize, Phase)]) -> String {
    phases.iter().map(|(_, p)| p.mark()).collect()
}

/// What is wrong with a span of beats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SlipKind {
    /// The tracker put a beat too many through the span.
    Added,
    /// It put one too few.
    Dropped,
    /// It went half a beat out and came back: the count is right, the
    /// beats between are on the offbeats.
    Shifted,
    /// The backbeat changes sides here though the span holds the beats
    /// its length says: the music turned (an odd bar, or the drummer moved
    /// the backbeat).
    Turnaround,
}

/// A span of beats that is miscounted, or where the backbeat turns.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Slip {
    pub kind: SlipKind,
    /// The span, as beat indices, and where it starts, seconds.
    pub from: usize,
    pub to: usize,
    pub time: f64,
    /// The intervals the span holds, and those its length says it should.
    pub actual: usize,
    pub expected: usize,
}

/// A run of intervals off the tempo around them, between steady beats.
#[derive(Debug, Clone, Copy)]
struct Run {
    from: usize,
    to: usize,
    /// The beat's length either side of it, seconds.
    period: f64,
}

/// How many beats more than its length says the span `from..to` holds
/// (negative: fewer), at `period` a beat.
fn excess(beats: &[f64], from: usize, to: usize, period: f64) -> f64 {
    (to - from) as f64 - (beats[to] - beats[from]) / period
}

/// The spans of `beats` that hold a different number of beats than their
/// length over the tempo either side says: a run of intervals off the
/// tempo around them (the tracker squeezing a beat in, or stretching over
/// one), between steady beats at one tempo. A run half a beat out is
/// joined with the next one that brings the count back to whole beats:
/// the tracker went onto the offbeats there and came back.
#[must_use]
pub fn miscounted(beats: &[f64]) -> Vec<Slip> {
    let gaps: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    let n = gaps.len();
    if n < 16 {
        return Vec::new();
    }
    // Off the tempo of the twelve bars around: a long squeezed run is
    // still off it, where the sixteen intervals around it are not.
    let off: Vec<bool> = (0..n)
        .map(|i| {
            let around = fit::median(&gaps[i.saturating_sub(24)..(i + 25).min(n)]);
            around > 0.0 && (gaps[i] - around).abs() / around > 0.04
        })
        .collect();
    let mut runs: Vec<Run> = Vec::new();
    let mut i = 0;
    while i < n {
        if !off[i] {
            i += 1;
            continue;
        }
        // The run: intervals off the tempo no more than two apart.
        let first = i;
        let mut last = i;
        let mut k = i + 1;
        while k < n && k <= last + 3 {
            if off[k] {
                last = k;
            }
            k += 1;
        }
        i = last + 1;
        let steady = |range: std::ops::Range<usize>| -> Vec<f64> {
            range.filter(|k| !off[*k]).map(|k| gaps[k]).collect()
        };
        let before = steady(first.saturating_sub(16)..first);
        let after = steady(last + 1..(last + 17).min(n));
        // Steady beats both sides, at one tempo: without them a run is a
        // change of tempo (an intro held back, an ending slowing down),
        // which is the music's.
        if before.len() < 6 || after.len() < 6 {
            continue;
        }
        let (a, b) = (fit::median(&before), fit::median(&after));
        if (a - b).abs() / a.max(b) > 0.03 {
            continue;
        }
        runs.push(Run {
            from: first,
            to: last + 1,
            period: (a + b) / 2.0,
        });
    }
    let mut out = Vec::new();
    let mut r = 0;
    while r < runs.len() {
        let run = runs[r];
        let mut to = run.to;
        let mut period = run.period;
        let mut over = excess(beats, run.from, to, period);
        // Half a beat out: joined with the runs after it (within sixteen
        // bars) until the count is whole.
        let mut next = r + 1;
        while (over - over.round()).abs() > 0.3 && runs.get(next).is_some_and(|n| n.from <= to + 64)
        {
            period = (period + runs[next].period) / 2.0;
            to = runs[next].to;
            over = excess(beats, run.from, to, period);
            next += 1;
        }
        if (over - over.round()).abs() > 0.3 {
            r += 1;
            continue;
        }
        let joined = next > r + 1;
        r = next;
        let actual = to - run.from;
        let whole = over.round() as i64;
        let kind = match whole {
            1 | 2 => SlipKind::Added,
            -2 | -1 => SlipKind::Dropped,
            0 if joined => SlipKind::Shifted,
            _ => continue,
        };
        let expected = (actual as i64 - whole).max(1) as usize;
        out.push(Slip {
            kind,
            from: run.from,
            to,
            time: beats[run.from],
            actual,
            expected,
        });
    }
    out
}

/// Whether a phase says which way the bar is.
const fn decided(p: Phase) -> Option<bool> {
    match p {
        Phase::On => Some(true),
        Phase::Turned(_) => Some(false),
        _ => None,
    }
}

/// Which way most of `bars` read, when at least two say and they mostly
/// agree.
fn side(bars: &[(usize, bool)]) -> Option<bool> {
    let on = bars.iter().filter(|(_, d)| *d).count();
    let turned = bars.len() - on;
    if bars.len() < 2 || on == turned {
        None
    } else if on > turned && on * 4 >= bars.len() * 3 {
        Some(true)
    } else if turned > on && turned * 4 >= bars.len() * 3 {
        Some(false)
    } else {
        None
    }
}

/// The places where the backbeat changes sides and stays there: of the
/// four bars that say before, most one way; of the four after, most the
/// other.
#[must_use]
pub fn turns(beats: &[f64], phases: &[(usize, Phase)]) -> Vec<Slip> {
    let said: Vec<(usize, bool)> = phases
        .iter()
        .filter_map(|(start, p)| decided(*p).map(|d| (*start, d)))
        .collect();
    let mut out: Vec<Slip> = Vec::new();
    let mut k = 2;
    while k + 2 <= said.len() {
        let before = side(&said[k.saturating_sub(4)..k]);
        let after = side(&said[k..(k + 4).min(said.len())]);
        if let (Some(b), Some(a)) = (before, after) {
            if a != b && said[k].1 == a && said[k - 1].1 == b {
                let (from, to) = (said[k - 1].0, said[k].0);
                out.push(Slip {
                    kind: SlipKind::Turnaround,
                    from,
                    to,
                    time: beats.get(to).copied().unwrap_or(0.0),
                    actual: to - from,
                    expected: to - from,
                });
                k += 3;
                continue;
            }
        }
        k += 1;
    }
    out
}

/// `beats` with those from `from` to `to` replaced by `intervals` evenly
/// spaced ones (the hand fix: one beat more or fewer between two good
/// beats). The new beats are `manual` until snapped again.
#[must_use]
pub fn respace(
    beats: &[f64],
    sources: &[String],
    from: usize,
    to: usize,
    intervals: usize,
) -> (Vec<f64>, Vec<String>) {
    if from >= to || to >= beats.len() || intervals == 0 {
        return (beats.to_vec(), sources.to_vec());
    }
    let (a, b) = (beats[from], beats[to]);
    let source = |i: usize| sources.get(i).cloned().unwrap_or_default();
    let mut out: Vec<f64> = beats[..=from].to_vec();
    let mut who: Vec<String> = (0..=from).map(source).collect();
    for k in 1..intervals {
        out.push(a + (b - a) * k as f64 / intervals as f64);
        who.push("manual".to_owned());
    }
    out.extend_from_slice(&beats[to..]);
    who.extend((to..beats.len()).map(source));
    (out, who)
}

/// What a repair did.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Repair {
    /// The spans re-spaced, as they were found.
    pub fixed: Vec<Slip>,
    /// The miscounted spans left as they are (re-spacing them made the
    /// backbeat turn), and the turns of the backbeat no span explains.
    pub left: Vec<Slip>,
}

/// Which way the backbeat reads in the bars before `time` and from
/// `time` on (six bars that say, each side).
fn sides(
    beats: &[f64],
    read: &[(usize, Phase)],
    before: f64,
    after: f64,
) -> (Option<bool>, Option<bool>) {
    let said: Vec<(f64, bool)> = read
        .iter()
        .filter_map(|(start, p)| Some((*beats.get(*start)?, decided(*p)?)))
        .collect();
    let earlier: Vec<(usize, bool)> = said
        .iter()
        .rev()
        .filter(|(t, _)| *t < before)
        .take(6)
        .map(|(_, d)| (0, *d))
        .collect();
    let later: Vec<(usize, bool)> = said
        .iter()
        .filter(|(t, _)| *t >= after)
        .take(6)
        .map(|(_, d)| (0, *d))
        .collect();
    (side(&earlier), side(&later))
}

/// Repairs the beats the tracker added or dropped: each miscounted span
/// is re-spaced with the beats its length says and snapped again. Where
/// the backbeat can be read either side of it, the fix is kept only if it
/// reads the same way on both sides afterwards.
#[must_use]
pub fn repair(
    features: &Features,
    hits: &[KitHit],
    beats: &[f64],
    sources: &[String],
    layout: &Layout,
    params: &TrackParams,
) -> (Vec<f64>, Vec<String>, Repair) {
    let mut beats = beats.to_vec();
    let mut sources = sources.to_vec();
    let mut report = Repair::default();
    // The snap of a repaired song must not drop its first beats again.
    let params = TrackParams {
        from: 0.0,
        ..*params
    };
    let stem = backbeat_stem(features, &beats);
    let read_of =
        |list: &[f64]| stem.map_or_else(Vec::new, |s| phases(&beat_peaks(s, list), layout, 0));
    for _ in 0..24 {
        let refused = |s: &Slip| report.left.iter().any(|l| (l.time - s.time).abs() < 1.0);
        let Some(slip) = miscounted(&beats).into_iter().find(|s| !refused(s)) else {
            break;
        };
        let (from_time, to_time) = (beats[slip.from], beats[slip.to]);
        let (spaced, who) = respace(&beats, &sources, slip.from, slip.to, slip.expected);
        let (snapped, who_snapped) = track::snap(&spaced, hits, &params);
        // A beat that sits on no hit after the snap keeps what it was on,
        // unless it is one of the new ones.
        let who: Vec<String> = who_snapped
            .into_iter()
            .zip(who)
            .map(|(new, old)| {
                if new == track::BETWEEN && old != "manual" && !track::on_a_hit(&old) {
                    old
                } else {
                    new
                }
            })
            .collect();
        let (was_before, was_after) = sides(&beats, &read_of(&beats), from_time, to_time);
        let (now_before, now_after) = sides(&snapped, &read_of(&snapped), from_time, to_time);
        let keep = match (now_before, now_after) {
            // Read both sides: they must agree now.
            (Some(b), Some(a)) => a == b,
            // Not readable after the fix: kept unless it was readable and
            // agreed before it.
            _ => !matches!((was_before, was_after), (Some(b), Some(a)) if a == b),
        };
        if keep {
            beats = snapped;
            sources = who;
            report.fixed.push(slip);
        } else {
            report.left.push(slip);
        }
    }
    let unexplained = turns(&beats, &read_of(&beats));
    report.left.extend(unexplained);
    report.left.sort_by(|a, b| a.time.total_cmp(&b.time));
    (beats, sources, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_reads_by_its_backbeat() {
        let four = Meter::COMMON;
        assert_eq!(bar_phase(&[0.1, 0.9, 0.1, 0.8], four, 0.2), Phase::On);
        assert_eq!(
            bar_phase(&[0.9, 0.1, 0.8, 0.1], four, 0.2),
            Phase::Turned(1)
        );
        assert_eq!(bar_phase(&[0.05, 0.1, 0.0, 0.1], four, 0.2), Phase::Silent);
        // The snare on 2 only: no 4, so not a plain backbeat.
        assert_eq!(bar_phase(&[0.0, 0.9, 0.0, 0.0], four, 0.2), Phase::Unclear);
    }

    #[test]
    fn three_four_reads_by_rotation() {
        let waltz = Meter::new(3, 4);
        assert_eq!(bar_phase(&[0.0, 0.8, 0.7], waltz, 0.2), Phase::On);
        // One beat late: the quiet beat is second.
        assert_eq!(bar_phase(&[0.7, 0.0, 0.8], waltz, 0.2), Phase::Turned(1));
        assert_eq!(bar_phase(&[0.8, 0.7, 0.0], waltz, 0.2), Phase::Turned(2));
    }

    #[test]
    fn six_eight_has_its_backbeat_on_two() {
        let six = Meter::new(6, 8);
        assert_eq!(bar_phase(&[0.1, 0.9], six, 0.2), Phase::On);
        assert_eq!(bar_phase(&[0.9, 0.1], six, 0.2), Phase::Turned(1));
    }

    #[test]
    fn irregular_intervals_are_listed() {
        let mut beats: Vec<f64> = (0..32).map(|i| f64::from(i) * 0.5).collect();
        beats[16] += 0.06;
        let found = irregular(&beats, 0.04);
        let at: Vec<usize> = found.iter().map(|i| i.beat).collect();
        assert_eq!(at, vec![15, 16]);
    }

    /// The backbeat peaks of steady 4/4 beats: quiet, loud, quiet, loud.
    fn backbeat(count: usize, offset: usize) -> Vec<f32> {
        (0..count)
            .map(|i| if (i + offset) % 2 == 1 { 0.9 } else { 0.05 })
            .collect()
    }

    /// 96 true beats at 0.5 s with nine squeezed into the time of eight
    /// from beat 40.
    fn squeezed() -> (Vec<f64>, Vec<f64>) {
        let truth: Vec<f64> = (0..96).map(|i| f64::from(i) * 0.5).collect();
        let mut beats: Vec<f64> = truth[..=40].to_vec();
        for k in 1..9 {
            beats.push(truth[40] + 4.0 * f64::from(k) / 9.0);
        }
        beats.extend_from_slice(&truth[48..]);
        (truth, beats)
    }

    #[test]
    fn an_added_beat_is_found_by_its_span_s_length_and_respacing_mends_it() {
        let (truth, beats) = squeezed();
        let found = miscounted(&beats);
        assert_eq!(found.len(), 1, "{found:?}");
        let slip = found[0];
        assert_eq!(slip.kind, SlipKind::Added);
        assert_eq!(slip.actual, slip.expected + 1);
        let sources = vec!["Kick".to_owned(); beats.len()];
        let (mended, who) = respace(&beats, &sources, slip.from, slip.to, slip.expected);
        assert_eq!(mended.len(), truth.len());
        assert_eq!(who.len(), mended.len());
        for (got, want) in mended.iter().zip(&truth) {
            assert!((got - want).abs() < 1e-9, "{got} for {want}");
        }
        assert!(miscounted(&mended).is_empty());
    }

    #[test]
    fn a_dropped_beat_is_found_too() {
        let truth: Vec<f64> = (0..96).map(|i| f64::from(i) * 0.5).collect();
        // Seven beats stretched over the time of eight from beat 40.
        let mut beats: Vec<f64> = truth[..=40].to_vec();
        for k in 1..7 {
            beats.push(truth[40] + 4.0 * f64::from(k) / 7.0);
        }
        beats.extend_from_slice(&truth[48..]);
        let found = miscounted(&beats);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, SlipKind::Dropped);
        assert_eq!(found[0].expected, found[0].actual + 1);
    }

    #[test]
    fn one_late_hit_and_a_slowing_ending_are_not_miscounts() {
        let mut beats: Vec<f64> = (0..96).map(|i| f64::from(i) * 0.5).collect();
        beats[30] += 0.05;
        assert!(miscounted(&beats).is_empty());
        // The last eight beats slow down.
        let mut t = beats[87];
        for (k, beat) in beats.iter_mut().enumerate().skip(88) {
            t += 0.5 * (1.0 + 0.08 * (k - 87) as f64);
            *beat = t;
        }
        assert!(miscounted(&beats).is_empty(), "{:?}", miscounted(&beats));
    }

    #[test]
    fn half_a_beat_out_and_back_is_one_span() {
        // 120 true beats at 0.5 s. From beat 40 the tracker squeezes
        // half a beat in over four beats, runs on the offbeats for
        // sixteen, and squeezes another half in: one beat too many.
        let truth: Vec<f64> = (0..120).map(|i| f64::from(i) * 0.5).collect();
        let mut beats: Vec<f64> = truth[..=40].to_vec();
        let mut t = truth[40];
        for _ in 0..4 {
            t += 0.5 * 3.5 / 4.0;
            beats.push(t);
        }
        for _ in 0..16 {
            t += 0.5;
            beats.push(t);
        }
        for _ in 0..4 {
            t += 0.5 * 3.5 / 4.0;
            beats.push(t);
        }
        let back = (t / 0.5).round() as usize;
        assert!((truth[back] - t).abs() < 1e-9);
        beats.extend_from_slice(&truth[back + 1..]);
        let found = miscounted(&beats);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, SlipKind::Added);
        assert_eq!((found[0].actual, found[0].expected), (24, 23));
        let sources = vec!["Kick".to_owned(); beats.len()];
        let (mended, _) = respace(
            &beats,
            &sources,
            found[0].from,
            found[0].to,
            found[0].expected,
        );
        for (got, want) in mended.iter().zip(&truth) {
            assert!((got - want).abs() < 1e-9, "{got} for {want}");
        }
    }

    #[test]
    fn a_held_back_intro_is_not_a_miscount() {
        // Eight bars at 0.54 s a beat, then the song at 0.5.
        let mut beats: Vec<f64> = (0..32).map(|i| f64::from(i) * 0.54).collect();
        let mut t = 31.0 * 0.54;
        for _ in 0..96 {
            t += 0.5;
            beats.push(t);
        }
        assert!(miscounted(&beats).is_empty(), "{:?}", miscounted(&beats));
    }

    #[test]
    fn a_tempo_change_is_not_a_miscount() {
        let mut beats: Vec<f64> = (0..48).map(|i| f64::from(i) * 0.5).collect();
        let mut t = 23.5;
        for _ in 0..48 {
            t += 0.42;
            beats.push(t);
        }
        assert!(miscounted(&beats).is_empty(), "{:?}", miscounted(&beats));
    }

    #[test]
    fn the_squeezed_beat_turns_the_backbeat() {
        let (_, beats) = squeezed();
        // Peaks as the snare gives them: on the true 2 and 4.
        let peaks: Vec<f32> = beats
            .iter()
            .map(|t| {
                let nearest = (t / 0.5).round();
                let on_beat = (t - nearest * 0.5).abs() < 0.08;
                if on_beat && (nearest as usize) % 2 == 1 {
                    0.9
                } else {
                    0.05
                }
            })
            .collect();
        let read = phases(&peaks, &Layout::default(), 0);
        let found = turns(&beats, &read);
        assert_eq!(found.len(), 1, "{}", line(&read));
    }

    #[test]
    fn a_real_odd_bar_is_a_turn_with_nothing_miscounted() {
        // Steady beats, but the backbeat moves by one from beat 32: a bar
        // of five in the music.
        let beats: Vec<f64> = (0..72).map(|i| f64::from(i) * 0.5).collect();
        let mut peaks = backbeat(32, 0);
        peaks.extend(backbeat(40, 1));
        let read = phases(&peaks, &Layout::default(), 0);
        assert_eq!(turns(&beats, &read).len(), 1, "{}", line(&read));
        assert!(miscounted(&beats).is_empty());
    }
}
