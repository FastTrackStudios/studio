//! The `document` contract: frame's own document model ([`model`]) and the
//! typed edits over it ([`ops`]).

mod hug;
pub mod model;
pub mod ops;

pub use model::*;
pub use ops::{History, Op, OpError, Prop};
