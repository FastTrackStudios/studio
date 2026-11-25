//! Snapping to the grid that is drawn: the division in force (a Grid
//! region's where one says, the setting's elsewhere), counted through the
//! surface's [`TimeMap`] so a snap after a tempo change lands on a beat.
//!
//! [`Snap`] is what every gesture snaps through — an item's move, an
//! edge, the edit cursor, a time selection, a fade, a hit marker, a
//! marker, a region, an automation point: the grid ([`Snapper`]), whether
//! snapping is on for this gesture ([`Snap::resolve`]: the toggle, flipped
//! while Shift is held — one rule for every surface), and the things worth
//! landing on that are not lines (item edges, markers, hits, the cursor),
//! which win when the pointer is within a few pixels of one.

use crate::grid::{Grid, GridRegions};
use crate::time::TimeMap;

/// How a drag snaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapRule {
    /// The moved point lands on a line (edges, creation, cursor, time
    /// selection).
    Absolute,
    /// The distance moved is a whole number of steps: something off the
    /// grid stays off it by the same amount (the Editor's note move, a
    /// razor move).
    Relative,
    /// The thing's start lands on a line whatever the grab offset (the
    /// arrangement's item move).
    AnchorStart,
}

/// The grid in force, ready to snap to.
#[derive(Clone, Copy)]
pub struct Snapper<'a> {
    pub grid: &'a Grid,
    /// Stretches on a grid of their own, if any.
    pub regions: Option<&'a GridRegions>,
    pub map: &'a dyn TimeMap,
}

impl Snapper<'_> {
    /// The step in beats in force at `t`.
    #[must_use]
    pub fn step_beats_at(&self, t: f64) -> f64 {
        self.regions.map_or_else(
            || self.grid.step_beats(),
            |regions| 4.0 / f64::from(regions.at(t).max(1)),
        )
    }

    /// `t` on the nearest line in force there, or `t` with snapping off.
    #[must_use]
    pub fn snap(&self, t: f64) -> f64 {
        if !self.grid.enabled {
            return t;
        }
        let step = self.step_beats_at(t);
        if !(step.is_finite() && step > 0.0) {
            return t;
        }
        let beats = self.map.beats_at(t);
        self.map.time_at_beats((beats / step).round() * step)
    }

    /// How far a drag of `raw_dt` moves something that started at
    /// `start`, under `rule`.
    #[must_use]
    pub fn delta(&self, rule: SnapRule, start: f64, raw_dt: f64) -> f64 {
        if !self.grid.enabled {
            return raw_dt;
        }
        match rule {
            SnapRule::Absolute | SnapRule::AnchorStart => self.snap(start + raw_dt) - start,
            SnapRule::Relative => {
                let step = self.step_beats_at(start);
                if !(step.is_finite() && step > 0.0) {
                    return raw_dt;
                }
                let from = self.map.beats_at(start);
                let moved = self.map.beats_at(start + raw_dt) - from;
                self.map
                    .time_at_beats((moved / step).round().mul_add(step, from))
                    - start
            }
        }
    }
}

/// One snap for a gesture: the grid, whether it is on, and its magnets.
#[derive(Clone, Copy)]
pub struct Snap<'a> {
    pub grid: Snapper<'a>,
    /// Snapping on for this gesture ([`Snap::resolve`]).
    pub on: bool,
    /// Times worth landing on that are not lines — item edges, markers,
    /// region edges, hits, the cursor — sorted. The dragged thing's own
    /// are left out by whoever gathers them.
    pub magnets: &'a [f64],
    /// How near a magnet pulls, in the surface's time (a few pixels).
    pub reach: f64,
}

