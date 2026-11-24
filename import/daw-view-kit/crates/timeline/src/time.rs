//! How a surface's time unit maps to musical time: beats, measures and
//! seconds, through one tempo ([`ConstantTempo`]) or a tempo map
//! ([`TempoMap`]).
//!
//! Time is in the surface's own units: seconds for the arrangement,
//! ticks or analysis frames for the Editor. `units_per_second` says how
//! many of them a second is.

/// How the surface's time unit maps to musical time.
pub trait TimeMap {
    /// Beats from the timeline origin at time `t`.
    fn beats_at(&self, t: f64) -> f64;
    /// Inverse of [`TimeMap::beats_at`].
    fn time_at_beats(&self, beats: f64) -> f64;
    /// The measure containing `beats`.
    fn measure_at(&self, beats: f64) -> Measure;
    /// Seconds at time `t`, for snapping in ms and for sample limits.
    fn seconds_at(&self, t: f64) -> f64;
    /// Units one beat lasts at time `t`.
    fn units_per_beat_at(&self, t: f64) -> f64;
}

/// A measure: where it starts and how long it is (in beats from the
/// origin), and its number as the user counts it — from one, with no bar
/// zero (the bar before bar 1 is bar -1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    pub start: f64,
    pub beats: f64,
    pub number: i64,
    pub denom: u32,
}

/// The user's bar number for a zero-based measure index: no bar zero.
const fn bar_number(index: i64) -> i64 {
    if index >= 0 { index + 1 } else { index }
}

/// One tempo, one signature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConstantTempo {
    pub units_per_beat: f64,
    pub beats_per_bar: f64,
    /// Where beat zero is, in units.
    pub origin: f64,
    pub units_per_second: f64,
}

impl ConstantTempo {
    fn upb(&self) -> f64 {
        self.units_per_beat.max(f64::EPSILON)
    }
}

impl TimeMap for ConstantTempo {
    fn beats_at(&self, t: f64) -> f64 {
        (t - self.origin) / self.upb()
    }

    fn time_at_beats(&self, beats: f64) -> f64 {
        beats.mul_add(self.upb(), self.origin)
    }

    #[expect(clippy::cast_possible_truncation, reason = "a bar index")]
    fn measure_at(&self, beats: f64) -> Measure {
        let per = self.beats_per_bar.max(1.0);
        let index = (beats / per).floor();
        Measure {
            start: index * per,
            beats: per,
            number: bar_number(index as i64),
            denom: 4,
        }
    }

    fn seconds_at(&self, t: f64) -> f64 {
        t / self.units_per_second.max(f64::EPSILON)
    }

    fn units_per_beat_at(&self, _t: f64) -> f64 {
        self.upb()
    }
}

/// A tempo or signature change, at a time in the surface's units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoPoint {
    pub at: f64,
    pub bpm: f64,
    /// The signature's top number: how many beats to a bar.
    pub beats_per_bar: u32,
    /// Its bottom number: which note gets the beat.
    pub beat_unit: u32,
}

/// One beat of the map, and where it falls.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Beat {
    /// When it sounds, in units.
    pub at: f64,
    /// Which measure it is in, counted from one.
    pub measure: u32,
    /// Which beat of that measure, counted from one.
    pub beat: u32,
    /// How many beats this measure has — the signature in force.
    pub per_bar: u32,
    /// How long a beat lasts here, in units.
    pub units_per_beat: f64,
}

impl Beat {
    /// The first beat of a measure.
    #[must_use]
    pub const fn is_downbeat(self) -> bool {
        self.beat == 1
    }
}

/// A stretch of the map under one point.
#[derive(Clone, Copy, Debug)]
struct Segment {
    point: TempoPoint,
    /// Units a beat lasts.
    upb: f64,
    /// Beats from the origin at `point.at` (whole: a change inside a
    /// beat starts the next one there).
    beats: f64,
    /// The zero-based measure and beat-in-measure at `point.at`.
    measure: u32,
    beat: u32,
}

/// A piecewise tempo map.
///
/// Every bar line is where the tempo and the signature BEFORE it put it.
/// Two rules:
///
/// - A **tempo** change alters how long the following beats take. The
///   count carries on through it: beat three is still beat three.
/// - A **signature** change starts a NEW MEASURE at that point, because a
///   bar of four and a bar of three cannot share a bar line.
///
/// A change that falls inside a beat cuts it short: the grid restarts at
/// the change rather than carrying the old beat length across it.
#[derive(Clone, Debug)]
pub struct TempoMap {
    segments: Vec<Segment>,
    units_per_second: f64,
}

fn upb_of(bpm: f64, units_per_second: f64) -> f64 {
    60.0 / if bpm > 0.0 { bpm } else { 120.0 } * units_per_second
}

