//! Meters, and how a song's bars are laid over its beats.
//!
//! The scripts assumed four beats to every bar. Here a song has a meter
//! and may change it: a [`Layout`] says how many tracked beats each bar
//! holds, so bars group the beats whatever the meter, and a tempo point's
//! bpm is always in quarter notes (what a session's tempo map holds).

/// A time signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Meter {
    pub numerator: u32,
    pub denominator: u32,
}

impl Default for Meter {
    fn default() -> Self {
        Self::COMMON
    }
}

impl Meter {
    /// 4/4.
    pub const COMMON: Self = Self {
        numerator: 4,
        denominator: 4,
    };

    #[must_use]
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// `3/4`, `6/8`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (n, d) = text.trim().split_once('/')?;
        let numerator: u32 = n.trim().parse().ok()?;
        let denominator: u32 = d.trim().parse().ok()?;
        (numerator > 0 && denominator > 0 && denominator.is_power_of_two()).then_some(Self {
            numerator,
            denominator,
        })
    }

    /// Whether the beat is a dotted note: 6/8, 9/8, 12/8.
    #[must_use]
    pub const fn compound(self) -> bool {
        self.denominator >= 8 && self.numerator > 3 && self.numerator.is_multiple_of(3)
    }

    /// How many tracked beats a bar holds: the numerator, or a third of it
    /// in a compound meter (6/8 is felt, and tracked, in two).
    #[must_use]
    pub const fn pulses(self) -> usize {
        if self.compound() {
            (self.numerator / 3) as usize
        } else {
            self.numerator as usize
        }
    }

    /// A bar's length in quarter notes.
    #[must_use]
    pub fn quarters(self) -> f64 {
        f64::from(self.numerator) * 4.0 / f64::from(self.denominator.max(1))
    }

    /// The tempo, in quarter notes a minute, of a bar `seconds` long.
    #[must_use]
    pub fn bpm_of(self, seconds: f64) -> f64 {
        60.0 * self.quarters() / seconds.max(1e-9)
    }

    /// The beats of the bar (from 0) the backbeat falls on: 2 and 4 in
    /// four, 2 in two (6/8 too), 2 and 3 in three (the waltz's "chick,
    /// chick"). Empty where there is no usual one.
    #[must_use]
    pub fn backbeat(self) -> Vec<usize> {
        match self.pulses() {
            2 => vec![1],
            3 => vec![1, 2],
            4 => vec![1, 3],
            6 => vec![3],
            _ => Vec::new(),
        }
    }
}

impl std::fmt::Display for Meter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

/// A meter from a bar on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MeterChange {
    /// The song bar it starts at, from 1.
    pub bar: u32,
    pub meter: Meter,
}

/// The meter of every bar of a song.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Layout {
    /// The meter of bar 1.
    pub meter: Meter,
    /// The changes after it, by bar.
    #[serde(default)]
    pub changes: Vec<MeterChange>,
}

/// One bar of a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarSpan {
    /// The bar, from 0 (song bar 1).
    pub index: usize,
    /// Its first beat, counted from bar 1's downbeat.
    pub start: usize,
    pub meter: Meter,
    /// Whether the meter changes at this bar.
    pub changes: bool,
}

impl Layout {
    #[must_use]
    pub fn new(meter: Meter, mut changes: Vec<MeterChange>) -> Self {
        changes.retain(|c| c.bar > 1);
        changes.sort_by_key(|c| c.bar);
        changes.dedup_by_key(|c| c.bar);
        Self { meter, changes }
    }

    /// The meter of bar `index` (from 0).
    #[must_use]
    pub fn meter_at(&self, index: usize) -> Meter {
        let bar = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
        self.changes
            .iter()
            .take_while(|c| c.bar <= bar)
            .last()
            .map_or(self.meter, |c| c.meter)
    }

    /// The bars that start within `beats` beats from bar 1's downbeat.
    #[must_use]
    pub fn bars(&self, beats: usize) -> Vec<BarSpan> {
        let mut out = Vec::new();
        let mut start = 0;
        let mut before = None;
        while start < beats {
            let meter = self.meter_at(out.len());
            out.push(BarSpan {
                index: out.len(),
                start,
                meter,
                changes: before.is_some_and(|m| m != meter),
            });
            before = Some(meter);
            start += meter.pulses().max(1);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meters_parse_and_count() {
        let waltz = Meter::parse("3/4").unwrap_or_default();
        assert_eq!((waltz.pulses(), waltz.quarters()), (3, 3.0));
        let six = Meter::parse(" 6 / 8 ").unwrap_or_default();
        assert_eq!((six.pulses(), six.quarters()), (2, 3.0));
        assert_eq!(Meter::parse("12/8").map(Meter::pulses), Some(4));
        assert_eq!(Meter::parse("4/5"), None);
        assert_eq!(Meter::parse("four"), None);
    }

    #[test]
    fn a_bar_s_tempo_is_in_quarters() {
        // A 3/4 bar 1.5 s long is 120 bpm; a 6/8 bar as long is too.
        assert!((Meter::new(3, 4).bpm_of(1.5) - 120.0).abs() < 1e-9);
        assert!((Meter::new(6, 8).bpm_of(1.5) - 120.0).abs() < 1e-9);
        assert!((Meter::COMMON.bpm_of(2.0) - 120.0).abs() < 1e-9);
    }

    #[test]
    fn a_layout_groups_beats_through_a_change() {
        // Two bars of 4/4, one of 2/4, then 3/4.
        let layout = Layout::new(
            Meter::COMMON,
            vec![
                MeterChange {
                    bar: 4,
                    meter: Meter::new(3, 4),
                },
                MeterChange {
                    bar: 3,
                    meter: Meter::new(2, 4),
                },
            ],
        );
        let bars = layout.bars(17);
        let starts: Vec<usize> = bars.iter().map(|b| b.start).collect();
        assert_eq!(starts, vec![0, 4, 8, 10, 13, 16]);
        assert!(bars[2].changes && bars[3].changes && !bars[4].changes);
    }
}
