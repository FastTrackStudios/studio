//! Live's time, in seconds.
//!
//! Live keeps every arrangement position in beats; a DAW that keeps
//! seconds needs the set's tempo map to place anything. [`TempoMap`] is
//! that map (steps and linear ramps, as Live plays its tempo automation),
//! and [`sample_seconds`] reads a warped clip's warp markers.

use crate::types::{AbletonLiveSet, AutomationPoint, WarpMarker};

/// Beats → seconds through a tempo map: the set's tempo, or its tempo
/// automation (linear ramps between breakpoints, as Live plays them).
pub struct TempoMap {
    /// (beat, bpm), sorted, the first at or before beat 0.
    points: Vec<(f64, f64)>,
}

impl TempoMap {
    /// The map of `tempo`, or of its automation (beats → bpm) when there is any.
    #[must_use]
    pub fn new(tempo: f64, automation: &[AutomationPoint]) -> Self {
        let mut points: Vec<(f64, f64)> = automation
            .iter()
            .filter(|p| p.value > 0.0)
            .map(|p| (p.time, p.value))
            .collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        let fallback = if tempo > 0.0 { tempo } else { 120.0 };
        // Live parks a breakpoint before the arrangement, at a far negative
        // time; breakpoints ramp linearly, so the tempo at beat 0 is read
        // off the segment that crosses it.
        let before = points.iter().rev().find(|p| p.0 <= 0.0).copied();
        let after = points.iter().find(|p| p.0 > 0.0).copied();
        let at_zero = match (before, after) {
            (Some((t0, v0)), Some((t1, v1))) if t1 > t0 => v0 + (v1 - v0) * (0.0 - t0) / (t1 - t0),
            (Some((_, v0)), _) => v0,
            (None, _) => fallback,
        };
        points.retain(|p| p.0 > 0.0);
        points.insert(0, (0.0, at_zero));
        Self { points }
    }

    /// Seconds from the arrangement's start to `beat`.
    #[must_use]
    pub fn seconds_at(&self, beat: f64) -> f64 {
        let mut seconds = 0.0;
        for (i, &(from, bpm)) in self.points.iter().enumerate() {
            if beat <= from {
                break;
            }
            let (to, bpm_to) = self
                .points
                .get(i + 1)
                .copied()
                .unwrap_or((f64::INFINITY, bpm));
            // Two breakpoints at one beat are a step: no time passes.
            if to - from <= 1e-12 {
                continue;
            }
            let until = beat.min(to);
            let span = until - from;
            if to.is_infinite() || (bpm_to - bpm).abs() < 1e-9 {
                seconds += 60.0 * span / bpm;
            } else {
                // Linear ramp: bpm(b) = bpm + k (b - from); ∫ 60 / bpm(b) db.
                let k = (bpm_to - bpm) / (to - from);
                let at_until = bpm + k * span;
                seconds += 60.0 / k * (at_until / bpm).ln();
            }
            if beat <= to {
                break;
            }
        }
        seconds
    }
}

impl TempoMap {
    /// The map of `set`'s tempo and tempo automation.
    #[must_use]
    pub fn of(set: &AbletonLiveSet) -> Self {
        Self::new(set.tempo, &set.tempo_automation)
    }

    /// The tempo at `beat`, bpm.
    #[must_use]
    pub fn bpm_at(&self, beat: f64) -> f64 {
        let i = self.points.iter().rposition(|p| p.0 <= beat).unwrap_or(0);
        let (from, bpm) = self.points[i];
        match self.points.get(i + 1) {
            Some(&(to, bpm_to)) if to > from => bpm + (bpm_to - bpm) * (beat - from) / (to - from),
            _ => bpm,
        }
    }

    /// The map as steps, `(beat, bpm)` from beat 0: one per change of
    /// tempo, held until the next. Live writes a step as two breakpoints
    /// at one beat — the tempo it leaves, then the one it takes — and pads
    /// a held tempo with breakpoints that repeat it; a map of held points
    /// (REAPER's square tempo points, the engine's) wants neither: two
    /// points at one moment read as either tempo depending on who reads
    /// them. The last breakpoint at a beat is the tempo from it. A ramp
    /// comes out as its breakpoints, each held.
    #[must_use]
    pub fn steps(&self) -> Vec<(f64, f64)> {
        let mut out: Vec<(f64, f64)> = Vec::with_capacity(self.points.len());
        for &(beat, bpm) in &self.points {
            match out.last_mut() {
                Some(last) if (beat - last.0).abs() < 1e-9 => last.1 = bpm,
                _ => out.push((beat, bpm)),
            }
        }
        out.dedup_by(|later, earlier| (later.1 - earlier.1).abs() < 1e-9);
        out
    }

    /// Seconds `beats` long starting at `at`.
    #[must_use]
    pub fn span(&self, at: f64, beats: f64) -> f64 {
        self.seconds_at(at + beats) - self.seconds_at(at)
    }
}

