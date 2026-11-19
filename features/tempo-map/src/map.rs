//! The tempo map: one tempo point per bar.
//!
//! Each downbeat is a weighted line through the beats four either side of
//! it, so a fill or one late hit cannot drag a bar line; beats that sit on
//! no hit count for less. A ritard is taken as played (the beats as
//! given) from the bar it starts at. The audio before bar 1 is one
//! count-in bar.

use crate::fit;
use crate::meter::{Layout, Meter};
use crate::track::on_a_hit;

/// How many beats either side of a downbeat its line is fitted through.
pub const FIT_REACH: usize = 4;

/// One bar of a map.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bar {
    /// The song bar, from 1.
    pub number: u32,
    /// Its downbeat, seconds of the recording.
    pub time: f64,
    pub meter: Meter,
    /// Its tempo, quarter notes a minute.
    pub bpm: f64,
}

/// The bar before bar 1, holding the audio that comes before the song.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CountIn {
    /// How many bars it is: one, unless one would be slower than a tempo
    /// can be.
    pub bars: u32,
    pub bpm: f64,
    pub meter: Meter,
}

/// A tempo point, as a session takes it.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TempoPoint {
    pub time: f64,
    pub bpm: f64,
    /// The meter from here, where it is set or changes.
    pub meter: Option<Meter>,
}

/// A song's tempo map.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct TempoMap {
    /// The song's bars, bar 1 first.
    pub bars: Vec<Bar>,
    /// Where the last bar ends, seconds.
    pub end: f64,
    /// The bars before bar 1, when the song does not start at 0.
    pub count_in: Option<CountIn>,
}

/// What a map is built from besides the beats.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct MapOptions {
    /// The index of the beat that is bar 1's downbeat.
    pub bar1: usize,
    pub layout: Layout,
    /// From this song bar on the downbeats are the beats as given (a
    /// ritard).
    #[serde(default)]
    pub raw_from_bar: Option<u32>,
}

/// The slowest and fastest a tempo point may be.
pub const BPM_RANGE: std::ops::RangeInclusive<f64> = 20.0..=400.0;

impl TempoMap {
    /// The map of `beats` (each with what it sits on, `sources`).
    #[must_use]
    pub fn build(beats: &[f64], sources: &[String], options: &MapOptions) -> Self {
        let from = options.bar1.min(beats.len());
        let song = &beats[from..];
        if song.is_empty() {
            return Self::default();
        }
        let weights: Vec<f64> = (0..song.len())
            .map(|i| match sources.get(from + i) {
                Some(s) if !on_a_hit(s) => 0.3,
                _ => 1.0,
            })
            .collect();
        let index: Vec<f64> = (0..song.len()).map(|i| i as f64).collect();
        let spans = options.layout.bars(song.len());
        let mut downbeats: Vec<(f64, Meter)> = spans
            .iter()
            .map(|span| {
                let i = span.start;
                let number = u32::try_from(span.index)
                    .unwrap_or(u32::MAX)
                    .saturating_add(1);
                let time = if options.raw_from_bar.is_some_and(|raw| number >= raw) {
                    song[i]
                } else {
                    let lo = i.saturating_sub(FIT_REACH);
                    let hi = (i + FIT_REACH + 1).min(song.len());
                    fit::line_at(&index[lo..hi], &song[lo..hi], &weights[lo..hi], i as f64)
                };
                (time, span.meter)
            })
            .collect();
        // The last beat on a bar line closes the song; otherwise the last
        // bar runs its length at the tempo of the last beats.
        let closes = spans.len() > 1 && spans.last().is_some_and(|s| s.start + 1 == song.len());
        let end = if closes {
            downbeats.pop().map_or(0.0, |(t, _)| t)
        } else {
            let tail = &song[song.len().saturating_sub(8)..];
            let gaps: Vec<f64> = tail.windows(2).map(|w| w[1] - w[0]).collect();
            let (last, meter) = downbeats.last().copied().unwrap_or((0.0, Meter::COMMON));
            last + meter.pulses() as f64 * fit::median(&gaps)
        };
        let mut bars = Vec::with_capacity(downbeats.len());
        for (i, (time, meter)) in downbeats.iter().enumerate() {
            let next = downbeats.get(i + 1).map_or(end, |(t, _)| *t);
            bars.push(Bar {
                number: u32::try_from(i).unwrap_or(u32::MAX).saturating_add(1),
                time: *time,
                meter: *meter,
                bpm: meter.bpm_of(next - time),
            });
        }
        let count_in = bars.first().and_then(|first| count_in(first, first.time));
        Self {
            bars,
            end,
            count_in,
        }
    }

