//! Curve shapes across the editor/DAW boundary.
//!
//! `CurveShape` in the editor's core mirrors `EnvelopeShape` in
//! `daw_proto` variant for variant. Core cannot depend on the proto
//! crate — it has one dependency by design — so the two are a deliberate
//! copy, and this module is where that copy is *proved* to be exact.
//!
//! Both conversions are total: no fallback arm, no default, nothing that
//! could silently degrade a shape on the way out to a DAW or back. If
//! either enum grows a variant, this stops compiling, which is the whole
//! reason it is written as an exhaustive match rather than a lookup.

use crate::CurveShape;
use daw::service::EnvelopeShape;

/// Editor shape → DAW shape.
pub fn to_daw(shape: CurveShape) -> EnvelopeShape {
    match shape {
        CurveShape::Linear => EnvelopeShape::Linear,
        CurveShape::Square => EnvelopeShape::Square,
        CurveShape::SlowStartEnd => EnvelopeShape::SlowStartEnd,
        CurveShape::FastStart => EnvelopeShape::FastStart,
        CurveShape::FastEnd => EnvelopeShape::FastEnd,
        CurveShape::Bezier => EnvelopeShape::Bezier,
    }
}

/// DAW shape → editor shape.
pub fn from_daw(shape: EnvelopeShape) -> CurveShape {
    match shape {
        EnvelopeShape::Linear => CurveShape::Linear,
        EnvelopeShape::Square => CurveShape::Square,
        EnvelopeShape::SlowStartEnd => CurveShape::SlowStartEnd,
        EnvelopeShape::FastStart => CurveShape::FastStart,
        EnvelopeShape::FastEnd => CurveShape::FastEnd,
        EnvelopeShape::Bezier => CurveShape::Bezier,
    }
}
