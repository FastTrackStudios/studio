//! Beats and the timeline they are counted along, through the tempo map:
//! the project's tempo changes as a [`::timeline::time::TempoMap`] in
//! seconds, whose rules (a tempo change carries the count, a signature
//! change starts a measure, a change inside a beat cuts it short) live
//! there.

use ::timeline::time::{TempoMap, TempoPoint};

/// One beat of the project, and where it falls (in seconds: `at`,
/// `units_per_beat`).
pub use ::timeline::time::Beat;

/// The project's beat grid, walked through its tempo map.
#[derive(Clone, Copy)]
pub struct Timeline<'a> {
    changes: &'a [daw_ui::studio::project::TempoChange],
}

impl<'a> Timeline<'a> {
    #[must_use]
    pub const fn new(changes: &'a [daw_ui::studio::project::TempoChange]) -> Self {
        Self { changes }
    }

    /// The tempo map in seconds.
    #[must_use]
    pub fn map(self) -> TempoMap {
        TempoMap::new(self.changes.iter().map(point), 1.0)
    }

    /// Every beat from the start of the project up to `to`, in order —
    /// none for a project with no tempo. Stopped by `to`, and by `limit`
    /// so a corrupt tempo map cannot spin.
    #[must_use]
    pub fn beats(self, to: f64, limit: usize) -> Vec<Beat> {
        if self.changes.is_empty() {
            return Vec::new();
        }
        self.map().beats(to, limit)
    }
}

/// A project tempo change as a map point.
#[must_use]
pub const fn point(change: &daw_ui::studio::project::TempoChange) -> TempoPoint {
    TempoPoint {
        at: change.at,
        bpm: change.bpm,
        beats_per_bar: change.beats_per_bar,
        beat_unit: change.beat_unit,
    }
}