    /// The downbeats of the song's bars, then where the last ends.
    #[must_use]
    pub fn downbeats(&self) -> Vec<f64> {
        self.bars
            .iter()
            .map(|b| b.time)
            .chain(std::iter::once(self.end))
            .collect()
    }

    /// The bar starting at or before `time` (none before bar 1).
    #[must_use]
    pub fn bar_at(&self, time: f64) -> Option<&Bar> {
        let i = self.bars.partition_point(|b| b.time <= time);
        i.checked_sub(1).and_then(|i| self.bars.get(i))
    }

    /// The length of bar `index` (from 0), seconds.
    #[must_use]
    pub fn bar_length(&self, index: usize) -> f64 {
        let Some(bar) = self.bars.get(index) else {
            return 0.0;
        };
        self.bars.get(index + 1).map_or(self.end, |b| b.time) - bar.time
    }

    /// Every beat of the grid: the bar (from 0), the beat in it (from 0)
    /// and its time, each bar's beats evenly spaced.
    #[must_use]
    pub fn grid(&self) -> Vec<GridBeat> {
        let mut out = Vec::new();
        for (i, bar) in self.bars.iter().enumerate() {
            let pulses = bar.meter.pulses().max(1);
            let length = self.bar_length(i);
            for pulse in 0..pulses {
                out.push(GridBeat {
                    bar: i,
                    pulse,
                    time: bar.time + length * pulse as f64 / pulses as f64,
                });
            }
        }
        out
    }

    /// Where `time` is on the grid: the song bar (from 1; 0 and below
    /// before bar 1) and the beat in it (from 1, with its fraction).
    #[must_use]
    pub fn position(&self, time: f64) -> (i64, f64) {
        let Some(first) = self.bars.first() else {
            return (0, 1.0);
        };
        if time < first.time {
            // Before bar 1: bars of bar 1's length, counted back.
            let length = self.bar_length(0).max(1e-9);
            let back = (first.time - time) / length;
            let bars = back.ceil();
            let into = (bars - back) * first.meter.pulses() as f64;
            return (1 - bars as i64, 1.0 + into);
        }
        let i = self
            .bars
            .partition_point(|b| b.time <= time)
            .saturating_sub(1);
        let bar = &self.bars[i];
        let length = self.bar_length(i).max(1e-9);
        let into = (time - bar.time) / length * bar.meter.pulses() as f64;
        (i64::from(bar.number), 1.0 + into)
    }

    /// Where a hit at `time` is, for something that syncs to it: as
    /// [`TempoMap::position`], but a time within half a beat before a bar
    /// line belongs to that bar (beat 0.5 to 1), so the beat rounds to the
    /// beat the hit is on.
    #[must_use]
    pub fn hit_position(&self, time: f64) -> (i64, f64) {
        let (bar, beat) = self.position(time);
        let pulses = usize::try_from(bar - 1)
            .ok()
            .and_then(|i| self.bars.get(i))
            .or_else(|| self.bars.first())
            .map_or(4, |b| b.meter.pulses()) as f64;
        if beat >= pulses + 0.5 {
            (bar + 1, beat - pulses)
        } else {
            (bar, beat)
        }
    }

    /// The map as tempo points: the count-in from 0, one point per bar
    /// (with its meter where that is set or changes), and one at the end
    /// holding the last tempo.
    #[must_use]
    pub fn points(&self) -> Vec<TempoPoint> {
        self.points_at(0.0)
    }

    /// The map as tempo points of a session whose audio starts `offset`
    /// seconds in: every bar that much later, and the count-in from the
    /// session's start to bar 1 where it then is.
    #[must_use]
    pub fn points_at(&self, offset: f64) -> Vec<TempoPoint> {
        let mut points = Vec::with_capacity(self.bars.len() + 2);
        let mut meter = None;
        if let Some(count) = self
            .bars
            .first()
            .and_then(|first| count_in(first, first.time + offset))
        {
            points.push(TempoPoint {
                time: 0.0,
                bpm: count.bpm,
                meter: Some(count.meter),
            });
            meter = Some(count.meter);
        }
        for bar in &self.bars {
            let changed = meter != Some(bar.meter);
            points.push(TempoPoint {
                time: bar.time + offset,
                bpm: bar.bpm,
                meter: changed.then_some(bar.meter),
            });
            meter = Some(bar.meter);
        }
        if let Some(last) = self.bars.last() {
            points.push(TempoPoint {
                time: self.end + offset,
                bpm: last.bpm,
                meter: None,
            });
        }
        points
    }

