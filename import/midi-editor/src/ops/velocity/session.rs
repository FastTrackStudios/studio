//! The four engines composed over one held baseline.
//!
//! An editing session: you open it on a take, and from then on every
//! control is a *parameter* rather than an *action*. Nothing accumulates.
//! Push the pattern to 60%, draw a curve, pull the compressor down, put
//! the compressor back — you're exactly where you were before you
//! touched it, because the whole result is recomputed from the baseline
//! every time rather than layered onto whatever the take currently holds.
//!
//! MVelocity aims at this too but leaks: `slider_gr.onchange` and
//! `slider_reduce.onchange` both re-snapshot `baseInitialVelocities`
//! mid-interaction, so which section you touched last silently changes
//! what "back to neutral" means. Owning the baseline in exactly one place
//! is the fix, and it's why the engines themselves are pure functions.
//!
//! ## The chain
//!
//! ```text
//! baseline ─→ curve ─→ pattern ─→ randomize ─→ dynamics ─→ edits
//!             shape    accent     humanize     glue
//! ```
//!
//! In that order because it's how you'd build the part by hand: draw the
//! macro dynamic shape of the phrase, stamp the rhythmic accents onto it,
//! rough up the result so it isn't machine-perfect, then squeeze the
//! whole thing into the range the part needs to sit in. Compression last
//! is the one that really matters — it's the stage that guarantees the
//! final spread, and anything after it would undo that guarantee.

use super::{Curve, Dynamics, Note, Pattern, Randomize, Range, VelocityEdit};

/// A live velocity-editing session over a fixed set of notes.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// Velocities as they were when the session opened. The single
    /// source of truth for "neutral".
    baseline: Vec<Note>,

    /// The velocity window everything is clamped into.
    pub range: Range,

    /// The drawn contour. `None` until you draw one — a curve is a
    /// replacement, not a blend, so an always-on curve would mean the
    /// other engines never got to speak.
    pub curve: Option<Curve>,

    /// The cyclic accent pattern, and how far it's blended in.
    pub pattern: Pattern,
    pub pattern_amount: f64,

    /// The held random hand, and how far it's blended in.
    pub randomize: Randomize,
    pub randomize_amount: f64,

    /// Compress/expand, applied to whatever the earlier stages produced.
    pub dynamics: Dynamics,
}

impl Session {
    /// Open a session over `notes`, taking their current velocities as
    /// the baseline.
    pub fn new(notes: Vec<Note>) -> Self {
        Self {
            baseline: notes,
            ..Self::default()
        }
    }

    /// The notes this session was opened on.
    pub fn baseline(&self) -> &[Note] {
        &self.baseline
    }

    pub fn is_empty(&self) -> bool {
        self.baseline.is_empty()
    }

    /// Adopt a new set of notes as the baseline.
    ///
    /// For when the take changed underneath us — notes added, deleted, or
    /// the selection moved. Everything downstream recomputes against the
    /// new material; parameters are kept, so a pattern you dialled in
    /// survives selecting a different bar. The random hand is dropped,
    /// because targets dealt for notes that no longer exist would land on
    /// the wrong ones.
    pub fn resync(&mut self, notes: Vec<Note>) {
        self.baseline = notes;
        self.randomize = Randomize::default();
    }

    /// Return every control to neutral, keeping the baseline.
    pub fn reset(&mut self) {
        let baseline = std::mem::take(&mut self.baseline);
        *self = Self {
            baseline,
            ..Self::default()
        };
    }

    /// Deal a fresh random hand sized to the current baseline.
    pub fn roll(&mut self) {
        self.randomize.roll(self.baseline.len(), self.range);
    }

    /// [`Session::roll`] from a fixed seed, for reproducible takes.
    pub fn roll_seeded(&mut self, seed: u64) {
        self.randomize
            .roll_seeded(self.baseline.len(), self.range, seed);
    }

    /// The full result of the chain, as absolute velocities per note.
    ///
    /// Every note in the baseline appears, whether or not it moved.
    pub fn resolve(&self) -> Vec<Note> {
        let mut working = self.baseline.clone();

        // Each stage reads the previous stage's output, so the edits have
        // to be computed before they're folded back in.
        if let Some(curve) = &self.curve {
            let staged = curve.apply(&working, self.range);
            merge(&mut working, staged);
        }
        let staged = self
            .pattern
            .apply(&working, self.pattern_amount, self.range);
        merge(&mut working, staged);

        let staged = self
            .randomize
            .apply(&working, self.randomize_amount, self.range);
        merge(&mut working, staged);

        let staged = self.dynamics.apply(&working, self.range);
        merge(&mut working, staged);

        working
    }

    /// What the DAW needs to be told: only the notes that actually moved.
    ///
    /// Filtering here rather than in the sink because the sink writes one
    /// note per call, and a session at neutral over a 600-note take
    /// should cost nothing rather than 600 redundant writes.
    pub fn edits(&self) -> Vec<VelocityEdit> {
        self.resolve()
            .into_iter()
            .zip(&self.baseline)
            .filter(|(now, was)| now.velocity != was.velocity)
            .map(|(now, _)| VelocityEdit {
                index: now.index,
                velocity: now.velocity,
            })
            .collect()
    }
}

/// Fold a stage's output back into the working set, in place.
fn merge(working: &mut [Note], edits: Vec<VelocityEdit>) {
    for edit in edits {
        if let Some(note) = working.iter_mut().find(|n| n.index == edit.index) {
            note.velocity = edit.velocity;
        }
    }
}
