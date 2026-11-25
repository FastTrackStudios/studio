//! One drawn curve, pointed at any lane.
//!
//! [`velocity::Curve`](crate::ops::velocity::Curve) turned out to be the
//! general tool in this crate and velocity turned out to be one of its
//! uses. A Bézier over a span is equally the right answer for a CC swell,
//! a pitch-bend scoop, a gradual legato, or pushing a phrase off the
//! grid. This module is that observation made concrete: pick a [`Lane`],
//! draw once, apply.
//!
//! ## Two kinds of lane
//!
//! The split that shapes the whole module:
//!
//! - **Per-note lanes** ([`Lane::Velocity`], [`Lane::Length`],
//!   [`Lane::Timing`]) sample the curve once per note, at that note's
//!   position in the selection. There are exactly as many results as
//!   notes.
//! - **Continuous lanes** ([`Lane::Cc`], [`Lane::PitchBend`]) sample the
//!   curve at a fixed *resolution* across the time span, because a CC
//!   ramp is a stream of events that exists independently of where the
//!   notes happen to fall. Drawing a swell under one held chord has to
//!   work, and it can't if the lane only emits where notes are.
//!
//! That's why [`ShapeEdit`] is an enum rather than a `Vec<something>`:
//! the two kinds genuinely produce different things, and flattening them
//! into a common "edit" type would mean inventing a position for CC
//! events that don't have a note to belong to.

use crate::ops::velocity::{Curve, MAX_VELOCITY, MIN_VELOCITY, Note, Range, VelocityEdit, targets};

/// What a shape is applied to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Note velocity — the original use.
    Velocity,
    /// Note length, as a multiplier on each note's current length.
    /// Curve height 1.0 means "leave it", 0.0 means "as short as
    /// allowed" — see [`Shape::length_range`].
    Length,
    /// Note start, nudged earlier or later. Curve height 0.5 is "don't
    /// move"; above pushes late, below pulls early.
    Timing,
    /// A continuous controller lane.
    Cc(u8),
    /// The pitch-bend lane.
    PitchBend,
}

impl Lane {
    /// Whether this lane emits one value per note (rather than a stream
    /// across time).
    pub fn is_per_note(self) -> bool {
        matches!(self, Lane::Velocity | Lane::Length | Lane::Timing)
    }

    pub fn label(self) -> String {
        match self {
            Lane::Velocity => "Velocity".to_string(),
            Lane::Length => "Length".to_string(),
            Lane::Timing => "Timing".to_string(),
            Lane::Cc(n) => format!("CC{n}"),
            Lane::PitchBend => "Pitch Bend".to_string(),
        }
    }
}

/// A note with the timing information the continuous lanes need.
///
/// [`crate::ops::velocity::Note`] deliberately carries only what the velocity
/// engines use; shaping length and timing needs more.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedNote {
    pub index: u32,
    pub velocity: u8,
    pub selected: bool,
    pub start_ppq: f64,
    pub length_ppq: f64,
}

impl ShapedNote {
    pub fn end_ppq(&self) -> f64 {
        self.start_ppq + self.length_ppq
    }

    fn as_note(&self) -> Note {
        Note {
            index: self.index,
            velocity: self.velocity,
            selected: self.selected,
        }
    }
}

/// A note's new length, in PPQ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LengthEdit {
    pub index: u32,
    pub length_ppq: f64,
}

/// A note's new start, in PPQ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimingEdit {
    pub index: u32,
    pub start_ppq: f64,
}

/// One controller event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CcPoint {
    pub position_ppq: f64,
    pub value: u8,
}

/// One pitch-bend event. `value` is centred at 0, ±8191.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BendPoint {
    pub position_ppq: f64,
    pub value: i16,
}

/// What applying a shape produced.
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeEdit {
    Velocity(Vec<VelocityEdit>),
    Length(Vec<LengthEdit>),
    Timing(Vec<TimingEdit>),
    Cc {
        controller: u8,
        points: Vec<CcPoint>,
    },
    PitchBend(Vec<BendPoint>),
}

impl ShapeEdit {
    /// How many events this would write. For status lines.
    pub fn len(&self) -> usize {
        match self {
            ShapeEdit::Velocity(v) => v.len(),
            ShapeEdit::Length(v) => v.len(),
            ShapeEdit::Timing(v) => v.len(),
            ShapeEdit::Cc { points, .. } => points.len(),
            ShapeEdit::PitchBend(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A curve aimed at a lane.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub curve: Curve,
    pub lane: Lane,
    /// Velocity clamp, for [`Lane::Velocity`].
    pub range: Range,
    /// Shortest and longest a note may become, as a multiplier on its
    /// current length. Only [`Lane::Length`] reads it.
    pub length_range: (f64, f64),
    /// How far [`Lane::Timing`] may push a note, in PPQ, at full
    /// deflection.
    pub timing_depth_ppq: f64,
    /// Events per quarter note for the continuous lanes.
    ///
    /// Eight is REAPER's own default CC density for drawn ramps: fine
    /// enough that a swell sounds smooth, coarse enough that a bar
    /// doesn't cost hundreds of events. Raising it past ~32 mostly buys
    /// project bloat.
    pub resolution_per_qn: f64,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            curve: Curve::default(),
            lane: Lane::Velocity,
            range: Range::default(),
            length_range: (0.1, 2.0),
            timing_depth_ppq: 120.0,
            resolution_per_qn: 8.0,
        }
    }
}

/// Ticks per quarter note, matching [`crate::ops::arp::PPQ`].
const PPQ: f64 = 960.0;

impl Shape {
    pub fn new(curve: Curve, lane: Lane) -> Self {
        Self {
            curve,
            lane,
            ..Self::default()
        }
    }

