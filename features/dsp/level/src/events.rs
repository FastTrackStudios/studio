//! Leveling by events: one gain an event.
//!
//! A voice is ridden phrase by phrase, block by block ([`crate::envelope`]).
//! Anything struck — a drum, a picked note, a slapped string — is not:
//! ridden block by block it pumps, every ring pulled up as it decays,
//! every attack pushed down as it lands. It is leveled the way an
//! engineer does it by hand with clip gain: each event's level read once,
//! at its attack, and the gain that brings it toward the others set just
//! before it and held through its ring until the next event lands.
//!
//! An event that does not **ride** — a ghost, bleed, a flam's grace —
//! is not leveled on its own: it sits under the gain of the event before
//! it, so a ghost stays a ghost.
//!
//! The curve is points in the caller's timeline (an item's, its source's):
//! a step a little before each riding event whose gain differs from the
//! one before it, ramped so it does not click. What an event is, which
//! ones ride and how hard is the material's ([`crate::drums`] for a kit).

use crate::envelope::EnvPoint;

/// One event, as the leveler reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    /// Where, seconds.
    pub t_s: f64,
    /// How loud its attack is, dB (a peak, or any consistent measure).
    pub level_db: f64,
    /// Leveled — or left under the event before it.
    pub rides: bool,
}

/// How hard events are leveled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ride {
    /// How much of the way to the target each event is brought, 0..1: 1
    /// makes every event as loud as the target, 0.5 halves the spread.
    pub amount: f64,
    /// The most an event is raised, dB.
    pub max_gain_db: f64,
    /// The most an event is lowered, dB.
    pub max_cut_db: f64,
    /// How long before its event a gain is in place, seconds: before the
    /// attack, after the ring before it.
    pub lead_s: f64,
    /// How long a change takes, seconds.
    pub ramp_s: f64,
    /// Changes smaller than this are not made, dB.
    pub least_db: f64,
}

impl Ride {
    /// This ride with a hand's knobs on it: `amount` (0..1) and a
    /// symmetric `range_db`, each kept as it is when not given.
    #[must_use]
    pub fn turned(self, amount: Option<f64>, range_db: Option<f64>) -> Self {
        let range = range_db.map(f64::abs);
        Self {
            amount: amount.map_or(self.amount, |a| a.clamp(0.0, 1.0)),
            max_gain_db: range.unwrap_or(self.max_gain_db),
            max_cut_db: range.unwrap_or(self.max_cut_db),
            ..self
        }
    }
}

/// The level the riding events are brought toward over all of `events`:
/// their median, dB. `None` with none that ride.
#[must_use]
pub fn target(events: &[Event]) -> Option<f64> {
    let mut levels: Vec<f64> = events
        .iter()
        .filter(|e| e.rides)
        .map(|e| e.level_db)
        .collect();
    levels.sort_by(f64::total_cmp);
    levels.get(levels.len() / 2).copied()
}

/// The gain an event at `level_db` takes toward `target_db`, dB: a share
/// (`amount`) of the way, within the ride's range. The one gain law of
/// every leveler in this crate.
#[must_use]
pub fn gain(level_db: f64, target_db: f64, ride: &Ride) -> f64 {
    law(
        level_db,
        target_db,
        ride.amount,
        (ride.max_gain_db, ride.max_cut_db),
    )
}

/// [`gain`] without a [`Ride`]: for the levelers that keep their own knobs.
#[must_use]
pub fn law(
    level_db: f64,
    target_db: f64,
    amount: f64,
    (max_gain_db, max_cut_db): (f64, f64),
) -> f64 {
    ((target_db - level_db) * amount.clamp(0.0, 1.0)).clamp(-max_cut_db.abs(), max_gain_db.abs())
}

/// The level the riding events around `t` sit at: the median of the
/// `around` of them (by time) nearest it. `None` with none that ride.
///
/// Held up to the playing around it, a quiet passage stays quieter than
/// a loud one — only event to event is it made even.
#[must_use]
pub fn target_at(events: &[Event], t: f64, around: usize) -> Option<f64> {
    let riding: Vec<&Event> = events.iter().filter(|e| e.rides).collect();
    let at = riding.partition_point(|e| e.t_s < t);
    let (mut lo, mut hi) = (at, at);
    while hi.saturating_sub(lo) < around {
        let left = lo
            .checked_sub(1)
            .and_then(|i| riding.get(i))
            .map(|e| t - e.t_s);
        let right = riding.get(hi).map(|e| e.t_s - t);
        match (left, right) {
            (Some(l), Some(r)) if l <= r => lo = lo.saturating_sub(1),
            (Some(_), None) => lo = lo.saturating_sub(1),
            (_, Some(_)) => hi = hi.saturating_add(1),
            (None, None) => break,
        }
    }
    let mut levels: Vec<f64> = riding
        .get(lo..hi)
        .unwrap_or_default()
        .iter()
        .map(|e| e.level_db)
        .collect();
    levels.sort_by(f64::total_cmp);
    levels.get(levels.len() / 2).copied()
}

/// The leveling curve for `events` (by time), each riding event toward
/// the level `target` says for its time: unity before the first, then
/// each one's gain from a lead before it until the next one's.
#[must_use]
pub fn ride(events: &[Event], target: impl Fn(f64) -> f64, ride: &Ride) -> Vec<EnvPoint> {
    let mut points: Vec<EnvPoint> = Vec::new();
    let mut held = 0.0_f64;
    for event in events.iter().filter(|e| e.rides) {
        let g = gain(event.level_db, target(event.t_s), ride);
        if (g - held).abs() < ride.least_db {
            continue;
        }
        let at = (event.t_s - ride.lead_s).max(0.0);
        let from = (at - ride.ramp_s).max(points.last().map_or(0.0, |p| p.t_s));
        if points.is_empty() || from > points.last().map_or(0.0, |p| p.t_s) {
            points.push(EnvPoint::new(from, held));
        }
        points.push(EnvPoint::new(at.max(from), g));
        held = g;
    }
    points
}

/// What a ride did to `events`: how many rode, were raised and lowered,
/// and the most either way, dB.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moved {
    pub riding: u32,
    pub raised: u32,
    pub lowered: u32,
    pub most_db: f64,
}

/// Count what [`ride`] does to `events`, without drawing it.
#[must_use]
pub fn moved(events: &[Event], target: impl Fn(f64) -> f64, ride: &Ride) -> Moved {
    let mut said = Moved::default();
    for event in events.iter().filter(|e| e.rides) {
        let g = gain(event.level_db, target(event.t_s), ride);
        said.riding = said.riding.saturating_add(1);
        if g > ride.least_db {
            said.raised = said.raised.saturating_add(1);
        } else if g < -ride.least_db {
            said.lowered = said.lowered.saturating_add(1);
        }
        if g.abs() > said.most_db.abs() {
            said.most_db = g;
        }
    }
    said
}
