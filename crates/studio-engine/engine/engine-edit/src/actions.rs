//! The edit domain's actions: drum editing (quantize, groove, place,
//! stretch, revert, requantize, the review, the lanes, the samples), the
//! drum level and fundamental, removing a range ([`edit`]), beside what
//! every domain's actions share (the core's, re-exported here so
//! `crate::actions::…` reads as it did in studio-engine).

pub use engine_core::actions::*;

pub mod edit;
pub use edit::*;

pub use editing::drums::KitPiece;
