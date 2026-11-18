//! Bars and beats in seconds, from the session's tempo data
//! (`fts.session` `ReadStructure`), for a ruler and a grid.
//!
//! Tempo is REAPER's (quarter notes a minute); a beat is the meter's
//! unit. Each change holds until the next: a ramp is drawn as a step at
//! its start (the session's engine integrates ramps; a ruler drawn this
//! way is off inside a ramp, and right again after it). A meter change
//! takes effect at the next bar.

/// A tempo change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    pub seconds: f64,
    pub bpm: f64,
    /// `(numerator, denominator)`, when the meter changes here.
    pub meter: Option<(u32, u32)>,
}

/// A tempo map: where it starts and how it changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Tempo {
    pub bpm: f64,
    pub meter: (u32, u32),
    pub changes: Vec<Change>,
}

impl Default for Tempo {
    fn default() -> Self {
        Self {
            bpm: 120.0,
            meter: (4, 4),
            changes: Vec::new(),
        }
    }
}

/// A line of the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub seconds: f64,
    /// 1-based.
    pub bar: u32,
    /// 1-based; 1 is the bar line.
    pub beat: u32,
}

/// Most lines one call gives: a ruler asks for what it shows.
pub const MAX_LINES: usize = 200_000;

/// Every beat from 0 to `until` seconds.
#[must_use]
pub fn lines(tempo: &Tempo, until: f64) -> Vec<Line> {
    let mut changes: Vec<Change> = tempo
        .changes
        .iter()
        .copied()
        .filter(|c| c.seconds.is_finite() && c.bpm.is_finite() && c.bpm > 0.0)
        .collect();
    changes.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    let mut bpm = tempo.bpm.max(1.0);
    let (mut num, mut den) = tempo.meter;
    let mut pending: Option<(u32, u32)> = None;
    let mut next = 0;
    // A change at 0 is the start.
    while let Some(c) = changes.get(next).filter(|c| c.seconds <= 1e-9) {
        bpm = c.bpm;
        if let Some(m) = c.meter {
            (num, den) = m;
        }
        next += 1;
    }
    let mut out = Vec::new();
    let (mut t, mut bar, mut beat) = (0.0_f64, 1_u32, 1_u32);
    while t <= until && out.len() < MAX_LINES {
        out.push(Line {
            seconds: t,
            bar,
            beat,
        });
        // One beat on, crossing tempo changes on the way.
        let mut left = 1.0_f64;
        loop {
            let len = 60.0 / bpm * 4.0 / f64::from(den.max(1));
            let end = t + left * len;
            match changes.get(next) {
                Some(c) if c.seconds < end - 1e-9 => {
                    left -= (c.seconds - t) / len;
                    t = c.seconds;
                    bpm = c.bpm;
                    if c.meter.is_some() {
                        pending = c.meter;
                    }
                    next += 1;
                }
                _ => {
                    t = end;
                    break;
                }
            }
        }
        beat += 1;
        if beat > num.max(1) {
            beat = 1;
            bar += 1;
            if let Some(m) = pending.take() {
                (num, den) = m;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_four_at_120_is_a_bar_every_two_seconds() {
        let l = lines(&Tempo::default(), 4.0);
        assert_eq!(l.len(), 9);
        assert_eq!((l[4].seconds, l[4].bar, l[4].beat), (2.0, 2, 1));
    }

    #[test]
    fn a_change_moves_the_beats_after_it_and_a_meter_waits_for_the_bar() {
        let t = Tempo {
            changes: vec![Change {
                seconds: 2.0,
                bpm: 60.0,
                meter: Some((3, 4)),
            }],
            ..Tempo::default()
        };
        let l = lines(&t, 6.0);
        // Bar 2 at 2 s, then a beat a second, three to a bar.
        assert_eq!((l[4].seconds, l[4].bar), (2.0, 2));
        assert!((l[5].seconds - 3.0).abs() < 1e-9);
        let bar3 = l.iter().find(|x| x.bar == 3).unwrap();
        assert!((bar3.seconds - 6.0).abs() < 1e-9, "{bar3:?}");
    }
}
