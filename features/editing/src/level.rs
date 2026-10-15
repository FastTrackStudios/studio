//! Leveling a stretch of a part, not the whole of it.
//!
//! A part is leveled by events ([`level::events`]): one gain an event,
//! set a lead before it and held through its ring until the next. Leveled
//! over a selection, the curve is the part's own outside it — an earlier
//! pass, a hand's drawing — and the selection's inside: each event in it
//! owns the curve from a lead before it until a lead before the next
//! event, so the last one in the selection keeps its gain through its
//! whole ring, and the first one after it takes the curve back.
//!
//! Times here are one timeline throughout: an item's, as its curve is.

use level::envelope::EnvPoint;
use level::events::{Event, Ride};

/// The stretches of a curve `spans` own: from a lead (and a ramp) before
/// the first riding event in a span until a lead before the first riding
/// event after it — open (`f64::INFINITY`) with none after. Spans with no
/// riding event own nothing. Merged where they meet.
#[must_use]
pub fn owned(events: &[Event], spans: &[(f64, f64)], ride: &Ride) -> Vec<(f64, f64)> {
    let before = ride.lead_s + ride.ramp_s;
    let riding: Vec<f64> = events.iter().filter(|e| e.rides).map(|e| e.t_s).collect();
    let mut out: Vec<(f64, f64)> = spans
        .iter()
        .filter_map(|&(from, to)| {
            let first = riding.iter().find(|&&t| t >= from && t < to)?;
            let next = riding
                .iter()
                .find(|&&t| t >= to)
                .map_or(f64::INFINITY, |t| t - before);
            Some(((first - before).max(0.0), next))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(out.len());
    for (lo, hi) in out {
        match merged.last_mut() {
            Some(last) if lo <= last.1 => last.1 = last.1.max(hi),
            _ => merged.push((lo, hi)),
        }
    }
    merged
}

/// Whether `t` is in any of `spans` (`from` inclusive, `to` not).
#[must_use]
pub fn within(spans: &[(f64, f64)], t: f64) -> bool {
    spans.iter().any(|&(from, to)| t >= from && t < to)
}

/// The curve's gain at `t`, dB: straight between points, the first's
/// before it and the last's after it, unity with none.
#[must_use]
pub fn value_at(curve: &[EnvPoint], t: f64) -> f64 {
    let after = curve.partition_point(|p| p.t_s <= t);
    match (
        after.checked_sub(1).and_then(|i| curve.get(i)),
        curve.get(after),
    ) {
        (Some(a), Some(b)) if !a.hold && b.t_s > a.t_s => {
            let along = (t - a.t_s) / (b.t_s - a.t_s);
            (b.db - a.db).mul_add(along, a.db)
        }
        (Some(a), _) => a.db,
        (None, Some(b)) => b.db,
        (None, None) => 0.0,
    }
}

/// `old` with `new` in place of it over each of `owned` (sorted, apart):
/// at each window's start the old gain ramps (over `ramp_s`) to the new
/// one, and at its end back to the old one.
#[must_use]
pub fn splice(
    old: &[EnvPoint],
    new: &[EnvPoint],
    owned: &[(f64, f64)],
    ramp_s: f64,
) -> Vec<EnvPoint> {
    let mut out: Vec<EnvPoint> = Vec::new();
    let push = |out: &mut Vec<EnvPoint>, p: EnvPoint| {
        if out.last().is_none_or(|last| p.t_s > last.t_s) {
            out.push(p);
        }
    };
    let mut from = f64::NEG_INFINITY;
    for &(lo, hi) in owned {
        for p in old.iter().filter(|p| p.t_s >= from && p.t_s < lo) {
            push(&mut out, *p);
        }
        push(&mut out, EnvPoint::new(lo, value_at(old, lo)));
        let into = lo + ramp_s;
        push(&mut out, EnvPoint::new(into, value_at(new, into)));
        for p in new.iter().filter(|p| p.t_s > into && p.t_s < hi) {
            push(&mut out, *p);
        }
        if hi.is_finite() {
            push(&mut out, EnvPoint::new(hi, value_at(new, hi)));
            from = hi + ramp_s;
            push(&mut out, EnvPoint::new(from, value_at(old, from)));
        } else {
            from = f64::INFINITY;
        }
    }
    for p in old.iter().filter(|p| p.t_s > from) {
        push(&mut out, *p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RIDE: Ride = Ride {
        amount: 1.0,
        max_gain_db: 12.0,
        max_cut_db: 12.0,
        lead_s: 0.004,
        ramp_s: 0.001,
        least_db: 0.1,
    };

    fn stroke(t_s: f64) -> Event {
        Event {
            t_s,
            level_db: -12.0,
            rides: true,
        }
    }

    /// A span owns its strokes' curve, the last one's through its ring:
    /// until the lead of the first stroke after it.
    #[test]
    fn a_span_owns_until_the_next_stroke() {
        let events = [stroke(1.0), stroke(2.0), stroke(3.0), stroke(4.0)];
        assert_eq!(owned(&events, &[(1.5, 3.5)], &RIDE), [(1.995, 3.995)]);
        assert_eq!(
            owned(&events, &[(3.5, 9.0)], &RIDE),
            [(3.995, f64::INFINITY)]
        );
        assert!(
            owned(&events, &[(4.5, 9.0)], &RIDE).is_empty(),
            "no stroke in it"
        );
        assert_eq!(
            owned(&events, &[(0.5, 1.5), (1.5, 2.5)], &RIDE),
            [(0.995, 2.995)],
            "two that meet are one"
        );
    }

    #[test]
    fn the_curve_is_read_between_its_points() {
        let curve = [EnvPoint::new(1.0, 0.0), EnvPoint::new(2.0, 6.0)];
        assert!(value_at(&curve, 0.0).abs() < 1e-12);
        assert!((value_at(&curve, 1.5) - 3.0).abs() < 1e-12);
        assert!((value_at(&curve, 9.0) - 6.0).abs() < 1e-12);
        assert!(value_at(&[], 1.0).abs() < 1e-12);
    }

    /// Outside the window the old curve stays as it was; inside it is the
    /// new one, ramped into and out of.
    #[test]
    fn the_old_curve_is_kept_outside() {
        let old = [
            EnvPoint::new(0.5, -3.0),
            EnvPoint::new(1.2, -3.0),
            EnvPoint::new(3.5, -3.0),
        ];
        // Steps, as a ride draws them: held, then ramped over a moment.
        let new = [
            EnvPoint::new(1.0, 4.0),
            EnvPoint::new(2.499, 4.0),
            EnvPoint::new(2.5, 2.0),
        ];
        let spliced = splice(&old, &new, &[(2.0, 3.0)], 0.001);
        let at = |t| value_at(&spliced, t);
        assert!((at(1.5) + 3.0).abs() < 1e-9, "before: the old");
        assert!((at(2.2) - 4.0).abs() < 1e-9, "inside: the new");
        assert!((at(2.7) - 2.0).abs() < 1e-9);
        assert!((at(3.2) + 3.0).abs() < 1e-9, "after: the old again");
        assert!(spliced.windows(2).all(|w| w[0].t_s < w[1].t_s), "in order");
        // An open window ends the old curve there.
        let open = splice(&old, &new, &[(2.0, f64::INFINITY)], 0.001);
        assert!((value_at(&open, 9.0) - 2.0).abs() < 1e-9);
    }
}
