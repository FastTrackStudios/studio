//! RANGE + UPDATE — humanize by rolling a target per note, then blending.
//!
//! The subtlety worth preserving from MVelocity: the random targets are
//! **rolled once and held**, not re-rolled on every parameter change.
//! Pressing UPDATE deals a new hand; dragging the amount slider walks
//! toward that same hand. If the targets re-rolled per frame the slider
//! would jitter under your hand and you could never audition a
//! particular randomization — you'd just be shaking the notes.
//!
//! So [`Randomize`] is stateful (it owns the dealt targets) while the
//! blend stays a pure function of `amount`.

use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::{Note, Range, VelocityEdit, lerp, targets};

/// A held set of random target velocities, one per note.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Randomize {
    /// Indexed by note ordinal, parallel to the slice passed to
    /// [`Randomize::apply`].
    dealt: Vec<u8>,
}

impl Randomize {
    /// Deal `count` fresh targets uniformly inside `range`.
    ///
    /// This is the UPDATE button.
    pub fn roll(&mut self, count: usize, range: Range) {
        self.roll_with(count, range, &mut StdRng::from_entropy());
    }

    /// [`Randomize::roll`] against a caller-supplied RNG, so tests (and
    /// anything wanting a reproducible take) can pin the hand.
    pub fn roll_with(&mut self, count: usize, range: Range, rng: &mut impl Rng) {
        self.dealt.clear();
        self.dealt
            .extend((0..count).map(|_| rng.gen_range(range.min..=range.max)));
    }

    /// Deal from a fixed seed. Two calls with the same seed, count and
    /// range give the same hand.
    pub fn roll_seeded(&mut self, count: usize, range: Range, seed: u64) {
        self.roll_with(count, range, &mut StdRng::seed_from_u64(seed));
    }

    /// The targets currently held, by note ordinal.
    pub fn dealt(&self) -> &[u8] {
        &self.dealt
    }

    pub fn is_empty(&self) -> bool {
        self.dealt.is_empty()
    }

    /// Blend each note from its baseline toward its dealt target.
    ///
    /// Notes past the end of the dealt hand are left alone rather than
    /// dealt a target on the fly: a take that grew since the last UPDATE
    /// should wait for the next one, not have its new notes randomized
    /// behind your back.
    pub fn apply(&self, notes: &[Note], amount: f64, range: Range) -> Vec<VelocityEdit> {
        let amount = amount.clamp(0.0, 1.0);
        targets(notes)
            .filter_map(|(ordinal, note)| {
                let target = *self.dealt.get(ordinal)?;
                Some(VelocityEdit {
                    index: note.index,
                    velocity: range.clamp(lerp(
                        f64::from(note.velocity),
                        f64::from(target),
                        amount,
                    )),
                })
            })
            .collect()
    }
}