impl Snap<'_> {
    /// Whether a gesture snaps: the toggle, flipped while Shift is held —
    /// the rule every surface (the arrangement, the Editor) shares.
    #[must_use]
    pub const fn resolve(toggle: bool, shift: bool) -> bool {
        toggle != shift
    }

    /// The magnet within reach of `t` nearest it, if one is.
    #[must_use]
    pub fn magnet(&self, t: f64) -> Option<f64> {
        let at = self.magnets.partition_point(|m| *m < t);
        [at.checked_sub(1), Some(at)]
            .into_iter()
            .flatten()
            .filter_map(|i| self.magnets.get(i).copied())
            .filter(|m| (m - t).abs() <= self.reach)
            .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
    }

    /// `t` snapped: a magnet in reach, else the nearest line; `t` with
    /// snapping off.
    #[must_use]
    pub fn at(&self, t: f64) -> f64 {
        if !self.on {
            return t;
        }
        self.magnet(t).unwrap_or_else(|| self.grid.snap(t))
    }

    /// How far a drag of `raw_dt` moves something that started at
    /// `start`, under `rule`: the moved point onto a magnet in reach, else
    /// the grid's rule; `raw_dt` with snapping off.
    #[must_use]
    pub fn delta(&self, rule: SnapRule, start: f64, raw_dt: f64) -> f64 {
        if !self.on {
            return raw_dt;
        }
        self.magnet(start + raw_dt)
            .map_or_else(|| self.grid.delta(rule, start, raw_dt), |m| m - start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{ConstantTempo, TempoMap, TempoPoint};

    fn quarters() -> Grid {
        Grid::new(0.25)
    }

    fn tempo(bpm: f64) -> ConstantTempo {
        ConstantTempo {
            units_per_beat: 60.0 / bpm,
            beats_per_bar: 4.0,
            origin: 0.0,
            units_per_second: 1.0,
        }
    }

    #[test]
    fn a_quarter_grid_at_one_tempo_is_the_old_rule() {
        let grid = quarters();
        let map = tempo(120.0);
        let s = Snapper {
            grid: &grid,
            regions: None,
            map: &map,
        };
        for t in [0.0, 0.2, 0.26, 1.74, 3.1] {
            let old = (t / 0.5_f64).round() * 0.5;
            assert!((s.snap(t) - old).abs() < 1e-12, "{t}");
        }
    }

    #[test]
    fn the_snap_follows_the_tempo_map() {
        let grid = quarters();
        let map = TempoMap::new(
            [
                TempoPoint {
                    at: 0.0,
                    bpm: 120.0,
                    beats_per_bar: 4,
                    beat_unit: 4,
                },
                TempoPoint {
                    at: 2.0,
                    bpm: 60.0,
                    beats_per_bar: 4,
                    beat_unit: 4,
                },
            ],
            1.0,
        );
        let s = Snapper {
            grid: &grid,
            regions: None,
            map: &map,
        };
        // After the change beats are a second apart: 3.4 is nearer 3.0
        // than 3.5, which one tempo would have said.
        assert!((s.snap(3.4) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_region_snaps_to_its_own_grid() {
        let grid = quarters();
        let map = tempo(60.0);
        let regions = GridRegions::new(4, &[(10.0, 20.0, 16)], 1e-6);
        let s = Snapper {
            grid: &grid,
            regions: Some(&regions),
            map: &map,
        };
        assert!((s.snap(5.3) - 5.0).abs() < 1e-12);
        assert!((s.snap(12.3) - 12.25).abs() < 1e-12);
    }

    #[test]
    fn relative_keeps_the_offset_and_absolute_lands_on_the_line() {
        let grid = quarters();
        let map = tempo(60.0);
        let s = Snapper {
            grid: &grid,
            regions: None,
            map: &map,
        };
        // From 0.3, dragged 1.9: relative moves two beats, keeping .3;
        // absolute lands the point on 2.0.
        assert!((s.delta(SnapRule::Relative, 0.3, 1.9) - 2.0).abs() < 1e-12);
        assert!((s.delta(SnapRule::Absolute, 0.3, 1.9) - 1.7).abs() < 1e-12);
        assert!((s.delta(SnapRule::AnchorStart, 0.3, 1.9) - 1.7).abs() < 1e-12);
    }

    #[test]
    fn off_is_off() {
        let mut grid = quarters();
        grid.enabled = false;
        let map = tempo(60.0);
        let s = Snapper {
            grid: &grid,
            regions: None,
            map: &map,
        };
        assert_eq!(s.snap(0.37), 0.37);
        assert_eq!(s.delta(SnapRule::Relative, 0.3, 1.9), 1.9);
    }

    #[test]
    fn a_magnet_in_reach_wins_over_the_grid() {
        let grid = quarters();
        let map = tempo(120.0);
        let grid = Snapper {
            grid: &grid,
            regions: None,
            map: &map,
        };
        let magnets = [1.13, 2.9];
        let snap = Snap {
            grid,
            on: true,
            magnets: &magnets,
            reach: 0.02,
        };
        assert!((snap.at(1.12) - 1.13).abs() < 1e-12, "the magnet");
        assert!((snap.at(1.2) - 1.0).abs() < 1e-12, "out of reach: the line");
        assert!((snap.delta(SnapRule::AnchorStart, 2.0, 0.89) - 0.9).abs() < 1e-12);
        let off = Snap { on: false, ..snap };
        assert!((off.at(1.12) - 1.12).abs() < 1e-12, "off: as it is");
    }

    #[test]
    fn shift_flips_the_toggle() {
        assert!(Snap::resolve(true, false));
        assert!(!Snap::resolve(true, true));
        assert!(Snap::resolve(false, true));
        assert!(!Snap::resolve(false, false));
    }
}