    /// How far into a session the audio would sit for the count-in to be
    /// one whole bar at bar 1's tempo (what the project script did by
    /// moving the items): none when it already is at least that.
    #[must_use]
    pub fn count_in_offset(&self) -> Option<f64> {
        let first = self.bars.first()?;
        let bar = self.bar_length(0);
        (first.time + 0.05 < bar).then_some(bar - first.time)
    }

    /// The map a list of tempo points describes, each holding its tempo
    /// until the next (a ramp is read as a hold): bars generated from the
    /// first point to `end`, the first `count_in_bars` of them the
    /// count-in.
    #[must_use]
    pub fn from_points(points: &[TempoPoint], end: f64, count_in_bars: u32) -> Self {
        let mut sorted: Vec<TempoPoint> = points.to_vec();
        sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
        let mut all: Vec<Bar> = Vec::new();
        let mut meter = Meter::COMMON;
        let mut at = sorted.first().map_or(0.0, |p| p.time);
        let mut next = 0;
        let mut bpm = sorted.first().map_or(120.0, |p| p.bpm);
        // A bar started under one tempo and crossed by a point: the part
        // of it (0 to 1) already played.
        let mut done = 0.0_f64;
        let mut start = at;
        while at < end - 1e-6 && all.len() < 100_000 {
            while sorted.get(next).is_some_and(|p| p.time <= at + 1e-6) {
                let p = sorted[next];
                bpm = p.bpm;
                // A meter takes hold on a bar line only.
                if done < 1e-9
                    && let Some(m) = p.meter
                {
                    meter = m;
                }
                next += 1;
            }
            let full = meter.quarters() * 60.0 / bpm.max(1e-9);
            let rest = full * (1.0 - done);
            let limit = sorted.get(next).map_or(f64::INFINITY, |p| p.time);
            if at + rest <= limit + 2e-3 {
                // A bar that ends within 2 ms of a point ends on it: a
                // project file rounds its tempos.
                at = if (at + rest - limit).abs() < 2e-3 {
                    limit
                } else {
                    at + rest
                };
                all.push(Bar {
                    number: 0,
                    time: start,
                    meter,
                    bpm: meter.bpm_of(at - start),
                });
                start = at;
                done = 0.0;
            } else {
                done += (limit - at) / full;
                at = limit;
            }
        }
        let lead = (count_in_bars as usize).min(all.len());
        let count_in = (lead > 0).then(|| CountIn {
            bars: count_in_bars,
            bpm: all[0].bpm,
            meter: all[0].meter,
        });
        let mut bars: Vec<Bar> = all.split_off(lead);
        for (i, bar) in bars.iter_mut().enumerate() {
            bar.number = u32::try_from(i).unwrap_or(u32::MAX).saturating_add(1);
        }
        Self {
            bars,
            end: at,
            count_in,
        }
    }

    /// The map with every time moved by `seconds` (a reference kept in
    /// project time, brought to the recording's).
    #[must_use]
    pub fn shifted(mut self, seconds: f64) -> Self {
        for bar in &mut self.bars {
            bar.time += seconds;
        }
        self.end += seconds;
        self
    }

    /// The slowest, the median and the fastest bar, bpm.
    #[must_use]
    pub fn tempo_range(&self) -> (f64, f64, f64) {
        let bpms: Vec<f64> = self.bars.iter().map(|b| b.bpm).collect();
        (
            bpms.iter().copied().fold(f64::INFINITY, f64::min),
            fit::median(&bpms),
            bpms.iter().copied().fold(0.0, f64::max),
        )
    }
}

/// A beat of a map's grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridBeat {
    /// The bar, from 0.
    pub bar: usize,
    /// The beat in it, from 0.
    pub pulse: usize,
    pub time: f64,
}

