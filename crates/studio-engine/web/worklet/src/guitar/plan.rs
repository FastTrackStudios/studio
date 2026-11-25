//! Where each NAM model of a chain runs — the render thread, or a worker —
//! and at what latency.
//!
//! One A2 model costs about half a 128-frame quantum in wasm, and a chain's
//! models are mostly in series (boost → drives → amp): each needs the one
//! before it, so they cannot run side by side within a quantum. The plan:
//!
//! 1. Models run **inline**, on the render thread, while the chain still fits
//!    the render budget. No latency.
//! 2. **Amp R** runs on a worker *at the same time as Amp L* — both hear the
//!    dry guitar, so its input is known the moment Amp L starts. No latency,
//!    and it costs the render thread only the wait beyond Amp L.
//! 3. A serial model that does not fit runs on a worker **one quantum
//!    behind** ([`Lag::One`]): it gets a whole quantum to itself, for 2.7 ms
//!    at 48 kHz. Each such model adds one quantum.
//!
//! Costs are estimates to begin with and measurements once the models have
//! run; a host re-plans when a worker starts missing.

use crate::guitar::remote::Lag;

/// Where one model runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Inline,
    /// On a worker, at this lag. `Lag::Zero` is only planned for Amp R.
    Worker(Lag),
}

/// One NAM model in chain order, as the planner sees it.
#[derive(Clone, Debug)]
pub struct Model {
    /// Its slot in the chain.
    pub slot: usize,
    /// Microseconds per quantum.
    pub cost_us: u32,
    /// Amp R of a dual-amp stage — runs beside Amp L, not after it.
    pub parallel: bool,
    /// Amp L of a dual-amp stage. It is summed with Amp R, so both must be
    /// at the same lag — zero — or the blend combs.
    pub pinned: bool,
    /// Playing now (not bypassed). A bypassed model costs nothing until it
    /// is switched on, so it waits on a worker.
    pub active: bool,
}

/// A chain's placements, parallel to the `models` it was planned from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The chain slot of each placed model.
    pub slots: Vec<usize>,
    pub places: Vec<Place>,
    /// Added latency as planned — the lagged models that are playing — in
    /// quanta. Switching a lagged model on or off moves it by one.
    pub lag_quanta: u32,
}

/// What the render thread may spend on models per quantum, in µs.
///
/// A quantum is `quantum_frames / sample_rate`; the browser keeps some of it
/// for itself, and the rest of the chain (EQ, delay, reverb) needs its share,
/// so the models get `share` of it (0.55 is the default the host uses).
#[must_use]
pub fn model_budget_us(quantum_frames: u32, sample_rate: f64, share: f64) -> u32 {
    (f64::from(quantum_frames) / sample_rate * 1e6 * share) as u32
}

/// Plan `models` against `budget_us` (see [`model_budget_us`]).
///
/// The dual-amp stage is placed first and never lags: Amp L inline, Amp R
/// on a zero-lag worker beside it. Then the serial models (boost, drives, a
/// single amp) that are *playing* go inline in chain order while the budget
/// lasts; then the bypassed ones, if room is left; everything else waits on
/// a lag-one worker.
#[must_use]
pub fn plan(models: &[Model], budget_us: u32) -> Plan {
    let mut spent: u32 = models.iter().filter(|m| m.pinned).map(|m| m.cost_us).sum();
    let mut places: Vec<Option<Place>> = models
        .iter()
        .map(|m| {
            if m.parallel {
                Some(Place::Worker(Lag::Zero))
            } else if m.pinned {
                Some(Place::Inline)
            } else {
                None
            }
        })
        .collect();
    for active in [true, false] {
        for (m, place) in models.iter().zip(places.iter_mut()) {
            if place.is_some() || m.active != active {
                continue;
            }
            if spent + m.cost_us <= budget_us {
                spent += m.cost_us;
                *place = Some(Place::Inline);
            }
        }
    }
    let places: Vec<Place> = places
        .into_iter()
        .map(|p| p.unwrap_or(Place::Worker(Lag::One)))
        .collect();
    let lag_quanta = models
        .iter()
        .zip(&places)
        .filter(|(m, p)| m.active && **p == Place::Worker(Lag::One))
        .count() as u32;
    Plan {
        slots: models.iter().map(|m| m.slot).collect(),
        places,
        lag_quanta,
    }
}

impl Plan {
    /// Where the block in chain slot `slot` runs (a non-model: inline).
    #[must_use]
    pub fn place(&self, slot: usize) -> Place {
        self.slots
            .iter()
            .position(|&s| s == slot)
            .map_or(Place::Inline, |i| self.places[i])
    }

    /// The slots that run on a worker, with their lag.
    pub fn remote(&self) -> impl Iterator<Item = (usize, Lag)> + '_ {
        self.slots
            .iter()
            .zip(&self.places)
            .filter_map(|(&s, p)| match p {
                Place::Worker(lag) => Some((s, *lag)),
                Place::Inline => None,
            })
    }
}