/// The next zero-based (measure, beat) after one of `per_bar` beats.
const fn step(measure: u32, beat: u32, per_bar: u32) -> (u32, u32) {
    if beat + 1 >= per_bar {
        (measure.saturating_add(1), 0)
    } else {
        (measure, beat + 1)
    }
}

impl TempoMap {
    /// From points in any order. Two at the same time: the first given
    /// wins. With none, 120 bpm in 4/4 from zero.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "whole beat counts within one segment"
    )]
    pub fn new(points: impl IntoIterator<Item = TempoPoint>, units_per_second: f64) -> Self {
        let mut points: Vec<TempoPoint> = points.into_iter().collect();
        points.sort_by(|a, b| a.at.total_cmp(&b.at));
        points.dedup_by(|later, first| later.at <= first.at + 1e-9);
        if points.is_empty() {
            points.push(TempoPoint {
                at: 0.0,
                bpm: 120.0,
                beats_per_bar: 4,
                beat_unit: 4,
            });
        }
        let mut segments: Vec<Segment> = Vec::with_capacity(points.len());
        for point in points {
            let upb = upb_of(point.bpm, units_per_second);
            let Some(prev) = segments.last() else {
                segments.push(Segment {
                    point,
                    upb,
                    beats: 0.0,
                    measure: 0,
                    beat: 0,
                });
                continue;
            };
            let per_bar = prev.point.beats_per_bar.max(1);
            // Whole beats laid down in the previous segment before this
            // change: every one that starts before it.
            let span = (point.at - prev.point.at) / prev.upb;
            let laid = (span - 1e-9).ceil().max(1.0);
            let (mut measure, mut beat) = (prev.measure, prev.beat);
            // The last beat laid, then one step on from it.
            let last = laid as u64 - 1;
            let pos = u64::from(beat) + last;
            measure = measure.saturating_add((pos / u64::from(per_bar)) as u32);
            beat = (pos % u64::from(per_bar)) as u32;
            if point.beats_per_bar == prev.point.beats_per_bar {
                (measure, beat) = step(measure, beat, per_bar);
            } else {
                (measure, beat) = (measure.saturating_add(1), 0);
            }
            segments.push(Segment {
                point,
                upb,
                beats: prev.beats + laid,
                measure,
                beat,
            });
        }
        Self {
            segments,
            units_per_second,
        }
    }

    /// The point in force at `t`: the last at or before it (the first
    /// before the map starts).
    #[must_use]
    pub fn point_at(&self, t: f64) -> TempoPoint {
        self.segment_at(t).point
    }

    fn segment_at(&self, t: f64) -> &Segment {
        let i = self
            .segments
            .partition_point(|s| s.point.at <= t + 1e-9)
            .saturating_sub(1);
        &self.segments[i]
    }

    fn segment_at_beats(&self, beats: f64) -> &Segment {
        let i = self
            .segments
            .partition_point(|s| s.beats <= beats + 1e-9)
            .saturating_sub(1);
        &self.segments[i]
    }

    /// Every beat from the start of the map up to `to`, in order.
    ///
    /// From the start rather than from the visible left edge, because a
    /// measure number is a COUNT from the beginning. Stopped by `to`, and
    /// by `limit` so a corrupt tempo map cannot spin.
    #[must_use]
    pub fn beats(&self, to: f64, limit: usize) -> Vec<Beat> {
        let mut out = Vec::new();
        for (i, seg) in self.segments.iter().enumerate() {
            let end = self
                .segments
                .get(i + 1)
                .map_or(f64::INFINITY, |n| n.point.at);
            let per_bar = seg.point.beats_per_bar.max(1);
            let (mut measure, mut beat) = (seg.measure, seg.beat);
            let mut k = 0.0_f64;
            loop {
                let at = k.mul_add(seg.upb, seg.point.at);
                if at > to || out.len() >= limit {
                    return out;
                }
                if at >= end - 1e-9 {
                    break;
                }
                out.push(Beat {
                    at,
                    measure: measure + 1,
                    beat: beat + 1,
                    per_bar,
                    units_per_beat: seg.upb,
                });
                (measure, beat) = step(measure, beat, per_bar);
                k += 1.0;
            }
        }
        out
    }

    /// The beats between `t0` and `t1` (inclusive), counted from the
    /// start of the map.
    pub fn beats_between(&self, t0: f64, t1: f64, limit: usize) -> impl Iterator<Item = Beat> {
        self.beats(t1, limit)
            .into_iter()
            .filter(move |b| b.at >= t0)
    }
}

impl TimeMap for TempoMap {
    fn beats_at(&self, t: f64) -> f64 {
        let seg = self.segment_at(t);
        seg.beats + (t - seg.point.at) / seg.upb
    }