/// The count-in before `first`, whose downbeat is `lead` seconds after
/// the session's start: one bar of its meter from 0 to that downbeat;
/// more, where one would be slower than a tempo can be; and where the
/// lead is too short for a bar at any tempo, a bar of one beat (1/4, 1/8,
/// ...), so the grid still lands on bar 1. None when the song starts at 0.
fn count_in(first: &Bar, lead: f64) -> Option<CountIn> {
    if lead < 0.02 {
        return None;
    }
    let mut bars = 1_u32;
    let mut bpm = first.meter.bpm_of(lead);
    while bpm < *BPM_RANGE.start() && bars < 64 {
        bars += 1;
        bpm = first.meter.bpm_of(lead / f64::from(bars));
    }
    if bpm <= *BPM_RANGE.end() {
        return Some(CountIn {
            bars,
            bpm,
            meter: first.meter,
        });
    }
    let mut meter = Meter::new(1, first.meter.denominator.max(4));
    while meter.bpm_of(lead) > *BPM_RANGE.end() && meter.denominator < 64 {
        meter = Meter::new(1, meter.denominator * 2);
    }
    Some(CountIn {
        bars: 1,
        bpm: meter.bpm_of(lead).min(*BPM_RANGE.end()),
        meter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meter::MeterChange;

    fn steady(count: usize, from: f64, period: f64) -> Vec<f64> {
        (0..count).map(|i| from + period * i as f64).collect()
    }

    fn hits(count: usize) -> Vec<String> {
        vec!["Kick".to_owned(); count]
    }

    #[test]
    fn a_steady_song_is_one_tempo() {
        // 17 beats at 120: four bars, closed by the last beat.
        let beats = steady(17, 2.0, 0.5);
        let map = TempoMap::build(&beats, &hits(17), &MapOptions::default());
        assert_eq!(map.bars.len(), 4);
        assert!((map.end - 10.0).abs() < 1e-9);
        for bar in &map.bars {
            assert!((bar.bpm - 120.0).abs() < 1e-6, "{}", bar.bpm);
        }
        let count = map.count_in.unwrap_or(CountIn {
            bars: 0,
            bpm: 0.0,
            meter: Meter::COMMON,
        });
        assert_eq!(count.bars, 1);
        assert!((count.bpm - 120.0).abs() < 1e-6);
    }

    #[test]
    fn one_late_beat_does_not_move_its_bar_line() {
        let mut beats = steady(33, 0.0, 0.5);
        beats[8] += 0.05;
        let map = TempoMap::build(&beats, &hits(33), &MapOptions::default());
        // Nine points share the 50 ms: the bar line moves by a ninth.
        assert!(
            (map.bars[2].time - 4.0).abs() < 0.007,
            "{}",
            map.bars[2].time
        );
    }

    #[test]
    fn a_beat_on_no_hit_counts_for_less() {
        let mut beats = steady(33, 0.0, 0.5);
        beats[8] += 0.05;
        let mut sources = hits(33);
        "between".clone_into(&mut sources[8]);
        let loose = TempoMap::build(&beats, &sources, &MapOptions::default());
        let tight = TempoMap::build(&beats, &hits(33), &MapOptions::default());
        assert!((loose.bars[2].time - 4.0).abs() < (tight.bars[2].time - 4.0).abs());
    }

    #[test]
    fn three_four_groups_in_threes_at_the_quarter_s_tempo() {
        let beats = steady(13, 1.5, 0.5);
        let options = MapOptions {
            layout: Layout::new(Meter::new(3, 4), Vec::new()),
            ..MapOptions::default()
        };
        let map = TempoMap::build(&beats, &hits(13), &options);
        assert_eq!(map.bars.len(), 4);
        assert!((map.bars[1].time - 3.0).abs() < 1e-9);
        assert!((map.bars[0].bpm - 120.0).abs() < 1e-6);
        let points = map.points();
        assert_eq!(points[0].meter, Some(Meter::new(3, 4)));
        assert_eq!(points[1].meter, None);
    }

    #[test]
    fn a_meter_change_moves_the_bar_lines_and_is_a_point() {
        // 4/4, 4/4, then 3/4 from bar 3.
        let beats = steady(18, 0.0, 0.5);
        let options = MapOptions {
            layout: Layout::new(
                Meter::COMMON,
                vec![MeterChange {
                    bar: 3,
                    meter: Meter::new(3, 4),
                }],
            ),
            ..MapOptions::default()
        };
        let map = TempoMap::build(&beats, &hits(18), &options);
        let times: Vec<f64> = map.bars.iter().map(|b| b.time).collect();
        for (got, want) in times.iter().zip([0.0, 2.0, 4.0, 5.5, 7.0]) {
            assert!((got - want).abs() < 1e-9, "{got} for {want}");
        }
        let with_meter: Vec<(f64, Meter)> = map
            .points()
            .iter()
            .filter_map(|p| p.meter.map(|m| (p.time, m)))
            .collect();
        assert_eq!(with_meter.len(), 2);
        assert!((with_meter[1].0 - 4.0).abs() < 1e-9);
        assert_eq!(with_meter[1].1, Meter::new(3, 4));
        // Every bar is 120 in quarters, whatever its meter.
        for bar in &map.bars {
            assert!((bar.bpm - 120.0).abs() < 1e-6);
        }
    }

    #[test]
    fn a_ritard_is_taken_as_played() {
        let mut beats = steady(17, 0.0, 0.5);
        // The last bar slows: its beats 0.6, 0.7, 0.8, 0.9 apart.
        let mut t = beats[12];
        for (i, gap) in [0.6, 0.7, 0.8, 0.9].iter().enumerate() {
            t += gap;
            beats[13 + i] = t;
        }
        let fitted = TempoMap::build(&beats, &hits(17), &MapOptions::default());
        let raw = TempoMap::build(
            &beats,
            &hits(17),
            &MapOptions {
                raw_from_bar: Some(4),
                ..MapOptions::default()
            },
        );
        assert!((raw.bars[3].time - 6.0).abs() < 1e-9);
        assert!((raw.end - 9.0).abs() < 1e-9);
        assert!((fitted.bars[3].time - 6.0).abs() > 0.01);
        assert!((raw.bars[3].bpm - 80.0).abs() < 1e-6);
    }

    #[test]
    fn a_short_lead_in_still_lands_the_grid_on_bar_one() {
        // Bar 1 a quarter of a second in: no 4/4 bar is that short.
        let beats = steady(17, 0.25, 0.5);
        let map = TempoMap::build(&beats, &hits(17), &MapOptions::default());
        let count = map.count_in.unwrap_or(CountIn {
            bars: 0,
            bpm: 0.0,
            meter: Meter::COMMON,
        });
        // One bar of one beat, a quarter of a second long.
        assert_eq!((count.bars, count.meter), (1, Meter::new(1, 4)));
        assert!((count.bpm - 240.0).abs() < 1e-6);
        let back = TempoMap::from_points(&map.points(), map.end, 1);
        assert!((back.bars[0].time - 0.25).abs() < 1e-6);
        assert_eq!(back.bars[0].meter, Meter::COMMON);
        // With the audio moved so a whole bar fits, the count-in is one
        // bar at bar 1's tempo.
        let offset = map.count_in_offset().unwrap_or(0.0);
        assert!((offset - 1.75).abs() < 1e-9);
        let points = map.points_at(offset);
        assert_eq!(points[0].meter, Some(Meter::COMMON));
        assert!((points[0].bpm - 120.0).abs() < 1e-6);
        assert!((points[1].time - 2.0).abs() < 1e-9);
        assert_eq!(points[1].meter, None);
    }

    #[test]
    fn points_round_trip_through_a_map() {
        let beats = steady(18, 1.0, 0.5);
        let options = MapOptions {
            layout: Layout::new(
                Meter::COMMON,
                vec![MeterChange {
                    bar: 3,
                    meter: Meter::new(3, 4),
                }],
            ),
            ..MapOptions::default()
        };
        let map = TempoMap::build(&beats, &hits(18), &options);
        let back = TempoMap::from_points(&map.points(), map.end, 1);
        assert_eq!(back.bars.len(), map.bars.len());
        for (a, b) in back.bars.iter().zip(&map.bars) {
            assert!((a.time - b.time).abs() < 1e-6, "{} {}", a.time, b.time);
            assert_eq!(a.meter, b.meter);
        }
    }

    #[test]
    fn a_time_s_position_on_the_grid() {
        let beats = steady(17, 2.0, 0.5);
        let map = TempoMap::build(&beats, &hits(17), &MapOptions::default());
        assert_eq!(map.position(2.0), (1, 1.0));
        let (bar, beat) = map.position(4.75);
        assert_eq!(bar, 2);
        assert!((beat - 2.5).abs() < 1e-9);
        let (bar, beat) = map.position(1.5);
        assert_eq!(bar, 0);
        assert!((beat - 4.0).abs() < 1e-9);
        // A hit just ahead of a bar line is on that bar's first beat.
        let (bar, beat) = map.hit_position(3.995);
        assert_eq!(bar, 2);
        assert!((beat - 0.99).abs() < 1e-9, "{beat}");
        // The and of four stays in its bar.
        assert_eq!(map.hit_position(3.74).0, 1);
        let (bar, beat) = map.hit_position(1.99);
        assert_eq!(bar, 1);
        assert!((beat - 0.98).abs() < 1e-9, "{beat}");
    }
}
