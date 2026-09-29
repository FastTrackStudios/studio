//! Faces: the tube compressors — the two optical units (LA-2A, CL 1B) and
//! the two variable-mu ones (the 670, the Manley-style bus limiter).
//!
//! Each is laid out from its unit's panel: the controls it really has, the
//! scales printed round them, its meter and its paint. Params are
//! namespaced per face (`la2a/…`, `cl1b/…`, `fairchild/…`, `varimu/…`) with
//! each control's real range, unit and detents; every face meters a
//! `<face>/gr` gain-reduction stream (0 GR rests on 0 VU) and declares a
//! `<face>/out` output level.

mod cl1b;
mod fairchild;
mod la2a;
pub mod ui;
mod varimu;

use crate::face::Face;

pub fn faces() -> Vec<Face> {
    vec![la2a::face(), cl1b::face(), fairchild::face(), varimu::face()]
}
