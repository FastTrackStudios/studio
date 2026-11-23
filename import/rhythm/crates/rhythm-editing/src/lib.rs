//! Pure timeline editing math for moving individual hits and quantizing
//! performances — deciding per-seam between natural slip+crossfade editing,
//! transient-protected tail stretching, and full warp stretching.
//!
//! Designed with zero DAW/DSP dependencies so it can graduate directly into
//! `FastTrackStudios/daw` (`daw-editing`) and be shared by `rhythm`, `pitch`,
//! MIDI editors, and third-party extensions.

pub mod hybrid;
pub mod placement;

pub use hybrid::{
    AudioSegment, EditStrategy, HitEditMode, HitMove, HybridPlan, HybridPolicy, PlanAudit,
    RecordedHit, SegmentKind, plan_hybrid, quantize_to_grid,
};
pub use placement::{Cut, Hit, Piece, place, place_within};
