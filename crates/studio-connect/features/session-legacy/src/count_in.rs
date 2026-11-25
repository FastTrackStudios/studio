//! Getting REAPER's pre-roll to agree with the guide's count-in.
//!
//! A MIDI part is recorded to the session's own count — the guide plays
//! "1 2 3 4" into the headphone buses — and the transport has to start
//! at the same moment, or the punch is in the wrong place. REAPER's
//! pre-roll is what makes the transport start early, so the two have to
//! be set from one number.
//!
//! # The mismatch, and which way to resolve it
//!
//! The guide counts a **whole number of measures**, 1 to 8. REAPER's
//! pre-roll is reachable only as a **half measure, one, or two, then
//! doubled or halved** — so 1, 2, 4 and 8 are reachable and 3, 5, 6 and
//! 7 are not.
//!
//! Where a count is not reachable, this rounds **up**, and the
//! direction is not arbitrary. A pre-roll *shorter* than the count means
//! the guide starts counting before the transport rolls: the player
//! hears beats that are not being recorded, and the take starts partway
//! through the count. A pre-roll *longer* costs a moment of silence
//! before the count and nothing else. One is a wrong punch; the other is
//! a pause.

/// A pre-roll REAPER can actually be set to, in measures.
///
/// Not an integer, because half a measure is reachable and three
/// measures is not — a number would invite arithmetic the transport
/// cannot honour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreRoll {
    Half,
    One,
    Two,
    Four,
    Eight,
}

impl PreRoll {
    /// Every setting, shortest first.
    pub const ALL: [Self; 5] = [Self::Half, Self::One, Self::Two, Self::Four, Self::Eight];

    /// The setting in measures.
    #[must_use]
    pub const fn measures(self) -> f64 {
        match self {
            Self::Half => 0.5,
            Self::One => 1.0,
            Self::Two => 2.0,
            Self::Four => 4.0,
            Self::Eight => 8.0,
        }
    }

    /// The shortest setting that is **at least** `measures`.
    ///
    /// Rounds up for the reason in the module note: a short pre-roll
    /// puts the punch in the wrong place, a long one costs a pause.
    /// A count longer than anything REAPER offers gets the longest.
    #[must_use]
    pub fn covering(measures: f64) -> Self {
        Self::ALL
            .into_iter()
            .find(|setting| setting.measures() >= measures)
            .unwrap_or(Self::Eight)
    }

    /// Whether this setting is exactly the count, or longer than it.
    ///
    /// Worth asking out loud: an engineer who set a three-measure count
    /// and got four should be able to find out why without timing it.
    #[must_use]
    pub fn is_exact(self, measures: f64) -> bool {
        (self.measures() - measures).abs() < f64::EPSILON
    }

    /// The facade calls that reach this setting, in order.
    ///
    /// The facade offers half, one and two directly and then doubling,
    /// so anything longer is a base plus doublings. Returned as a plan
    /// rather than executed here so it can be asserted without a DAW.
    #[must_use]
    pub fn steps(self) -> Vec<Step> {
        match self {
            Self::Half => vec![Step::SetHalfMeasure],
            Self::One => vec![Step::SetOneMeasure],
            Self::Two => vec![Step::SetTwoMeasures],
            Self::Four => vec![Step::SetTwoMeasures, Step::Double],
            Self::Eight => vec![Step::SetTwoMeasures, Step::Double, Step::Double],
        }
    }
}

/// One facade call on the way to a pre-roll setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    SetHalfMeasure,
    SetOneMeasure,
    SetTwoMeasures,
    Double,
}

/// The pre-roll a count of `measures` needs, and how to get there.
#[must_use]
pub fn plan(measures: f64) -> (PreRoll, Vec<Step>) {
    let setting = PreRoll::covering(measures);
    (setting, setting.steps())
}