    fn time_at_beats(&self, beats: f64) -> f64 {
        let seg = self.segment_at_beats(beats);
        (beats - seg.beats).mul_add(seg.upb, seg.point.at)
    }

    #[expect(clippy::cast_possible_truncation, reason = "a bar index")]
    fn measure_at(&self, beats: f64) -> Measure {
        let seg = self.segment_at_beats(beats);
        let per = f64::from(seg.point.beats_per_bar.max(1));
        // Beats into the segment's first (possibly partial) measure.
        // A signature change mid-measure starts its segment at beat zero
        // of a new measure, so the measure starts at the change.
        let pos = f64::from(seg.beat) + (beats - seg.beats);
        let offset = (pos / per).floor();
        let start = offset.mul_add(per, seg.beats - f64::from(seg.beat));
        let index = i64::from(seg.measure) + offset as i64;
        Measure {
            start,
            beats: per,
            number: bar_number(index),
            denom: seg.point.beat_unit.max(1),
        }
    }

    fn seconds_at(&self, t: f64) -> f64 {
        t / self.units_per_second.max(f64::EPSILON)
    }

    fn units_per_beat_at(&self, t: f64) -> f64 {
        self.segment_at(t).upb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(at: f64, bpm: f64, beats_per_bar: u32) -> TempoPoint {
        TempoPoint {
            at,
            bpm,
            beats_per_bar,
            beat_unit: 4,
        }
    }

    #[test]
    fn a_constant_tempo_counts_bars_from_one_with_no_bar_zero() {
        let c = ConstantTempo {
            units_per_beat: 960.0,
            beats_per_bar: 3.0,
            origin: 480.0,
            units_per_second: 1920.0,
        };
        assert_eq!(c.beats_at(480.0 + 960.0 * 4.0), 4.0);
        assert_eq!(c.time_at_beats(4.0), 480.0 + 960.0 * 4.0);
        assert_eq!(c.measure_at(4.0).number, 2);
        assert_eq!(c.measure_at(4.0).start, 3.0);
        assert_eq!(c.measure_at(-0.5).number, -1);
        assert_eq!(c.seconds_at(1920.0), 1.0);
    }

    #[test]
    fn a_tempo_change_carries_the_count() {
        // 120 bpm 4/4, then 60 bpm from 1 s (beat 3 of bar 1).
        let map = TempoMap::new([point(0.0, 120.0, 4), point(1.0, 60.0, 4)], 1.0);
        let beats = map.beats(4.0, 100);
        let labels: Vec<_> = beats.iter().map(|b| (b.at, b.measure, b.beat)).collect();
        assert_eq!(
            labels,
            vec![
                (0.0, 1, 1),
                (0.5, 1, 2),
                (1.0, 1, 3),
                (2.0, 1, 4),
                (3.0, 2, 1),
                (4.0, 2, 2)
            ]
        );
        assert_eq!(map.beats_at(3.0), 4.0);
        assert_eq!(map.time_at_beats(4.0), 3.0);
        assert_eq!(map.measure_at(4.0).number, 2);
    }

    #[test]
    fn a_signature_change_starts_a_measure_there() {
        // 4/4 at 60 bpm, then 3/4 from beat 2 of bar 1.
        let map = TempoMap::new([point(0.0, 60.0, 4), point(2.0, 60.0, 3)], 1.0);
        let beats = map.beats(5.0, 100);
        let labels: Vec<_> = beats.iter().map(|b| (b.measure, b.beat)).collect();
        assert_eq!(labels, vec![(1, 1), (1, 2), (2, 1), (2, 2), (2, 3), (3, 1)]);
        let m = map.measure_at(2.5);
        assert_eq!((m.number, m.start, m.beats), (2, 2.0, 3.0));
    }

    #[test]
    fn a_change_inside_a_beat_cuts_it_short() {
        // 60 bpm; 120 bpm from 1.5 s, mid-beat 2.
        let map = TempoMap::new([point(0.0, 60.0, 4), point(1.5, 120.0, 4)], 1.0);
        let ats: Vec<_> = map.beats(2.6, 100).iter().map(|b| (b.at, b.beat)).collect();
        assert_eq!(ats, vec![(0.0, 1), (1.0, 2), (1.5, 3), (2.0, 4), (2.5, 1)]);
    }

    #[test]
    fn units_scale_the_map() {
        // Ticks at 960 per second.
        let map = TempoMap::new([point(0.0, 60.0, 4)], 960.0);
        assert_eq!(map.units_per_beat_at(0.0), 960.0);
        assert_eq!(map.seconds_at(1920.0), 2.0);
    }

    #[test]
    fn an_empty_map_is_120_in_four() {
        let map = TempoMap::new([], 1.0);
        assert_eq!(map.point_at(10.0), point(0.0, 120.0, 4));
        assert_eq!(map.beats_at(1.0), 2.0);
    }
}