/// Where a warped clip's `beat` falls in its audio file, seconds: linear
/// between the warp markers around it, and past the first or last at that
/// segment's rate. `None` with fewer than two markers.
#[must_use]
pub fn sample_seconds(markers: &[WarpMarker], beat: f64) -> Option<f64> {
    let mut sorted: Vec<&WarpMarker> = markers.iter().collect();
    sorted.sort_by(|a, b| a.beat_time.total_cmp(&b.beat_time));
    if sorted.len() < 2 {
        return None;
    }
    let i = sorted
        .windows(2)
        .position(|w| beat <= w[1].beat_time)
        .unwrap_or(sorted.len() - 2);
    let (a, b) = (sorted[i], sorted[i + 1]);
    let beats = b.beat_time - a.beat_time;
    if beats <= 0.0 {
        return Some(a.sec_time);
    }
    Some(a.sec_time + (b.sec_time - a.sec_time) * (beat - a.beat_time) / beats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempo(time: f64, bpm: f64) -> AutomationPoint {
        AutomationPoint {
            time,
            value: bpm,
            curve_control_1: None,
            curve_control_2: None,
        }
    }

    #[test]
    fn seconds_follow_the_tempo_map_steps_and_ramps() {
        // 120 bpm parked before the set and held to beat 8; a step to 60
        // there (two breakpoints at one beat); held to 16; a ramp from 60
        // to 120 over beats 16..32; then 120.
        let map = TempoMap::new(
            100.0,
            &[
                tempo(-63_072_000.0, 120.0),
                tempo(8.0, 120.0),
                tempo(8.0, 60.0),
                tempo(16.0, 60.0),
                tempo(32.0, 120.0),
            ],
        );
        assert!((map.seconds_at(8.0) - 4.0).abs() < 1e-9, "8 beats at 120");
        assert!((map.seconds_at(16.0) - 12.0).abs() < 1e-9, "+8 beats at 60");
        // ∫ 60 / (60 + 60 (b-16)/16) db over 16 beats = 16 ln 2.
        let ramp = 16.0 * std::f64::consts::LN_2;
        assert!((map.seconds_at(32.0) - (12.0 + ramp)).abs() < 1e-9);
        assert!(
            (map.seconds_at(36.0) - (12.0 + ramp + 2.0)).abs() < 1e-9,
            "then 120"
        );
        assert!(map.seconds_at(1e6).is_finite());
    }

    #[test]
    fn the_tempo_at_zero_is_read_off_the_ramp_that_crosses_it() {
        // Parked at 100 bpm far back, ramping to 140 at beat 40: the ramp
        // is so long that beat 0 is still at (almost) 100.
        let map = TempoMap::new(120.0, &[tempo(-1000.0, 100.0), tempo(40.0, 140.0)]);
        let at_zero = map.points[0].1;
        assert!((at_zero - (100.0 + 40.0 * 1000.0 / 1040.0)).abs() < 1e-9);
    }

    #[test]
    fn a_warped_clip_reads_its_audio_through_its_warp_markers() {
        let m = |sec_time, beat_time| WarpMarker {
            sec_time,
            beat_time,
        };
        // 0..8 beats over 0..4 s of audio, then 8..16 beats over 4..10 s.
        let markers = [m(0.0, 0.0), m(4.0, 8.0), m(10.0, 16.0)];
        assert_eq!(sample_seconds(&markers, 4.0), Some(2.0));
        assert_eq!(sample_seconds(&markers, 12.0), Some(7.0));
        assert_eq!(
            sample_seconds(&markers, 20.0),
            Some(13.0),
            "past the last at its rate"
        );
        assert_eq!(sample_seconds(&markers[..1], 4.0), None);
    }

    #[test]
    fn a_step_is_one_point_at_the_tempo_it_takes() {
        // JT's recital: 160 parked before the set, a step to 116 at beat 768
        // (160 then 116 there), 116 repeated at 1092, a step to 95 at
        // 1094.75.
        let map = TempoMap::new(
            120.0,
            &[
                tempo(-63_072_000.0, 160.0),
                tempo(768.0, 160.0),
                tempo(768.0, 116.0),
                tempo(1092.0, 116.0),
                tempo(1092.0, 116.0),
                tempo(1094.75, 116.0),
                tempo(1094.75, 95.0),
            ],
        );
        assert_eq!(
            map.steps(),
            [(0.0, 160.0), (768.0, 116.0), (1094.75, 95.0)],
            "one point per change, the tempo taken, nothing repeated"
        );
        // A step back to the tempo before it is no change at all.
        let back = TempoMap::new(
            120.0,
            &[tempo(0.0, 100.0), tempo(8.0, 140.0), tempo(8.0, 100.0)],
        );
        assert_eq!(back.steps(), [(0.0, 100.0)]);
    }

    #[test]
    fn the_tempo_between_breakpoints_is_on_the_ramp() {
        let map = TempoMap::new(
            120.0,
            &[tempo(-1.0, 100.0), tempo(0.0, 100.0), tempo(10.0, 200.0)],
        );
        assert!((map.bpm_at(5.0) - 150.0).abs() < 1e-9);
        assert!((map.bpm_at(50.0) - 200.0).abs() < 1e-9);
    }
}
