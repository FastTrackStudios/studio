//! Faces: saturation — five circuits, one per family of the FTS
//! saturator's profiles (`saturate-ui/src/faces.rs`'s `SatDesign`s), each
//! drawn as the box that circuit lives in.
//!
//! - **Tube** — a valve preamp: the valves lit behind a window, the stage
//!   selector (triode / pentode), output VU, brown bakelite.
//! - **Tape** — a studio tape machine's electronics: a meter bridge with
//!   two VUs, tape speed and formulation selectors, record level and bias.
//! - **Transformer** — iron and copper: the core and its winding drawn in
//!   an open chassis, warming as it is driven; wing knobs, LED meters.
//! - **Transistor** — a fuzz pedal: hammertone enclosure, a stomp switch,
//!   the circuit's own symbol printed large.
//! - **Digital** — a clipper / bit-crusher: an LCD drawing the staircase
//!   the word length makes, bits and rate on the glass.
//!
//! Every control but drive, mix and output is a *trim* around the circuit
//! as designed (noon = the profile's voicing), so those print − 0 + and read in % off the voicing.
//! Drive, mix and output sit in the same places on every face.

mod digital;
mod tape;
mod transformer;
mod transistor;
mod tube;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![tube::face(), tape::face(), transformer::face(), transistor::face(), digital::face()]
}

/// What a trim prints: its ends and noon (the circuit as designed).
pub(crate) const TRIM_MARKS: &[(f64, &str)] = &[(0.0, "-"), (0.5, "0"), (1.0, "+")];
/// Output's printed scale over −24 … +12 dB.
pub(crate) const OUT_MARKS: &[(f64, &str)] = &[(0.0, "-24"), (12.0 / 36.0, "-12"), (24.0 / 36.0, "0"), (1.0, "+12")];

/// Output: −24 … +12 dB in whole dB, bipolar about unity.
pub(crate) fn output(addr: &str) -> surface_proto::ParamInfo {
    let r = surface_proto::Range { steps: Some(37), ..surface_proto::Range::linear(-24.0, 12.0, surface_proto::Unit::Db) };
    crate::delay::parts::param(addr, "Output", r, 0.0).bipolar().featured()
}
