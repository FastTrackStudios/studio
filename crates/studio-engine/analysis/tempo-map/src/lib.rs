//! Tempo maps from a recording's separated stems.
//!
//! The method is the one worked out song by song with scripts
//! (`.claude/skills/song-tempo-map`): beats from the drum stems, snapped
//! to hits, checked by the backbeat's phase, bar 1 chosen, one tempo point
//! per bar from a line through the neighbouring beats. Here it is a
//! library, with what the scripts could not do (meters other than 4/4,
//! meter changes, repairing an added or dropped beat) and the measurements
//! that say how good a map is.
//!
//! - [`features`]: the stems measured once (envelopes, hits, levels).
//! - [`track`]: the pulse and the snap to hits ([`onset`], [`beat`]).
//! - [`phase`]: the backbeat's phase, irregular intervals, the repair.
//! - [`downbeat`]: the candidates for bar 1, and the meter's guess.
//! - [`meter`], [`map`]: bars over beats in any meter, one point per bar.
//! - [`eval`]: a map measured, two maps compared.
//! - [`analysis`]: a song's analysis as kept data, and what is decided on
//!   it. [`rpp`] reads a REAPER project's map; [`bench`] runs a test set.
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::float_cmp,
    clippy::while_float,
    clippy::manual_midpoint,
    clippy::suspicious_operation_groupings,
    clippy::needless_range_loop,
    reason = "numeric code over slices whose bounds are computed beside each use"
)]
#![allow(
    clippy::too_many_lines,
    clippy::too_long_first_doc_paragraph,
    clippy::items_after_statements,
    clippy::option_if_let_else,
    clippy::single_match_else,
    clippy::if_not_else,
    clippy::useless_let_if_seq,
    clippy::collapsible_if,
    clippy::assert_is_empty,
    reason = "the ported steps are kept in the order and shape of the scripts they came from"
)]

pub mod analysis;
pub mod audio;
pub mod beat;
pub mod bench;
pub mod downbeat;
pub mod eval;
pub mod features;
pub mod fit;
pub mod map;
pub mod meter;
pub mod onset;
pub mod phase;
pub mod rpp;
pub mod track;
