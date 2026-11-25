//! COMPRESS / EXPAND — squeeze velocities toward a pivot, or throw them apart.
//!
//! A compressor for a MIDI performance rather than for audio: pick a
//! pivot velocity, then pull every note toward it (compress, flattening
//! the dynamics) or push every note away from it (expand, exaggerating
//! them). One bipolar control does both, because they're the same
//! operation with opposite sign and that's how it should feel under the
//! hand.
//!
//! ## Two divergences from the Lua
//!
//! **The labels are swapped upstream.** MVelocity draws "Expand" above
//! the slider's midpoint, but the branch it takes there is
//! `base + (target - base) * t` — which moves notes *toward* the target,
//! i.e. compresses. Below the midpoint it moves them away, i.e. expands.
//! We implement the arithmetic and name it for what it does:
//! [`Dynamics::amount`] is negative to compress, positive to expand.
//!
//! **FACTOR does nothing upstream.** The script draws a FACTOR button
//! beside TARGET and toggles a border on it, but no code path reads the
//! mode — both buttons run the same fixed-pivot math. Here FACTOR is
//! [`Pivot::Mean`]: the pivot is the selection's own average velocity, so
//! compressing pulls a phrase toward its own center instead of toward a
//! number you had to guess. That's what an audio compressor does, and
//! it's the mode you want on a take you didn't play yourself.

use super::{MAX_VELOCITY, MIN_VELOCITY, Note, Range, VelocityEdit, targets};

/// How far expansion throws a note, per unit of amount.
///
/// Two, from MVelocity's `expandFactor`. Expansion needs more travel
/// than compression to feel symmetric: compression has a hard floor (all
/// notes land on the pivot and can go no further), while expansion has
/// none, so matching their slider throws one-to-one makes expansion feel
/// inert next to compression.
const EXPAND_FACTOR: f64 = 2.0;

/// What velocities are squeezed toward or thrown away from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pivot {
    /// A fixed velocity — MVelocity's TARGET, dragged or scrolled.
    Fixed(u8),
    /// The mean velocity of the notes being edited — MVelocity's FACTOR
    /// button, given the meaning its name implies. Recomputed per apply,
    /// so it tracks the material.
    Mean,
}

impl Default for Pivot {
    /// 80, MVelocity's `currentValue`.
    fn default() -> Self {
        Self::Fixed(80)
    }
}

/// The compress/expand engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dynamics {
    /// -1.0 = every note collapsed onto the pivot, 0.0 = untouched,
    /// +1.0 = every note thrown twice its distance from the pivot
    /// further out (`EXPAND_FACTOR`, private — the curve's shape is this
    /// module's business, not API).
    pub amount: f64,
    pub pivot: Pivot,
}

impl Default for Dynamics {
    fn default() -> Self {
        Self {
            amount: 0.0,
            pivot: Pivot::default(),
        }
    }
}

impl Dynamics {
    pub fn new(amount: f64, pivot: Pivot) -> Self {
        Self {
            amount: amount.clamp(-1.0, 1.0),
            pivot,
        }
    }

    /// The pivot velocity this engine will use against `notes`.
    ///
    /// Exposed so the UI can show where [`Pivot::Mean`] actually landed —
    /// an invisible pivot is an unpredictable control.
    pub fn pivot_velocity(&self, notes: &[Note]) -> f64 {
        match self.pivot {
            Pivot::Fixed(v) => f64::from(v.clamp(MIN_VELOCITY, MAX_VELOCITY)),
            Pivot::Mean => {
                let (sum, count) = targets(notes).fold((0.0, 0u32), |(s, c), (_, n)| {
                    (s + f64::from(n.velocity), c + 1)
                });
                if count == 0 {
                    f64::from(MIN_VELOCITY)
                } else {
                    sum / f64::from(count)
                }
            }
        }
    }

    /// Compress or expand `notes` around the pivot.
    pub fn apply(&self, notes: &[Note], range: Range) -> Vec<VelocityEdit> {
        let amount = self.amount.clamp(-1.0, 1.0);
        let pivot = self.pivot_velocity(notes);
        targets(notes)
            .map(|(_, note)| {
                let base = f64::from(note.velocity);
                let distance = base - pivot;
                // One expression for both directions: compression scales
                // the distance down toward zero, expansion scales it up.
                let scaled = if amount >= 0.0 {
                    distance * (1.0 + amount * EXPAND_FACTOR)
                } else {
                    distance * (1.0 + amount)
                };
                VelocityEdit {
                    index: note.index,
                    velocity: range.clamp(pivot + scaled),
                }
            })
            .collect()
    }
}