    /// Apply the shape to `notes`.
    ///
    /// The time span for the continuous lanes is taken from the notes
    /// themselves — first onset to last release — so drawing a swell
    /// under a selection covers exactly that selection.
    pub fn apply(&self, notes: &[ShapedNote]) -> ShapeEdit {
        let picked: Vec<ShapedNote> = {
            let plain: Vec<Note> = notes.iter().map(|n| n.as_note()).collect();
            targets(&plain).map(|(i, _)| notes[i]).collect()
        };

        match self.lane {
            Lane::Velocity => ShapeEdit::Velocity(self.velocity(&picked)),
            Lane::Length => ShapeEdit::Length(self.length(&picked)),
            Lane::Timing => ShapeEdit::Timing(self.timing(&picked)),
            Lane::Cc(controller) => ShapeEdit::Cc {
                controller,
                points: self.cc(&picked),
            },
            Lane::PitchBend => ShapeEdit::PitchBend(self.bend(&picked)),
        }
    }

    /// Curve height at note `i` of `n`, in 0.0..=1.0.
    fn at_note(&self, i: usize, n: usize) -> f64 {
        let last = n.saturating_sub(1);
        let t = if last == 0 {
            0.0
        } else {
            i as f64 / last as f64
        };
        self.curve.evaluate(t).1
    }

    fn velocity(&self, notes: &[ShapedNote]) -> Vec<VelocityEdit> {
        notes
            .iter()
            .enumerate()
            .map(|(i, note)| VelocityEdit {
                index: note.index,
                velocity: self
                    .range
                    .clamp(self.at_note(i, notes.len()) * f64::from(MAX_VELOCITY)),
            })
            .collect()
    }

    fn length(&self, notes: &[ShapedNote]) -> Vec<LengthEdit> {
        let (lo, hi) = self.length_range;
        notes
            .iter()
            .enumerate()
            .map(|(i, note)| {
                let factor = lo + self.at_note(i, notes.len()) * (hi - lo);
                LengthEdit {
                    index: note.index,
                    // Never zero: a zero-length note is not a note, and
                    // some backends treat it as a delete.
                    length_ppq: (note.length_ppq * factor).max(1.0),
                }
            })
            .collect()
    }

    fn timing(&self, notes: &[ShapedNote]) -> Vec<TimingEdit> {
        notes
            .iter()
            .enumerate()
            .map(|(i, note)| {
                // Centred: 0.5 is no movement, so a flat curve at half
                // height is the identity and the control is bipolar
                // without needing a separate sign.
                let offset = (self.at_note(i, notes.len()) - 0.5) * 2.0 * self.timing_depth_ppq;
                TimingEdit {
                    index: note.index,
                    start_ppq: (note.start_ppq + offset).max(0.0),
                }
            })
            .collect()
    }

    /// Onset of the first note to release of the last.
    fn span(&self, notes: &[ShapedNote]) -> Option<(f64, f64)> {
        let start = notes
            .iter()
            .map(|n| n.start_ppq)
            .fold(f64::INFINITY, f64::min);
        let end = notes
            .iter()
            .map(|n| n.end_ppq())
            .fold(f64::NEG_INFINITY, f64::max);
        (end > start).then_some((start, end))
    }

    /// Sample positions across the span, inclusive of both ends.
    fn samples(&self, notes: &[ShapedNote]) -> Vec<(f64, f64)> {
        let Some((start, end)) = self.span(notes) else {
            return Vec::new();
        };
        let step = PPQ / self.resolution_per_qn.max(1.0);
        let count = (((end - start) / step).ceil() as usize).max(1);
        (0..=count)
            .map(|i| {
                let t = i as f64 / count as f64;
                (start + (end - start) * t, self.curve.evaluate(t).1)
            })
            .collect()
    }

    fn cc(&self, notes: &[ShapedNote]) -> Vec<CcPoint> {
        self.samples(notes)
            .into_iter()
            .map(|(position_ppq, y)| CcPoint {
                position_ppq,
                value: (y * f64::from(MAX_VELOCITY)).round().clamp(0.0, 127.0) as u8,
            })
            .collect()
    }

    fn bend(&self, notes: &[ShapedNote]) -> Vec<BendPoint> {
        self.samples(notes)
            .into_iter()
            .map(|(position_ppq, y)| BendPoint {
                position_ppq,
                // Centred like the timing lane: half height is no bend,
                // so a flat curve leaves the pitch alone.
                value: (((y - 0.5) * 2.0) * 8191.0).round().clamp(-8192.0, 8191.0) as i16,
            })
            .collect()
    }
}

/// Convenience: the velocity clamp bounds, re-exported so a UI building a
/// [`Shape`] doesn't have to reach into `velocity`.
pub const VELOCITY_BOUNDS: (u8, u8) = (MIN_VELOCITY, MAX_VELOCITY);
