//! The grid as the user sets it: a division, its feel, whether it snaps,
//! and how it follows the zoom.

use super::Adaptive;

/// Straight, triplet or dotted: what a step of the division is worth.
///
/// One value rather than two flags, because a dotted triplet is a real
/// thing nobody sets from a grid menu, and two toggles that can both be
/// on would make the readout say something no one asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Feel {
    #[default]
    Straight,
    /// Two thirds of the division.
    Triplet,
    /// Half again as long.
    Dotted,
}

impl Feel {
    /// A step's length against the straight division's.
    #[must_use]
    pub const fn scale(self) -> f64 {
        match self {
            Self::Straight => 1.0,
            Self::Triplet => 2.0 / 3.0,
            Self::Dotted => 1.5,
        }
    }
}

/// The grid setting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    /// Division as a fraction of a whole note (0.25 = quarter).
    ///
    /// **What the user asked for, which is the *finest* the grid ever
    /// gets.** When the grid is adaptive this is a ceiling rather than
    /// the value in use: zooming out coarsens away from it and zooming
    /// in returns to it, but nothing goes past it. [`Grid::effective`]
    /// is what is actually being snapped to.
    pub division: f64,
    /// What the zoom coarsened the division to, if anything.
    ///
    /// Kept separate from `division` rather than overwriting it, because
    /// a setting the zoom can quietly rewrite is not a setting — the
    /// user could never get back to 1/16 once a zoom had moved them to
    /// 1/4, and the control would stop meaning anything.
    fitted: Option<f64>,
    pub feel: Feel,
    pub enabled: bool,
    /// Whether — and how tightly — the division follows the zoom.
    ///
    /// **On** by default, at the middle density — see
    /// [`Adaptive::default`]. [`Grid::division`] stays the user's
    /// ceiling and the zoom only ever coarsens away from it, so a grid
    /// that follows the view can never snap finer than was asked for.
    /// [`Grid::label`] reports what is in use, not the ceiling, which is
    /// what keeps that honest.
    ///
    /// The feel stays separate precisely so this can scale the division
    /// by powers of two without ever straightening a triplet grid.
    pub adaptive: Adaptive,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            division: 1.0 / 16.0,
            fitted: None,
            feel: Feel::Straight,
            enabled: true,
            adaptive: Adaptive::default(),
        }
    }
}

impl Grid {
    /// The default grid at `division` (a fraction of a whole note).
    #[must_use]
    pub fn new(division: f64) -> Self {
        let mut grid = Self::default();
        grid.set_division(division);
        grid
    }

    /// The division actually in use — the setting, or what the zoom
    /// coarsened it to.
    ///
    /// Everything that *acts* on the grid goes through here: the step,
    /// the snap, the readout. `division` alone is the user's ceiling and
    /// snapping to it while the screen showed something coarser would be
    /// the grid lying about where notes will land.
    #[must_use]
    pub fn effective(&self) -> f64 {
        self.fitted.unwrap_or(self.division)
    }

    /// Grid step in beats (quarter notes).
    #[must_use]
    pub fn step_beats(&self) -> f64 {
        self.effective() * 4.0 * self.feel.scale()
    }

    /// Grid step in the surface's units.
    #[must_use]
    pub fn step(&self, units_per_beat: f64) -> f64 {
        self.step_beats() * units_per_beat
    }

    #[must_use]
    pub fn triplet(&self) -> bool {
        self.feel == Feel::Triplet
    }

    #[must_use]
    pub fn dotted(&self) -> bool {
        self.feel == Feel::Dotted
    }

    /// Arm the triplet grid (clearing dotted), or straighten it.
    pub fn set_triplet(&mut self, on: bool) {
        if on {
            self.feel = Feel::Triplet;
        } else if self.triplet() {
            self.feel = Feel::Straight;
        }
    }

    /// Arm the dotted grid (clearing triplet), or straighten it.
    pub fn set_dotted(&mut self, on: bool) {
        if on {
            self.feel = Feel::Dotted;
        } else if self.dotted() {
            self.feel = Feel::Straight;
        }
    }

