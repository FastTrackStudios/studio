//! Where grid lines fall, through a [`TimeMap`]: multiples of a step in
//! beats, and the bar lines. Counted from the map's beat zero, so after a
//! tempo change a line still sits on the beat it belongs to rather than
//! where one tempo multiplied out would put it.

use crate::time::TimeMap;

/// The times of every multiple of `step_beats` beats between `from` and
/// `to` (inclusive), at most `limit` of them.
#[must_use]
pub fn lines(map: &impl TimeMap, from: f64, to: f64, step_beats: f64, limit: usize) -> Vec<f64> {
    let mut out = Vec::new();
    if !(step_beats.is_finite() && step_beats > 0.0) || to < from {
        return out;
    }
    let mut n = (map.beats_at(from) / step_beats - 1e-9).ceil();
    while out.len() < limit {
        let t = map.time_at_beats(n * step_beats);
        if t > to + 1e-9 {
            break;
        }
        if t >= from - 1e-9 {
            out.push(t);
        }
        n += 1.0;
    }
    out
}

/// The times of the bar lines between `from` and `to` (inclusive), at
/// most `limit` of them: every whole beat that starts a measure.
#[must_use]
pub fn bars(map: &impl TimeMap, from: f64, to: f64, limit: usize) -> Vec<f64> {
    let mut out = Vec::new();
    if to < from {
        return out;
    }
    let mut beat = (map.beats_at(from) - 1e-9).ceil();
    // Whole beats from the first one in view; bounded by the beats a
    // `limit` of bars can hold so a degenerate map cannot spin.
    for _ in 0..limit.saturating_mul(64) {
        let t = map.time_at_beats(beat);
        if t > to + 1e-9 || out.len() >= limit {
            break;
        }
        let measure = map.measure_at(beat);
        if (measure.start - beat).abs() < 1e-6 {
            out.push(t);
        }
        beat += 1.0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{ConstantTempo, TempoMap, TempoPoint};

    fn point(at: f64, bpm: f64, beats_per_bar: u32) -> TempoPoint {
        TempoPoint {
            at,
            bpm,
            beats_per_bar,
            beat_unit: 4,
        }
    }

    #[test]
    fn a_constant_tempo_lays_lines_every_step() {
        let c = ConstantTempo {
            units_per_beat: 0.5,
            beats_per_bar: 4.0,
            origin: 0.0,
            units_per_second: 1.0,
        };
        assert_eq!(lines(&c, 0.1, 1.0, 0.5, 100), vec![0.25, 0.5, 0.75, 1.0]);
        assert_eq!(bars(&c, 0.0, 4.5, 100), vec![0.0, 2.0, 4.0]);
    }

    /// The lane grid's bug: one tempo multiplied out. After a change to
    /// 60 bpm at 2 s, beats are a second apart, not half.
    #[test]
    fn lines_follow_a_tempo_change() {
        let map = TempoMap::new([point(0.0, 120.0, 4), point(2.0, 60.0, 4)], 1.0);
        assert_eq!(
            lines(&map, 1.0, 5.0, 1.0, 100),
            vec![1.0, 1.5, 2.0, 3.0, 4.0, 5.0]
        );
        // Bar 2 starts at 2 s (four half-second beats), bar 3 four
        // one-second beats later.
        assert_eq!(bars(&map, 0.0, 7.0, 100), vec![0.0, 2.0, 6.0]);
    }

    #[test]
    fn a_signature_change_mid_bar_starts_a_bar_there() {
        // 4/4 at 60, 3/4 from beat 3 of bar 1.
        let map = TempoMap::new([point(0.0, 60.0, 4), point(2.0, 60.0, 3)], 1.0);
        assert_eq!(bars(&map, 0.0, 8.0, 100), vec![0.0, 2.0, 5.0, 8.0]);
    }

    #[test]
    fn the_limit_holds() {
        let map = TempoMap::new([point(0.0, 120.0, 4)], 1.0);
        assert_eq!(lines(&map, 0.0, 1e9, 0.25, 10).len(), 10);
        assert_eq!(bars(&map, 0.0, 1e9, 10).len(), 10);
    }
}