    #[must_use]
    pub fn snap(&self, t: f64, origin: f64, units_per_beat: f64) -> f64 {
        if !self.enabled {
            return t;
        }
        let step = self.step(units_per_beat);
        if step <= 0.0 {
            return t;
        }
        ((t - origin) / step).round().mul_add(step, origin)
    }

    /// Follow the zoom, given how wide a whole note (four beats) is on
    /// screen — not a bar, which is the same only in 4/4.
    ///
    /// Returns whether the division moved, so a caller can tell a real
    /// change from the usual no-op — this runs on every camera change,
    /// and almost all of them leave the grid exactly where it was.
    pub fn refit(&mut self, whole_px: f64) -> bool {
        let next = self.adaptive.fit(self.division, whole_px);
        // Compared as a ratio rather than a difference: divisions span
        // four orders of magnitude, so an absolute epsilon is meaningless
        // at one end and everything at the other.
        let same = match (next, self.fitted) {
            (Some(a), Some(b)) => (a / b - 1.0).abs() < 1e-9,
            (None, None) => true,
            _ => false,
        };
        if same {
            return false;
        }
        self.fitted = next;
        true
    }

    /// Whether the zoom is currently holding the grid coarser than the
    /// setting — which is what the readout shows the user.
    #[must_use]
    pub fn is_coarsened(&self) -> bool {
        self.fitted
            .is_some_and(|f| (f / self.division - 1.0).abs() > 1e-9)
    }

    /// `1/16`, `1/16T`, `1/16.` — for the fixed-width toolbar readout.
    ///
    /// A whole note reads as `1`, not `1/1`: it is the bar, and every
    /// grid menu in every DAW calls it that.
    #[must_use]
    pub fn label(&self) -> String {
        let base = spell(self.effective());
        match self.feel {
            Feel::Straight => base,
            Feel::Triplet => format!("{base}T"),
            Feel::Dotted => format!("{base}."),
        }
    }

    /// The *setting*, spelled the same way [`Grid::label`] spells the
    /// division in use — so a readout showing both cannot render the
    /// same number two ways.
    #[must_use]
    pub fn ceiling_label(&self) -> String {
        spell(self.division)
    }

    /// Both of these move the *setting*, and drop whatever the zoom had
    /// fitted — a new ceiling makes the old fit meaningless, and leaving
    /// it would show the user their change doing nothing until they next
    /// moved the view. The surface refits straight after.
    pub fn coarser(&mut self) {
        self.division = (self.division * 2.0).min(1.0);
        self.fitted = None;
    }

    /// Set the division, dropping whatever the zoom had fitted — same
    /// reasoning as [`Grid::coarser`].
    pub fn set_division(&mut self, division: f64) {
        self.division = division.clamp(1.0 / 128.0, 1.0);
        self.fitted = None;
    }

    pub fn finer(&mut self) {
        self.division = (self.division / 2.0).max(1.0 / 128.0);
        self.fitted = None;
    }
}

/// `1`, `1/4`, `1/16` for a division.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a grid denominator, 1 to 128"
)]
fn spell(division: f64) -> String {
    let denom = (1.0 / division).round() as i64;
    if denom <= 1 {
        "1".to_string()
    } else {
        format!("1/{denom}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_feel_at_a_time() {
        let mut g = Grid::default();
        g.set_dotted(true);
        g.set_triplet(true);
        assert!(g.triplet() && !g.dotted());
        g.set_dotted(false);
        assert!(g.triplet(), "clearing dotted left the triplet alone");
        g.set_triplet(false);
        assert_eq!(g.feel, Feel::Straight);
    }

    #[test]
    fn the_feel_scales_the_step_and_the_label() {
        let mut g = Grid {
            division: 1.0 / 8.0,
            ..Grid::default()
        };
        assert!((g.step(960.0) - 480.0).abs() < 1e-9);
        g.feel = Feel::Triplet;
        assert!((g.step(960.0) - 320.0).abs() < 1e-9);
        assert_eq!(g.label(), "1/8T");
        g.feel = Feel::Dotted;
        assert!((g.step(960.0) - 720.0).abs() < 1e-9);
        assert_eq!(g.label(), "1/8.");
    }
}
